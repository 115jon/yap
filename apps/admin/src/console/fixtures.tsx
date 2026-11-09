/**
 * Dev-only fixtures for the admin console (`vite --mode fixtures`, see the
 * apps/admin README; `/console.html?surface=…&theme=…`). The
 * real console, fed by an in-memory transport with a friend group's worth
 * of accounts, rooms and history, on a pinned clock so captures never
 * drift.
 *
 *   ?surface=admin-people      the people table
 *   ?surface=admin-person      the table with one account's panel open
 *   ?surface=admin-live        live ops
 *   ?surface=admin-insights    the charts
 *   ?surface=admin-switches    kill switches (two off) and the banner editor
 *   ?surface=admin-audit       the audit log
 *   ?surface=admin-palette     the command palette open over people
 *   ?surface=admin-banner      the site banner in each tone
 *
 * `&state=loading` holds every request open; `&state=error` fails them, so
 * the skeletons and error states can be captured too.
 */

import { useEffect, useMemo, useState } from "react";
import { useTheme } from "next-themes";

import { AdminConsole, type AdminSection } from "./AdminConsole";
import { AdminTransportContext, type AdminTransport } from "./admin-data";
import { AdminNowContext } from "./admin-format";
import { SiteAnnouncementBar } from "@/components/chat/SiteAnnouncementBar";
import type {
  AdminAuditEntry,
  AdminBadge,
  AdminInsights,
  AdminLiveSnapshot,
  AdminPerson,
  AdminPersonDetail,
  AdminSettingsResponse,
  AdminVoiceMember,
} from "../shared/admin-api";
import type { BadgeId } from "@/lib/badges";
import type { Entitlement } from "@/lib/entitlements";
import { DEFAULT_SITE_FLAGS } from "@/lib/site-flags";

const NOW = Date.parse("2026-11-09T12:00:00Z");
const DAY = 86_400_000;
const iso = (ms: number) => new Date(ms).toISOString();
const ADMIN_ID = "k3RcTGruSN9UCswqRpO12bDHqoPXUe3Q";

/** A small deterministic generator, so every capture draws the same data. */
function rng(seed: number) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 2 ** 32;
  };
}

type Seed = {
  id: string;
  name: string;
  username: string;
  plan: "founder" | "loud" | "free" | "loud-ending" | "lapsed";
  badges: BadgeId[];
  joinedDaysAgo: number;
  seenMinutesAgo: number;
  messages: number;
  voiceHours: number;
  streamHours: number;
  servers: number;
  mb: number;
  online?: boolean;
  suspended?: string;
};

