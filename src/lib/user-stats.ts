/**
 * Account stats arithmetic shared by the realtime gateway and its tests.
 * Pure: no D1 here (that is `services/stats.service.ts`).
 */

/** No one session counts for more than a day: a stuck socket or a clock jump
 * must not hand out Voice regular. */
export const MAX_SESSION_SECONDS = 24 * 60 * 60;

/**
 * Whole seconds from `startedAt` to `endedAt` (epoch ms), capped at
 * `MAX_SESSION_SECONDS`. Missing, reversed or non-finite times count zero.
 */
export function sessionSeconds(
  startedAt: number | null | undefined,
  endedAt: number,
  cap: number = MAX_SESSION_SECONDS,
): number {
  if (
    typeof startedAt !== "number" ||
    !Number.isFinite(startedAt) ||
    !Number.isFinite(endedAt) ||
    endedAt <= startedAt
  ) {
    return 0;
  }
  return Math.min(cap, Math.floor((endedAt - startedAt) / 1000));
}
