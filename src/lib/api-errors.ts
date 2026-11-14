import { blockErrorMessage } from "@/lib/blocks";
import { formatUploadLimit, parseServerLimitBytes } from "@/lib/upload-limits";

/**
 * User-facing copy for a failed API call.
 *
 * Server messages are written for logs ("D1_ERROR: …", "Authentication
 * required"), so they never reach the screen. Pick the copy from the HTTP
 * status instead and name what the person was trying to do:
 *
 *   describeApiError(err, "load your sessions")
 *   // "Unable to load your sessions. Check your connection and try again."
 *
 * `action` is a lowercase verb phrase that reads after "Unable to".
 *
 * A 413 is the one server message that is read: upload routes say their
 * limit ("Avatar too large (max 8MB)"), and that number is shown so the
 * person knows what will fit. `uploadLimitBytes` (the client's own limit
 * for that kind) stands in when the body names none, e.g. an edge 413.
 *
 * A 415 is shown as the server wrote it: only upload routes send one, and
 * each says which formats it takes or why a file was refused ("This image
 * format can't be checked for location data. Use JPEG, PNG, WebP or GIF.").
 *
 * So is a refusal over a block (`USER_BLOCKED`, `BLOCKED_BY_USER`): "You
 * can't message this user." is written for people.
 */
export function describeApiError(
  error: unknown,
  action: string,
  options: { uploadLimitBytes?: number } = {},
): string {
  const status = errorStatus(error);
  // A site switch is off: the server's sentence says what is paused.
  const paused = pausedMessage(error);
  if (paused) return paused;
  const refused = refusedMediaMessage(error);
  if (refused) return refused;
  // A block: the server's sentence is written for people ("You can't
  // message this user.").
  const blocked = blockErrorMessage(error);
  if (blocked) return blocked;
  if (status === 401) {
    return `Your session has ended. Sign in again to ${action}.`;
  }
  if (status === 403) {
    return `You don't have permission to ${action}.`;
  }
  if (status === 404 || status === 410) {
    return `Unable to ${action}. It may have been deleted.`;
  }
  if (status === 413) {
    const limit = serverUploadLimit(error) ?? options.uploadLimitBytes ?? null;
    return limit !== null
      ? `Unable to ${action}. The server says the limit is ${formatUploadLimit(limit)}.`
      : `Unable to ${action}. It's over the server's size limit.`;
  }
  if (status === 429) {
    return `Unable to ${action} right now. Wait a moment and try again.`;
  }
  if (status !== null && status >= 400 && status < 500) {
    return `Unable to ${action}. Check the details and try again.`;
  }
  if (status !== null && status >= 500) {
    return `Unable to ${action}. Try again in a moment.`;
  }
  return `Unable to ${action}. Check your connection and try again.`;
}

/** True for a 413 (the server refused an upload as too large). */
export function isTooLargeError(error: unknown): boolean {
  return errorStatus(error) === 413;
}

/**
 * The limit, in bytes, a 413 names in its message ("max 8MB"). Null when the
 * error is not a 413 or its message names no limit.
 */
export function serverUploadLimit(error: unknown): number | null {
  if (!isTooLargeError(error)) return null;
  return parseServerLimitBytes((error as { message?: unknown }).message);
}

function errorStatus(error: unknown): number | null {
  if (!error || typeof error !== "object") return null;
  const status = (error as { status?: unknown }).status;
  return typeof status === "number" && Number.isFinite(status) ? status : null;
}

/**
 * The server's own sentence for a feature a site admin switched off (503
 * FEATURE_PAUSED, services/site-flags.service.ts), a suspended account
 * (403 ACCOUNT_SUSPENDED) or one waiting for deletion (403
 * ACCOUNT_PENDING_DELETION). All are written for people, not logs.
 */
/** The `error` sentence of a 415's body, when it had one. */
function refusedMediaMessage(error: unknown): string | null {
  if (errorStatus(error) !== 415) return null;
  const body = (error as { body?: unknown }).body;
  const sentence =
    body && typeof body === "object"
      ? (body as { error?: unknown }).error
      : undefined;
  return typeof sentence === "string" && sentence.trim() ? sentence : null;
}

function pausedMessage(error: unknown): string | null {
  if (!error || typeof error !== "object") return null;
  const { code, message } = error as { code?: unknown; message?: unknown };
  if (
    code !== "FEATURE_PAUSED" &&
    code !== "ACCOUNT_SUSPENDED" &&
    code !== "ACCOUNT_PENDING_DELETION"
  )
    return null;
  return typeof message === "string" && message.trim() ? message : null;
}
