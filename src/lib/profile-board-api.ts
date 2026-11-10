/**
 * The profile board's client calls, one per route (see
 * services/profile-board-handlers.ts). Typed with the shared model from
 * `profile-board.ts`. Errors are the api-client's: `status`, `code`,
 * `retryAfterMs`, and `body` (the 409 conflict carries the server's board).
 */

import {
  apiDelete,
  apiGet,
  apiPatch,
  apiPost,
  apiPut,
  apiUpload,
} from "@/lib/api-client";
import type {
  ActivityKind,
  ActivityPage,
  ActivitySettings,
  BoardConflictBody,
  BoardResponse,
  BoardSaveResponse,
  GuestbookDenial,
  GuestbookNote,
  ProfileBoard,
  ProfilePrefs,
  WishlistItem,
} from "@/lib/profile-board";
import type { BoardUserRef } from "@/lib/profile-board";

const base = (userId: string) => `/api/users/${encodeURIComponent(userId)}`;

export function fetchProfileBoard(userId: string, signal?: AbortSignal) {
  return apiGet<BoardResponse>(`${base(userId)}/board`, { signal });
}

export function saveProfileBoard(board: ProfileBoard, version: number) {
  return apiPut<BoardSaveResponse>(`${base("me")}/board`, { board, version });
}

/** The server's copy from a 409 save, or null for any other error. */
export function boardConflictOf(error: unknown): BoardConflictBody | null {
  const body = (error as { status?: number; body?: unknown } | null)?.body;
  const status = (error as { status?: number } | null)?.status;
  if (status !== 409 || !body || typeof body !== "object") return null;
  const conflict = body as Partial<BoardConflictBody>;
  return conflict.code === "BOARD_VERSION_CONFLICT" &&
    conflict.board &&
    typeof conflict.version === "number"
    ? (conflict as BoardConflictBody)
    : null;
}

export interface WidgetAssetUpload {
  asset: string;
  url: string;
  content_type: string;
  size: number;
}

export function uploadWidgetAsset(file: File) {
  const form = new FormData();
  form.append("file", file);
  return apiUpload<WidgetAssetUpload>(`${base("me")}/board/assets`, form);
}

export function fetchProfileActivity(
  userId: string,
  options: {
    cursor?: string | null;
    kind?: ActivityKind | null;
    signal?: AbortSignal;
  } = {},
) {
  const params = new URLSearchParams();
  if (options.cursor) params.set("cursor", options.cursor);
  if (options.kind) params.set("kind", options.kind);
  const query = params.toString();
  return apiGet<ActivityPage>(
    `${base(userId)}/activity${query ? `?${query}` : ""}`,
    { signal: options.signal },
  );
}

export function setActivityEventHidden(eventId: string, hidden: boolean) {
  return apiPatch<{ id: string; hidden: boolean }>(
    `${base("me")}/activity/${encodeURIComponent(eventId)}`,
    { hidden },
  );
}

export function fetchProfilePrefs() {
  return apiGet<ProfilePrefs>(`${base("me")}/profile-prefs`);
}

export function updateProfilePrefs(patch: {
  activity?: Partial<ActivitySettings>;
  guestbook_enabled?: boolean;
}) {
  return apiPatch<ProfilePrefs>(`${base("me")}/profile-prefs`, patch);
}

export interface GuestbookPage {
  notes: GuestbookNote[];
  total: number;
  next_cursor: string | null;
  enabled: boolean;
  can_post: boolean;
  denial?: GuestbookDenial | null;
  blocked?: BoardUserRef[];
}

export function fetchGuestbook(userId: string, cursor?: string | null) {
  return apiGet<GuestbookPage>(
    `${base(userId)}/guestbook${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ""}`,
  );
}

export function postGuestbookNote(userId: string, body: string) {
  return apiPost<GuestbookNote>(`${base(userId)}/guestbook`, { body });
}

export function deleteGuestbookNote(ownerId: string, noteId: string) {
  return apiDelete<{ id: string; deleted: true }>(
    `${base(ownerId)}/guestbook/${encodeURIComponent(noteId)}`,
  );
}

export function blockFromGuestbook(userId: string) {
  return apiPost<{ user_id: string; blocked: true }>(
    `${base("me")}/guestbook/blocks`,
    { user_id: userId },
  );
}

export function unblockFromGuestbook(userId: string) {
  return apiDelete<{ user_id: string; blocked: false }>(
    `${base("me")}/guestbook/blocks/${encodeURIComponent(userId)}`,
  );
}

export function fetchWishlist(userId: string) {
  return apiGet<{ items: WishlistItem[]; is_owner: boolean }>(
    `${base(userId)}/wishlist`,
  );
}

export function saveWishlist(items: WishlistItem[]) {
  return apiPut<{ items: WishlistItem[]; is_owner: true }>(
    `${base("me")}/wishlist`,
    { items },
  );
}
