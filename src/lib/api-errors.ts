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
 */
export function describeApiError(
  error: unknown,
  action: string,
  options: { uploadLimitBytes?: number } = {},
): string {
  const status = errorStatus(error);
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
