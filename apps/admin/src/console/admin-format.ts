/**
 * Readout formatting for the console: bytes, hours, counts and times, all
 * short enough for a 40px table row. Pure.
 */

import { createContext, useContext } from "react";

import { DEFAULT_PUBLIC_WEB_URL } from "@/lib/site-defaults";

const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** "0 B", "812 KB", "4.2 GB" (base 1024, one decimal under 10). */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes))
    return "—";
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 10 ? 0 : 1;
  return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

/** "12", "1.2k", "48k", "1.3M". */
export function formatCount(value: number): string {
  if (!Number.isFinite(value)) return "—";
  const abs = Math.abs(value);
  if (abs < 1000) return String(Math.round(value));
  if (abs < 10_000) return `${(value / 1000).toFixed(1)}k`;
  if (abs < 1_000_000) return `${Math.round(value / 1000)}k`;
  return `${(value / 1_000_000).toFixed(1)}M`;
}

/** Seconds as hours: "0 h", "0.4 h", "37 h", "1.2k h". */
export function formatHours(seconds: number): string {
  const hours = Math.max(0, seconds) / 3600;
  if (hours === 0) return "0 h";
  if (hours < 10) return `${hours.toFixed(1)} h`;
  if (hours < 1000) return `${Math.round(hours)} h`;
  return `${(hours / 1000).toFixed(1)}k h`;
}

/** D1's `datetime('now')` ("2026-11-09 12:00:00") or ISO, to epoch ms. */
export function parseTime(value: string | null | undefined): number | null {
  if (!value) return null;
  const iso = value.includes("T") ? value : `${value.replace(" ", "T")}Z`;
  const ms = Date.parse(iso);
  return Number.isFinite(ms) ? ms : null;
}

/** "now", "42 s", "5 min", "3 h", "6 d", then a date. */
export function formatAgo(
  value: string | number | null | undefined,
  now: number,
): string {
  const ms = typeof value === "number" ? value : parseTime(value);
  if (ms === null) return "—";
  const seconds = Math.round((now - ms) / 1000);
  if (seconds < 5) return "now";
  if (seconds < 60) return `${seconds} s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} d ago`;
  return formatDate(ms);
}

/** Time until `value`: "in 3 d", "in 5 h", or "ended". */
export function formatUntil(
  value: string | null | undefined,
  now: number,
): string {
  const ms = parseTime(value);
  if (ms === null) return "no end";
  const seconds = Math.round((ms - now) / 1000);
  if (seconds <= 0) return "ended";
  const hours = seconds / 3600;
  if (hours < 1) return `in ${Math.max(1, Math.round(seconds / 60))} min`;
  if (hours < 48) return `in ${Math.round(hours)} h`;
  return `in ${Math.round(hours / 24)} d`;
}

const DATE = new Intl.DateTimeFormat("en", {
  month: "short",
  day: "numeric",
  year: "numeric",
  timeZone: "UTC",
});
const DATE_TIME = new Intl.DateTimeFormat("en", {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
  timeZone: "UTC",
});
const DAY_SHORT = new Intl.DateTimeFormat("en", {
  month: "short",
  day: "numeric",
  timeZone: "UTC",
});

/** "Nov 9, 2026" (UTC). */
export function formatDate(value: string | number | null | undefined): string {
  const ms = typeof value === "number" ? value : parseTime(value);
  return ms === null ? "—" : DATE.format(ms);
}

/** "Nov 9, 14:05" (UTC). */
export function formatDateTime(
  value: string | number | null | undefined,
): string {
  const ms = typeof value === "number" ? value : parseTime(value);
  return ms === null ? "—" : `${DATE_TIME.format(ms)} UTC`;
}

/** "Nov 9" for a chart's UTC day key. */
export function formatDay(day: string): string {
  const ms = Date.parse(`${day}T00:00:00Z`);
  return Number.isFinite(ms) ? DAY_SHORT.format(ms) : day;
}

/** The first 7 characters of a commit. */
export function shortCommit(commit: string): string {
  return /^[0-9a-f]{7,40}$/i.test(commit) ? commit.slice(0, 7) : commit;
}

/**
 * The console's clock. Live, it ticks; fixtures pin it so captures never
 * drift ("5 min ago" stays 5 min).
 */
export const AdminNowContext = createContext<number | null>(null);

export function useFixedNow(): number | null {
  return useContext(AdminNowContext);
}

/**
 * An image the main app stores (avatars, server icons): absolute paths are
 * on the Yap host, not this one. A picture that will not load falls back to
 * the initial (ui/avatar).
 */
export function assetUrl(pathOrUrl: string): string {
  if (/^https?:\/\//i.test(pathOrUrl)) return pathOrUrl;
  return `${DEFAULT_PUBLIC_WEB_URL}${pathOrUrl.startsWith("/") ? "" : "/"}${pathOrUrl}`;
}