const SEEDS: Seed[] = [
  {
    id: ADMIN_ID,
    name: "Jon",
    username: "jon",
    plan: "founder",
    badges: [
      "founder",
      "yap-team",
      "early-yapper",
      "server-owner",
      "voice-regular",
      "night-owl",
    ],
    joinedDaysAgo: 412,
    seenMinutesAgo: 0,
    messages: 18_420,
    voiceHours: 612,
    streamHours: 141,
    servers: 4,
    mb: 2_310,
    online: true,
  },
  {
    id: "u-ada",
    name: "Ada Lovelace",
    username: "ada",
    plan: "founder",
    badges: ["founder", "early-yapper", "voice-regular", "bug-hunter"],
    joinedDaysAgo: 401,
    seenMinutesAgo: 3,
    messages: 9_812,
    voiceHours: 388,
    streamHours: 64,
    servers: 3,
    mb: 914,
    online: true,
  },
  {
    id: "u-grace",
    name: "Grace Hopper",
    username: "grace",
    plan: "founder",
    badges: ["founder", "early-yapper", "server-owner"],
    joinedDaysAgo: 398,
    seenMinutesAgo: 12,
    messages: 7_204,
    voiceHours: 97,
    streamHours: 12,
    servers: 2,
    mb: 420,
    online: true,
  },
  {
    id: "u-linus",
    name: "Linus",
    username: "linus_t",
    plan: "loud",
    badges: ["loud", "early-yapper", "voice-regular"],
    joinedDaysAgo: 233,
    seenMinutesAgo: 1,
    messages: 5_120,
    voiceHours: 251,
    streamHours: 88,
    servers: 2,
    mb: 3_880,
    online: true,
  },
  {
    id: "u-margaret",
    name: "Margaret H.",
    username: "mham",
    plan: "founder",
    badges: ["founder", "early-yapper"],
    joinedDaysAgo: 390,
    seenMinutesAgo: 95,
    messages: 3_377,
    voiceHours: 44,
    streamHours: 3,
    servers: 2,
    mb: 128,
    online: false,
  },
  {
    id: "u-alan",
    name: "alan",
    username: "turing",
    plan: "loud-ending",
    badges: ["loud", "early-yapper"],
    joinedDaysAgo: 180,
    seenMinutesAgo: 540,
    messages: 2_211,
    voiceHours: 63,
    streamHours: 9,
    servers: 1,
    mb: 77,
    online: false,
  },
  {
    id: "u-katherine",
    name: "Katherine",
    username: "kj",
    plan: "founder",
    badges: ["founder", "early-yapper", "night-owl"],
    joinedDaysAgo: 377,
    seenMinutesAgo: 22,
    messages: 6_002,
    voiceHours: 131,
    streamHours: 20,
    servers: 3,
    mb: 640,
    online: true,
  },
  {
    id: "u-barbara",
    name: "Barbara Liskov",
    username: "liskov",
    plan: "free",
    badges: [],
    joinedDaysAgo: 9,
    seenMinutesAgo: 1_440,
    messages: 41,
    voiceHours: 1.4,
    streamHours: 0,
    servers: 1,
    mb: 3,
    online: false,
  },
  {
    id: "u-dennis",
    name: "dmr",
    username: "dennis",
    plan: "founder",
    badges: ["founder", "early-yapper", "voice-regular"],
    joinedDaysAgo: 366,
    seenMinutesAgo: 4,
    messages: 4_480,
    voiceHours: 166,
    streamHours: 31,
    servers: 2,
    mb: 288,
    online: true,
  },
  {
    id: "u-hedy",
    name: "Hedy",
    username: "hedy.l",
    plan: "lapsed",
    badges: ["early-yapper"],
    joinedDaysAgo: 210,
    seenMinutesAgo: 8_000,
    messages: 902,
    voiceHours: 18,
    streamHours: 0,
    servers: 1,
    mb: 12,
    online: false,
  },
  {
    id: "u-ken",
    name: "Ken",
    username: "kenthompson",
    plan: "founder",
    badges: ["founder", "early-yapper"],
    joinedDaysAgo: 360,
    seenMinutesAgo: 190,
    messages: 1_633,
    voiceHours: 72,
    streamHours: 5,
    servers: 2,
    mb: 64,
    online: false,
  },
  {
    id: "u-spam",
    name: "free crypto 💸",
    username: "crypto_giveaway_2026",
    plan: "free",
    badges: [],
    joinedDaysAgo: 2,
    seenMinutesAgo: 2_000,
    messages: 318,
    voiceHours: 0,
    streamHours: 0,
    servers: 1,
    mb: 41,
    online: false,
    suspended: "Spam in #general, invites to scam servers",
  },
  {
    id: "u-radia",
    name: "Radia Perlman",
    username: "radia",
    plan: "free",
    badges: [],
    joinedDaysAgo: 4,
    seenMinutesAgo: 30,
    messages: 12,
    voiceHours: 0.6,
    streamHours: 0,
    servers: 1,
    mb: 0.4,
    online: true,
  },
  {
    id: "u-frances",
    name: "Frances",
    username: "fran.allen",
    plan: "loud",
    badges: ["loud", "early-yapper", "server-owner"],
    joinedDaysAgo: 150,
    seenMinutesAgo: 60,
    messages: 2_901,
    voiceHours: 55,
    streamHours: 14,
    servers: 3,
    mb: 1_204,
    online: false,
  },
  {
    id: "u-john",
    name: "John McCarthy",
    username: "lisp",
    plan: "founder",
    badges: ["founder", "early-yapper"],
    joinedDaysAgo: 355,
    seenMinutesAgo: 20_000,
    messages: 208,
    voiceHours: 3,
    streamHours: 0,
    servers: 1,
    mb: 2,
    online: false,
  },
  {
    id: "u-ida",
    name: "Ida",
    username: "ida_rhodes",
    plan: "free",
    badges: [],
    joinedDaysAgo: 1,
    seenMinutesAgo: 6,
    messages: 3,
    voiceHours: 0,
    streamHours: 0,
    servers: 1,
    mb: 0,
    online: true,
  },
];

