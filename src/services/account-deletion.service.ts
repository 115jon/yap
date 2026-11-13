// ── Account deletion ────────────────────────────────────────────────────────
// Self-service deletion with a 30-day grace period (migration 046,
// lib/account-deletion.ts for the gate).
//
// Request: the account must own no server (as on Discord; Yap has no
// ownership transfer yet, so it deletes the server first) and confirm with
// its username. In one batch the account
//   - gets an account_deletions row with `delete_after` 30 days out and a
//     snapshot of what its profile showed and which servers it was in;
//   - shows "Deleted user" everywhere (its users row is blanked; ids stay, so
//     member lists, DMs and mentions follow without other changes);
//   - leaves every server (memberships and roles kept in the snapshot);
//   - loses its push tokens.
// The route then signs out its other sessions and closes its sockets; from
// then on every request but status and restore gets 403
// ACCOUNT_PENDING_DELETION.
//
// Restore (any time before the purge claims the row) puts the profile back,
// rejoins the servers that still exist and have not banned it, and deletes
// the row, in one transaction that does nothing once the purge has claimed it.
//
// Purge (hourly cron, rows past `delete_after`): what other people keep is
// moved to a new "Deleted user" row, as Discord does: messages (server and
// DM; content stays, the author is anonymised), the DM memberships that keep
// those conversations listed, attachments on kept messages, server soundboard
// sounds and voice status media, invites, server audit entries and bans
// made, notifications others got. Then the account's users row is deleted,
// and with it, by ON DELETE CASCADE, everything else that is the account's
// own: profile, board and widgets, Activity, guestbook, settings, its friends
// and the blocks it made, reactions, read states, notifications, push tokens,
// badges, plan, stats, connections, favourites, avatars and emojis. Its files
// in R2 (avatars, banner and nameplate, board images, generated emojis,
// camera backgrounds, uploads never sent) are deleted, and any that fail go
// to the R2 cleanup retry queue (they are queued there before the rows go).
// The Kova identity is not Yap's and is left alone, so the same Kova account
// can sign up to Yap again, as a new account with the same id. Bans and
// blocks against the account are kept under that id (server_bans,
// relationships type 1 and profile_guestbook_blocks have no foreign key on
// that side since migration 046), so they still hold for it. Mentions in
// kept messages are not rewritten: they are `@username` text.
// An account suspended after asking, or owning a server again, is held, not
// purged (its row stays for the site admin); the sweep tries it last.

import { DELETED_USER_DISPLAY_NAME } from "@/lib/account-deletion";
import { CacheKey } from "@/lib/cache";
import { PROFILE_ASSET_PREFIX } from "@/lib/profile-assets";
import { widgetAssetPrefix } from "@/lib/profile-board";
import { ServiceError } from "@/lib/service-error";
import type { BroadcastDescriptor } from "@/services/server.service";

export { deletionDateFrom } from "@/lib/account-deletion";

type Db = D1Database;

/** The profile fields the grace period blanks, as they were. */
const PROFILE_FIELDS = [
  "username",
  "display_name",
  "avatar_url",
  "avatar_display",
  "banner_url",
  "banner_content_type",
  "nameplate_url",
  "nameplate_content_type",
  "profile_banner_color",
  "display_name_style",
  "bio",
  "pronouns",
  "status",
  "custom_status",
] as const;

type ProfileField = (typeof PROFILE_FIELDS)[number];
type ProfileSnapshot = Record<ProfileField, unknown>;

interface MembershipSnapshot {
  server_id: string;
  joined_at: string;
  role_ids: string[];
}

export interface DeletionSnapshot {
  profile: ProfileSnapshot;
  servers: MembershipSnapshot[];
}

export type DeletionStatus =
  | { status: "active" }
  | {
      status: "pending";
      requested_at: string;
      delete_after: string;
      /** True once the purge has begun: too late to restore. */
      purging: boolean;
    };

interface DeletionRow {
  user_id: string;
  requested_at: string;
  delete_after: string;
  reserved_username: string;
  snapshot: string;
  tombstone_id: string | null;
  purge_started_at: string | null;
  attempts: number;
}

/** What a route does after a service call: cache keys and broadcasts. */
export interface DeletionEffects {
  cacheKeysToInvalidate: string[];
  broadcasts: BroadcastDescriptor[];
}

