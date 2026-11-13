/**
 * The profile board's HTTP handlers: the board, its images, Activity, the
 * guestbook, the owner's prefs and the wishlist. Dependencies (auth, rate
 * limits, broadcast) come in as `ProfileBoardDeps`, so the worker tests run
 * these against a real D1 and R2 with stubbed auth; `profile-board-routes`
 * wires the production ones.
 *
 * Every route lives under `/api/users/:id/...`, where `:id` is a user id or
 * `me`. Owner-only routes accept either for the caller and refuse anyone
 * else's id.
 */

import type { D1Database } from "@cloudflare/workers-types";

import { hasPerk, type Entitlement } from "@/lib/entitlements";
import { genId } from "@/lib/id";
import { validateImageBuffer } from "@/lib/image-validation";
import {
  metadataResponseFields,
  stripMetadataInPlace,
  UNCHECKED_UPLOAD_CODE,
  uncheckedUploadError,
} from "@/lib/upload-metadata/strip-upload";
import {
  ACTIVITY_RETENTION_DAYS,
  isActivityKind,
  newWidgetId,
  widgetAssetPrefix,
  widgetAssetUrl,
  type ActivityKind,
  type BoardConflictBody,
} from "@/lib/profile-board";
import { isAnimatedImage } from "@/lib/profile-board-images";
import type { RateLimitOptions } from "@/lib/rate-limit";
import { ServiceError } from "@/lib/service-error";
import { tooLargeMessage, uploadLimitFor } from "@/lib/upload-limits";
import { deadlockBoardData } from "@/services/deadlock/board";
import { getEntitlement } from "@/services/entitlement.service";
import {
  listProfileActivity,
  setActivityEventHidden,
} from "@/services/profile-activity.service";
import {
  cleanupWidgetAssets,
  getBoardView,
  saveBoard,
} from "@/services/profile-board.service";
import {
  blockFromGuestbook,
  deleteGuestbookNote,
  eitherBlocked,
  listGuestbook,
  postGuestbookNote,
  unblockFromGuestbook,
} from "@/services/profile-guestbook.service";
import {
  getProfilePrefs,
  getWishlist,
  setWishlist,
  updateProfilePrefs,
} from "@/services/profile-prefs.service";

/** The limits these routes apply, per user. */
export const PROFILE_BOARD_RATE_LIMITS = {
  /** Board saves (autosave included): 30/min. */
  save: { limit: 30, windowMs: 60_000 },
  /** Widget image uploads: 10/min (the shared upload budget). */
  upload: { limit: 10, windowMs: 60_000 },
  /** Guestbook notes: 3 a minute, on top of the daily cap per owner. */
  guestbook: { limit: 3, windowMs: 60_000 },
  /** Prefs, wishlist, hiding events, guestbook moderation: 60/min. */
  write: { limit: 60, windowMs: 60_000 },
} as const satisfies Record<string, RateLimitOptions>;

export type ProfileBoardRateAction = keyof typeof PROFILE_BOARD_RATE_LIMITS;

export interface ProfileBoardDeps {
  db: D1Database;
  bucket: R2Bucket;
  authenticate(request: Request): Promise<{ userId: string } | Response>;
  rateLimit(
    userId: string,
    action: ProfileBoardRateAction,
  ): Promise<Response | null>;
  /**
   * Queues one Deadlock stats fetch (a viewed plate is old, or the owner
   * just put the widget on their board). Unset: plates refresh on the cron.
   */
  deadlockRefresh?(accountId: number): Promise<void>;
  /** Tells the owner's servers the board moved on (open profiles refetch). */
  broadcastBoardUpdate?(userId: string, version: number): Promise<void>;
  waitUntil?(promise: Promise<unknown>): void;
  now?(): number;
}

function json(data: unknown, status = 200): Response {
  return Response.json(data, { status });
}

function error(message: string, status: number, code?: string): Response {
  return Response.json({ error: message, code }, { status });
}