function planOf(seed: Seed): Entitlement {
  switch (seed.plan) {
    case "founder":
      return {
        tier: "founder",
        source: "grant",
        started_at: iso(NOW - 120 * DAY),
        expires_at: null,
      };
    case "loud":
      return {
        tier: "loud",
        source: "admin",
        started_at: iso(NOW - 64 * DAY),
        expires_at: iso(NOW + 23 * DAY),
      };
    case "loud-ending":
      return {
        tier: "loud",
        source: "stripe",
        started_at: iso(NOW - 31 * DAY),
        expires_at: iso(NOW + 2 * DAY),
      };
    case "lapsed":
      return {
        tier: "loud",
        source: "stripe",
        started_at: iso(NOW - 90 * DAY),
        expires_at: iso(NOW - 4 * DAY),
      };
    case "free":
      return { tier: "free", source: null, started_at: null, expires_at: null };
  }
}

function person(seed: Seed): AdminPerson {
  const badges: AdminBadge[] = seed.badges.map((id, index) => ({
    id,
    source:
      id === "loud"
        ? "tier"
        : id === "founder"
          ? "grant"
          : id === "yap-team" || id === "bug-hunter"
            ? "admin"
            : "auto",
    awarded_at: iso(NOW - (30 + index * 17) * DAY),
  }));
  return {
    id: seed.id,
    username: seed.username,
    display_name: seed.name,
    avatar_url: null,
    created_at: iso(NOW - seed.joinedDaysAgo * DAY),
    last_active_at: iso(NOW - seed.seenMinutesAgo * 60_000),
    suspended_at: seed.suspended ? iso(NOW - 26 * 3_600_000) : null,
    suspended_reason: seed.suspended ?? null,
    plan: planOf(seed),
    badges,
    messages_sent: seed.messages,
    voice_seconds: Math.round(seed.voiceHours * 3600),
    stream_seconds: Math.round(seed.streamHours * 3600),
    servers_joined: seed.servers,
    servers_owned: seed.badges.includes("server-owner") ? 1 : 0,
    storage_bytes: Math.round(seed.mb * 1024 * 1024),
    uploads: Math.round(seed.mb * 3),
  };
}

const PEOPLE = SEEDS.map(person);
const ONLINE = SEEDS.filter((seed) => seed.online && !seed.suspended).map(
  (seed) => seed.id,
);