/** `deleted_user_` and 8 hex characters: unique, and never a real name. */
export function deletedUsername(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(4));
  return `deleted_user_${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}`;
}

/** A typed confirmation matches when it is the username, `@` and case aside. */
export function confirmationMatches(typed: string, username: string): boolean {
  const normalize = (value: string) =>
    value.trim().replace(/^@/, "").toLowerCase();
  return normalize(typed) !== "" && normalize(typed) === normalize(username);
}

async function deletionRow(db: Db, userId: string) {
  return db
    .prepare(`SELECT * FROM account_deletions WHERE user_id = ?`)
    .bind(userId)
    .first<DeletionRow>();
}

function statusOf(row: DeletionRow | null): DeletionStatus {
  return row
    ? {
        status: "pending",
        requested_at: row.requested_at,
        delete_after: row.delete_after,
        purging: row.tombstone_id !== null,
      }
    : { status: "active" };
}

export async function getDeletionStatus(
  db: Db,
  userId: string,
): Promise<DeletionStatus> {
  return statusOf(await deletionRow(db, userId));
}

/** The servers the account owns: deletion waits until there are none. */
export async function ownedServers(
  db: Db,
  userId: string,
): Promise<Array<{ id: string; name: string }>> {
  const { results } = await db
    .prepare(`SELECT id, name FROM servers WHERE owner_id = ? ORDER BY name`)
    .bind(userId)
    .all<{ id: string; name: string }>();
  return results ?? [];
}

export class OwnsServersError extends ServiceError {
  constructor(readonly servers: Array<{ id: string; name: string }>) {
    super(
      servers.length === 1
        ? `You own ${servers[0]!.name}. Delete it in server settings first.`
        : `You own ${servers.length} servers. Delete them in server settings first.`,
      409,
      "OWNS_SERVERS",
    );
  }
}

/**
 * Schedule the account for deletion. Idempotent: asking again while it is
 * pending answers the same date and changes nothing.
 */
export async function requestAccountDeletion(
  db: Db,
  userId: string,
  input: { confirmUsername: string; now: number; deleteAfter: string },
): Promise<DeletionEffects & { status: DeletionStatus; serverIds: string[] }> {
  const existing = await deletionRow(db, userId);
  if (existing) {
    return {
      status: statusOf(existing),
      serverIds: [],
      cacheKeysToInvalidate: [],
      broadcasts: [],
    };
  }

  const user = await db
    .prepare(`SELECT ${PROFILE_FIELDS.join(", ")} FROM users WHERE id = ?`)
    .bind(userId)
    .first<ProfileSnapshot>();
  if (!user) throw ServiceError.notFound("Account not found");
  const username = String(user.username ?? "");

  if (!confirmationMatches(input.confirmUsername, username)) {
    throw new ServiceError(
      "Type your username exactly to confirm.",
      400,
      "CONFIRMATION_MISMATCH",
    );
  }

  const owned = await ownedServers(db, userId);
  if (owned.length > 0) throw new OwnsServersError(owned);

  const [{ results: memberships }, { results: roles }] = await Promise.all([
    db
      .prepare(
        `SELECT server_id, joined_at FROM server_members WHERE user_id = ?`,
      )
      .bind(userId)
      .all<{ server_id: string; joined_at: string }>(),
    db
      .prepare(`SELECT server_id, role_id FROM member_roles WHERE user_id = ?`)
      .bind(userId)
      .all<{ server_id: string; role_id: string }>(),
  ]);
  const servers: MembershipSnapshot[] = (memberships ?? []).map((m) => ({
    server_id: m.server_id,
    joined_at: m.joined_at,
    role_ids: (roles ?? [])
      .filter((r) => r.server_id === m.server_id)
      .map((r) => r.role_id),
  }));
  const snapshot: DeletionSnapshot = { profile: user, servers };
  const requestedAt = new Date(input.now).toISOString();
  const placeholder = deletedUsername();

  // The row goes in only while the account still owns no server, so a server
  // made after the check above cannot slip through; the other statements run
  // only if it went in. One batch is one transaction.
  const requested = `EXISTS (SELECT 1 FROM account_deletions WHERE user_id = ? AND requested_at = ?)`;
  const [inserted] = await db.batch([
    db
      .prepare(
        `INSERT INTO account_deletions
           (user_id, requested_at, delete_after, reserved_username, snapshot)
         SELECT ?, ?, ?, ?, ?
         WHERE NOT EXISTS (SELECT 1 FROM servers WHERE owner_id = ?)`,
      )
      .bind(
        userId,
        requestedAt,
        input.deleteAfter,
        username.toLowerCase(),
        JSON.stringify(snapshot),
        userId,
      ),
    blankProfileStatement(db, userId, placeholder, requestedAt, requested),
    db
      .prepare(`DELETE FROM member_roles WHERE user_id = ? AND ${requested}`)
      .bind(userId, userId, requestedAt),
    db
      .prepare(`DELETE FROM server_members WHERE user_id = ? AND ${requested}`)
      .bind(userId, userId, requestedAt),
    db
      .prepare(`DELETE FROM push_tokens WHERE user_id = ? AND ${requested}`)
      .bind(userId, userId, requestedAt),
  ]);
  if (Number(inserted?.meta?.changes ?? 0) === 0) {
    throw new OwnsServersError(await ownedServers(db, userId));
  }

  const serverIds = servers.map((s) => s.server_id);
  const hidden = hiddenProfile(userId, placeholder);
  return {
    status: {
      status: "pending",
      requested_at: requestedAt,
      delete_after: input.deleteAfter,
      purging: false,
    },
    serverIds,
    cacheKeysToInvalidate: [
      CacheKey.userProfile(userId),
      CacheKey.userServers(userId),
      ...serverIds.map((id) => CacheKey.serverMembers(id)),
    ],
    broadcasts: serverIds.flatMap((serverId): BroadcastDescriptor[] => [
      // Open member lists drop the account, and its old messages there
      // re-render as "Deleted user".
      {
        type: "server",
        target: serverId,
        event: "USER_PROFILE_UPDATE",
        data: hidden,
      },
      {
        type: "server",
        target: serverId,
        event: "GUILD_MEMBER_REMOVE",
        data: { server_id: serverId, user_id: userId },
      },
    ]),
  };
}

