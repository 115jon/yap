/**
 * The site-admin API's shapes (/api/admin/*), shared by the Worker
 * (services/admin.service.ts) and the console (components/admin). Pure.
 */

import type { BadgeAwardSource, BadgeId } from "@/lib/badges";
import type { Entitlement, Tier } from "@/lib/entitlements";
import type { NativeHandoffConfig } from "@/lib/native-handoff";
import type { SiteAnnouncement, SiteFlags } from "@/lib/site-flags";

// ── People ──────────────────────────────────────────────────────────────────

export interface AdminBadge {
  id: BadgeId;
  source: BadgeAwardSource | "tier";
  awarded_at: string;
}

export interface AdminPerson {
  id: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  created_at: string | null;
  /** The latest of a message, a voice session ending, or a profile save. */
  last_active_at: string | null;
  suspended_at: string | null;
  suspended_reason: string | null;
  plan: Entitlement;
  badges: AdminBadge[];
  messages_sent: number;
  voice_seconds: number;
  stream_seconds: number;
  servers_joined: number;
  servers_owned: number;
  /** Attachment bytes this account uploaded (messages and soundboard). */
  storage_bytes: number;
  uploads: number;
}

export interface AdminPeopleResponse {
  people: AdminPerson[];
  /** From the global gateway; null when it could not be asked. */
  online_user_ids: string[] | null;
  generated_at: string;
}

export interface AdminDevice {
  platform: string;
  app_variant: string | null;
  push_format: string | null;
  created_at: string;
  last_seen_at: string;
}

export interface AdminLiveSession {
  platform: string | null;
  status: string | null;
  voice_channel_id: string | null;
  standby: boolean;
}

export interface AdminServerMembership {
  id: string;
  name: string;
  icon_url: string | null;
  owner: boolean;
  members: number;
  joined_at: string | null;
}

export interface AdminPersonDetail {
  person: AdminPerson;
  servers: AdminServerMembership[];
  /** Devices registered for push (Yap's own record; Kova keeps sessions). */
  devices: AdminDevice[];
  /** Live sockets on the global gateway; null when it could not be asked. */
  live_sessions: AdminLiveSession[] | null;
  audit: AdminAuditEntry[];
}

export type AdminPlanChange =
  | { tier: "free" }
  | { tier: "loud"; expires_at: string | null }
  | { tier: "founder" };

export interface AdminPlanRequest {
  tier: Tier;
  /** Loud only: ISO time, or null for no end. */
  expires_at?: string | null;
}

export interface AdminBadgeRequest {
  badge_id: string;
  action: "award" | "revoke";
}

export interface AdminSuspensionRequest {
  suspended: boolean;
  reason?: string | null;
}

// ── Audit ───────────────────────────────────────────────────────────────────

export const ADMIN_ACTIONS = [
  "plan.set",
  "badge.award",
  "badge.revoke",
  "user.suspend",
  "user.unsuspend",
  "user.disconnect",
  "flags.update",
  "announcement.update",
  "native_handoff.update",
  "catalog.refresh",
] as const;
export type AdminAction = (typeof ADMIN_ACTIONS)[number];

export interface AdminAuditEntry {
  id: string;
  actor_id: string;
  actor_name: string | null;
  action: AdminAction | string;
  target_type: "user" | "site";
  target_id: string | null;
  target_name: string | null;
  before: unknown;
  after: unknown;
  cf_ray: string | null;
  created_at: string;
}

export interface AdminAuditResponse {
  entries: AdminAuditEntry[];
  /** Pass as `before` for the next page; null at the end. */
  next_before: string | null;
}

// ── Live ────────────────────────────────────────────────────────────────────

export interface AdminVoiceMember {
  user_id: string;
  name: string;
  avatar_url: string | null;
  connection_state: "connected" | "reconnecting";
  self_mute: boolean;
  self_deaf: boolean;
  self_video: boolean;
  self_stream: boolean;
  joined_at: number | null;
}

export interface AdminVoiceRoom {
  channel_id: string;
  channel_name: string | null;
  server_id: string | null;
  server_name: string | null;
  /** DM calls have no server. */
  kind: "server" | "dm";
  started_at: number | null;
  members: AdminVoiceMember[];
}

export interface AdminOnlineUser {
  user_id: string;
  name: string;
  avatar_url: string | null;
  platforms: string[];
  status: string | null;
  sockets: number;
  voice_channel_id: string | null;
}

export interface AdminPendingUpload {
  id: string;
  user_id: string;
  user_name: string | null;
  filename: string;
  declared_size: number;
  received_bytes: number;
  part_count: number;
  parts_received: number;
  status: "open" | "completing";
  created_at: string;
  last_activity_at: string;
  expires_at: string;
}