function auditEntries(): AdminAuditEntry[] {
  const at = (minutes: number) => iso(NOW - minutes * 60_000);
  const base = {
    actor_id: ADMIN_ID,
    actor_name: "Jon",
    cf_ray: null as string | null,
  };
  return [
    {
      ...base,
      id: "a1",
      action: "flags.update",
      target_type: "site",
      target_id: "linkEmbeds",
      target_name: null,
      before: { linkEmbeds: true },
      after: { linkEmbeds: false },
      cf_ray: "8e1f2a9c4b7d03e1-SJC",
      created_at: at(18),
    },
    {
      ...base,
      id: "a2",
      action: "user.suspend",
      target_type: "user",
      target_id: "u-spam",
      target_name: "free crypto 💸",
      before: { suspended_at: null, suspended_reason: null },
      after: {
        suspended_at: at(1560),
        suspended_reason: "Spam in #general, invites to scam servers",
      },
      cf_ray: "8e1c77d0aa1e4f2b-SJC",
      created_at: at(1560),
    },
    {
      ...base,
      id: "a3",
      action: "plan.set",
      target_type: "user",
      target_id: "u-linus",
      target_name: "Linus",
      before: { tier: "free" },
      after: { tier: "loud", source: "admin", expires_at: iso(NOW + 23 * DAY) },
      cf_ray: "8e19c3b1e0f05d6a-LAX",
      created_at: at(64 * 1440),
    },
    {
      ...base,
      id: "a4",
      action: "badge.award",
      target_type: "user",
      target_id: "u-ada",
      target_name: "Ada Lovelace",
      before: { badge_id: "bug-hunter", held: false },
      after: { badge_id: "bug-hunter", held: true },
      cf_ray: "8e10a4c2d3e9f871-LAX",
      created_at: at(3 * 1440 + 200),
    },
    {
      ...base,
      id: "a5",
      action: "announcement.update",
      target_type: "site",
      target_id: "announcement",
      target_name: null,
      before: null,
      after: {
        text: "Voice moves to the new SFU tonight at 22:00 UTC. Expect a 30-second blip.",
        tone: "warning",
      },
      cf_ray: "8e0f11aa9b2c3d4e-SJC",
      created_at: at(4 * 1440),
    },
    {
      ...base,
      id: "a6",
      action: "user.disconnect",
      target_type: "user",
      target_id: "u-spam",
      target_name: "free crypto 💸",
      before: null,
      after: { closed_sockets: 2 },
      cf_ray: "8e1c77d0aa1e4f2c-SJC",
      created_at: at(1559),
    },
    {
      ...base,
      id: "a7",
      action: "plan.set",
      target_type: "user",
      target_id: "u-frances",
      target_name: "Frances",
      before: { tier: "loud", expires_at: iso(NOW - 2 * DAY) },
      after: { tier: "loud", expires_at: iso(NOW + 23 * DAY) },
      cf_ray: "8e0a9d8c7b6a5f4e-ORD",
      created_at: at(9 * 1440),
    },
    {
      ...base,
      id: "a8",
      action: "badge.revoke",
      target_type: "user",
      target_id: "u-hedy",
      target_name: "Hedy",
      before: { badge_id: "yap-team", held: true },
      after: { badge_id: "yap-team", held: false },
      cf_ray: "8e0544e3f2a1b0c9-ORD",
      created_at: at(12 * 1440),
    },
  ].sort((a, b) =>
    b.created_at.localeCompare(a.created_at),
  ) as AdminAuditEntry[];
}

function detailFor(id: string): AdminPersonDetail {
  const found = PEOPLE.find((row) => row.id === id) ?? PEOPLE[1]!;
  return {
    person: found,
    servers: [
      {
        id: "s-hq",
        name: "Ralph HQ",
        icon_url: null,
        owner: found.id === ADMIN_ID,
        members: 14,
        joined_at: iso(NOW - 400 * DAY),
      },
      {
        id: "s-games",
        name: "Game night",
        icon_url: null,
        owner: found.id === "u-grace",
        members: 9,
        joined_at: iso(NOW - 200 * DAY),
      },
      {
        id: "s-music",
        name: "Listening room",
        icon_url: null,
        owner: false,
        members: 6,
        joined_at: iso(NOW - 40 * DAY),
      },
    ].slice(0, Math.max(1, found.servers_joined)),
    devices:
      found.id === "u-ada"
        ? [
            {
              platform: "android",
              app_variant: "release",
              push_format: "native",
              created_at: iso(NOW - 80 * DAY),
              last_seen_at: iso(NOW - 3 * 60_000),
            },
            {
              platform: "windows",
              app_variant: "desktop",
              push_format: null,
              created_at: iso(NOW - 200 * DAY),
              last_seen_at: iso(NOW - 2 * DAY),
            },
          ]
        : [],
    live_sessions: ONLINE.includes(found.id)
      ? [
          {
            platform: "desktop",
            status: "online",
            voice_channel_id: "c-lounge",
            standby: false,
          },
          {
            platform: "mobile",
            status: "idle",
            voice_channel_id: null,
            standby: false,
          },
        ]
      : [],
    audit: auditEntries().filter((entry) => entry.target_id === found.id),
  };
}

