/**
 * Connections (migration 042): verified links to accounts on other services.
 *
 *   link flow   createLinkState → provider → consumeLinkState → linkConnection
 *   owner       listOwnConnections, setShowOnProfile, unlinkConnection
 *   profiles    getShownConnections (what viewers get)
 *   widgets     getVerifiedConnection (e.g. the Steam account id for stats)
 *
 * A link state is a random nonce (in the provider's return_to) and a random
 * browser binding (an HttpOnly cookie). Only their SHA-256 hashes are
 * stored, and a state is consumed by DELETE … RETURNING, so it works once.
 *
 * A link the native apps start (their session is not a browser cookie)
 * reaches the system browser through the begin route instead: it carries
 * the nonce and the binding, and checkLinkBinding lets the browser take the
 * binding as its cookie (as often as the browser loads the URL: some load a
 * link twice). Its callback links nothing: holdLinkResult keeps the verified account
 * behind a one-time code the browser hands to the app, and takeLinkResult
 * gives it only to the account that started the link (migration 050).
 */

import {
  isConnectionProvider,
  type ConnectionProvider,
  type OwnConnection,
  type PublicConnection,
} from "@/lib/connections";
import { clog } from "@/lib/console-logger";

import type { VerifiedAccount } from "./connections";

const log = clog("connections");

/** How long a started link waits for the provider's callback. */
export const LINK_STATE_TTL_MS = 10 * 60_000;

/* ------------------------------------------------------------------ */
/* Tokens                                                               */
/* ------------------------------------------------------------------ */

function base64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

/** 32 random bytes, base64url (43 characters). */
export function randomToken(): string {
  return base64Url(crypto.getRandomValues(new Uint8Array(32)));
}

export async function sha256Hex(value: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(value),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

function constantTimeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let index = 0; index < a.length; index += 1) {
    diff |= a.charCodeAt(index) ^ b.charCodeAt(index);
  }
  return diff === 0;
}

/** A token as the routes accept it: base64url, 43 characters. */
export function isLinkToken(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_-]{43}$/.test(value);
}

/* ------------------------------------------------------------------ */
/* Link states                                                          */
/* ------------------------------------------------------------------ */

export type LinkStart = {
  /** Goes in return_to. */
  nonce: string;
  /** Goes in the HttpOnly cookie. */
  binding: string;
  /** The exact return_to the provider must echo. */
  returnTo: string;
};

/**
 * Starts a link: one pending state per user and provider (a new start
 * replaces the last), expired states swept on the way.
 */
