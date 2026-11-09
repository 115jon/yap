/**
 * Admin Service: the console's reads and writes (the admin Worker's /api/*).
 *
 * `createAdminHandlers(deps)` returns one handler per route. The Worker
 * (./worker.ts) calls them only after its gate has verified a site-admin
 * session, and passes that admin's Yap user id as the actor. Every mutation
 * is one D1 batch: the change and its `admin_audit_log` entry land together
 * or not at all.
 *
 * Live state comes from the global gateway Durable Object through two
 * internal stub paths (MeetingRoom /admin/introspect and
 * /admin/disconnect-user); nothing here reads message content.
 */

import {
  BADGES,
  isBadgeId,
  isManualBadge,
  profileBadges,
  type BadgeAwardSource,
  type UserBadge,
} from "@/lib/badges";
import {
  describeError,
  getCollectiblesCatalogHealth,
  isCollectiblesSyncInFlight,
  manualSyncRetryAfterMs,
  syncCollectiblesCatalog,
} from "@/lib/collectibles-catalog";
import { isTier, parseEntitlement, type Entitlement } from "@/lib/entitlements";
import { genId } from "@/lib/id";
import {
  ANNOUNCEMENT_MAX_LENGTH,
  isAnnouncementTone,
  parseSiteFlags,
  safeAnnouncementUrl,
  SITE_FLAGS,
  type SiteAnnouncement,
} from "@/lib/site-flags";
import type {
  AdminAuditEntry,
  AdminAuditResponse,
  AdminBadge,
  AdminCatalogRefreshResponse,
  AdminCatalogStatus,
  AdminCronRun,
  AdminDailyPoint,
  AdminDevice,
  AdminInsights,
  AdminLiveSession,
  AdminLiveSnapshot,
  AdminOnlineUser,
  AdminPendingUpload,
  AdminPeopleResponse,
  AdminPerson,
  AdminPersonDetail,
  AdminServerMembership,
  AdminSettingsResponse,
  AdminStorageKind,
  AdminTableCount,
  AdminTopServer,
  AdminVoiceRoom,
} from "../shared/admin-api";
import {
  deleteSiteSettingStatement,
  readSiteSettings,
  siteSettingStatement,
} from "@/services/site-settings-store";

// ── Dependencies ────────────────────────────────────────────────────────────

/** What the global gateway answers on /admin/introspect. */
export interface GatewayIntrospection {
  room: string;
  sockets: number;
  resumable_sessions: number;
  pending_calls: number;
  active_calls: number;
  sessions: Array<{
    user_id: string | null;
    platform: string | null;
    status: string | null;
    voice_channel_id: string | null;
    standby: boolean;
    last_heartbeat: number | null;
  }>;
  voice_channels: Array<{
    channel_id: string;
    started_at: number | null;
    members: AdminVoiceRoom["members"];
  }>;
}

export interface AdminGateway {
  /** Null when the gateway could not be asked. */
  introspect: () => Promise<GatewayIntrospection | null>;
  /** Sockets closed, or null when the gateway could not be asked. */
  disconnectUser: (userId: string, reason: string) => Promise<number | null>;
}

export interface AdminDeps {
  db: D1Database;
  gateway: AdminGateway;
  /** After a plan, badge or suspension change: drop caches. */
  afterUserChange?: (userId: string) => Promise<void>;
  /** What the main Worker runs (its /api/healthz); null when unknown. */
  deploy?: () => Promise<{
    version: string;
    commit: string;
    built_at: string | null;
  } | null>;
  now?: () => number;
}

// ── Small helpers ───────────────────────────────────────────────────────────

const PEOPLE_LIMIT = 2000;
const DAY_MS = 86_400_000;

function json(data: unknown, status = 200): Response {
  return Response.json(data, {
    status,
    headers: { "Cache-Control": "no-store" },
  });
}

function fail(message: string, status: number, code: string): Response {
  return Response.json(
    { error: message, code },
    { status, headers: { "Cache-Control": "no-store" } },
  );
}