export interface AdminTableCount {
  name: string;
  rows: number;
}

export interface AdminStorageKind {
  kind: string;
  objects: number;
  /** Null where D1 keeps no size for the kind (avatars). */
  bytes: number | null;
}

export interface AdminCronRun {
  name: string;
  cron: string;
  ok: boolean;
  duration_ms: number;
  error: string | null;
  ran_at: string;
}

export interface AdminJobBacklog {
  /** Message embeds and notifications waiting for another try. */
  postprocessing_pending: number;
  postprocessing_oldest: string | null;
  /** R2 deletes that failed and wait for the cron. */
  r2_cleanup_pending: number;
  r2_cleanup_last_error: string | null;
}

/**
 * The collectibles shop catalog. The main Worker's hourly cron tries the
 * upstream itself; an off-Cloudflare fetcher pushes it in (ingest) because
 * upstream blocks Cloudflare.
 */
export interface AdminCatalogStatus {
  /** When the stored catalog was last refreshed; null if nothing is stored. */
  synced_at: string | null;
  items: number;
  categories: number;
  /** Older than four hours, so several refreshes in a row were missed. */
  stale: boolean;
  /** The last refresh of any kind: cron, manual, request or ingest. */
  last_attempt_at: string | null;
  last_success_at: string | null;
  last_failure_at: string | null;
  last_error: string | null;
  last_outcome: "updated" | "not_modified" | "rejected" | "failed" | null;
  last_trigger: string | null;
  consecutive_failures: number;
  /** Pushes from the off-Cloudflare fetcher only. */
  fetcher: {
    last_success_at: string | null;
    last_failure_at: string | null;
    last_error: string | null;
  };
  /** Until "Refresh now" may run again (0 = now), from generated_at. */
  retry_after_ms: number;
}

/** POST /api/catalog/refresh, when the refresh ran and stored a catalog. */
export interface AdminCatalogRefreshResponse {
  catalog: AdminCatalogStatus;
}

export interface AdminLiveSnapshot {
  generated_at: string;
  /** Null when the gateway could not be asked. */
  gateway: {
    sockets: number;
    resumable_sessions: number;
    pending_calls: number;
    active_calls: number;
  } | null;
  online: AdminOnlineUser[] | null;
  voice_rooms: AdminVoiceRoom[] | null;
  pending_uploads: AdminPendingUpload[];
  d1: { tables: AdminTableCount[]; size_bytes: number | null };
  storage: AdminStorageKind[];
  deploy: { version: string; commit: string; built_at: string | null };
  cron: AdminCronRun[];
  jobs: AdminJobBacklog;
  /** Null when it could not be read. */
  catalog: AdminCatalogStatus | null;
}

// ── Insights ────────────────────────────────────────────────────────────────

export interface AdminDailyPoint {
  /** UTC day, "2026-11-09". */
  day: string;
  value: number;
}

export interface AdminTopServer {
  id: string;
  name: string;
  icon_url: string | null;
  messages: number;
  active_members: number;
  members: number;
}

export interface AdminInsights {
  days: number;
  from: string;
  to: string;
  daily_actives: AdminDailyPoint[];
  messages: AdminDailyPoint[];
  voice_hours: AdminDailyPoint[];
  stream_hours: AdminDailyPoint[];
  signups: AdminDailyPoint[];
  uploads: AdminDailyPoint[];
  upload_bytes: AdminDailyPoint[];
  top_servers: AdminTopServer[];
  /** The rollup's first day: voice, stream and actives before it are partial. */
  rollup_since: string | null;
  totals: {
    people: number;
    servers: number;
    messages: number;
  };
}

// ── Switches ────────────────────────────────────────────────────────────────

export interface AdminSettingsResponse {
  flags: SiteFlags;
  announcement: SiteAnnouncement | null;
  /** The Tauri app's handoff to the native app (GET /api/desktop/native-handoff). */
  nativeHandoff: NativeHandoffConfig;
  updated: {
    flags: { at: string; by: string | null } | null;
    announcement: { at: string; by: string | null } | null;
    nativeHandoff: { at: string; by: string | null } | null;
  };
}

export interface AdminSettingsRequest {
  flags?: Partial<SiteFlags>;
  /** null clears the banner. */
  announcement?: {
    text: string;
    tone: SiteAnnouncement["tone"];
    link_url?: string | null;
    link_label?: string | null;
    expires_at?: string | null;
  } | null;
  /** Either field, or both; the other keeps its stored value. */
  nativeHandoff?: Partial<NativeHandoffConfig>;
}