/** What a hidden account looks like in a USER_PROFILE_UPDATE. */
function hiddenProfile(userId: string, username: string) {
  return {
    user_id: userId,
    username,
    display_name: DELETED_USER_DISPLAY_NAME,
    avatar_url: null,
    avatar_display: null,
    updated_at: new Date().toISOString(),
  };
}

/** `requested`: the condition that the request row went in (see above). */
function blankProfileStatement(
  db: Db,
  userId: string,
  username: string,
  at: string,
  requested: string,
) {
  return db
    .prepare(
      `UPDATE users SET
         username = ?, display_name = ?, avatar_url = NULL,
         avatar_display = NULL, banner_url = NULL, banner_content_type = NULL,
         nameplate_url = NULL, nameplate_content_type = NULL,
         profile_banner_color = NULL, display_name_style = NULL, bio = NULL,
         pronouns = NULL, status = 'offline', custom_status = NULL,
         updated_at = ?
       WHERE id = ? AND ${requested}`,
    )
    .bind(username, DELETED_USER_DISPLAY_NAME, at, userId, userId, at);
}

function parseSnapshot(raw: string): DeletionSnapshot | null {
  try {
    const parsed = JSON.parse(raw) as Partial<DeletionSnapshot>;
    if (!parsed || typeof parsed !== "object" || !parsed.profile) return null;
    return {
      profile: parsed.profile,
      servers: Array.isArray(parsed.servers) ? parsed.servers : [],
    };
  } catch {
    return null;
  }
}

/**
 * Cancel a pending deletion. Answers `active` when there was none; refuses
 * once the purge has started.
 */
