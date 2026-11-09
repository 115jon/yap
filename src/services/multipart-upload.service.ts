// ── Chunked attachment uploads (R2 multipart through the Worker) ────────────
//
// A file at or over MULTIPART_THRESHOLD goes up in parts instead of one
// multipart/form-data request, so no single request nears the Worker's
// ~100 MB body limit:
//
//   POST   /api/channels/:id/uploads                       create
//   GET    /api/channels/:id/uploads/:uploadId             status (resume)
//   PUT    /api/channels/:id/uploads/:uploadId/parts/:n    one part (raw body)
//   POST   /api/channels/:id/uploads/:uploadId/complete    attachment row
//   DELETE /api/channels/:id/uploads/:uploadId             abort
//
// The server picks the R2 key and the part size; the client only ever names
// its own upload id, which every route looks up together with the caller and
// the channel, so an id is useless to anyone else and no route can write to
// a key the server did not choose. Sizes are checked three times: the
// declared size at create, each part's exact length as it streams (every
// part but the last is `part_size`), and the sum of the stored parts at
// complete, before and after R2 assembles them.
//
// Dependencies are injected (see multipart-upload-routes.ts for the real
// wiring) so the worker tests run this against miniflare's D1 and R2.

import { FREE_ENTITLEMENT } from "@/lib/entitlements";
import { genId } from "@/lib/id";
import { logger } from "@/lib/logger";
import { hasPermission, PERMISSIONS } from "@/lib/permissions";
import {
  attachmentUploadLimit,
  MAX_ATTACHMENT_BYTES,
  MULTIPART_PART_SIZE,
  tooLargeMessage,
} from "@/lib/upload-limits";
import { getEntitlement } from "@/services/entitlement.service";
import { recordR2CleanupFailure } from "@/services/r2-cleanup.service";

export type MultipartRateLimitAction = "create" | "part" | "control";

/**
 * The chunked attachment limit for `userId`, from their plan as D1 stores it
 * (500 MB with bigUploads, else 25 MB), never from anything the client
 * sends. A plan that cannot be read counts as free.
 */
export async function attachmentLimitForUser(
  db: D1Database,
  userId: string,
): Promise<number> {
  const entitlement = await getEntitlement(db, userId).catch(
    () => FREE_ENTITLEMENT,
  );
  return attachmentUploadLimit(entitlement);
}

export interface MultipartUploadDeps {
  db: D1Database;
  bucket: R2Bucket;
  /** The signed-in user, or the 401/503 to return. */
  authenticate: (request: Request) => Promise<{ userId: string } | Response>;
  /** Channel membership: the channel's server (null for a DM), or a 403. */
  channelAccess: (
    userId: string,
    channelId: string,
  ) => Promise<{ serverId: string | null } | Response>;
  /** The member's permission bits in a server, or null for a non-member. */
  serverPermissions: (
    serverId: string,
    userId: string,
  ) => Promise<number | null>;
  /** A 429 to return, or null to go on. */
  rateLimit: (
    userId: string,
    action: MultipartRateLimitAction,
  ) => Promise<Response | null>;
  /**
   * The attachment limit for this uploader, in bytes. The service clamps it
   * to MAX_ATTACHMENT_BYTES.
   */
  attachmentLimit: (userId: string) => Promise<number> | number;
  /** Bytes per part (every part but the last). Default MULTIPART_PART_SIZE. */
  partSize?: number;
  /** How long an upload may stay open. Default 24 hours. */
  ttlMs?: number;
  /**
   * Open uploads one user may hold at once. Default 5. At the cap, create
   * aborts the caller's least recently active open upload to make room.
   */
  maxOpenUploads?: number;
  now?: () => number;
}

export interface MultipartRouteParams {
  channelId: string;
  uploadId?: string;
  partNumber?: string;
}

/** R2 rejects parts under 5 MiB (except the last). */
const R2_MIN_PART_SIZE = 5 * 1024 * 1024;
/** R2's ceiling on parts per upload. */
const R2_MAX_PARTS = 10_000;
const DEFAULT_TTL_MS = 24 * 60 * 60 * 1000;
const DEFAULT_MAX_OPEN_UPLOADS = 5;
const MAX_FILENAME_LENGTH = 255;
const MAX_CONTENT_TYPE_LENGTH = 255;
const STALE_CLEANUP_BATCH = 100;
/**
 * An open upload with no create, status or part for this long is abandoned
 * (a closed tab): the sweep aborts it. A resume after a reload is well inside.
 */