async function readJson(
  request: Request,
): Promise<Record<string, unknown> | null> {
  try {
    const body = (await request.json()) as unknown;
    return body && typeof body === "object" && !Array.isArray(body)
      ? (body as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function parseStored(value: string | null): unknown {
  if (value === null) return null;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function num(value: unknown): number {
  const n = typeof value === "number" ? value : Number(value);
  return Number.isFinite(n) ? n : 0;
}

/** `YYYY-MM-DD` of an epoch ms, in UTC. */
function dayOf(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

/** Every day from `from` to `to` (inclusive) with the values filled in. */
function fillDays(
  from: number,
  days: number,
  rows: Array<{ day: string; value: unknown }>,
): AdminDailyPoint[] {
  const byDay = new Map(rows.map((row) => [row.day, num(row.value)]));
  return Array.from({ length: days }, (_, index) => {
    const day = dayOf(from + index * DAY_MS);
    return { day, value: byDay.get(day) ?? 0 };
  });
}

async function safeAll<T>(statement: D1PreparedStatement): Promise<T[]> {
  try {
    const { results } = await statement.all<T>();
    return results ?? [];
  } catch {
    return [];
  }
}

// ── Audit ───────────────────────────────────────────────────────────────────

export interface AuditWrite {
  /** Set when the caller updates the row later (see disconnectUser). */
  id?: string;
  actorId: string;
  action: string;
  targetType: "user" | "site";
  targetId: string | null;
  before: unknown;
  after: unknown;
  request: Request;
  at: string;
}

/** The audit row for one mutation; batch it with the change it records. */
export function auditStatement(
  db: D1Database,
  entry: AuditWrite,
): D1PreparedStatement {
  return db
    .prepare(
      `INSERT INTO admin_audit_log
         (id, actor_id, action, target_type, target_id, before_json, after_json, cf_ray, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    )
    .bind(
      entry.id ?? genId(),
      entry.actorId,
      entry.action,
      entry.targetType,
      entry.targetId,
      entry.before === undefined ? null : JSON.stringify(entry.before),
      entry.after === undefined ? null : JSON.stringify(entry.after),
      entry.request.headers.get("cf-ray"),
      entry.at,
    );
}

type AuditRow = {
  id: string;
  actor_id: string;
  actor_name: string | null;
  action: string;
  target_type: "user" | "site";
  target_id: string | null;
  target_name: string | null;
  before_json: string | null;
  after_json: string | null;
  cf_ray: string | null;
  created_at: string;
};

const AUDIT_SELECT = `SELECT l.id, l.actor_id,
    COALESCE(NULLIF(a.display_name, ''), a.username) AS actor_name,
    l.action, l.target_type, l.target_id,
    CASE WHEN l.target_type = 'user'
      THEN COALESCE(NULLIF(t.display_name, ''), t.username) END AS target_name,
    l.before_json, l.after_json, l.cf_ray, l.created_at
  FROM admin_audit_log l
  LEFT JOIN users a ON a.id = l.actor_id
  LEFT JOIN users t ON t.id = l.target_id AND l.target_type = 'user'`;

function auditEntry(row: AuditRow): AdminAuditEntry {
  return {
    id: row.id,
    actor_id: row.actor_id,
    actor_name: row.actor_name,
    action: row.action,
    target_type: row.target_type,
    target_id: row.target_id,
    target_name: row.target_name,
    before: parseStored(row.before_json),
    after: parseStored(row.after_json),
    cf_ray: row.cf_ray,
    created_at: row.created_at,
  };
}

export async function listAudit(
  db: D1Database,
  options: {
    limit?: number;
    before?: string | null;
    targetId?: string | null;
  } = {},
): Promise<AdminAuditResponse> {
  const limit = Math.max(1, Math.min(200, options.limit ?? 50));
  const where: string[] = [];
  const binds: unknown[] = [];
  if (options.before) {
    where.push("l.created_at < ?");
    binds.push(options.before);
  }
  if (options.targetId) {
    where.push("l.target_type = 'user' AND l.target_id = ?");
    binds.push(options.targetId);
  }
  const { results } = await db
    .prepare(
      `${AUDIT_SELECT}
       ${where.length ? `WHERE ${where.join(" AND ")}` : ""}
       ORDER BY l.created_at DESC, l.id DESC
       LIMIT ?`,
    )
    .bind(...binds, limit + 1)
    .all<AuditRow>();
  const rows = results ?? [];
  const page = rows.slice(0, limit);
  return {
    entries: page.map(auditEntry),
    next_before:
      rows.length > limit ? (page[page.length - 1]?.created_at ?? null) : null,
  };
}

// ── People ──────────────────────────────────────────────────────────────────

type PersonRow = {
  id: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  created_at: string | null;
  updated_at: string | null;
  suspended_at: string | null;
  suspended_reason: string | null;
  tier: string | null;
  source: string | null;
  started_at: string | null;
  expires_at: string | null;
  messages_sent: number | null;
  voice_seconds: number | null;
  stream_seconds: number | null;
  last_active_at: string | null;
  servers_joined: number | null;
  servers_owned: number | null;
  storage_bytes: number | null;
  uploads: number | null;
};

const PEOPLE_SELECT = `SELECT u.id, u.username, u.display_name, u.avatar_url,
    u.created_at, u.updated_at, u.suspended_at, u.suspended_reason,
    e.tier, e.source, e.started_at, e.expires_at,
    s.messages_sent, s.voice_seconds, s.stream_seconds, s.last_active_at,
    sm.servers_joined, so.servers_owned, a.storage_bytes, a.uploads
  FROM users u
  LEFT JOIN user_entitlements e ON e.user_id = u.id
  LEFT JOIN user_stats s ON s.user_id = u.id
  LEFT JOIN (SELECT user_id, COUNT(*) AS servers_joined FROM server_members GROUP BY user_id) sm
    ON sm.user_id = u.id
  LEFT JOIN (SELECT owner_id, COUNT(*) AS servers_owned FROM servers GROUP BY owner_id) so
    ON so.owner_id = u.id
  LEFT JOIN (SELECT user_id, SUM(size_bytes) AS storage_bytes, COUNT(*) AS uploads FROM attachments GROUP BY user_id) a
    ON a.user_id = u.id`;

type BadgeRow = {
  user_id: string;
  badge_id: string;
  source: string;
  awarded_at: string;
};

function latest(...values: Array<string | null | undefined>): string | null {
  let best: string | null = null;
  let bestMs = -Infinity;
  for (const value of values) {
    if (!value) continue;
    // D1's datetime('now') has no T or Z; read it as UTC.
    const ms = Date.parse(
      value.includes("T") ? value : `${value.replace(" ", "T")}Z`,
    );
    if (Number.isFinite(ms) && ms > bestMs) {
      bestMs = ms;
      best = value;
    }
  }
  return best;
}

function personFrom(
  row: PersonRow,
  badgeRows: BadgeRow[],
  now: number,
): AdminPerson {
  const plan: Entitlement = row.tier
    ? parseEntitlement({
        tier: row.tier,
        source: row.source,
        started_at: row.started_at,
        expires_at: row.expires_at,
      })
    : parseEntitlement(null);
  const stored: UserBadge[] = badgeRows.flatMap((badge) =>
    isBadgeId(badge.badge_id)
      ? [{ id: badge.badge_id, awarded_at: badge.awarded_at }]
      : [],
  );
  const sourceById = new Map(
    badgeRows.map((badge) => [badge.badge_id, badge.source]),
  );
  const badges: AdminBadge[] = profileBadges(stored, plan, now).map(
    (badge) => ({
      id: badge.id,
      awarded_at: badge.awarded_at,
      source:
        badge.id === "loud"
          ? "tier"
          : ((sourceById.get(badge.id) as BadgeAwardSource | undefined) ??
            "auto"),
    }),
  );
  return {
    id: row.id,
    username: row.username,
    display_name: row.display_name,
    avatar_url: row.avatar_url,
    created_at: row.created_at,
    last_active_at: latest(row.last_active_at, row.updated_at),
    suspended_at: row.suspended_at,
    suspended_reason: row.suspended_reason,
    plan,
    badges,
    messages_sent: num(row.messages_sent),
    voice_seconds: num(row.voice_seconds),
    stream_seconds: num(row.stream_seconds),
    servers_joined: num(row.servers_joined),
    servers_owned: num(row.servers_owned),
    storage_bytes: num(row.storage_bytes),
    uploads: num(row.uploads),
  };
}

export async function listPeople(
  db: D1Database,
  now: number = Date.now(),
): Promise<AdminPerson[]> {
  const [people, badges] = await db.batch([
    db
      .prepare(`${PEOPLE_SELECT} ORDER BY u.created_at DESC, u.id ASC LIMIT ?`)
      .bind(PEOPLE_LIMIT),
    db.prepare(`SELECT user_id, badge_id, source, awarded_at FROM user_badges`),
  ]);
  const badgesByUser = new Map<string, BadgeRow[]>();
  for (const badge of (badges?.results ?? []) as BadgeRow[]) {
    const list = badgesByUser.get(badge.user_id) ?? [];
    list.push(badge);
    badgesByUser.set(badge.user_id, list);
  }
  return ((people?.results ?? []) as PersonRow[]).map((row) =>
    personFrom(row, badgesByUser.get(row.id) ?? [], now),
  );
}

export async function getPerson(
  db: D1Database,
  userId: string,
  now: number = Date.now(),
): Promise<AdminPerson | null> {
  const [person, badges] = await db.batch([
    db.prepare(`${PEOPLE_SELECT} WHERE u.id = ?`).bind(userId),
    db
      .prepare(
        `SELECT user_id, badge_id, source, awarded_at FROM user_badges WHERE user_id = ?`,
      )
      .bind(userId),
  ]);
  const row = (person?.results?.[0] ?? null) as PersonRow | null;
  if (!row) return null;
  return personFrom(row, (badges?.results ?? []) as BadgeRow[], now);
}

async function personDetail(
  deps: AdminDeps,
  userId: string,
  now: number,
): Promise<AdminPersonDetail | null> {
  const { db } = deps;
  const person = await getPerson(db, userId, now);
  if (!person) return null;
  const [servers, devices, audit, gateway] = await Promise.all([
    safeAll<{
      id: string;
      name: string;
      icon_url: string | null;
      owner_id: string;
      joined_at: string | null;
      members: number;
    }>(
      db
        .prepare(
          `SELECT s.id, s.name, s.icon_url, s.owner_id, sm.joined_at,
             (SELECT COUNT(*) FROM server_members x WHERE x.server_id = s.id) AS members
           FROM server_members sm JOIN servers s ON s.id = sm.server_id
           WHERE sm.user_id = ?
           ORDER BY sm.joined_at ASC`,
        )
        .bind(userId),
    ),
    safeAll<AdminDevice>(
      db
        .prepare(
          `SELECT platform, app_variant, push_format, created_at, last_seen_at
           FROM push_tokens WHERE user_id = ? ORDER BY last_seen_at DESC`,
        )
        .bind(userId),
    ),
    listAudit(db, { targetId: userId, limit: 20 }).catch(() => ({
      entries: [],
      next_before: null,
    })),
    deps.gateway.introspect().catch(() => null),
  ]);
  const memberships: AdminServerMembership[] = servers.map((server) => ({
    id: server.id,
    name: server.name,
    icon_url: server.icon_url,
    owner: server.owner_id === userId,
    members: num(server.members),
    joined_at: server.joined_at,
  }));
  const live: AdminLiveSession[] | null = gateway
    ? gateway.sessions
        .filter((session) => session.user_id === userId)
        .map((session) => ({
          platform: session.platform,
          status: session.status,
          voice_channel_id: session.voice_channel_id,
          standby: session.standby,
        }))
    : null;
  return {
    person,
    servers: memberships,
    devices,
    live_sessions: live,
    audit: audit.entries,
  };
}

// ── Mutations ───────────────────────────────────────────────────────────────

type EntitlementRow = {
  tier: string;
  source: string;
  started_at: string;
  expires_at: string | null;
};

async function readEntitlementRow(
  db: D1Database,
  userId: string,
): Promise<EntitlementRow | null> {
  return db
    .prepare(
      `SELECT tier, source, started_at, expires_at FROM user_entitlements WHERE user_id = ?`,
    )
    .bind(userId)
    .first<EntitlementRow>();
}

async function userExists(db: D1Database, userId: string): Promise<boolean> {
  return !!(await db
    .prepare(`SELECT 1 AS one FROM users WHERE id = ?`)
    .bind(userId)
    .first());
}

/**
 * Sets a plan by hand. Free deletes the row (no row is free); Loud takes an
 * optional end; Founder never ends. Unlike billing, an admin may move a
 * founder: that is the point of the console. The Founder badge follows the
 * Founder plan. Returns the response to send.
 */
async function setPlan(
  deps: AdminDeps,
  actorId: string,
  userId: string,
  body: Record<string, unknown> | null,
  request: Request,
  now: number,
): Promise<Response> {
  const { db } = deps;
  const tier = body?.tier;
  if (!isTier(tier)) {
    return fail("Pick free, loud or founder.", 400, "INVALID_TIER");
  }
  let expiresAt: string | null = null;
  if (tier === "loud" && body?.expires_at != null) {
    const ms =
      typeof body.expires_at === "string" ? Date.parse(body.expires_at) : NaN;
    if (!Number.isFinite(ms)) {
      return fail("That end date is not a date.", 400, "INVALID_EXPIRY");
    }
    if (ms <= now) {
      return fail(
        "The end date has to be in the future.",
        400,
        "INVALID_EXPIRY",
      );
    }
    expiresAt = new Date(ms).toISOString();
  }
  if (!(await userExists(db, userId))) {
    return fail("No account has that id.", 404, "USER_NOT_FOUND");
  }
  const at = new Date(now).toISOString();
  // Read, then write in one batch. Two admins saving the same account in the
  // same instant would each log the plan they read as `before`; the last
  // write wins and both are logged. Acceptable for a single-admin console.
  const before = await readEntitlementRow(db, userId);
  const after: EntitlementRow | null =
    tier === "free"
      ? null
      : {
          tier,
          source: "admin",
          // Extending Loud keeps when it started.
          started_at: before && before.tier === tier ? before.started_at : at,
          expires_at: tier === "loud" ? expiresAt : null,
        };

  const statements: D1PreparedStatement[] = [];
  if (after) {
    statements.push(
      db
        .prepare(
          `INSERT INTO user_entitlements (user_id, tier, source, started_at, expires_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(user_id) DO UPDATE SET
             tier = excluded.tier,
             source = excluded.source,
             started_at = excluded.started_at,
             expires_at = excluded.expires_at,
             updated_at = excluded.updated_at`,
        )
        .bind(
          userId,
          after.tier,
          after.source,
          after.started_at,
          after.expires_at,
          at,
        ),
    );
  } else {
    statements.push(
      db
        .prepare(`DELETE FROM user_entitlements WHERE user_id = ?`)
        .bind(userId),
    );
  }
  if (tier === "founder") {
    statements.push(
      db
        .prepare(
          `INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
           VALUES (?, 'founder', ?, 'admin')`,
        )
        .bind(userId, at),
    );
  } else if (before?.tier === "founder") {
    statements.push(
      db
        .prepare(
          `DELETE FROM user_badges WHERE user_id = ? AND badge_id = 'founder'`,
        )
        .bind(userId),
    );
  }
  statements.push(
    auditStatement(db, {
      actorId,
      action: "plan.set",
      targetType: "user",
      targetId: userId,
      before: before ?? { tier: "free" },
      after: after ?? { tier: "free" },
      request,
      at,
    }),
  );
  await db.batch(statements);
  await deps.afterUserChange?.(userId);
  return json({ person: await getPerson(db, userId, now) });
}

async function setBadge(
  deps: AdminDeps,
  actorId: string,
  userId: string,
  body: Record<string, unknown> | null,
  request: Request,
  now: number,
): Promise<Response> {
  const { db } = deps;
  const badgeId = body?.badge_id;
  const action = body?.action ?? "award";
  if (action !== "award" && action !== "revoke") {
    return fail("Award or revoke.", 400, "INVALID_ACTION");
  }
  if (!isBadgeId(badgeId)) {
    return fail("There is no badge with that id.", 400, "BADGE_UNKNOWN");
  }
  if (!isManualBadge(badgeId)) {
    return fail(
      "That badge is awarded automatically, not by hand.",
      400,
      "BADGE_NOT_MANUAL",
    );
  }
  if (!(await userExists(db, userId))) {
    return fail("No account has that id.", 404, "USER_NOT_FOUND");
  }
  const at = new Date(now).toISOString();
  const had = await db
    .prepare(
      `SELECT awarded_at, source FROM user_badges WHERE user_id = ? AND badge_id = ?`,
    )
    .bind(userId, badgeId)
    .first<{ awarded_at: string; source: string }>();
  const changed = action === "award" ? !had : !!had;
  if (changed) {
    await db.batch([
      action === "award"
        ? db
            .prepare(
              `INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
               VALUES (?, ?, ?, 'admin')`,
            )
            .bind(userId, badgeId, at)
        : db
            .prepare(
              `DELETE FROM user_badges WHERE user_id = ? AND badge_id = ?`,
            )
            .bind(userId, badgeId),
      auditStatement(db, {
        actorId,
        action: action === "award" ? "badge.award" : "badge.revoke",
        targetType: "user",
        targetId: userId,
        before: { badge_id: badgeId, held: !!had, ...(had ?? {}) },
        after: {
          badge_id: badgeId,
          held: action === "award",
          ...(action === "award" ? { awarded_at: at, source: "admin" } : {}),
        },
        request,
        at,
      }),
    ]);
    await deps.afterUserChange?.(userId);
  }
  return json({
    user_id: userId,
    badge_id: badgeId,
    action,
    changed,
    person: await getPerson(db, userId, now),
  });
}

export const SUSPENSION_REASON_MAX = 200;

async function setSuspension(
  deps: AdminDeps,
  actorId: string,
  userId: string,
  body: Record<string, unknown> | null,
  request: Request,
  now: number,
): Promise<Response> {
  const { db } = deps;
  if (typeof body?.suspended !== "boolean") {
    return fail(
      "Say whether the account is suspended.",
      400,
      "INVALID_SUSPENSION",
    );
  }
  const suspend = body.suspended;
  if (suspend && userId === actorId) {
    return fail(
      "You can't suspend your own account.",
      400,
      "CANNOT_SUSPEND_SELF",
    );
  }
  const reason =
    suspend && typeof body.reason === "string" && body.reason.trim()
      ? body.reason.trim().slice(0, SUSPENSION_REASON_MAX)
      : null;
  const before = await db
    .prepare(`SELECT suspended_at, suspended_reason FROM users WHERE id = ?`)
    .bind(userId)
    .first<{ suspended_at: string | null; suspended_reason: string | null }>();
  if (!before) return fail("No account has that id.", 404, "USER_NOT_FOUND");
  const at = new Date(now).toISOString();
  const after = suspend
    ? { suspended_at: before.suspended_at ?? at, suspended_reason: reason }
    : { suspended_at: null, suspended_reason: null };
  await db.batch([
    db
      .prepare(
        `UPDATE users SET suspended_at = ?, suspended_reason = ? WHERE id = ?`,
      )
      .bind(after.suspended_at, after.suspended_reason, userId),
    auditStatement(db, {
      actorId,
      action: suspend ? "user.suspend" : "user.unsuspend",
      targetType: "user",
      targetId: userId,
      before,
      after,
      request,
      at,
    }),
  ]);
  // The main Worker's isolates pick the change up within SUSPENSION_TTL_MS
  // (src/lib/suspension.ts); the sockets close now.
  let closed: number | null = null;
  if (suspend) {
    // Refused from the next request; close what is already open too.
    closed = await deps.gateway
      .disconnectUser(userId, "Account suspended")
      .catch(() => null);
  }
  await deps.afterUserChange?.(userId);
  return json({
    person: await getPerson(db, userId, now),
    closed_sockets: closed,
  });
}

async function disconnectUser(
  deps: AdminDeps,
  actorId: string,
  userId: string,
  request: Request,
  now: number,
): Promise<Response> {
  const { db } = deps;
  if (!(await userExists(db, userId))) {
    return fail("No account has that id.", 404, "USER_NOT_FOUND");
  }
  // Audit first: if the log cannot be written, nothing is closed. The row
  // says "requested" until the gateway answers, then holds the outcome.
  const auditId = genId();
  await auditStatement(db, {
    id: auditId,
    actorId,
    action: "user.disconnect",
    targetType: "user",
    targetId: userId,
    before: null,
    after: { requested: true },
    request,
    at: new Date(now).toISOString(),
  }).run();
  const closed = await deps.gateway
    .disconnectUser(userId, "Disconnected by an admin")
    .catch(() => null);
  await db
    .prepare(`UPDATE admin_audit_log SET after_json = ? WHERE id = ?`)
    .bind(
      JSON.stringify(
        closed === null
          ? { error: "gateway_unavailable" }
          : { closed_sockets: closed },
      ),
      auditId,
    )
    .run()
    .catch((error: unknown) =>
      console.error({
        level: "ERROR",
        message: "admin.audit_update_failed",
        audit_id: auditId,
        closed_sockets: closed,
        error: error instanceof Error ? error.message : String(error),
      }),
    );
  if (closed === null) {
    return fail(
      "The gateway did not answer. Nothing was closed.",
      503,
      "GATEWAY_UNAVAILABLE",
    );
  }
  return json({ closed_sockets: closed });
}

// ── Shop catalog ────────────────────────────────────────────────────────────

/** The collectibles catalog's freshness and its refreshes, for the console. */
export async function catalogStatus(
  db: D1Database,
  now: number,
): Promise<AdminCatalogStatus> {
  const health = await getCollectiblesCatalogHealth(db, now);
  const { sync, ingest } = health;
  return {
    synced_at: health.catalog?.syncedAt ?? null,
    items: health.catalog?.items ?? 0,
    categories: health.catalog?.categories ?? 0,
    stale: health.catalog?.stale ?? true,
    last_attempt_at: sync.lastAttemptAt ?? null,
    last_success_at: sync.lastSuccessAt ?? null,
    last_failure_at: sync.lastFailureAt ?? null,
    last_error: sync.lastError ?? null,
    last_outcome: sync.lastOutcome ?? null,
    last_trigger: sync.lastTrigger ?? null,
    consecutive_failures: sync.consecutiveFailures,
    fetcher: {
      last_success_at: ingest.lastSuccessAt ?? null,
      last_failure_at: ingest.lastFailureAt ?? null,
      last_error: ingest.lastError ?? null,
    },
    // The cooldown spares upstream. A fetcher push never calls upstream
    // from here, so it does not start one.
    retry_after_ms:
      sync.lastTrigger === "ingest" ? 0 : manualSyncRetryAfterMs(sync, now),
  };
}

/** A `catalog.refresh` audit row's outcome (after_json). */
interface CatalogRefreshAudit {
  outcome: AdminCatalogStatus["last_outcome"];
  synced_at: string | null;
  items: number;
  error?: string;
}

/**
 * "Refresh now": the same fetch, validate and store path as the cron, run
 * here with trigger "manual" (the main Worker has no refresh of its own).
 * Like a disconnect, the audit row is written first ("requested") and then
 * holds the outcome. A failed fetch keeps the last good catalog and answers
 * 502 with the reason, which the status row also records.
 */
async function refreshCatalog(
  deps: AdminDeps,
  actorId: string,
  request: Request,
  now: number,
): Promise<Response> {
  const { db } = deps;
  const before = await catalogStatus(db, now);
  if (isCollectiblesSyncInFlight()) {
    return fail(
      "A refresh is already running. Try again in a moment.",
      429,
      "CATALOG_REFRESH_COOLDOWN",
    );
  }
  if (before.retry_after_ms > 0) {
    const seconds = Math.ceil(before.retry_after_ms / 1000);
    return fail(
      `The catalog was tried moments ago. Try again in ${seconds} s.`,
      429,
      "CATALOG_REFRESH_COOLDOWN",
    );
  }
  const auditId = genId();
  await auditStatement(db, {
    id: auditId,
    actorId,
    action: "catalog.refresh",
    targetType: "site",
    targetId: null,
    before: { synced_at: before.synced_at, items: before.items },
    after: { requested: true },
    request,
    at: new Date(now).toISOString(),
  }).run();
  let error: string | null = null;
  try {
    await syncCollectiblesCatalog(db, { trigger: "manual" });
  } catch (cause) {
    error = describeError(cause);
  }
  const after = await catalogStatus(db, deps.now?.() ?? Date.now());
  const outcome: CatalogRefreshAudit = {
    outcome: after.last_outcome,
    synced_at: after.synced_at,
    items: after.items,
  };
  if (error) outcome.error = error;
  await db
    .prepare(`UPDATE admin_audit_log SET after_json = ? WHERE id = ?`)
    .bind(JSON.stringify(outcome), auditId)
    .run()
    .catch((cause: unknown) =>
      console.error({
        level: "ERROR",
        message: "admin.audit_update_failed",
        audit_id: auditId,
        error: describeError(cause),
      }),
    );
  if (error) {
    return fail(
      `The refresh failed; the last good catalog stays. ${error}`,
      502,
      "CATALOG_REFRESH_FAILED",
    );
  }
  const response: AdminCatalogRefreshResponse = { catalog: after };
  return json(response);
}

// ── Settings (switches and the announcement) ────────────────────────────────

async function settingsResponse(
  db: D1Database,
): Promise<AdminSettingsResponse> {
  const settings = await readSiteSettings(db);
  const meta = await safeAll<{
    key: string;
    updated_at: string;
    updated_by: string | null;
  }>(
    db.prepare(
      `SELECT key, updated_at, updated_by FROM site_settings WHERE key IN ('flags', 'announcement')`,
    ),
  );
  const byKey = new Map(meta.map((row) => [row.key, row]));
  const stamp = (key: string) => {
    const row = byKey.get(key);
    return row ? { at: row.updated_at, by: row.updated_by } : null;
  };
  return {
    flags: settings.flags,
    announcement: settings.announcement,
    updated: { flags: stamp("flags"), announcement: stamp("announcement") },
  };
}

function parseAnnouncementInput(
  value: unknown,
  now: number,
):
  | { ok: true; announcement: SiteAnnouncement | null }
  | { ok: false; response: Response } {
  if (value === null) return { ok: true, announcement: null };
  if (!value || typeof value !== "object") {
    return {
      ok: false,
      response: fail(
        "The announcement is not readable.",
        400,
        "INVALID_ANNOUNCEMENT",
      ),
    };
  }
  const raw = value as Record<string, unknown>;
  const text = typeof raw.text === "string" ? raw.text.trim() : "";
  if (!text) {
    return {
      ok: false,
      response: fail(
        "Write the announcement first.",
        400,
        "INVALID_ANNOUNCEMENT",
      ),
    };
  }
  if (text.length > ANNOUNCEMENT_MAX_LENGTH) {
    return {
      ok: false,
      response: fail(
        `Keep it under ${ANNOUNCEMENT_MAX_LENGTH} characters.`,
        400,
        "INVALID_ANNOUNCEMENT",
      ),
    };
  }
  if (raw.tone !== undefined && !isAnnouncementTone(raw.tone)) {
    return {
      ok: false,
      response: fail(
        "Pick info, warning or critical.",
        400,
        "INVALID_ANNOUNCEMENT",
      ),
    };
  }
  let linkUrl: string | null = null;
  if (raw.link_url != null && raw.link_url !== "") {
    linkUrl = safeAnnouncementUrl(raw.link_url);
    if (!linkUrl) {
      return {
        ok: false,
        response: fail(
          "Links have to start with https://.",
          400,
          "INVALID_ANNOUNCEMENT",
        ),
      };
    }
  }
  let expiresAt: string | null = null;
  if (raw.expires_at != null && raw.expires_at !== "") {
    const ms =
      typeof raw.expires_at === "string" ? Date.parse(raw.expires_at) : NaN;
    if (!Number.isFinite(ms) || ms <= now) {
      return {
        ok: false,
        response: fail(
          "The end time has to be in the future.",
          400,
          "INVALID_ANNOUNCEMENT",
        ),
      };
    }
    expiresAt = new Date(ms).toISOString();
  }
  return {
    ok: true,
    announcement: {
      id: genId(),
      text,
      tone: isAnnouncementTone(raw.tone) ? raw.tone : "info",
      link_url: linkUrl,
      link_label:
        linkUrl && typeof raw.link_label === "string" && raw.link_label.trim()
          ? raw.link_label.trim().slice(0, 40)
          : null,
      expires_at: expiresAt,
      updated_at: new Date(now).toISOString(),
    },
  };
}

async function putSettings(
  deps: AdminDeps,
  actorId: string,
  body: Record<string, unknown> | null,
  request: Request,
  now: number,
): Promise<Response> {
  const { db } = deps;
  if (!body) return fail("Expected a JSON body.", 400, "INVALID_BODY");
  const current = await readSiteSettings(db);
  const at = new Date(now).toISOString();
  const statements: D1PreparedStatement[] = [];

  if (body.flags !== undefined) {
    if (!body.flags || typeof body.flags !== "object") {
      return fail("Flags have to be on or off.", 400, "INVALID_FLAGS");
    }
    const patch = body.flags as Record<string, unknown>;
    for (const [key, value] of Object.entries(patch)) {
      if (
        !(SITE_FLAGS as readonly string[]).includes(key) ||
        typeof value !== "boolean"
      ) {
        return fail(`Unknown switch: ${key}.`, 400, "INVALID_FLAGS");
      }
    }
    const next = parseSiteFlags({ ...current.flags, ...patch });
    const changed = SITE_FLAGS.filter(
      (flag) => next[flag] !== current.flags[flag],
    );
    if (changed.length) {
      statements.push(siteSettingStatement(db, "flags", next, actorId, at));
      statements.push(
        auditStatement(db, {
          actorId,
          action: "flags.update",
          targetType: "site",
          targetId: changed.join(","),
          before: Object.fromEntries(
            changed.map((flag) => [flag, current.flags[flag]]),
          ),
          after: Object.fromEntries(changed.map((flag) => [flag, next[flag]])),
          request,
          at,
        }),
      );
    }
  }

  if (body.announcement !== undefined) {
    const parsed = parseAnnouncementInput(body.announcement, now);
    if (!parsed.ok) return parsed.response;
    statements.push(
      parsed.announcement
        ? siteSettingStatement(
            db,
            "announcement",
            parsed.announcement,
            actorId,
            at,
          )
        : deleteSiteSettingStatement(db, "announcement"),
    );
    statements.push(
      auditStatement(db, {
        actorId,
        action: "announcement.update",
        targetType: "site",
        targetId: "announcement",
        before: current.announcement,
        after: parsed.announcement,
        request,
        at,
      }),
    );
  }

  if (statements.length) {
    // The main Worker reads it within SITE_SETTINGS_TTL_MS.
    await db.batch(statements);
  }
  return json(await settingsResponse(db));
}

// ── Live ────────────────────────────────────────────────────────────────────

const TABLE_NAME = /^[A-Za-z0-9_]+$/;

async function d1Overview(
  db: D1Database,
): Promise<{ tables: AdminTableCount[]; size_bytes: number | null }> {
  const listing = await db
    .prepare(
      `SELECT name FROM sqlite_master
       WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_cf_%' AND name NOT LIKE 'd1_%'
       ORDER BY name`,
    )
    .all<{ name: string }>();
  const names = (listing.results ?? [])
    .map((row) => row.name)
    .filter((name) => TABLE_NAME.test(name));
  const sizeAfter = (listing.meta as { size_after?: number } | undefined)
    ?.size_after;
  if (names.length === 0) {
    return {
      tables: [],
      size_bytes: typeof sizeAfter === "number" ? sizeAfter : null,
    };
  }
  const counts = await db.batch(
    names.map((name) => db.prepare(`SELECT COUNT(*) AS rows FROM "${name}"`)),
  );
  const tables = names
    .map((name, index) => ({
      name,
      rows: num(
        (counts[index]?.results?.[0] as { rows?: number } | undefined)?.rows,
      ),
    }))
    .sort((a, b) => b.rows - a.rows || a.name.localeCompare(b.name));
  return {
    tables,
    size_bytes: typeof sizeAfter === "number" ? sizeAfter : null,
  };
}

async function storageByKind(db: D1Database): Promise<AdminStorageKind[]> {
  const kinds: Array<[string, string, boolean]> = [
    [
      "Message attachments",
      `SELECT COUNT(*) AS objects, COALESCE(SUM(size_bytes), 0) AS bytes FROM attachments WHERE soundboard_server_id IS NULL`,
      true,
    ],
    [
      "Soundboard sounds",
      `SELECT COUNT(*) AS objects, COALESCE(SUM(size_bytes), 0) AS bytes FROM attachments WHERE soundboard_server_id IS NOT NULL`,
      true,
    ],
    [
      "Voice status media",
      `SELECT COUNT(*) AS objects, COALESCE(SUM(size_bytes), 0) AS bytes FROM voice_status_media_assets`,
      true,
    ],
    [
      "Generated emoji",
      `SELECT COUNT(*) AS objects, COALESCE(SUM(size_bytes), 0) AS bytes FROM generated_emojis WHERE file_key IS NOT NULL`,
      true,
    ],
    [
      "Chunked parts in flight",
      `SELECT COUNT(*) AS objects, COALESCE(SUM(size_bytes), 0) AS bytes FROM pending_upload_parts`,
      true,
    ],
    [
      "Avatars",
      `SELECT COUNT(*) AS objects, NULL AS bytes FROM user_avatar_uploads WHERE pending_delete = 0`,
      false,
    ],
  ];
  const rows = await Promise.all(
    kinds.map(async ([kind, sql, sized]) => {
      const [row] = await safeAll<{ objects: number; bytes: number | null }>(
        db.prepare(sql),
      );
      if (!row) return null;
      return {
        kind,
        objects: num(row.objects),
        bytes: sized ? num(row.bytes) : null,
      } satisfies AdminStorageKind;
    }),
  );
  return rows.filter((row): row is AdminStorageKind => row !== null);
}

async function pendingUploads(db: D1Database): Promise<AdminPendingUpload[]> {
  const rows = await safeAll<AdminPendingUpload>(
    db.prepare(
      `SELECT p.id, p.user_id,
         COALESCE(NULLIF(u.display_name, ''), u.username) AS user_name,
         p.filename, p.declared_size, p.part_count, p.status,
         p.created_at, p.last_activity_at, p.expires_at,
         COALESCE(SUM(pp.size_bytes), 0) AS received_bytes,
         COUNT(pp.part_number) AS parts_received
       FROM pending_uploads p
       LEFT JOIN users u ON u.id = p.user_id
       LEFT JOIN pending_upload_parts pp ON pp.upload_id = p.id
       GROUP BY p.id
       ORDER BY p.last_activity_at DESC
       LIMIT 100`,
    ),
  );
  return rows.map((row) => ({
    ...row,
    declared_size: num(row.declared_size),
    received_bytes: num(row.received_bytes),
    part_count: num(row.part_count),
    parts_received: num(row.parts_received),
  }));
}

async function jobBacklog(db: D1Database) {
  const [post] = await safeAll<{ pending: number; oldest: string | null }>(
    db.prepare(
      `SELECT COUNT(*) AS pending, MIN(created_at) AS oldest FROM message_postprocessing_jobs`,
    ),
  );
  const [cleanup] = await safeAll<{
    pending: number;
    last_error: string | null;
  }>(
    db.prepare(
      `SELECT COUNT(*) AS pending,
         (SELECT last_error FROM r2_cleanup_jobs WHERE last_error IS NOT NULL ORDER BY updated_at DESC LIMIT 1) AS last_error
       FROM r2_cleanup_jobs`,
    ),
  );
  return {
    postprocessing_pending: num(post?.pending),
    postprocessing_oldest: post?.oldest ?? null,
    r2_cleanup_pending: num(cleanup?.pending),
    r2_cleanup_last_error: cleanup?.last_error ?? null,
  };
}

type NameRow = { id: string; name: string | null; avatar_url: string | null };

async function namesFor(
  db: D1Database,
  ids: string[],
): Promise<Map<string, NameRow>> {
  const unique = [...new Set(ids)].filter(Boolean).slice(0, 500);
  if (unique.length === 0) return new Map();
  const rows = await safeAll<NameRow>(
    db
      .prepare(
        `SELECT id, COALESCE(NULLIF(display_name, ''), username) AS name, avatar_url
         FROM users WHERE id IN (${unique.map(() => "?").join(", ")})`,
      )
      .bind(...unique),
  );
  return new Map(rows.map((row) => [row.id, row]));
}

function onlineFrom(
  gateway: GatewayIntrospection,
  names: Map<string, NameRow>,
): AdminOnlineUser[] {
  const byUser = new Map<string, AdminOnlineUser>();
  for (const session of gateway.sessions) {
    if (!session.user_id || session.standby) continue;
    const existing = byUser.get(session.user_id);
    const name = names.get(session.user_id);
    const entry: AdminOnlineUser = existing ?? {
      user_id: session.user_id,
      name: name?.name ?? session.user_id,
      avatar_url: name?.avatar_url ?? null,
      platforms: [],
      status: session.status,
      sockets: 0,
      voice_channel_id: null,
    };
    entry.sockets += 1;
    if (session.platform && !entry.platforms.includes(session.platform)) {
      entry.platforms.push(session.platform);
    }
    entry.voice_channel_id ??= session.voice_channel_id;
    byUser.set(session.user_id, entry);
  }
  return [...byUser.values()].sort((a, b) => a.name.localeCompare(b.name));
}

async function voiceRoomsFrom(
  db: D1Database,
  gateway: GatewayIntrospection,
): Promise<AdminVoiceRoom[]> {
  const channels = gateway.voice_channels.filter(
    (room) => room.members.length > 0,
  );
  if (channels.length === 0) return [];
  const ids = channels.map((room) => room.channel_id);
  const rows = await safeAll<{
    id: string;
    name: string;
    server_id: string | null;
    server_name: string | null;
  }>(
    db
      .prepare(
        `SELECT c.id, c.name, c.server_id, s.name AS server_name
         FROM channels c LEFT JOIN servers s ON s.id = c.server_id
         WHERE c.id IN (${ids.map(() => "?").join(", ")})`,
      )
      .bind(...ids),
  );
  const byId = new Map(rows.map((row) => [row.id, row]));
  return channels
    .map((room) => {
      const channel = byId.get(room.channel_id);
      return {
        channel_id: room.channel_id,
        channel_name: channel?.name ?? null,
        server_id: channel?.server_id ?? null,
        server_name: channel?.server_name ?? null,
        kind: channel?.server_id ? ("server" as const) : ("dm" as const),
        started_at: room.started_at,
        members: room.members,
      };
    })
    .sort((a, b) => b.members.length - a.members.length);
}

async function liveSnapshot(
  deps: AdminDeps,
  now: number,
): Promise<AdminLiveSnapshot> {
  const { db } = deps;
  const [gateway, uploads, d1, storage, cron, jobs, deploy, catalog] =
    await Promise.all([
      deps.gateway.introspect().catch(() => null),
      pendingUploads(db),
      d1Overview(db).catch(() => ({ tables: [], size_bytes: null })),
      storageByKind(db),
      safeAll<AdminCronRun>(
        db.prepare(
          `SELECT name, cron, ok, duration_ms, error, ran_at FROM scheduled_job_runs ORDER BY name`,
        ),
      ),
      jobBacklog(db),
      deps.deploy?.().catch(() => null) ?? Promise.resolve(null),
      catalogStatus(db, now).catch(() => null),
    ]);
  let online: AdminOnlineUser[] | null = null;
  let voiceRooms: AdminVoiceRoom[] | null = null;
  if (gateway) {
    const names = await namesFor(
      db,
      gateway.sessions.flatMap((session) =>
        session.user_id ? [session.user_id] : [],
      ),
    );
    online = onlineFrom(gateway, names);
    voiceRooms = await voiceRoomsFrom(db, gateway);
  }
  return {
    generated_at: new Date(now).toISOString(),
    gateway: gateway
      ? {
          sockets: gateway.sockets,
          resumable_sessions: gateway.resumable_sessions,
          pending_calls: gateway.pending_calls,
          active_calls: gateway.active_calls,
        }
      : null,
    online,
    voice_rooms: voiceRooms,
    pending_uploads: uploads,
    d1,
    storage,
    deploy: deploy ?? {
      version: "unknown",
      commit: "unknown",
      built_at: null,
    },
    cron: cron.map((run) => ({
      ...run,
      ok: !!run.ok,
      duration_ms: num(run.duration_ms),
    })),
    jobs,
    catalog,
  };
}

// ── Insights ────────────────────────────────────────────────────────────────

export const INSIGHT_RANGES = [7, 30, 90] as const;

async function insights(
  db: D1Database,
  days: number,
  now: number,
): Promise<AdminInsights> {
  const today = Date.parse(`${dayOf(now)}T00:00:00Z`);
  const fromMs = today - (days - 1) * DAY_MS;
  const from = dayOf(fromMs);
  const to = dayOf(today);
  type DayRow = { day: string; value: number };
  const [
    actives,
    messages,
    voice,
    stream,
    signups,
    uploads,
    uploadBytes,
    top,
    since,
    totals,
  ] = await Promise.all([
    safeAll<DayRow>(
      db
        .prepare(
          `SELECT day, COUNT(DISTINCT user_id) AS value FROM (
               SELECT substr(created_at, 1, 10) AS day, author_id AS user_id
                 FROM messages WHERE created_at >= ? AND type IS NULL
               UNION ALL
               SELECT day, user_id FROM daily_activity WHERE day >= ?
             ) GROUP BY day`,
        )
        .bind(from, from),
    ),
    safeAll<DayRow>(
      db
        .prepare(
          `SELECT substr(created_at, 1, 10) AS day, COUNT(*) AS value
             FROM messages WHERE created_at >= ? AND type IS NULL GROUP BY day`,
        )
        .bind(from),
    ),
    safeAll<DayRow>(
      db
        .prepare(
          `SELECT day, SUM(voice_seconds) / 3600.0 AS value FROM daily_activity WHERE day >= ? GROUP BY day`,
        )
        .bind(from),
    ),
    safeAll<DayRow>(
      db
        .prepare(
          `SELECT day, SUM(stream_seconds) / 3600.0 AS value FROM daily_activity WHERE day >= ? GROUP BY day`,
        )
        .bind(from),
    ),
    safeAll<DayRow>(
      db
        .prepare(
          `SELECT substr(created_at, 1, 10) AS day, COUNT(*) AS value FROM users WHERE created_at >= ? GROUP BY day`,
        )
        .bind(from),
    ),
    safeAll<DayRow>(
      db
        .prepare(
          `SELECT substr(created_at, 1, 10) AS day, COUNT(*) AS value FROM attachments WHERE created_at >= ? GROUP BY day`,
        )
        .bind(from),
    ),
    safeAll<DayRow>(
      db
        .prepare(
          `SELECT substr(created_at, 1, 10) AS day, SUM(size_bytes) AS value FROM attachments WHERE created_at >= ? GROUP BY day`,
        )
        .bind(from),
    ),
    safeAll<AdminTopServer>(
      db
        .prepare(
          `SELECT s.id, s.name, s.icon_url, t.messages, t.active_members,
               (SELECT COUNT(*) FROM server_members sm WHERE sm.server_id = s.id) AS members
             FROM (
               SELECT c.server_id, COUNT(*) AS messages, COUNT(DISTINCT m.author_id) AS active_members
               FROM messages m JOIN channels c ON c.id = m.channel_id
               WHERE m.created_at >= ? AND m.type IS NULL AND c.server_id IS NOT NULL
               GROUP BY c.server_id
             ) t JOIN servers s ON s.id = t.server_id
             ORDER BY t.messages DESC LIMIT 8`,
        )
        .bind(from),
    ),
    safeAll<{ first: string | null }>(
      db.prepare(`SELECT MIN(day) AS first FROM daily_activity`),
    ),
    safeAll<{ people: number; servers: number; messages: number }>(
      db.prepare(
        `SELECT (SELECT COUNT(*) FROM users) AS people,
                  (SELECT COUNT(*) FROM servers) AS servers,
                  (SELECT COUNT(*) FROM messages WHERE type IS NULL) AS messages`,
      ),
    ),
  ]);
  const round1 = (points: AdminDailyPoint[]) =>
    points.map((point) => ({
      ...point,
      value: Math.round(point.value * 10) / 10,
    }));
  return {
    days,
    from,
    to,
    daily_actives: fillDays(fromMs, days, actives),
    messages: fillDays(fromMs, days, messages),
    voice_hours: round1(fillDays(fromMs, days, voice)),
    stream_hours: round1(fillDays(fromMs, days, stream)),
    signups: fillDays(fromMs, days, signups),
    uploads: fillDays(fromMs, days, uploads),
    upload_bytes: fillDays(fromMs, days, uploadBytes),
    top_servers: top.map((server) => ({
      ...server,
      messages: num(server.messages),
      active_members: num(server.active_members),
      members: num(server.members),
    })),
    rollup_since: since[0]?.first ?? null,
    totals: {
      people: num(totals[0]?.people),
      servers: num(totals[0]?.servers),
      messages: num(totals[0]?.messages),
    },
  };
}

// ── Handlers ────────────────────────────────────────────────────────────────

export type AdminParams = { userId?: string };

/** One route: the verified admin's id is the actor. */
export type AdminHandler = (
  request: Request,
  actorId: string,
  params?: AdminParams,
) => Promise<Response>;

/** The definitions the console reads to label badges (manual ones editable). */
export const ADMIN_BADGE_CATALOG = BADGES.map((badge) => ({
  id: badge.id,
  name: badge.name,
  description: badge.description,
  kind: badge.rule.kind,
}));

export function createAdminHandlers(deps: AdminDeps) {
  const now = deps.now ?? Date.now;

  /** `run` with the admin's id; a throw is a logged 500. */
  function admin(
    run: (
      actorId: string,
      request: Request,
      params: AdminParams,
    ) => Promise<Response>,
  ): AdminHandler {
    return async (request, actorId, params = {}) => {
      try {
        return await run(actorId, request, params);
      } catch (error) {
        console.error({
          level: "ERROR",
          message: "admin.route_failed",
          path: new URL(request.url).pathname,
          error: error instanceof Error ? error.message : String(error),
        });
        return fail(
          "Something went wrong on the server.",
          500,
          "INTERNAL_ERROR",
        );
      }
    };
  }

  function target(params: AdminParams): string | null {
    const id = params.userId?.trim();
    return id ? id : null;
  }

  return {
    session: admin(async (actorId) =>
      json({ user_id: actorId, badges: ADMIN_BADGE_CATALOG }),
    ),

    people: admin(async () => {
      const [people, gateway] = await Promise.all([
        listPeople(deps.db, now()),
        deps.gateway.introspect().catch(() => null),
      ]);
      const response: AdminPeopleResponse = {
        people,
        online_user_ids: gateway
          ? [
              ...new Set(
                gateway.sessions.flatMap((session) =>
                  session.user_id && !session.standby ? [session.user_id] : [],
                ),
              ),
            ]
          : null,
        generated_at: new Date(now()).toISOString(),
      };
      return json(response);
    }),

    person: admin(async (_actorId, _request, params) => {
      const userId = target(params);
      if (!userId) return fail("Say which account.", 400, "INVALID_USER");
      const detail = await personDetail(deps, userId, now());
      return detail
        ? json(detail)
        : fail("No account has that id.", 404, "USER_NOT_FOUND");
    }),

    plan: admin(async (actorId, request, params) => {
      const userId = target(params);
      if (!userId) return fail("Say which account.", 400, "INVALID_USER");
      return setPlan(
        deps,
        actorId,
        userId,
        await readJson(request),
        request,
        now(),
      );
    }),

    badge: admin(async (actorId, request, params) => {
      const body = await readJson(request);
      // POST /api/admin/badges sends the user in the body.
      const userId =
        target(params) ??
        (typeof body?.user_id === "string" ? body.user_id : null);
      if (!userId) return fail("Say which account.", 400, "INVALID_USER");
      return setBadge(deps, actorId, userId, body, request, now());
    }),

    suspension: admin(async (actorId, request, params) => {
      const userId = target(params);
      if (!userId) return fail("Say which account.", 400, "INVALID_USER");
      return setSuspension(
        deps,
        actorId,
        userId,
        await readJson(request),
        request,
        now(),
      );
    }),

    disconnect: admin(async (actorId, request, params) => {
      const userId = target(params);
      if (!userId) return fail("Say which account.", 400, "INVALID_USER");
      return disconnectUser(deps, actorId, userId, request, now());
    }),

    live: admin(async () => json(await liveSnapshot(deps, now()))),

    insights: admin(async (_actorId, request) => {
      const requested = Number(new URL(request.url).searchParams.get("days"));
      const days = (INSIGHT_RANGES as readonly number[]).includes(requested)
        ? requested
        : 30;
      return json(await insights(deps.db, days, now()));
    }),

    settings: admin(async () => json(await settingsResponse(deps.db))),

    refreshCatalog: admin(async (actorId, request) =>
      refreshCatalog(deps, actorId, request, now()),
    ),

    putSettings: admin(async (actorId, request) =>
      putSettings(deps, actorId, await readJson(request), request, now()),
    ),

    audit: admin(async (_actorId, request) => {
      const url = new URL(request.url);
      const limit = Number(url.searchParams.get("limit") ?? 50);
      return json(
        await listAudit(deps.db, {
          limit: Number.isFinite(limit) ? limit : 50,
          before: url.searchParams.get("before"),
          targetId: url.searchParams.get("user"),
        }),
      );
    }),
  };
}

export type AdminHandlers = ReturnType<typeof createAdminHandlers>;