function series(
  days: number,
  seed: number,
  base: number,
  spread: number,
  weekend = 1.4,
) {
  const random = rng(seed);
  return Array.from({ length: days }, (_, index) => {
    const ms = NOW - (days - 1 - index) * DAY;
    const weekday = new Date(ms).getUTCDay();
    const boost = weekday === 5 || weekday === 6 ? weekend : 1;
    const trend = 0.75 + (index / days) * 0.5;
    const value = Math.max(0, base * boost * trend + (random() - 0.5) * spread);
    return { day: iso(ms).slice(0, 10), value };
  });
}

function insights(days: number): AdminInsights {
  const round = (points: { day: string; value: number }[], digits = 0) =>
    points.map((point) => ({
      ...point,
      value: Number(point.value.toFixed(digits)),
    }));
  const rollupSince = iso(NOW - 21 * DAY).slice(0, 10);
  const gate = (points: { day: string; value: number }[]) =>
    points.map((point) =>
      point.day < rollupSince ? { ...point, value: 0 } : point,
    );
  const uploads = round(series(days, 7, 9, 10, 1.2));
  return {
    days,
    from: iso(NOW - (days - 1) * DAY).slice(0, 10),
    to: iso(NOW).slice(0, 10),
    daily_actives: round(series(days, 1, 9, 5)),
    messages: round(series(days, 2, 410, 260, 1.6)),
    voice_hours: gate(round(series(days, 3, 21, 14, 1.8), 1)),
    stream_hours: gate(round(series(days, 4, 4.5, 4, 2), 1)),
    signups: round(series(days, 5, 0.4, 1.4, 1)),
    uploads,
    upload_bytes: uploads.map((point, index) => ({
      day: point.day,
      value: Math.round(point.value * (2 + (index % 5)) * 1024 * 1024 * 3.1),
    })),
    top_servers: [
      {
        id: "s-hq",
        name: "Ralph HQ",
        icon_url: null,
        messages: 6_120,
        active_members: 12,
        members: 14,
      },
      {
        id: "s-games",
        name: "Game night",
        icon_url: null,
        messages: 3_404,
        active_members: 8,
        members: 9,
      },
      {
        id: "s-music",
        name: "Listening room",
        icon_url: null,
        messages: 1_022,
        active_members: 5,
        members: 6,
      },
      {
        id: "s-dev",
        name: "Yap dev",
        icon_url: null,
        messages: 640,
        active_members: 3,
        members: 4,
      },
      {
        id: "s-old",
        name: "Minecraft 2024",
        icon_url: null,
        messages: 58,
        active_members: 2,
        members: 11,
      },
    ],
    rollup_since: rollupSince,
    totals: { people: PEOPLE.length, servers: 7, messages: 61_480 },
  };
}

