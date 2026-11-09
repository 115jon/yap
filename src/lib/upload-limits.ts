/**
 * Upload size limits, one per kind, shared by the client pre-check and the
 * server routes that enforce them. Pure: no server-only imports, so the
 * client bundle can take it.
 *
 * A megabyte here is 1024 * 1024 bytes, the unit the server checks against,
 * so a file the client lets through is never one the server turns away.
 *
 * The routes print their limit as "max 25MB" (`serverLimitLabel`);
 * `parseServerLimitBytes` reads it back when a 413 arrives anyway (the limit
 * changed under a stale client).
 *
 * `UPLOAD_LIMITS` are the free limits; `uploadLimitFor` applies the perks.
 */

import { hasPerk, type Entitlement } from "@/lib/entitlements";

export const MB = 1024 * 1024;

export const UPLOAD_LIMITS = {
  /** POST /api/avatar-upload */
  avatar: 8 * MB,
  /** POST /api/profile-assets/manage, kind=banner */
  banner: 8 * MB,
  /** POST /api/channels/:id/messages/upload */
  attachment: 25 * MB,
  /** POST /api/channels/:id/messages/gif (the reported size) */
  gif: 25 * MB,
  /** POST /api/channels/:id/voice-status-media (vibes) */
  vibe: 25 * MB,
  /** POST /api/camera-backgrounds */
  cameraBackground: 25 * MB,
  /** POST /api/channels/:id/messages/upload, purpose=soundboard */
  soundboard: 50 * MB,
} as const;

export type UploadKind = keyof typeof UPLOAD_LIMITS;

/**
 * Limits with the `bigUploads` perk (Yap Loud, Founder), where it raises
 * one. Kinds not listed keep their `UPLOAD_LIMITS` value on every tier.
 */
export const BIG_UPLOAD_LIMITS: Partial<Record<UploadKind, number>> = {
  attachment: 500 * MB,
};

/**
 * What one attachment through the single-request Worker path may weigh, on
 * every tier. The isolate holds the multipart body while it parses it
 * (128 MB of memory in all), so this path stays at the free limit; bigger
 * files go through the chunked direct-to-R2 path (`{ direct: true }`).
 */
export const WORKER_UPLOAD_CAP = 25 * MB;

/**
 * Whether the chunked direct-to-R2 upload path is live. Until it is, the
 * client pre-check, the composer and the plan cards use the Worker limit,
 * so nothing promises more than the server takes.
 */
export const CHUNKED_UPLOADS_ENABLED = false;

/**
 * The limit for one `kind` of upload under `entitlement`, as the server
 * enforces it. Through the Worker (the default) no kind goes above
 * `max(its free limit, WORKER_UPLOAD_CAP)`; `{ direct: true }` is the
 * chunked path's limit (500 MB attachments with bigUploads).
 */
export function uploadLimitFor(
  entitlement: Entitlement | null | undefined,
  kind: UploadKind,
  { direct = false }: { direct?: boolean } = {},
): number {
  const base = UPLOAD_LIMITS[kind];
  const big = BIG_UPLOAD_LIMITS[kind];
  const limit =
    big !== undefined && hasPerk(entitlement, "bigUploads") ? big : base;
  return direct ? limit : Math.min(limit, Math.max(base, WORKER_UPLOAD_CAP));
}

/**
 * The limit the client offers for `kind`: the chunked path's once it is
 * live (`CHUNKED_UPLOADS_ENABLED`), the Worker path's until then.
 */
export function clientUploadLimitFor(
  entitlement: Entitlement | null | undefined,
  kind: UploadKind,
): number {
  return uploadLimitFor(entitlement, kind, { direct: CHUNKED_UPLOADS_ENABLED });
}

function oneDecimal(bytes: number): number {
  return Math.round((bytes / MB) * 10) / 10;
}

/** A file size: MB with one decimal ("31.2 MB"). */
export function formatUploadSize(bytes: number): string {
  return `${oneDecimal(Math.max(0, bytes)).toFixed(1)} MB`;
}

/**
 * How far over a limit a file is, as `formatUploadSize`, rounded up to the
 * next 0.1 MB so a file a few bytes over never reads "0.0 MB over".
 */
export function formatUploadOverage(size: number, limit: number): string {
  const tenths = Math.max(1, Math.ceil(((size - limit) / MB) * 10));
  return `${(tenths / 10).toFixed(1)} MB`;
}

/** A limit: whole MB with no decimal ("25 MB"); a fractional one keeps one. */
export function formatUploadLimit(bytes: number): string {
  const mb = oneDecimal(bytes);
  return Number.isInteger(mb) ? `${mb} MB` : `${mb.toFixed(1)} MB`;
}

/** The server's spelling of a limit inside its 413 message ("25MB"). */
export function serverLimitLabel(bytes: number): string {
  return formatUploadLimit(bytes).replace(" ", "");
}

/** "Avatar too large (max 8MB)": the 413 body every upload route sends. */
export function tooLargeMessage(subject: string, limitBytes: number): string {
  return `${subject} too large (max ${serverLimitLabel(limitBytes)})`;
}

/**
 * The limit a server message names, in bytes: "max 8MB", "max 8 MB",
 * "max 7.5MB" (case-insensitive). Null when it names none.
 */
export function parseServerLimitBytes(message: unknown): number | null {
  if (typeof message !== "string") return null;
  const match = /\bmax\.?\s*(\d+(?:\.\d+)?)\s*MB\b/i.exec(message);
  if (!match) return null;
  const mb = Number(match[1]);
  return Number.isFinite(mb) && mb > 0 ? Math.round(mb * MB) : null;
}

export function isOverUploadLimit(size: number, limit: number): boolean {
  return size > limit;
}

/** Split a batch into the files that fit and the ones over `limit`. */
export function partitionBySize<T extends { size: number }>(
  files: Iterable<T>,
  limit: number,
) {
  const fits: T[] = [];
  const tooBig: T[] = [];
  for (const file of files) {
    (isOverUploadLimit(file.size, limit) ? tooBig : fits).push(file);
  }
  return { fits, tooBig };
}