/** The 400 every handler sends for a body it cannot read. */
async function readJson(request: Request): Promise<unknown | Response> {
  const length = Number(request.headers.get("content-length") ?? 0);
  if (length > 64 * 1024) {
    return error("That request is too big.", 413, "BODY_TOO_LARGE");
  }
  try {
    const text = await request.text();
    if (text.length > 64 * 1024) {
      return error("That request is too big.", 413, "BODY_TOO_LARGE");
    }
    return JSON.parse(text) as unknown;
  } catch {
    return error("Invalid JSON body", 400, "INVALID_JSON");
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function fromServiceError(e: unknown): Response {
  if (e instanceof ServiceError) return error(e.message, e.status, e.code);
  throw e;
}

/** `me` or the caller's own id: the owner. Anything else is refused. */
function ownerOnly(id: string, userId: string): Response | null {
  return id === "me" || id === userId
    ? null
    : error("You can only change your own profile.", 403, "NOT_OWNER");
}

function targetId(id: string, userId: string): string {
  return id === "me" ? userId : id;
}

export function createProfileBoardHandlers(deps: ProfileBoardDeps) {
  const now = () => deps.now?.() ?? Date.now();
  const later = (promise: Promise<unknown>) => {
    const guarded = promise.catch((e: unknown) =>
      console.warn(
        `profile board background task failed: ${e instanceof Error ? e.message : String(e)}`,
      ),
    );
    if (deps.waitUntil) deps.waitUntil(guarded);
  };

  async function auth(request: Request) {
    return deps.authenticate(request);
  }

  return {
    /** GET /api/users/:id/board */
    async getBoard(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      try {
        const view = await getBoardView(deps.db, {
          ownerId: targetId(id, session.userId),
          viewerId: session.userId,
          now: now(),
          onDeadlockRefresh: (accountId) => {
            if (deps.deadlockRefresh) later(deps.deadlockRefresh(accountId));
          },
        });
        return json(view);
      } catch (e) {
        return fromServiceError(e);
      }
    },

    /** PUT /api/users/me/board — `{ board, version }` */
    async putBoard(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const refused = ownerOnly(id, session.userId);
      if (refused) return refused;
      const limited = await deps.rateLimit(session.userId, "save");
      if (limited) return limited;
      const body = await readJson(request);
      if (body instanceof Response) return body;
      const raw = asRecord(body);
      const version = Number(raw.version);
      if (!Number.isInteger(version) || version < 0) {
        return error(
          "Save needs the version you started from.",
          400,
          "BOARD_VERSION_REQUIRED",
        );
      }
      if (!raw.board || typeof raw.board !== "object") {
        return error("Nothing to save.", 400, "BOARD_REQUIRED");
      }
      let entitlement: Entitlement;
      try {
        entitlement = await getEntitlement(deps.db, session.userId);
      } catch {
        return error("Couldn't save your board. Try again.", 503);
      }
      try {
        const result = await saveBoard(deps.db, {
          ownerId: session.userId,
          board: raw.board,
          baseVersion: version,
          entitlement,
          now: now(),
        });
        if (!result.ok) {
          const conflict: BoardConflictBody = {
            error:
              "Your board changed somewhere else since you started editing.",
            code: "BOARD_VERSION_CONFLICT",
            board: result.conflict.board,
            version: result.conflict.version,
          };
          return json(conflict, 409);
        }
        const { ok: _ok, ...saved } = result;
        void _ok;
        if (deps.broadcastBoardUpdate) {
          later(deps.broadcastBoardUpdate(session.userId, saved.version));
        }
        later(
          cleanupWidgetAssets(deps.bucket, session.userId, saved.board, now()),
        );
        const deadlock = saved.board.widgets.find((w) => w.type === "deadlock");
        if (deadlock?.type === "deadlock" && deps.deadlockRefresh) {
          // A plate just added starts tracking and fetches right away.
          const refresh = deps.deadlockRefresh;
          later(
            deadlockBoardData(deps.db, {
              ownerId: session.userId,
              isOwner: true,
              range: deadlock.cfg.range,
              track: true,
              now: now(),
            }).then((view) =>
              view.refresh !== null ? refresh(view.refresh) : undefined,
            ),
          );
        }
        return json(saved);
      } catch (e) {
        return fromServiceError(e);
      }
    },

    /**
     * POST /api/users/me/board/assets — multipart `file`. PNG, JPEG, GIF or
     * WebP, checked by its bytes; the plan's `profileWidget` limit; an
     * animated image needs `animatedProfile`. Returns the asset name the
     * board stores and its URL.
     */
    async uploadAsset(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const refused = ownerOnly(id, session.userId);
      if (refused) return refused;
      const limited = await deps.rateLimit(session.userId, "upload");
      if (limited) return limited;

      const entitlement = await getEntitlement(deps.db, session.userId).catch(
        () => null,
      );
      const limit = uploadLimitFor(entitlement, "profileWidget");
      const declared = Number(request.headers.get("content-length") ?? 0);
      if (declared > limit + 64 * 1024) {
        return error(tooLargeMessage("Image", limit), 413, "TOO_LARGE");
      }
      let file: File | null = null;
      try {
        const form = await request.formData();
        const value = form.get("file");
        file = value instanceof File ? value : null;
      } catch {
        return error("Send the image as a file.", 400, "INVALID_FORM");
      }
      if (!file) return error("No file provided", 400, "NO_FILE");
      if (file.size > limit) {
        return error(tooLargeMessage("Image", limit), 413, "TOO_LARGE");
      }
      const buffer = await file.arrayBuffer();
      const checked = validateImageBuffer(buffer);
      if (!checked.ok || checked.ext === "avif") {
        return error(
          "That isn't an image we can show. Use PNG, JPEG, WebP or GIF.",
          415,
          "UNSUPPORTED_IMAGE",
        );
      }
      if (
        isAnimatedImage(buffer, checked.mimeType) &&
        !hasPerk(entitlement, "animatedProfile")
      ) {
        return error(
          "Animated images on your profile come with Yap Loud. Use a still image.",
          403,
          "ANIMATED_NEEDS_LOUD",
        );
      }
      const metadata = await stripMetadataInPlace(buffer, {
        route: "profile-board",
        userId: session.userId,
      });
      const unclean = uncheckedUploadError(metadata);
      if (unclean) return error(unclean, 415, UNCHECKED_UPLOAD_CODE);
      const asset = `${newWidgetId()}${newWidgetId()}.${checked.ext}`;
      await deps.bucket.put(
        `${widgetAssetPrefix(session.userId)}${asset}`,
        buffer,
        {
          httpMetadata: {
            contentType: checked.mimeType,
            cacheControl: "public, max-age=31536000, immutable",
          },
        },
      );
      return json(
        {
          asset,
          url: widgetAssetUrl(session.userId, asset),
          content_type: checked.mimeType,
          size: file.size,
          ...metadataResponseFields(metadata),
        },
        201,
      );
    },

    /** GET /api/users/:id/activity?cursor=&kind= */
    async getActivity(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const ownerId = targetId(id, session.userId);
      if (
        ownerId !== session.userId &&
        (await eitherBlocked(deps.db, ownerId, session.userId))
      ) {
        return error("You can't see this profile.", 403, "BLOCKED");
      }
      const url = new URL(request.url);
      const kind = url.searchParams.get("kind");
      const page = await listProfileActivity(deps.db, {
        ownerId,
        viewerId: session.userId,
        cursor: url.searchParams.get("cursor"),
        kind: isActivityKind(kind) ? kind : null,
        now: now(),
      });
      return json(page);
    },

    /** PATCH /api/users/me/activity/:eventId — `{ hidden }` */
    async patchActivityEvent(
      request: Request,
      id: string,
      eventId: string,
    ): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const refused = ownerOnly(id, session.userId);
      if (refused) return refused;
      const limited = await deps.rateLimit(session.userId, "write");
      if (limited) return limited;
      const body = await readJson(request);
      if (body instanceof Response) return body;
      const hidden = asRecord(body).hidden;
      if (typeof hidden !== "boolean") {
        return error("Say whether to hide it.", 400, "INVALID_BODY");
      }
      const changed = await setActivityEventHidden(
        deps.db,
        session.userId,
        eventId,
        hidden,
      );
      return changed
        ? json({ id: eventId, hidden })
        : error("That event isn't on your timeline.", 404, "NOT_FOUND");
    },

    /** GET /api/users/me/profile-prefs */
    async getPrefs(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const refused = ownerOnly(id, session.userId);
      if (refused) return refused;
      return json(await getProfilePrefs(deps.db, session.userId));
    },

    /**
     * PATCH /api/users/me/profile-prefs —
     * `{ activity?: { retention_days?, hidden_kinds? }, guestbook_enabled? }`
     */
    async patchPrefs(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const refused = ownerOnly(id, session.userId);
      if (refused) return refused;
      const limited = await deps.rateLimit(session.userId, "write");
      if (limited) return limited;
      const body = await readJson(request);
      if (body instanceof Response) return body;
      const raw = asRecord(body);
      const activity = asRecord(raw.activity);
      const patch: Parameters<typeof updateProfilePrefs>[2] = {};
      if (raw.activity !== undefined) {
        patch.activity = {};
        if (activity.retention_days !== undefined) {
          const days = Number(activity.retention_days);
          if (!(ACTIVITY_RETENTION_DAYS as readonly number[]).includes(days)) {
            return error(
              "Keep activity for 7, 30 or 90 days.",
              400,
              "INVALID_RETENTION",
            );
          }
          patch.activity.retention_days =
            days as (typeof ACTIVITY_RETENTION_DAYS)[number];
        }
        if (activity.hidden_kinds !== undefined) {
          if (
            !Array.isArray(activity.hidden_kinds) ||
            !activity.hidden_kinds.every(isActivityKind)
          ) {
            return error("Unknown activity kind.", 400, "INVALID_KIND");
          }
          patch.activity.hidden_kinds = activity.hidden_kinds as ActivityKind[];
        }
      }
      if (raw.guestbook_enabled !== undefined) {
        if (typeof raw.guestbook_enabled !== "boolean") {
          return error("guestbook_enabled is on or off.", 400, "INVALID_BODY");
        }
        patch.guestbook_enabled = raw.guestbook_enabled;
      }
      const prefs = await updateProfilePrefs(
        deps.db,
        session.userId,
        patch,
        new Date(now()).toISOString(),
      );
      return json(prefs);
    },

    /** GET /api/users/:id/guestbook?cursor= */
    async getGuestbook(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      try {
        const page = await listGuestbook(deps.db, {
          ownerId: targetId(id, session.userId),
          viewerId: session.userId,
          cursor: new URL(request.url).searchParams.get("cursor"),
        });
        return json(page);
      } catch (e) {
        return fromServiceError(e);
      }
    },

    /** POST /api/users/:id/guestbook — `{ body }` */
    async postGuestbook(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const limited = await deps.rateLimit(session.userId, "guestbook");
      if (limited) return limited;
      const body = await readJson(request);
      if (body instanceof Response) return body;
      try {
        const note = await postGuestbookNote(deps.db, {
          ownerId: targetId(id, session.userId),
          authorId: session.userId,
          body: asRecord(body).body,
          id: genId(),
          now: now(),
        });
        return json(note, 201);
      } catch (e) {
        return fromServiceError(e);
      }
    },

    /** DELETE /api/users/:id/guestbook/:noteId — the owner or the author */
    async deleteGuestbookNote(
      request: Request,
      id: string,
      noteId: string,
    ): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const limited = await deps.rateLimit(session.userId, "write");
      if (limited) return limited;
      const deleted = await deleteGuestbookNote(deps.db, {
        ownerId: targetId(id, session.userId),
        noteId,
        actorId: session.userId,
      });
      return deleted
        ? json({ id: noteId, deleted: true })
        : error("That note isn't there, or isn't yours to delete.", 404);
    },

    /** POST /api/users/me/guestbook/blocks — `{ user_id }` */
    async blockGuestbook(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const refused = ownerOnly(id, session.userId);
      if (refused) return refused;
      const limited = await deps.rateLimit(session.userId, "write");
      if (limited) return limited;
      const body = await readJson(request);
      if (body instanceof Response) return body;
      const userId = asRecord(body).user_id;
      if (typeof userId !== "string" || !userId) {
        return error("Who should be blocked?", 400, "INVALID_BODY");
      }
      try {
        await blockFromGuestbook(
          deps.db,
          session.userId,
          userId,
          new Date(now()).toISOString(),
        );
        return json({ user_id: userId, blocked: true });
      } catch (e) {
        return fromServiceError(e);
      }
    },

    /** DELETE /api/users/me/guestbook/blocks/:userId */
    async unblockGuestbook(
      request: Request,
      id: string,
      userId: string,
    ): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const refused = ownerOnly(id, session.userId);
      if (refused) return refused;
      const limited = await deps.rateLimit(session.userId, "write");
      if (limited) return limited;
      await unblockFromGuestbook(deps.db, session.userId, userId);
      return json({ user_id: userId, blocked: false });
    },

    /** GET /api/users/:id/wishlist */
    async getWishlist(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const ownerId = targetId(id, session.userId);
      if (
        ownerId !== session.userId &&
        (await eitherBlocked(deps.db, ownerId, session.userId))
      ) {
        return error("You can't see this profile.", 403, "BLOCKED");
      }
      return json({
        items: await getWishlist(deps.db, ownerId),
        is_owner: ownerId === session.userId,
      });
    },

    /** PUT /api/users/me/wishlist — `{ items }` */
    async putWishlist(request: Request, id: string): Promise<Response> {
      const session = await auth(request);
      if (session instanceof Response) return session;
      const refused = ownerOnly(id, session.userId);
      if (refused) return refused;
      const limited = await deps.rateLimit(session.userId, "write");
      if (limited) return limited;
      const body = await readJson(request);
      if (body instanceof Response) return body;
      const items = asRecord(body).items;
      if (!Array.isArray(items)) {
        return error("Send the list of items.", 400, "INVALID_BODY");
      }
      const stored = await setWishlist(
        deps.db,
        session.userId,
        items,
        new Date(now()).toISOString(),
      );
      return json({ items: stored, is_owner: true });
    },
  };
}

export type ProfileBoardHandlers = ReturnType<
  typeof createProfileBoardHandlers
>;