function live(): AdminLiveSnapshot {
  const member = (
    id: string,
    extra: Partial<AdminVoiceMember> = {},
  ): AdminVoiceMember => {
    const row = PEOPLE.find((p) => p.id === id)!;
    return {
      user_id: id,
      name: row.display_name ?? row.username,
      avatar_url: null,
      connection_state: "connected" as const,
      self_mute: false,
      self_deaf: false,
      self_video: false,
      self_stream: false,
      joined_at: NOW - 3_600_000,
      ...extra,
    };
  };
  return {
    generated_at: iso(NOW),
    gateway: {
      sockets: 11,
      resumable_sessions: 1,
      pending_calls: 0,
      active_calls: 1,
    },
    online: ONLINE.map((id, index) => {
      const row = PEOPLE.find((p) => p.id === id)!;
      return {
        user_id: id,
        name: row.display_name ?? row.username,
        avatar_url: null,
        platforms:
          index % 3 === 0
            ? ["desktop", "mobile"]
            : index % 3 === 1
              ? ["web"]
              : ["desktop"],
        status: index === 4 ? "idle" : "online",
        sockets: index % 3 === 0 ? 2 : 1,
        voice_channel_id: index < 4 ? "c-lounge" : null,
      };
    }),
    voice_rooms: [
      {
        channel_id: "c-lounge",
        channel_name: "Lounge",
        server_id: "s-hq",
        server_name: "Ralph HQ",
        kind: "server",
        started_at: NOW - (2 * 3600 + 14 * 60 + 9) * 1000,
        members: [
          member(ADMIN_ID, { self_stream: true }),
          member("u-ada", { self_mute: true }),
          member("u-linus", { self_video: true }),
          member("u-katherine", { connection_state: "reconnecting" }),
        ],
      },
      {
        channel_id: "c-dm",
        channel_name: "Grace Hopper",
        server_id: null,
        server_name: null,
        kind: "dm",
        started_at: NOW - 17 * 60_000,
        members: [member("u-grace"), member("u-dennis", { self_deaf: true })],
      },
    ],
    pending_uploads: [
      {
        id: "up-1",
        user_id: "u-linus",
        user_name: "Linus",
        filename: "ranked-clutch-4k.mp4",
        declared_size: 412 * 1024 * 1024,
        received_bytes: 296 * 1024 * 1024,
        part_count: 52,
        parts_received: 37,
        status: "open",
        created_at: iso(NOW - 6 * 60_000),
        last_activity_at: iso(NOW - 4_000),
        expires_at: iso(NOW + DAY),
      },
      {
        id: "up-2",
        user_id: "u-ada",
        user_name: "Ada Lovelace",
        filename: "notes.pdf",
        declared_size: 21 * 1024 * 1024,
        received_bytes: 21 * 1024 * 1024,
        part_count: 3,
        parts_received: 3,
        status: "completing",
        created_at: iso(NOW - 60_000),
        last_activity_at: iso(NOW - 1_000),
        expires_at: iso(NOW + DAY),
      },
    ],
    d1: {
      size_bytes: 187 * 1024 * 1024,
      tables: [
        ["messages", 61_480],
        ["read_states", 9_204],
        ["message_reactions", 7_811],
        ["attachments", 3_390],
        ["notifications", 2_806],
        ["daily_activity", 296],
        ["server_members", 61],
        ["channels", 48],
        ["user_badges", 54],
        ["user_stats", 16],
        ["users", 16],
        ["servers", 7],
        ["admin_audit_log", 8],
        ["site_settings", 2],
        ["pending_uploads", 2],
      ].map(([name, rows]) => ({ name: name as string, rows: rows as number })),
    },
    storage: [
      { kind: "Message attachments", objects: 3_120, bytes: 9.8 * 1024 ** 3 },
      { kind: "Soundboard sounds", objects: 212, bytes: 61 * 1024 ** 2 },
      { kind: "Voice status media", objects: 34, bytes: 118 * 1024 ** 2 },
      { kind: "Generated emoji", objects: 19, bytes: 2.4 * 1024 ** 2 },
      { kind: "Chunked parts in flight", objects: 40, bytes: 317 * 1024 ** 2 },
      { kind: "Avatars", objects: 23, bytes: null },
    ],
    deploy: {
      version: "3.14.0",
      commit: "e78451f8c2a01d6b9e3f5a7c4d2b1e0f9a8c7d6e",
      built_at: iso(NOW - 3 * 3_600_000),
    },
    cron: [
      {
        name: "Badge sweep",
        cron: "0 */6 * * *",
        ok: true,
        duration_ms: 412,
        error: null,
        ran_at: iso(NOW - 12 * 3_600_000),
      },
      {
        name: "Collectibles catalog sync",
        cron: "17 * * * *",
        ok: false,
        duration_ms: 355,
        error:
          'Yapper catalog fetch failed with 403 Forbidden: an HTML page titled "Attention Required! | Cloudflare"; the fetcher last pushed 2026-10-24T00:41:25.807Z',
        ran_at: iso(NOW - 43 * 60_000),
      },
      {
        name: "Message postprocessing retry",
        cron: "17 * * * *",
        ok: true,
        duration_ms: 96,
        error: null,
        ran_at: iso(NOW - 43 * 60_000),
      },
      {
        name: "R2 cleanup retry",
        cron: "0 */6 * * *",
        ok: false,
        duration_ms: 2_204,
        error: "R2 delete failed: 503 Service Unavailable",
        ran_at: iso(NOW - 6 * 3_600_000),
      },
      {
        name: "Stale chunked upload cleanup",
        cron: "17 * * * *",
        ok: true,
        duration_ms: 141,
        error: null,
        ran_at: iso(NOW - 43 * 60_000),
      },
      {
        name: "yt-dlp upstream sync",
        cron: "0 */6 * * *",
        ok: true,
        duration_ms: 3_902,
        error: null,
        ran_at: iso(NOW - 6 * 3_600_000),
      },
    ],
    jobs: {
      postprocessing_pending: 3,
      postprocessing_oldest: iso(NOW - 40 * 60_000),
      r2_cleanup_pending: 2,
      r2_cleanup_last_error: "R2 delete failed: 503 Service Unavailable",
    },
    catalog: {
      synced_at: iso(NOW - 16 * DAY),
      items: 1_540,
      categories: 114,
      stale: true,
      last_attempt_at: iso(NOW - 43 * 60_000),
      last_success_at: iso(NOW - 16 * DAY),
      last_failure_at: iso(NOW - 43 * 60_000),
      last_error:
        'Yapper catalog fetch failed with 403 Forbidden: an HTML page titled "Attention Required! | Cloudflare"',
      last_outcome: "failed",
      last_trigger: "cron",
      consecutive_failures: 442,
      fetcher: {
        last_success_at: iso(NOW - 16 * DAY),
        last_failure_at: null,
        last_error: null,
      },
      retry_after_ms: 0,
    },
  };
}

