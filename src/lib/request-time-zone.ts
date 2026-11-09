/**
 * The sender's time zone for Night owl, taken from Cloudflare's view of the
 * request (`request.cf.timezone`, from the client IP), never from a value
 * the client chooses: a client-sent offset would make the badge farmable.
 *
 * REST routes read it from their request. The realtime gateway gets it the
 * same way: the Worker reads the upgrade request's `cf` and hands it to the
 * Durable Object in `TIME_ZONE_HEADER` (an `x-ralph-realtime-*` header, so
 * one a client sends is stripped with the admission headers), and the
 * session keeps it. Without `cf` (local dev, tests) everything reads UTC.
 */

/** Worker → Durable Object only; stripped from client requests. */
export const TIME_ZONE_HEADER = "x-ralph-realtime-time-zone";

/** Whether `value` is an IANA time zone this runtime knows. */
export function isTimeZone(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > 64) {
    return false;
  }
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: value });
    return true;
  } catch {
    return false;
  }
}

/** `request.cf.timezone`, or null when Cloudflare did not set one. */
export function cfTimeZone(request: Request | null | undefined): string | null {
  const cf = (request as { cf?: { timezone?: unknown } } | null | undefined)
    ?.cf;
  return isTimeZone(cf?.timezone) ? cf.timezone : null;
}

/**
 * Minutes east of UTC in `timeZone` at `at` (daylight saving included), or
 * null without a known zone.
 */
export function utcOffsetMinutesIn(
  timeZone: string | null | undefined,
  at: Date,
): number | null {
  if (!isTimeZone(timeZone) || Number.isNaN(at.getTime())) return null;
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone,
    hourCycle: "h23",
    year: "numeric",
    month: "numeric",
    day: "numeric",
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
  }).formatToParts(at);
  const get = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find((part) => part.type === type)?.value ?? 0);
  const wall = Date.UTC(
    get("year"),
    get("month") - 1,
    get("day"),
    get("hour"),
    get("minute"),
    get("second"),
  );
  const atSeconds = Math.floor(at.getTime() / 1000) * 1000;
  return Math.round((wall - atSeconds) / 60_000);
}