export async function createLinkState(
  db: D1Database,
  input: {
    userId: string;
    provider: ConnectionProvider;
    /** The callback URL without the state. */
    callbackUrl: string;
    now?: number;
  },
): Promise<LinkStart> {
  const now = input.now ?? Date.now();
  const nonce = randomToken();
  const binding = randomToken();
  const callback = new URL(input.callbackUrl);
  callback.searchParams.set("state", nonce);
  const returnTo = callback.toString();
  const [stateHash, bindingHash] = await Promise.all([
    sha256Hex(nonce),
    sha256Hex(binding),
  ]);
  await db.batch([
    db
      .prepare(`DELETE FROM connection_link_states WHERE expires_at < ?`)
      .bind(now),
    db
      .prepare(
        `DELETE FROM connection_link_states WHERE user_id = ? AND provider = ?`,
      )
      .bind(input.userId, input.provider),
    db
      .prepare(
        `INSERT INTO connection_link_states (state_hash, user_id, provider, binding_hash, return_to, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      )
      .bind(
        stateHash,
        input.userId,
        input.provider,
        bindingHash,
        returnTo,
        now + LINK_STATE_TTL_MS,
        new Date(now).toISOString(),
      ),
  ]);
  return { nonce, binding, returnTo };
}

export type CheckedLinkBinding =
  | { ok: true; returnTo: string }
  | { ok: false; reason: "expired" | "invalid" };

/**
 * The begin route's half of a native link: the state is still pending, for
 * this provider, and the binding is its own. Nothing changes, so a browser
 * that loads the begin URL twice (a prefetch, a reload) still gets through;
 * the URL never links anything by itself, since only the account that
 * started the link can claim what its callback verifies (takeLinkResult).
 */
export async function checkLinkBinding(
  db: D1Database,
  input: {
    provider: ConnectionProvider;
    nonce: unknown;
    binding: unknown;
    now?: number;
  },
): Promise<CheckedLinkBinding> {
  if (!isLinkToken(input.nonce) || !isLinkToken(input.binding)) {
    return { ok: false, reason: "invalid" };
  }
  const now = input.now ?? Date.now();
  const [stateHash, bindingHash] = await Promise.all([
    sha256Hex(input.nonce),
    sha256Hex(input.binding),
  ]);
  const row = await db
    .prepare(
      `SELECT return_to, binding_hash FROM connection_link_states
       WHERE state_hash = ? AND provider = ? AND expires_at >= ?`,
    )
    .bind(stateHash, input.provider, now)
    .first<{ return_to: string; binding_hash: string }>();
  if (!row || !constantTimeEqual(bindingHash, row.binding_hash)) {
    return { ok: false, reason: "expired" };
  }
  return { ok: true, returnTo: row.return_to };
}

/**
 * Keeps a verified account for the app that started the link, behind a
 * one-time code (returned; only its hash is stored).
 */
export async function holdLinkResult(
  db: D1Database,
  input: {
    userId: string;
    provider: ConnectionProvider;
    account: VerifiedAccount;
    now?: number;
  },
): Promise<string> {
  const now = input.now ?? Date.now();
  const code = randomToken();
  await db.batch([
    db
      .prepare(`DELETE FROM connection_link_results WHERE expires_at < ?`)
      .bind(now),
    db
      .prepare(
        `INSERT INTO connection_link_results (code_hash, user_id, provider, account, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?)`,
      )
      .bind(
        await sha256Hex(code),
        input.userId,
        input.provider,
        JSON.stringify(input.account),
        now + LINK_STATE_TTL_MS,
        new Date(now).toISOString(),
      ),
  ]);
  return code;
}

export type TakenLinkResult =
  | { ok: true; account: VerifiedAccount }
  | { ok: false; reason: "expired" | "invalid" | "other_account" };

/**
 * Claims a held result, once: unknown, used, expired, another provider's,
 * or started by another account all fail, and the code is gone either way.
 */
export async function takeLinkResult(
  db: D1Database,
  input: {
    userId: string;
    provider: ConnectionProvider;
    code: unknown;
    now?: number;
  },
): Promise<TakenLinkResult> {
  if (!isLinkToken(input.code)) return { ok: false, reason: "invalid" };
  const now = input.now ?? Date.now();
  const row = await db
    .prepare(
      `DELETE FROM connection_link_results WHERE code_hash = ?
       RETURNING user_id, provider, account, expires_at`,
    )
    .bind(await sha256Hex(input.code))
    .first<{
      user_id: string;
      provider: string;
      account: string;
      expires_at: number;
    }>();
  if (!row || row.expires_at < now) return { ok: false, reason: "expired" };
  if (row.provider !== input.provider) return { ok: false, reason: "invalid" };
  if (row.user_id !== input.userId) {
    return { ok: false, reason: "other_account" };
  }
  try {
    return { ok: true, account: JSON.parse(row.account) as VerifiedAccount };
  } catch {
    return { ok: false, reason: "invalid" };
  }
}

export type ConsumedLinkState =
  | { ok: true; userId: string; returnTo: string }
  | { ok: false; reason: "expired" | "invalid" };

/**
 * Takes the state for a callback, once. Unknown, used, expired or from
 * another browser (no or wrong binding cookie) all fail; the state is
 * gone either way, so a replay finds nothing.
 */
export async function consumeLinkState(
  db: D1Database,
  input: {
    provider: ConnectionProvider;
    nonce: unknown;
    binding: unknown;
    now?: number;
  },
): Promise<ConsumedLinkState> {
  if (!isLinkToken(input.nonce)) return { ok: false, reason: "invalid" };
  const now = input.now ?? Date.now();
  const row = await db
    .prepare(
      `DELETE FROM connection_link_states WHERE state_hash = ?
       RETURNING user_id, provider, binding_hash, return_to, expires_at`,
    )
    .bind(await sha256Hex(input.nonce))
    .first<{
      user_id: string;
      provider: string;
      binding_hash: string;
      return_to: string;
      expires_at: number;
    }>();
  if (!row) return { ok: false, reason: "expired" };
  if (row.provider !== input.provider) return { ok: false, reason: "invalid" };
  if (row.expires_at < now) return { ok: false, reason: "expired" };
  if (
    !isLinkToken(input.binding) ||
    !constantTimeEqual(await sha256Hex(input.binding), row.binding_hash)
  ) {
    return { ok: false, reason: "expired" };
  }
  return { ok: true, userId: row.user_id, returnTo: row.return_to };
}

/* ------------------------------------------------------------------ */
/* Connections                                                          */
/* ------------------------------------------------------------------ */

type ConnectionRow = {
  provider: string;
  external_id: string;
  display_name: string;
  profile_url: string | null;
  avatar_url: string | null;
  show_on_profile: number;
  metadata: string;
  verified_at: string;
};

function toPublic(row: ConnectionRow): PublicConnection | null {
  if (!isConnectionProvider(row.provider)) return null;
  return {
    provider: row.provider,
    display_name: row.display_name,
    profile_url: row.profile_url,
    avatar_url: row.avatar_url,
  };
}

function parseMetadata(raw: string): Record<string, unknown> {
  try {
    const value = JSON.parse(raw) as unknown;
    return value && typeof value === "object" && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

function isUniqueViolation(error: unknown): boolean {
  return /UNIQUE constraint failed/i.test(String((error as Error)?.message));
}

export type LinkResult = { ok: true } | { ok: false; reason: "taken" };

/**
 * Stores a verified account for the user. An account linked to someone
 * else is refused ("taken"); linking a different account for the same
 * provider replaces the old one and keeps the "Show on profile" choice.
 */
export async function linkConnection(
  db: D1Database,
  userId: string,
  provider: ConnectionProvider,
  account: VerifiedAccount,
  now = new Date().toISOString(),
): Promise<LinkResult> {
  const owner = await db
    .prepare(
      `SELECT user_id FROM user_connections WHERE provider = ? AND external_id = ?`,
    )
    .bind(provider, account.externalId)
    .first<{ user_id: string }>();
  if (owner && owner.user_id !== userId) return { ok: false, reason: "taken" };
  if (provider === "steam") {
    // Another Steam account replaces this one: its Deadlock stats go now,
    // not at the next reconcile.
    const current = await db
      .prepare(
        `SELECT external_id FROM user_connections WHERE user_id = ? AND provider = 'steam'`,
      )
      .bind(userId)
      .first<{ external_id: string }>();
    if (current && current.external_id !== account.externalId) {
      await db.batch([
        db
          .prepare(`DELETE FROM deadlock_fetch_state WHERE user_id = ?`)
          .bind(userId),
        db
          .prepare(`DELETE FROM deadlock_snapshots WHERE user_id = ?`)
          .bind(userId),
      ]);
    }
  }
  try {
    await db
      .prepare(
        `INSERT INTO user_connections (user_id, provider, external_id, display_name, profile_url, avatar_url, show_on_profile, metadata, verified_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?)
         ON CONFLICT(user_id, provider) DO UPDATE SET
           external_id = excluded.external_id,
           display_name = excluded.display_name,
           profile_url = excluded.profile_url,
           avatar_url = excluded.avatar_url,
           metadata = excluded.metadata,
           verified_at = excluded.verified_at,
           updated_at = excluded.updated_at`,
      )
      .bind(
        userId,
        provider,
        account.externalId,
        account.displayName,
        account.profileUrl,
        account.avatarUrl,
        JSON.stringify(account.metadata),
        now,
        now,
        now,
      )
      .run();
  } catch (error) {
    // Someone else linked the same account between the check and the write.
    if (isUniqueViolation(error)) return { ok: false, reason: "taken" };
    throw error;
  }
  return { ok: true };
}

const SELECT_CONNECTIONS = `SELECT provider, external_id, display_name, profile_url, avatar_url, show_on_profile, metadata, verified_at
  FROM user_connections WHERE user_id = ? ORDER BY created_at, provider`;

/** The owner's own list, hidden ones included. No external ids. */
export async function listOwnConnections(
  db: D1Database,
  userId: string,
): Promise<OwnConnection[]> {
  const { results } = await db
    .prepare(SELECT_CONNECTIONS)
    .bind(userId)
    .all<ConnectionRow>();
  return (results ?? []).flatMap((row) => {
    const shown = toPublic(row);
    return shown
      ? [
          {
            ...shown,
            show_on_profile: row.show_on_profile === 1,
            verified_at: row.verified_at,
          },
        ]
      : [];
  });
}

/**
 * What a profile shows: connections with "Show on profile" on, as
 * provider, name, profile URL and avatar. Never throws: a profile without
 * its connections beats no profile (and the table may be missing on a
 * database the migration has not reached).
 */
export async function getShownConnections(
  db: D1Database,
  userId: string,
): Promise<PublicConnection[]> {
  try {
    const { results } = await db
      .prepare(
        `SELECT provider, external_id, display_name, profile_url, avatar_url, show_on_profile, metadata, verified_at
         FROM user_connections WHERE user_id = ? AND show_on_profile = 1 ORDER BY created_at, provider`,
      )
      .bind(userId)
      .all<ConnectionRow>();
    return (results ?? []).flatMap((row) => {
      const shown = toPublic(row);
      return shown ? [shown] : [];
    });
  } catch (error) {
    log.warn("Couldn't read profile connections", { error });
    return [];
  }
}

/** Turns "Show on profile" on or off. False when nothing is linked. */
export async function setShowOnProfile(
  db: D1Database,
  userId: string,
  provider: ConnectionProvider,
  show: boolean,
  now = new Date().toISOString(),
): Promise<boolean> {
  const result = await db
    .prepare(
      `UPDATE user_connections SET show_on_profile = ?, updated_at = ? WHERE user_id = ? AND provider = ?`,
    )
    .bind(show ? 1 : 0, now, userId, provider)
    .run();
  return (result.meta?.changes ?? 0) > 0;
}

/**
 * Deletes the link and any link in flight. Unlinking Steam also deletes
 * the Deadlock stats kept for it (migration 044), at once. False when
 * nothing was linked.
 */
export async function unlinkConnection(
  db: D1Database,
  userId: string,
  provider: ConnectionProvider,
): Promise<boolean> {
  const [deleted] = await db.batch([
    db
      .prepare(
        `DELETE FROM user_connections WHERE user_id = ? AND provider = ?`,
      )
      .bind(userId, provider),
    db
      .prepare(
        `DELETE FROM connection_link_states WHERE user_id = ? AND provider = ?`,
      )
      .bind(userId, provider),
    ...(provider === "steam"
      ? [
          db
            .prepare(`DELETE FROM deadlock_fetch_state WHERE user_id = ?`)
            .bind(userId),
          db
            .prepare(`DELETE FROM deadlock_snapshots WHERE user_id = ?`)
            .bind(userId),
        ]
      : []),
  ]);
  return (deleted?.meta?.changes ?? 0) > 0;
}

export type VerifiedConnection = {
  provider: ConnectionProvider;
  externalId: string;
  displayName: string;
  profileUrl: string | null;
  avatarUrl: string | null;
  showOnProfile: boolean;
  verifiedAt: string;
  /** Provider data; Steam: `{ steam_id64, account_id }`. */
  metadata: Record<string, unknown>;
};

/**
 * The user's verified account on a provider, for server features built on
 * it (the Deadlock widget reads `metadata.account_id`). Server-side only:
 * it carries the external id. Whether to show anything is the caller's
 * call; `showOnProfile` is the owner's profile choice.
 */
export async function getVerifiedConnection(
  db: D1Database,
  userId: string,
  provider: ConnectionProvider,
): Promise<VerifiedConnection | null> {
  const row = await db
    .prepare(
      `SELECT provider, external_id, display_name, profile_url, avatar_url, show_on_profile, metadata, verified_at
       FROM user_connections WHERE user_id = ? AND provider = ?`,
    )
    .bind(userId, provider)
    .first<ConnectionRow>();
  if (!row || !isConnectionProvider(row.provider)) return null;
  return {
    provider: row.provider,
    externalId: row.external_id,
    displayName: row.display_name,
    profileUrl: row.profile_url,
    avatarUrl: row.avatar_url,
    showOnProfile: row.show_on_profile === 1,
    verifiedAt: row.verified_at,
    metadata: parseMetadata(row.metadata),
  };
}

/** True when `userId` has blocked `viewerId` (relationships type 1). */
export async function hasBlocked(
  db: D1Database,
  userId: string,
  viewerId: string,
): Promise<boolean> {
  if (userId === viewerId) return false;
  const row = await db
    .prepare(
      `SELECT 1 AS blocked FROM relationships WHERE user_id = ? AND target_user_id = ? AND type = 1`,
    )
    .bind(userId, viewerId)
    .first<{ blocked: number }>();
  return Boolean(row);
}
