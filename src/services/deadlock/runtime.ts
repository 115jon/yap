/**
 * Production wiring for the Deadlock fetcher: the bound D1, KV and R2, the
 * global outbound budget (the rate limiter Durable Object, 60 calls a
 * minute whatever happens), the gateway broadcast that tells open profiles
 * to refetch, and the queue. Routes, the cron and the queue consumer use
 * this; the tests build `FetcherDeps` themselves.
 */

import { env } from "cloudflare:workers";

import { checkRateLimitDOFailClosed } from "@/lib/rate-limit";
import type { BackgroundTaskEnvelope } from "@/lib/background-tasks";

import { createDeadlockApi } from "./api";
import { syncDeadlockAssets } from "./assets";
import {
  reconcileDeadlockTracking,
  refreshDeadlockAccounts,
  type FetcherDeps,
  type RefreshReason,
} from "./fetcher";
import { createDeadlockRefreshTask, queueDelaySeconds } from "./tasks";

/** Outbound calls to deadlock-api.com per minute, across every isolate. */
export const DEADLOCK_API_BUDGET = { limit: 60, windowMs: 60_000 } as const;

const USER_AGENT = "Yap (+https://yap.115jon.com)";

function bindings() {
  return env as unknown as CloudflareEnv;
}

async function boardVersion(db: D1Database, userId: string) {
  const row = await db
    .prepare(`SELECT version FROM profile_boards WHERE user_id = ?`)
    .bind(userId)
    .first<{ version: number }>();
  return row?.version ?? 0;
}

async function broadcast(target: Record<string, string>, data: unknown) {
  const namespace = bindings().MEETING_ROOM;
  const stub = namespace.get(namespace.idFromName("global-gateway"));
  await stub.fetch("https://internal/broadcast", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ...target, event: "PROFILE_BOARD_UPDATE", data }),
  });
}

/** KV key of the API-wide pause (epoch ms) after a 429/503/504. */
export const DEADLOCK_PAUSE_KEY = "deadlock:paused_until";

/** The shared pause in KV; it expires on its own. */
export function kvPause(kv: KVNamespace) {
  return {
    async until() {
      const value = Number(await kv.get(DEADLOCK_PAUSE_KEY));
      return Number.isFinite(value) && value > 0 ? value : null;
    },
    async hold(untilMs: number) {
      const ttl = Math.ceil((untilMs - Date.now()) / 1000);
      // KV needs at least 60 s; a shorter pause just lasts a minute.
      await kv.put(DEADLOCK_PAUSE_KEY, String(untilMs), {
        expirationTtl: Math.max(60, ttl),
      });
    },
  };
}

export function productionFetcherDeps(): FetcherDeps {
  const e = bindings();
  const db = e.DB as unknown as D1Database;
  return {
    db,
    kv: e.CACHE as unknown as KVNamespace,
    bucket: e.BUCKET as unknown as R2Bucket,
    fetch: (input, init) => fetch(input, init),
    api: createDeadlockApi({
      fetch: (input, init) => fetch(input, init),
      userAgent: USER_AGENT,
      // Fail closed: without the limiter, nothing is sent this run.
      budget: async () =>
        (await checkRateLimitDOFailClosed(
          "deadlock-api",
          "deadlock-api",
          DEADLOCK_API_BUDGET,
        )) === null,
      pause: kvPause(e.CACHE as unknown as KVNamespace),
    }),
    // A data-only update: open profiles refetch whatever the version.
    notify: async (userId) => {
      const data = {
        user_id: userId,
        version: await boardVersion(db, userId),
        data: true,
      };
      await Promise.all([
        broadcast({ member_user_id: userId }, data),
        broadcast({ target_user_id: userId }, data),
      ]);
    },
  };
}

export function runDeadlockRefresh(
  accountIds: readonly number[] | undefined,
  reason: RefreshReason,
) {
  return refreshDeadlockAccounts(productionFetcherDeps(), {
    accountIds,
    reason,
  });
}

/** The 10-minute sweep: track who has the widget, then fetch who is due. */
export async function runDeadlockCron() {
  const deps = productionFetcherDeps();
  const tracking = await reconcileDeadlockTracking(deps.db);
  const outcomes = await refreshDeadlockAccounts(deps, { reason: "cron" });
  return { ...tracking, fetched: outcomes.length };
}

export function runDeadlockAssetSync() {
  const deps = productionFetcherDeps();
  return syncDeadlockAssets(deps);
}

/**
 * Queues a fetch for `accountIds` (after `delayMs`). Without a queue (a
 * Worker Preview, local dev) an immediate fetch runs inline; a delayed one
 * is left to the 10-minute sweep, which `next_due_at` already points at.
 */
export async function scheduleDeadlockRefresh(
  accountIds: readonly number[],
  options: { reason: RefreshReason; delayMs?: number },
): Promise<void> {
  if (accountIds.length === 0) return;
  const queue = bindings().BACKGROUND_TASKS;
  const envelope = createDeadlockRefreshTask(accountIds, options.reason);
  if (queue) {
    await queue.send(envelope as unknown as BackgroundTaskEnvelope, {
      contentType: "json",
      delaySeconds: queueDelaySeconds(options.delayMs ?? 0),
    });
    return;
  }
  if (!options.delayMs) {
    await runDeadlockRefresh(envelope.task.accountIds, options.reason);
  }
}