export const UPLOAD_IDLE_MS = 60 * 60 * 1000;
/** Seconds a client waits when every open slot is mid-complete. */
const TOO_MANY_UPLOADS_RETRY_AFTER_S = 30;

/** `type/subtype` plus optional parameters (control characters are refused apart). */
const CONTENT_TYPE_PATTERN =
  /^[a-z0-9][a-z0-9!#$&^_.+-]*\/[a-z0-9][a-z0-9!#$&^_.+-]*(\s*;.*)?$/i;
const ID_PATTERN = /^[0-9a-f-]{36}$/i;
const PART_NUMBER_PATTERN = /^[1-9]\d{0,4}$/;

interface PendingUploadRow {
  id: string;
  user_id: string;
  channel_id: string;
  attachment_id: string;
  file_key: string;
  r2_upload_id: string;
  filename: string;
  content_type: string;
  declared_size: number;
  part_size: number;
  part_count: number;
  status: "open" | "completing";
  created_at: string;
  last_activity_at: string;
  expires_at: string;
}

interface PendingPartRow {
  part_number: number;
  etag: string;
  size_bytes: number;
}

function json(data: unknown, status = 200): Response {
  return Response.json(data, { status });
}

function error(
  message: string,
  status: number,
  code?: string,
  extra?: Record<string, unknown>,
  headers?: HeadersInit,
): Response {
  return Response.json({ error: message, code, ...extra }, { status, headers });
}

function describe(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * The name to store: path separators and control characters become "_", so
 * the key is always `attachments/<channel>/<attachment>/<one segment>`.
 * Null when nothing usable is left.
 */
export function sanitizeUploadFilename(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  // oxlint-disable-next-line no-control-regex -- stripping them is the point
  const cleaned = raw.replace(/[/\\\0-\x1f\x7f]/g, "_").trim();
  if (!cleaned || cleaned === "." || cleaned === "..") return null;
  if (cleaned.length > MAX_FILENAME_LENGTH) return null;
  return cleaned;
}

function hasControlCharacters(value: string): boolean {
  for (let i = 0; i < value.length; i += 1) {
    const code = value.charCodeAt(i);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

/**
 * The single-request route stores whatever type the browser reports, or
 * application/octet-stream when there is none. Same here, except a value
 * that is not a MIME type at all (or could split a header) also falls back.
 */
export function normalizeUploadContentType(raw: unknown): string {
  if (
    typeof raw === "string" &&
    raw.length > 0 &&
    raw.length <= MAX_CONTENT_TYPE_LENGTH &&
    CONTENT_TYPE_PATTERN.test(raw.trim()) &&
    !hasControlCharacters(raw)
  ) {
    return raw.trim();
  }
  return "application/octet-stream";
}

/** The exact byte length part `partNumber` (1-based) must have. */
export function expectedPartSize(
  partNumber: number,
  declaredSize: number,
  partSize: number,
  partCount: number,
): number {
  return partNumber < partCount
    ? partSize
    : declaredSize - (partCount - 1) * partSize;
}

export function createMultipartUploadHandlers(deps: MultipartUploadDeps) {
  const partSize = deps.partSize ?? MULTIPART_PART_SIZE;
  if (partSize < R2_MIN_PART_SIZE) {
    throw new Error(`Part size must be at least ${R2_MIN_PART_SIZE} bytes`);
  }
  const ttlMs = deps.ttlMs ?? DEFAULT_TTL_MS;
  const maxOpenUploads = deps.maxOpenUploads ?? DEFAULT_MAX_OPEN_UPLOADS;
  const now = deps.now ?? Date.now;
  const { db, bucket } = deps;

  async function effectiveLimit(userId: string): Promise<number> {
    const limit = await deps.attachmentLimit(userId);
    return Math.min(limit, MAX_ATTACHMENT_BYTES);
  }

  /** Auth, then the rate limit, then channel access: the single route's order. */
  async function gate(
    request: Request,
    channelId: string,
    action: MultipartRateLimitAction,
  ): Promise<{ userId: string; serverId: string | null } | Response> {
    const auth = await deps.authenticate(request);
    if (auth instanceof Response) return auth;
    const limited = await deps.rateLimit(auth.userId, action);
    if (limited) return limited;
    const access = await deps.channelAccess(auth.userId, channelId);
    if (access instanceof Response) return access;
    return { userId: auth.userId, serverId: access.serverId };
  }

  async function canAttach(
    serverId: string | null,
    userId: string,
  ): Promise<boolean> {
    if (!serverId) return true;
    const permissions = await deps.serverPermissions(serverId, userId);
    return (
      permissions !== null &&
      hasPermission(permissions, PERMISSIONS.ATTACH_FILES)
    );
  }

  /** The caller's own unexpired upload in this channel, or null. */
  async function loadUpload(
    uploadId: string | undefined,
    userId: string,
    channelId: string,
  ): Promise<PendingUploadRow | null> {
    if (!uploadId || !ID_PATTERN.test(uploadId)) return null;
    return db
      .prepare(
        `SELECT * FROM pending_uploads
         WHERE id = ? AND user_id = ? AND channel_id = ? AND expires_at > ?
         LIMIT 1`,
      )
      .bind(uploadId, userId, channelId, new Date(now()).toISOString())
      .first<PendingUploadRow>();
  }

  async function loadParts(uploadId: string): Promise<PendingPartRow[]> {
    const { results } = await db
      .prepare(
        `SELECT part_number, etag, size_bytes FROM pending_upload_parts
         WHERE upload_id = ? ORDER BY part_number ASC`,
      )
      .bind(uploadId)
      .all<PendingPartRow>();
    return results ?? [];
  }

  async function deleteRows(uploadId: string): Promise<void> {
    await db.batch([
      db
        .prepare("DELETE FROM pending_upload_parts WHERE upload_id = ?")
        .bind(uploadId),
      db.prepare("DELETE FROM pending_uploads WHERE id = ?").bind(uploadId),
    ]);
  }

  async function abortInR2(row: PendingUploadRow): Promise<void> {
    try {
      await bucket
        .resumeMultipartUpload(row.file_key, row.r2_upload_id)
        .abort();
    } catch (err) {
      // Already aborted or completed; R2 also drops stale multiparts itself.
      logger.warn("multipart_abort_failed", {
        uploadId: row.id,
        error: describe(err),
      });
    }
  }

  async function releaseClaim(uploadId: string): Promise<void> {
    await db
      .prepare(
        "UPDATE pending_uploads SET status = 'open' WHERE id = ? AND status = 'completing'",
      )
      .bind(uploadId)
      .run();
  }

  async function touch(uploadId: string): Promise<void> {
    await db
      .prepare("UPDATE pending_uploads SET last_activity_at = ? WHERE id = ?")
      .bind(new Date(now()).toISOString(), uploadId)
      .run();
  }

  /**
   * True when the caller is below the cap, or once room is made by aborting
   * their least recently active open upload (most likely one a closed tab
   * left behind). False only when every slot is mid-complete. Storage per
   * user stays bounded by `maxOpenUploads` either way.
   */
  async function makeRoom(userId: string, nowIso: string): Promise<boolean> {
    for (let tries = 0; tries < maxOpenUploads; tries += 1) {
      const open = await db
        .prepare(
          "SELECT COUNT(*) AS n FROM pending_uploads WHERE user_id = ? AND expires_at > ?",
        )
        .bind(userId, nowIso)
        .first<{ n: number }>();
      if (Number(open?.n ?? 0) < maxOpenUploads) return true;
      const oldest = await db
        .prepare(
          `SELECT * FROM pending_uploads
           WHERE user_id = ? AND status = 'open' AND expires_at > ?
           ORDER BY last_activity_at ASC LIMIT 1`,
        )
        .bind(userId, nowIso)
        .first<PendingUploadRow>();
      if (!oldest) return false;
      const evicted = await db
        .prepare("DELETE FROM pending_uploads WHERE id = ? AND status = 'open'")
        .bind(oldest.id)
        .run();
      if (evicted.meta.changes) {
        await db
          .prepare("DELETE FROM pending_upload_parts WHERE upload_id = ?")
          .bind(oldest.id)
          .run();
        await abortInR2(oldest);
        logger.info("multipart_evicted_oldest", {
          userId,
          uploadId: oldest.id,
          lastActivityAt: oldest.last_activity_at,
        });
      }
    }
    return false;
  }

  function notFound(): Response {
    return error("Upload not found or expired", 404, "UPLOAD_NOT_FOUND");
  }

  function busy(): Response {
    return error("Upload is already completing", 409, "UPLOAD_COMPLETING");
  }

  // ── create ────────────────────────────────────────────────────────────────
  async function create(
    request: Request,
    { channelId }: MultipartRouteParams,
  ): Promise<Response> {
    const gated = await gate(request, channelId, "create");
    if (gated instanceof Response) return gated;
    const { userId, serverId } = gated;

    if (!(await canAttach(serverId, userId))) {
      return error("You do not have permission to upload files", 403);
    }

    let body: { filename?: unknown; size?: unknown; content_type?: unknown };
    try {
      body = (await request.json()) as typeof body;
    } catch {
      return error("Expected a JSON body", 400);
    }
    if (!body || typeof body !== "object") {
      return error("Expected a JSON body", 400);
    }

    const filename = sanitizeUploadFilename(body.filename);
    if (!filename) return error("A file name is required", 400);

    const size = body.size;
    if (typeof size !== "number" || !Number.isSafeInteger(size) || size <= 0) {
      return error("size must be a positive whole number of bytes", 400);
    }

    const limit = await effectiveLimit(userId);
    if (size > limit) {
      return error(tooLargeMessage("File", limit), 413);
    }

    const partCount = Math.ceil(size / partSize);
    if (partCount > R2_MAX_PARTS) {
      return error(tooLargeMessage("File", limit), 413);
    }

    const contentType = normalizeUploadContentType(body.content_type);
    const createdAt = new Date(now()).toISOString();

    if (!(await makeRoom(userId, createdAt))) {
      return error(
        "Too many uploads in progress. Wait for one to finish.",
        429,
        "TOO_MANY_UPLOADS",
        undefined,
        { "Retry-After": String(TOO_MANY_UPLOADS_RETRY_AFTER_S) },
      );
    }

    const uploadId = genId();
    const attachmentId = genId();
    const key = `attachments/${channelId}/${attachmentId}/${filename}`;
    const expiresAt = new Date(now() + ttlMs).toISOString();

    let multipart: R2MultipartUpload;
    try {
      multipart = await bucket.createMultipartUpload(key, {
        httpMetadata: { contentType },
      });
    } catch (err) {
      logger.error("multipart_create_failed", {
        userId,
        channelId,
        error: describe(err),
      });
      return error("Couldn't start the upload", 502);
    }

    try {
      await db
        .prepare(
          `INSERT INTO pending_uploads
             (id, user_id, channel_id, attachment_id, file_key, r2_upload_id,
              filename, content_type, declared_size, part_size, part_count,
              status, created_at, last_activity_at, expires_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?, ?, ?)`,
        )
        .bind(
          uploadId,
          userId,
          channelId,
          attachmentId,
          key,
          multipart.uploadId,
          filename,
          contentType,
          size,
          partSize,
          partCount,
          createdAt,
          createdAt,
          expiresAt,
        )
        .run();
    } catch (err) {
      logger.error("multipart_row_insert_failed", {
        userId,
        channelId,
        error: describe(err),
      });
      await multipart.abort().catch(() => {});
      return error("Couldn't start the upload", 500);
    }

    return json(
      {
        upload_id: uploadId,
        part_size: partSize,
        part_count: partCount,
        size,
        content_type: contentType,
        file_name: filename,
        expires_at: expiresAt,
        max_bytes: limit,
      },
      201,
    );
  }

  // ── status ────────────────────────────────────────────────────────────────
  async function status(
    request: Request,
    { channelId, uploadId }: MultipartRouteParams,
  ): Promise<Response> {
    const gated = await gate(request, channelId, "control");
    if (gated instanceof Response) return gated;
    const row = await loadUpload(uploadId, gated.userId, channelId);
    if (!row) return notFound();
    // A resume probe counts as activity, so a reload doesn't race the sweep.
    if (row.status === "open") await touch(row.id);
    const parts = await loadParts(row.id);
    return json({
      upload_id: row.id,
      status: row.status,
      size: row.declared_size,
      part_size: row.part_size,
      part_count: row.part_count,
      content_type: row.content_type,
      file_name: row.filename,
      expires_at: row.expires_at,
      parts: parts.map((p) => ({
        part_number: p.part_number,
        size: p.size_bytes,
      })),
    });
  }

  // ── part ──────────────────────────────────────────────────────────────────
  async function part(
    request: Request,
    { channelId, uploadId, partNumber: rawPartNumber }: MultipartRouteParams,
  ): Promise<Response> {
    const gated = await gate(request, channelId, "part");
    if (gated instanceof Response) return gated;

    if (!rawPartNumber || !PART_NUMBER_PATTERN.test(rawPartNumber)) {
      return error("Invalid part number", 400, "INVALID_PART");
    }
    const partNumber = Number(rawPartNumber);

    const row = await loadUpload(uploadId, gated.userId, channelId);
    if (!row) return notFound();
    if (row.status !== "open") return busy();
    if (partNumber > row.part_count) {
      return error(
        `Part number must be between 1 and ${row.part_count}`,
        400,
        "INVALID_PART",
      );
    }

    const expected = expectedPartSize(
      partNumber,
      row.declared_size,
      row.part_size,
      row.part_count,
    );
    // Content-Length, when the request has one, refuses a wrong-sized part
    // before any byte moves. The stream check below is the one that counts.
    const lengthHeader = request.headers.get("content-length");
    if (lengthHeader !== null) {
      const length = Number(lengthHeader);
      if (!Number.isSafeInteger(length) || length !== expected) {
        return error(
          `Part ${partNumber} must be exactly ${expected} bytes`,
          length > expected ? 413 : 400,
          "PART_SIZE_MISMATCH",
        );
      }
    }
    if (!request.body) return error("Part body is empty", 400);

    // The body is streamed straight into R2. FixedLengthStream fails the
    // write when the client sends more or fewer bytes than `expected`, so
    // the size recorded below is the size R2 stored.
    let received = 0;
    let bodyEnded = false;
    const counter = new TransformStream<Uint8Array, Uint8Array>({
      transform(chunk, controller) {
        received += chunk.byteLength;
        controller.enqueue(chunk);
      },
      flush() {
        bodyEnded = true;
      },
    });
    const { readable, writable } = new FixedLengthStream(expected);
    const piped = request.body.pipeThrough(counter).pipeTo(writable);
    const multipart = bucket.resumeMultipartUpload(
      row.file_key,
      row.r2_upload_id,
    );
    const [pipeResult, uploadResult] = await Promise.allSettled([
      piped,
      multipart.uploadPart(partNumber, readable),
    ]);
    if (received > expected || (bodyEnded && received !== expected)) {
      return error(
        `Part ${partNumber} must be exactly ${expected} bytes`,
        received > expected ? 413 : 400,
        "PART_SIZE_MISMATCH",
      );
    }
    if (
      uploadResult.status === "rejected" ||
      pipeResult.status === "rejected"
    ) {
      // The connection dropped mid-part, or R2 failed: worth a retry.
      logger.error("multipart_part_failed", {
        uploadId: row.id,
        partNumber,
        received,
        error: describe(
          uploadResult.status === "rejected"
            ? uploadResult.reason
            : (pipeResult as PromiseRejectedResult).reason,
        ),
      });
      return error("Couldn't store that part. Try again.", 502);
    }

    // Only recorded while the upload is still open: an abort or a complete
    // that raced this part leaves nothing behind.
    const recorded = await db
      .prepare(
        `INSERT INTO pending_upload_parts
           (upload_id, part_number, etag, size_bytes, uploaded_at)
         SELECT ?, ?, ?, ?, ?
         WHERE EXISTS (SELECT 1 FROM pending_uploads WHERE id = ? AND status = 'open')
         ON CONFLICT(upload_id, part_number) DO UPDATE SET
           etag = excluded.etag,
           size_bytes = excluded.size_bytes,
           uploaded_at = excluded.uploaded_at`,
      )
      .bind(
        row.id,
        partNumber,
        uploadResult.value.etag,
        expected,
        new Date(now()).toISOString(),
        row.id,
      )
      .run();
    if (!recorded.meta.changes) {
      // Still there means a complete claimed it mid-part: say so, so the
      // client keeps its resume key instead of starting over.
      const still = await db
        .prepare("SELECT status FROM pending_uploads WHERE id = ?")
        .bind(row.id)
        .first<{ status: string }>();
      return still ? busy() : notFound();
    }
    await touch(row.id);

    return json({ part_number: partNumber, size: expected });
  }

  // ── complete ──────────────────────────────────────────────────────────────
  async function complete(
    request: Request,
    { channelId, uploadId }: MultipartRouteParams,
  ): Promise<Response> {
    const gated = await gate(request, channelId, "control");
    if (gated instanceof Response) return gated;
    const { userId, serverId } = gated;

    const row = await loadUpload(uploadId, userId, channelId);
    if (!row) return notFound();
    if (row.status !== "open") return busy();

    if (!(await canAttach(serverId, userId))) {
      return error("You do not have permission to upload files", 403);
    }

    // Claim it, so a second complete (or a late part) can't interleave.
    const claim = await db
      .prepare(
        "UPDATE pending_uploads SET status = 'completing' WHERE id = ? AND status = 'open'",
      )
      .bind(row.id)
      .run();
    if (!claim.meta.changes) return busy();

    const parts = await loadParts(row.id);
    const missing: number[] = [];
    const byNumber = new Map(parts.map((p) => [p.part_number, p]));
    for (let n = 1; n <= row.part_count; n += 1) {
      const stored = byNumber.get(n);
      const expected = expectedPartSize(
        n,
        row.declared_size,
        row.part_size,
        row.part_count,
      );
      if (!stored || stored.size_bytes !== expected) missing.push(n);
    }
    if (missing.length > 0 || parts.length !== row.part_count) {
      await releaseClaim(row.id);
      return error("Upload is missing parts", 400, "UPLOAD_INCOMPLETE", {
        missing,
      });
    }

    const total = parts.reduce((sum, p) => sum + p.size_bytes, 0);
    const limit = await effectiveLimit(userId);
    if (total !== row.declared_size || total > limit) {
      // The plan shrank since create, or the rows disagree: nothing to keep.
      await abortInR2(row);
      await deleteRows(row.id);
      return total > limit
        ? error(tooLargeMessage("File", limit), 413)
        : error("Upload size does not match", 400, "UPLOAD_SIZE_MISMATCH");
    }

    let object: R2Object | null;
    try {
      object = await bucket
        .resumeMultipartUpload(row.file_key, row.r2_upload_id)
        .complete(
          parts.map((p) => ({ partNumber: p.part_number, etag: p.etag })),
        );
    } catch (err) {
      logger.error("multipart_complete_failed", {
        uploadId: row.id,
        error: describe(err),
      });
      // complete() can assemble the object and still throw (a timeout, a
      // dropped answer). Look before releasing: an assembled object means
      // the parts are spent, so finish the attachment from it instead.
      try {
        object = await bucket.head(row.file_key);
      } catch (headError) {
        // Unknown either way: stay 'completing' so nothing reuses the parts;
        // the sweep deletes an orphan once the row expires.
        logger.error("multipart_complete_head_failed", {
          uploadId: row.id,
          error: describe(headError),
        });
        return error("Couldn't finish the upload. Try again.", 502);
      }
      if (!object) {
        await releaseClaim(row.id);
        return error("Couldn't finish the upload. Try again.", 502);
      }
      logger.warn("multipart_complete_recovered", { uploadId: row.id });
    }

    const failCleanup = async (reason: string) => {
      try {
        await bucket.delete(row.file_key);
      } catch (cleanupError) {
        await recordR2CleanupFailure(db, row.file_key, cleanupError).catch(
          () => {},
        );
      }
      await deleteRows(row.id);
      logger.error(reason, {
        uploadId: row.id,
        attachmentId: row.attachment_id,
      });
    };

    if (object.size !== total) {
      await failCleanup("multipart_size_mismatch_after_complete");
      return error("Upload size does not match", 500, "UPLOAD_SIZE_MISMATCH");
    }

    const createdAt = new Date(now()).toISOString();
    try {
      await db
        .prepare(
          `INSERT INTO attachments (id, message_id, channel_id, soundboard_server_id, filename, file_key, content_type, size_bytes, user_id, created_at, sound_name, sound_emoji, sound_volume)
           VALUES (?, NULL, ?, NULL, ?, ?, ?, ?, ?, ?, NULL, NULL, 1.0)`,
        )
        .bind(
          row.attachment_id,
          channelId,
          row.filename,
          row.file_key,
          row.content_type,
          total,
          userId,
          createdAt,
        )
        .run();
    } catch (err) {
      logger.error("attachment_insert_failed", {
        attachmentId: row.attachment_id,
        channelId,
        error: describe(err),
      });
      await failCleanup("multipart_attachment_insert_failed");
      return error("Failed to save uploaded file", 500);
    }

    await deleteRows(row.id);

    logger.info("file_uploaded", {
      userId,
      channelId,
      attachmentId: row.attachment_id,
      filename: row.filename,
      contentType: row.content_type,
      sizeBytes: total,
      parts: row.part_count,
      multipart: true,
    });

    return json(
      {
        id: row.attachment_id,
        file_url: `/api/${row.file_key}`,
        file_name: row.filename,
        file_size: total,
        content_type: row.content_type,
      },
      201,
    );
  }

  // ── abort ─────────────────────────────────────────────────────────────────
  async function abort(
    request: Request,
    { channelId, uploadId }: MultipartRouteParams,
  ): Promise<Response> {
    const gated = await gate(request, channelId, "control");
    if (gated instanceof Response) return gated;
    const row = await loadUpload(uploadId, gated.userId, channelId);
    if (!row) return notFound();
    const claimed = await db
      .prepare("DELETE FROM pending_uploads WHERE id = ? AND status = 'open'")
      .bind(row.id)
      .run();
    if (!claimed.meta.changes) return busy();
    await db
      .prepare("DELETE FROM pending_upload_parts WHERE upload_id = ?")
      .bind(row.id)
      .run();
    await abortInR2(row);
    return json({ aborted: true });
  }

  return { create, status, part, complete, abort };
}

/**
 * The cron sweep: aborts every upload past its expiry, and every open one
 * idle for `idleMs` (R2 multipart and rows). Any of them may have an
 * assembled object with no attachment row (a complete that died after R2
 * finished); that object is deleted too.
 */
export async function cleanupStalePendingUploads(
  db: D1Database,
  bucket: R2Bucket,
  options: { now?: number; limit?: number; idleMs?: number } = {},
): Promise<{ aborted: number }> {
  const now = options.now ?? Date.now();
  const nowIso = new Date(now).toISOString();
  const idleBefore = new Date(
    now - (options.idleMs ?? UPLOAD_IDLE_MS),
  ).toISOString();
  const { results } = await db
    .prepare(
      `SELECT id, file_key, r2_upload_id FROM pending_uploads
       WHERE expires_at <= ?
          OR (status = 'open' AND last_activity_at <= ?)
       ORDER BY last_activity_at ASC LIMIT ?`,
    )
    .bind(nowIso, idleBefore, options.limit ?? STALE_CLEANUP_BATCH)
    .all<Pick<PendingUploadRow, "id" | "file_key" | "r2_upload_id">>();

  let aborted = 0;
  for (const row of results ?? []) {
    // Claim it with the same condition, so an upload a part just touched
    // isn't swept from under its client.
    const claimed = await db
      .prepare(
        `DELETE FROM pending_uploads WHERE id = ?
           AND (expires_at <= ? OR (status = 'open' AND last_activity_at <= ?))`,
      )
      .bind(row.id, nowIso, idleBefore)
      .run();
    if (!claimed.meta.changes) continue;
    await db
      .prepare("DELETE FROM pending_upload_parts WHERE upload_id = ?")
      .bind(row.id)
      .run();
    try {
      await bucket
        .resumeMultipartUpload(row.file_key, row.r2_upload_id)
        .abort();
    } catch {
      // Already gone (completed or aborted): R2 also drops stale multiparts.
    }
    // The key is this upload's own; an object there with no attachment row
    // is an assembled file nobody can reach.
    const kept = await db
      .prepare("SELECT 1 FROM attachments WHERE file_key = ? LIMIT 1")
      .bind(row.file_key)
      .first();
    if (!kept) {
      const orphan = await bucket.head(row.file_key).catch(() => null);
      if (orphan) {
        await bucket
          .delete(row.file_key)
          .catch((err: unknown) =>
            recordR2CleanupFailure(db, row.file_key, err),
          );
      }
    }
    aborted += 1;
  }
  if (aborted > 0) logger.info("stale_uploads_aborted", { count: aborted });
  return { aborted };
}