function settings(): AdminSettingsResponse {
  return {
    flags: { ...DEFAULT_SITE_FLAGS, linkEmbeds: false, signups: false },
    announcement: {
      id: "ann-1",
      text: "Voice moves to the new SFU tonight at 22:00 UTC. Expect a 30-second blip.",
      tone: "warning",
      link_url: "https://yap.115jon.com/status",
      link_label: "Status",
      expires_at: iso(NOW + 10 * 3_600_000),
      updated_at: iso(NOW - 4 * DAY),
    },
    updated: {
      flags: { at: iso(NOW - 18 * 60_000), by: ADMIN_ID },
      announcement: { at: iso(NOW - 4 * DAY), by: ADMIN_ID },
    },
  };
}

function fixtureTransport(state: string | null): AdminTransport {
  const delay = <T,>(value: T) =>
    state === "loading"
      ? new Promise<T>(() => {})
      : state === "error"
        ? Promise.reject(
            Object.assign(new Error("Server error"), { status: 500 }),
          )
        : Promise.resolve(value);
  return {
    get: <T,>(path: string): Promise<T> => {
      const url = new URL(path, "https://fixture.local");
      const p = url.pathname;
      if (p === "/api/users") {
        return delay({
          people: PEOPLE,
          online_user_ids: ONLINE,
          generated_at: iso(NOW),
        } as T);
      }
      if (p.startsWith("/api/users/")) {
        return delay(detailFor(decodeURIComponent(p.split("/")[4] ?? "")) as T);
      }
      if (p === "/api/live") return delay(live() as T);
      if (p === "/api/insights") {
        return delay(insights(Number(url.searchParams.get("days") ?? 30)) as T);
      }
      if (p === "/api/settings") return delay(settings() as T);
      if (p === "/api/audit")
        return delay({ entries: auditEntries(), next_before: null } as T);
      return Promise.reject(
        Object.assign(new Error("Not found"), { status: 404 }),
      );
    },
    send: <T,>() =>
      Promise.reject(
        Object.assign(new Error("Fixtures are read-only"), { status: 400 }),
      ) as Promise<T>,
  };
}