export async function restoreAccount(
  db: Db,
  userId: string,
  now: number,
): Promise<DeletionEffects & { status: DeletionStatus; serverIds: string[] }> {
  const row = await deletionRow(db, userId);
  if (!row) {
    return {
      status: { status: "active" },
      serverIds: [],
      cacheKeysToInvalidate: [],
      broadcasts: [],
    };
  }
  if (row.tombstone_id) {
    throw new ServiceError(
      "This account is already being deleted and can't be restored.",
      409,
      "DELETION_IN_PROGRESS",
    );
  }
  const snapshot = parseSnapshot(row.snapshot);
  if (!snapshot) {
    throw new ServiceError(
      "This account's saved profile can't be read. Contact the Yap team.",
      500,
      "DELETION_SNAPSHOT_UNREADABLE",
    );
  }

  // The name was reserved, but a Kova sign-up does not ask Yap first.
  const original = String(snapshot.profile.username ?? "");
  const taken = await db
    .prepare(
      `SELECT 1 FROM users WHERE lower(username) = ? AND id != ? LIMIT 1`,
    )
    .bind(original.toLowerCase(), userId)
    .first();
  const username = taken
    ? `${original.slice(0, 27)}_${crypto.getRandomValues(new Uint16Array(1))[0]! % 10000}`
    : original;
  const profile: ProfileSnapshot = { ...snapshot.profile, username };
  const at = new Date(now).toISOString();

  // Every statement runs only while the row is still unclaimed by the purge;
  // the batch is one transaction, so the purge's claim comes before or after
  // all of it, never in between.
  const unclaimed = `EXISTS (SELECT 1 FROM account_deletions WHERE user_id = ? AND tombstone_id IS NULL)`;
  const statements: D1PreparedStatement[] = [
    db
      .prepare(
        `UPDATE users SET ${PROFILE_FIELDS.map((f) => `${f} = ?`).join(", ")},
           updated_at = ?
         WHERE id = ? AND ${unclaimed}`,
      )
      .bind(
        ...PROFILE_FIELDS.map((f) => profile[f] ?? null),
        at,
        userId,
        userId,
      ),
  ];
  for (const membership of snapshot.servers) {
    // Back in the servers that still exist and have not banned the account.
    statements.push(
      db
        .prepare(
          `INSERT OR IGNORE INTO server_members (server_id, user_id, joined_at)
           SELECT id, ?, ? FROM servers
           WHERE id = ?
             AND NOT EXISTS (
               SELECT 1 FROM server_bans WHERE server_id = ? AND user_id = ?
             )
             AND ${unclaimed}`,
        )
        .bind(
          userId,
          membership.joined_at,
          membership.server_id,
          membership.server_id,
          userId,
          userId,
        ),
    );
    for (const roleId of membership.role_ids) {
      statements.push(
        db
          .prepare(
            `INSERT OR IGNORE INTO member_roles (server_id, user_id, role_id)
             SELECT r.server_id, ?, r.id FROM roles r
             WHERE r.id = ? AND r.server_id = ?
               AND EXISTS (
                 SELECT 1 FROM server_members
                 WHERE server_id = r.server_id AND user_id = ?
               )
               AND ${unclaimed}`,
          )
          .bind(userId, roleId, membership.server_id, userId, userId),
      );
    }
  }
  statements.push(
    db
      .prepare(
        `DELETE FROM account_deletions WHERE user_id = ? AND tombstone_id IS NULL`,
      )
      .bind(userId),
  );
  const results = await db.batch(statements);
  if (Number(results.at(-1)?.meta?.changes ?? 0) === 0) {
    // The purge claimed it after the read above: nothing was changed.
    throw new ServiceError(
      "This account is already being deleted and can't be restored.",
      409,
      "DELETION_IN_PROGRESS",
    );
  }

  const { results: rejoined } = await db
    .prepare(`SELECT server_id FROM server_members WHERE user_id = ?`)
    .bind(userId)
    .all<{ server_id: string }>();
  const serverIds = (rejoined ?? []).map((r) => r.server_id);
  const shown = {
    user_id: userId,
    username,
    display_name: profile.display_name ?? null,
    avatar_url: profile.avatar_url ?? null,
    avatar_display: profile.avatar_display ?? null,
    updated_at: at,
  };
  return {
    status: { status: "active" },
    serverIds,
    cacheKeysToInvalidate: [
      CacheKey.userProfile(userId),
      CacheKey.userServers(userId),
      ...serverIds.map((id) => CacheKey.serverMembers(id)),
    ],
    broadcasts: [
      ...(await Promise.all(
        serverIds.map((serverId) => memberAdded(db, serverId, userId, shown)),
      )),
      ...serverIds.map((serverId): BroadcastDescriptor => ({
        type: "server",
        target: serverId,
        event: "USER_PROFILE_UPDATE",
        data: shown,
      })),
    ],
  };
}

