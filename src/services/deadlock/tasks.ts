/**
 * The queue task that fetches Deadlock stats for some accounts
 * (BACKGROUND_TASKS, consumed by lib/background-tasks). Kept apart from
 * the fetcher so the gateway Durable Object can queue one without loading
 * it.
 */

import type { RefreshReason } from "./fetcher";

export const DEADLOCK_REFRESH_TASK = "deadlock.refresh" as const;
/** Accounts one task names at most. */
export const DEADLOCK_TASK_MAX_ACCOUNTS = 50;
/** Cloudflare Queues caps a delivery delay at 12 hours. */
const MAX_DELAY_SECONDS = 12 * 60 * 60;

const REASONS: readonly RefreshReason[] = [
  "cron",
  "view",
  "post_game",
  "manual",
];

export interface DeadlockRefreshTask {
  type: typeof DEADLOCK_REFRESH_TASK;
  accountIds: number[];
  reason: RefreshReason;
}

export interface DeadlockRefreshEnvelope {
  version: 1;
  task: DeadlockRefreshTask;
}

export function createDeadlockRefreshTask(
  accountIds: readonly number[],
  reason: RefreshReason,
): DeadlockRefreshEnvelope {
  const ids = [...new Set(accountIds)]
    .filter((id) => Number.isInteger(id) && id > 0 && id <= 0xffffffff)
    .slice(0, DEADLOCK_TASK_MAX_ACCOUNTS);
  return {
    version: 1,
    task: { type: DEADLOCK_REFRESH_TASK, accountIds: ids, reason },
  };
}

/** A received task body, normalised; null when it is not a valid one. */
export function parseDeadlockRefreshTask(
  task: Record<string, unknown>,
): DeadlockRefreshTask | null {
  if (task.type !== DEADLOCK_REFRESH_TASK || !Array.isArray(task.accountIds)) {
    return null;
  }
  const reason = REASONS.includes(task.reason as RefreshReason)
    ? (task.reason as RefreshReason)
    : "cron";
  const envelope = createDeadlockRefreshTask(
    task.accountIds.filter((id): id is number => typeof id === "number"),
    reason,
  );
  return envelope.task.accountIds.length > 0 ? envelope.task : null;
}

export function queueDelaySeconds(delayMs: number): number {
  return Math.min(MAX_DELAY_SECONDS, Math.max(0, Math.ceil(delayMs / 1000)));
}