const SURFACE_SECTION: Record<string, AdminSection> = {
  "admin-people": "people",
  "admin-person": "people",
  "admin-palette": "people",
  "admin-live": "live",
  "admin-insights": "insights",
  "admin-switches": "switches",
  "admin-audit": "audit",
};

export function isAdminFixture(surface: string): boolean {
  return surface === "admin-banner" || surface in SURFACE_SECTION;
}

export function AdminFixture({
  surface,
  state,
}: {
  surface: string;
  state: string | null;
}) {
  const transport = useMemo(() => fixtureTransport(state), [state]);
  const [section, setSection] = useState<AdminSection>(
    SURFACE_SECTION[surface] ?? "people",
  );
  const [userId, setUserId] = useState<string | null>(
    surface === "admin-person" ? "u-ada" : null,
  );

  if (surface === "admin-banner") {
    return (
      <div className="flex h-full flex-col gap-6 bg-ply-0 p-6 max-md:p-3">
        {(["info", "warning", "critical"] as const).map((tone) => (
          <div
            key={tone}
            data-testid={`banner-${tone}`}
            className="overflow-hidden rounded-[2px] border border-line"
          >
            <SiteAnnouncementBar
              announcement={{
                id: tone,
                tone,
                text:
                  tone === "info"
                    ? "Yap 3.14 is out: chunked uploads up to 500 MB for Loud."
                    : tone === "warning"
                      ? "Voice moves to the new SFU tonight at 22:00 UTC. Expect a 30-second blip."
                      : "Sign-in is down while Kova restarts. Calls already running keep going.",
                link_url:
                  tone === "critical" ? null : "https://yap.115jon.com/status",
                link_label: tone === "info" ? "What's new" : "Status",
                expires_at: null,
                updated_at: iso(NOW),
              }}
              onDismiss={() => {}}
            />
          </div>
        ))}
      </div>
    );
  }

  return (
    <AdminNowContext.Provider value={NOW}>
      <AdminTransportContext.Provider value={transport}>
        <AdminConsole
          adminId={ADMIN_ID}
          section={section}
          selectedUserId={userId}
          initialPaletteOpen={surface === "admin-palette"}
          onNavigate={(next, nextUser) => {
            setSection(next);
            setUserId(nextUser);
          }}
        />
      </AdminTransportContext.Provider>
    </AdminNowContext.Provider>
  );
}

/** The fixture page: `?surface=admin-…&theme=…&state=…`. */
export function FixturesPage() {
  const params = new URLSearchParams(window.location.search);
  const surface = params.get("surface") ?? "admin-people";
  const theme = params.get("theme") ?? "dark";
  const { setTheme } = useTheme();
  const [ready, setReady] = useState(false);
  useEffect(() => {
    setTheme(theme);
    const id = window.setTimeout(() => setReady(true), 0);
    return () => window.clearTimeout(id);
  }, [setTheme, theme]);
  if (!ready) return <div className="h-dvh bg-ply-0" />;
  return (
    <div className="h-dvh w-full overflow-hidden bg-ply-0 text-ink-1 antialiased font-ui">
      <AdminFixture surface={surface} state={params.get("state")} />
    </div>
  );
}