/**
 * GUILD_MEMBER_ADD for a restored member, as a join announces it
 * (social.service), so open member lists show it again with its roles.
 */
async function memberAdded(
  db: Db,
  serverId: string,
  userId: string,
  shown: {
    username: string;
    display_name: unknown;
    avatar_url: unknown;
  },
): Promise<BroadcastDescriptor> {
  const { results: roles } = await db
    .prepare(
      `SELECT r.id, r.server_id, r.name, r.color, r.permissions, r.position,
              r.is_default, r.created_at
       FROM roles r
       JOIN member_roles mr ON mr.role_id = r.id
       WHERE mr.server_id = ? AND mr.user_id = ?`,
    )
    .bind(serverId, userId)
    .all<{ is_default: number } & Record<string, unknown>>();
  return {
    type: "server",
    target: serverId,
    event: "GUILD_MEMBER_ADD",
    data: {
      server_id: serverId,
      user: {
        id: userId,
        username: shown.username,
        display_name: shown.display_name,
        avatar_url: shown.avatar_url,
        status: "offline",
      },
      roles: (roles ?? []).map((role) => ({
        ...role,
        is_default: role.is_default === 1,
      })),
    },
  };
}

// ── Purge ────────────────────────────────────────────────────────────────────

/** Rows moved per UPDATE, so one account's history never makes one huge write. */
const REPOINT_CHUNK = 2_000;
/** Accounts purged per cron run. */
const PURGE_BATCH = 5;
/** R2 accepts up to 1000 keys per delete. */
const R2_DELETE_CHUNK = 1_000;

/**
 * Tables whose rows other people keep, re-pointed to the "Deleted user" row:
 * [table, column, extra condition]. Everything not listed here that names the
 * account goes with its users row (ON DELETE CASCADE) or is deleted below.
 */
const KEPT: ReadonlyArray<readonly [string, string, string?]> = [
  ["messages", "author_id"],
  // Files on kept messages and server soundboard sounds stay with them; an
  // upload never attached to anything is deleted.
  [
    "attachments",
    "user_id",
    "(message_id IS NOT NULL OR soundboard_server_id IS NOT NULL)",
  ],
  ["voice_status_media_assets", "user_id"],
  ["invites", "inviter_id"],
  ["server_audit_logs", "actor_id"],
  ["server_bans", "banned_by"],
  ["notifications", "from_user_id"],
  // The other person's DM keeps listing the conversation.
  ["dm_recipients", "user_id"],
];

async function repoint(
  db: Db,
  table: string,
  column: string,
  from: string,
  to: string,
  extra?: string,
): Promise<number> {
  const where = `${column} = ?${extra ? ` AND ${extra}` : ""}`;
  let moved = 0;
  for (;;) {
    const result = await db
      .prepare(
        `UPDATE ${table} SET ${column} = ?
         WHERE rowid IN (SELECT rowid FROM ${table} WHERE ${where} LIMIT ${REPOINT_CHUNK})`,
      )
      .bind(to, from)
      .run();
    const changes = Number(result.meta?.changes ?? 0);
    moved += changes;
    if (changes < REPOINT_CHUNK) return moved;
  }
}

/**
 * The R2 key behind one of the account's own profile URLs (avatar, banner,
 * nameplate) as the snapshot kept it, or null for a preset, an outside URL
 * or someone else's file.
 */
export function ownProfileFileKey(url: unknown, userId: string): string | null {
  if (typeof url !== "string" || !url.startsWith("/api/")) return null;
  const key = url.slice("/api/".length).split(/[?#]/, 1)[0] ?? "";
  const own = [
    `avatars/${userId}/`,
    `avatars/${userId}.`,
    `${PROFILE_ASSET_PREFIX}/banner/${userId}.`,
    `${PROFILE_ASSET_PREFIX}/nameplate/${userId}.`,
  ];
  return own.some((prefix) => key.startsWith(prefix)) && !key.includes("..")
    ? key
    : null;
}

/** The account's R2 objects that nothing kept refers to. */
async function filesToDelete(
  db: Db,
  bucket: Pick<R2Bucket, "list">,
  userId: string,
  snapshot: DeletionSnapshot | null,
): Promise<string[]> {
  const keys = new Set<string>();
  // The profile as it was before the grace period blanked it.
  for (const url of [
    snapshot?.profile.avatar_url,
    snapshot?.profile.banner_url,
    snapshot?.profile.nameplate_url,
  ]) {
    const key = ownProfileFileKey(url, userId);
    if (key) keys.add(key);
  }
  const rows = await db.batch<{ file_key: string | null }>([
    db
      .prepare(`SELECT file_key FROM user_avatar_uploads WHERE user_id = ?`)
      .bind(userId),
    db
      .prepare(
        `SELECT file_key FROM generated_emojis WHERE user_id = ? AND file_key IS NOT NULL`,
      )
      .bind(userId),
    db
      .prepare(
        `SELECT file_key FROM attachments
         WHERE user_id = ? AND message_id IS NULL AND soundboard_server_id IS NULL`,
      )
      .bind(userId),
  ]);
  for (const result of rows) {
    for (const row of result.results ?? []) {
      if (row.file_key) keys.add(row.file_key);
    }
  }
  // Keyed by user id: avatars, banner and nameplate, board images, generated
  // emojis, camera backgrounds. Avatars uploaded from 2026-03-01 to
  // 2026-07-17 sit at `avatars/<id>.<ext>`, outside the per-user folder.
  const prefixes = [
    `avatars/${userId}/`,
    `avatars/${userId}.`,
    `${PROFILE_ASSET_PREFIX}/banner/${userId}.`,
    `${PROFILE_ASSET_PREFIX}/nameplate/${userId}.`,
    `emoji-assets/${userId}/`,
    `camera-backgrounds/${userId}/`,
    widgetAssetPrefix(userId),
  ];
  for (const prefix of prefixes) {
    let cursor: string | undefined;
    do {
      const page = await bucket.list({ prefix, cursor });
      for (const object of page.objects) keys.add(object.key);
      cursor = page.truncated ? page.cursor : undefined;
    } while (cursor);
  }
  return [...keys];
}

async function abortPendingUploads(
  db: Db,
  bucket: Pick<R2Bucket, "resumeMultipartUpload">,
  userId: string,
): Promise<void> {
  const { results } = await db
    .prepare(
      `SELECT file_key, r2_upload_id FROM pending_uploads WHERE user_id = ?`,
    )
    .bind(userId)
    .all<{ file_key: string; r2_upload_id: string }>();
  for (const row of results ?? []) {
    try {
      await bucket
        .resumeMultipartUpload(row.file_key, row.r2_upload_id)
        .abort();
    } catch {
      // Already completed or aborted; R2 expires stray multiparts anyway.
    }
  }
}

/** Rows per multi-row statement: D1 caps bound parameters per statement. */
const KEY_ROWS_CHUNK = 20;

/**
 * Queue the keys in r2_cleanup_jobs before their rows go, so no file
 * outlives a crash or a failed delete: the R2 cleanup sweep retries them.
 * Due an hour from now, so the purge's own delete comes first.
 */
async function queueFiles(db: Db, keys: string[], now: number) {
  const at = new Date(now).toISOString();
  const due = new Date(now + 60 * 60 * 1000).toISOString();
  for (let i = 0; i < keys.length; i += KEY_ROWS_CHUNK) {
    const chunk = keys.slice(i, i + KEY_ROWS_CHUNK);
    await db
      .prepare(
        `INSERT OR IGNORE INTO r2_cleanup_jobs
           (file_key, attempts, created_at, updated_at, next_attempt_at)
         VALUES ${chunk.map(() => "(?, 0, ?, ?, ?)").join(", ")}`,
      )
      .bind(...chunk.flatMap((key) => [key, at, at, due]))
      .run();
  }
}

async function deleteFiles(
  db: Db,
  bucket: Pick<R2Bucket, "delete">,
  keys: string[],
): Promise<number> {
  let failed = 0;
  for (let i = 0; i < keys.length; i += R2_DELETE_CHUNK) {
    const chunk = keys.slice(i, i + R2_DELETE_CHUNK);
    try {
      await bucket.delete(chunk);
    } catch (error) {
      failed += chunk.length;
      console.warn(
        `account purge: ${chunk.length} files left for the R2 cleanup sweep: ${error instanceof Error ? error.message : String(error)}`,
      );
      continue;
    }
    for (let j = 0; j < chunk.length; j += KEY_ROWS_CHUNK) {
      const done = chunk.slice(j, j + KEY_ROWS_CHUNK);
      await db
        .prepare(
          `DELETE FROM r2_cleanup_jobs WHERE file_key IN (${done.map(() => "?").join(", ")})`,
        )
        .bind(...done)
        .run()
        .catch(() => undefined);
    }
  }
  return failed;
}

export type PurgeBucket = Pick<
  R2Bucket,
  "delete" | "list" | "resumeMultipartUpload"
>;

/** Why the purge holds an account instead of deleting it. */
export type PurgeHold = "owns_servers" | "suspended";

/** Whether the purge must hold the account, and why. */
async function purgeHold(db: Db, userId: string): Promise<PurgeHold | null> {
  // A server owned now would go with the users row (ON DELETE CASCADE).
  if ((await ownedServers(db, userId)).length > 0) return "owns_servers";
  // A suspension made after the request stays for the site admin to see.
  const suspended = await db
    .prepare(`SELECT 1 FROM users WHERE id = ? AND suspended_at IS NOT NULL`)
    .bind(userId)
    .first();
  return suspended ? "suspended" : null;
}

/**
 * Purge one account whose grace period is over. Null when there was nothing
 * to purge: the account was restored (or purged) since the sweep listed it.
 * `{ held }` when it cannot be purged as it is (see PurgeHold); nothing was
 * changed.
 */
export async function purgeAccount(
  db: Db,
  bucket: PurgeBucket,
  userId: string,
  now: number,
): Promise<
  | { tombstoneId: string; moved: number; files: number }
  | { held: PurgeHold }
  | null
> {
  const at = new Date(now).toISOString();

  const held = await purgeHold(db, userId);
  if (held) return { held };

  // Step 1: claim the row. From here a restore changes nothing, and a retry
  // reuses the same "Deleted user" row.
  const claim = await db
    .prepare(
      `UPDATE account_deletions
       SET tombstone_id = COALESCE(tombstone_id, ?),
           purge_started_at = COALESCE(purge_started_at, ?)
       WHERE user_id = ? AND delete_after <= ?
       RETURNING tombstone_id, snapshot`,
    )
    .bind(`deleted-${crypto.randomUUID()}`, at, userId, at)
    .first<{ tombstone_id: string; snapshot: string }>();
  if (!claim) return null;
  const tombstoneId = claim.tombstone_id;
  await db
    .prepare(
      `INSERT OR IGNORE INTO users (id, username, display_name, status, created_at, deleted_at)
       VALUES (?, ?, ?, 'offline', ?, ?)`,
    )
    .bind(tombstoneId, deletedUsername(), DELETED_USER_DISPLAY_NAME, at, at)
    .run();

  // Step 2: what other people keep moves to it.
  let moved = 0;
  for (const [table, column, extra] of KEPT) {
    moved += await repoint(db, table, column, userId, tombstoneId, extra);
  }
  // The id inside kept rows: DM call records and server audit targets.
  // (Mentions are `@username` text and are left as they are.)
  const quoted = JSON.stringify(userId);
  const replacement = JSON.stringify(tombstoneId);
  await db.batch([
    db
      .prepare(
        `UPDATE messages SET metadata = replace(metadata, ?, ?)
         WHERE type IS NOT NULL AND metadata IS NOT NULL
           AND instr(metadata, ?) > 0
           AND channel_id IN (
             SELECT channel_id FROM dm_recipients WHERE user_id = ?
           )`,
      )
      .bind(quoted, replacement, quoted, tombstoneId),
    db
      .prepare(`UPDATE server_audit_logs SET target_id = ? WHERE target_id = ?`)
      .bind(tombstoneId, userId),
  ]);
  // Public share links of those messages carry the author as it was.
  await db
    .prepare(
      `UPDATE message_shares SET snapshot_author = ?
       WHERE source_message_id IN (SELECT id FROM messages WHERE author_id = ?)`,
    )
    .bind(
      JSON.stringify({
        id: tombstoneId,
        username: "",
        display_name: DELETED_USER_DISPLAY_NAME,
        avatar_url: null,
      }),
      tombstoneId,
    )
    .run();

  // Step 3: the account's files, listed and queued before their rows go.
  const keys = await filesToDelete(
    db,
    bucket,
    userId,
    parseSnapshot(claim.snapshot),
  );
  await queueFiles(db, keys, now);
  await abortPendingUploads(db, bucket, userId);

  // Step 4: the account itself. Its own rows cascade from users. Others'
  // friend and pending rows naming it go too; their blocks of it stay
  // (relationships.target_user_id has no foreign key, migration 046).
  await db.batch([
    db
      .prepare(
        `DELETE FROM relationships WHERE target_user_id = ? AND type != 1`,
      )
      .bind(userId),
    db
      .prepare(
        `DELETE FROM channel_permission_overrides WHERE target_type = 'user' AND target_id = ?`,
      )
      .bind(userId),
    db.prepare(`DELETE FROM daily_activity WHERE user_id = ?`).bind(userId),
    db.prepare(`DELETE FROM pending_uploads WHERE user_id = ?`).bind(userId),
    db.prepare(`DELETE FROM users WHERE id = ?`).bind(userId),
    db.prepare(`DELETE FROM account_deletions WHERE user_id = ?`).bind(userId),
  ]);

  // Step 5: the files. Failures are retried by the R2 cleanup sweep.
  await deleteFiles(db, bucket, keys);
  return { tombstoneId, moved, files: keys.length };
}

/**
 * The hourly sweep: purge accounts whose grace period is over. Held accounts
 * (PurgeHold) are not failures: they are tried last, after every other due
 * account, and logged only when their reason changes.
 */
export async function purgeDueAccountDeletions(
  db: Db,
  bucket: PurgeBucket,
  now: number = Date.now(),
  limit: number = PURGE_BATCH,
): Promise<{ purged: number; failed: number; held: number }> {
  const { results } = await db
    .prepare(
      // Held accounts last, then fewest failures first, so accounts that
      // cannot be purged or keep failing never hold the rest back.
      `SELECT user_id, held_reason FROM account_deletions
       WHERE delete_after <= ?
       ORDER BY held_reason IS NOT NULL, attempts ASC, delete_after ASC
       LIMIT ?`,
    )
    .bind(new Date(now).toISOString(), limit)
    .all<{ user_id: string; held_reason: string | null }>();

  let purged = 0;
  let failed = 0;
  let held = 0;
  for (const row of results ?? []) {
    try {
      const result = await purgeAccount(db, bucket, row.user_id, now);
      if (result && "held" in result) {
        held += 1;
        if (row.held_reason !== result.held) {
          console.warn(
            `account purge held (${result.held}); left for the site admin`,
          );
          await db
            .prepare(
              `UPDATE account_deletions SET held_reason = ? WHERE user_id = ?`,
            )
            .bind(result.held, row.user_id)
            .run()
            .catch(() => undefined);
        }
      } else if (result) {
        purged += 1;
      }
    } catch (error) {
      failed += 1;
      const message = error instanceof Error ? error.message : String(error);
      console.error(`account purge failed: ${message}`);
      await db
        .prepare(
          `UPDATE account_deletions
           SET attempts = attempts + 1, last_error = ?, held_reason = NULL
           WHERE user_id = ?`,
        )
        .bind(message.slice(0, 500), row.user_id)
        .run()
        .catch(() => undefined);
    }
  }
  return { purged, failed, held };
}

/** Whether `userId` is waiting for deletion: its profile is hidden from others. */
export async function isPendingDeletion(
  db: Pick<D1Database, "prepare">,
  userId: string,
): Promise<boolean> {
  try {
    return !!(await db
      .prepare(`SELECT 1 FROM account_deletions WHERE user_id = ?`)
      .bind(userId)
      .first());
  } catch {
    return false;
  }
}

/** Whether a username is held for an account that can still be restored. */
export async function isUsernameReserved(
  db: Pick<D1Database, "prepare">,
  username: string,
): Promise<boolean> {
  try {
    return !!(await db
      .prepare(`SELECT 1 FROM account_deletions WHERE reserved_username = ?`)
      .bind(username.toLowerCase())
      .first());
  } catch {
    return false;
  }
}
