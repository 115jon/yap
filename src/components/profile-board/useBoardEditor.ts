/**
 * The one store both board editors drive (edit in place and the Studio):
 * `boardEditorReducer` plus everything around it that touches the outside
 * world, so the two share model, undo stack and save logic.
 *
 *   - Loads from a BoardResponse (the owner's unfiltered board).
 *   - Autosaves the draft to localStorage per user (debounced, wrapped in
 *     try/catch) and restores it on reopen, with a quiet notice and a way to
 *     drop it. Cleared on save, discard or "Use theirs".
 *   - Saves with the reducer's version. A 409 keeps the draft and exposes the
 *     server's copy: `takeTheirs` loads it, `keepMine` rebases the draft onto
 *     its version and saves again. Other failures keep a message for an
 *     inline retry (a 429 says how long to wait).
 *   - Works out which widgets are drafts ("Draft · only you") from the plan.
 *
 * `useWidgetAssetUpload` is the image upload both inspectors use.
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";

import { useMyEntitlement } from "@/hooks/useEntitlement";
import { describeApiError } from "@/lib/api-errors";
import { hasPerk, type Entitlement } from "@/lib/entitlements";
import {
  boardKey,
  boardEditorReducer,
  createEditorState,
  isDirty,
  type BoardEditorAction,
  type BoardEditorState,
} from "@/lib/profile-board-editor";
import {
  canShowWidget,
  parseBoard,
  type BoardConflictBody,
  type BoardResponse,
  type ProfileBoard,
} from "@/lib/profile-board";
import {
  boardConflictOf,
  saveProfileBoard,
  uploadWidgetAsset,
} from "@/lib/profile-board-api";
import {
  clientUploadLimitFor,
  formatUploadLimit,
  formatUploadSize,
} from "@/lib/upload-limits";

// ── Local draft ────────────────────────────────────────────────────────────

const DRAFT_PREFIX = "yap:board-draft:";
const AUTOSAVE_MS = 400;

interface StoredDraft {
  /** The server version the draft was made on. */
  version: number;
  board: ProfileBoard;
  at: number;
}

export function draftStorageKey(userId: string): string {
  return `${DRAFT_PREFIX}${userId}`;
}

function readDraft(userId: string): StoredDraft | null {
  try {
    const raw = globalThis.localStorage?.getItem(draftStorageKey(userId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<StoredDraft>;
    if (!parsed || typeof parsed !== "object") return null;
    // Cleaned like a save would clean it: a corrupt or old draft can never
    // break the editor.
    const board = parseBoard(parsed.board).board;
    return {
      version: typeof parsed.version === "number" ? parsed.version : 0,
      board,
      at: typeof parsed.at === "number" ? parsed.at : 0,
    };
  } catch {
    return null;
  }
}

function writeDraft(userId: string, draft: StoredDraft) {
  try {
    globalThis.localStorage?.setItem(
      draftStorageKey(userId),
      JSON.stringify(draft),
    );
  } catch {
    // Storage full or blocked: the draft just isn't kept.
  }
}

function clearDraft(userId: string) {
  try {
    globalThis.localStorage?.removeItem(draftStorageKey(userId));
  } catch {
    // Nothing to clear.
  }
}

function initialState(
  initial: BoardResponse,
  restore: boolean,
): { state: BoardEditorState; restored: boolean } {
  const base = createEditorState(initial.board, initial.version);
  if (!restore) return { state: base, restored: false };
  const stored = readDraft(initial.user_id);
  if (!stored || boardKey(stored.board) === boardKey(initial.board)) {
    if (stored) clearDraft(initial.user_id);
    return { state: base, restored: false };
  }
  return {
    state: boardEditorReducer(base, {
      type: "load",
      board: initial.board,
      version: initial.version,
      draft: stored.board,
    }),
    restored: true,
  };
}

// ── The hook ───────────────────────────────────────────────────────────────

export type SaveStatus = "idle" | "saving" | "saved" | "error" | "conflict";

export interface UseBoardEditorOptions {
  /** Plan override (fixtures, tests); defaults to the signed-in user's. */
  entitlement?: Entitlement;
  /** Restore and autosave the local draft. Default true. */
  persist?: boolean;
}

export interface BoardEditorStore {
  state: BoardEditorState;
  dispatch(action: BoardEditorAction): void;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  /** Ids of draft widgets: the owner's plan can't show them to others. */
  drafts: string[];
  entitlement: Entitlement;
  /** The plan covers Custom widgets. */
  canCustom: boolean;
  /** The board the editors render from (data, owner). */
  response: BoardResponse;
  saveStatus: SaveStatus;
  /** Why the last save failed (not a conflict), for an inline retry. */
  saveError: string | null;
  /** The server's copy after a 409. */
  conflict: BoardConflictBody | null;
  /** Saves the draft. Resolves to the saved board, or null if it failed. */
  save(): Promise<BoardResponse | null>;
  /** After a 409: load the server's copy and drop the draft. */
  takeTheirs(): void;
  /** After a 409: put the draft on the server's version and save again. */
  keepMine(): Promise<BoardResponse | null>;
  /** Discard every unsaved change (and the local draft). */
  discard(): void;
  /** The draft was restored from this device's storage. */
  restored: boolean;
  dismissRestored(): void;
  /** Drop the restored draft: back to the saved board. */
  dropRestored(): void;
  /** Forget the local draft (the editor is closing on a clean board). */
  forgetLocalDraft(): void;
}

export function useBoardEditor(
  initial: BoardResponse,
  { entitlement: override, persist = true }: UseBoardEditorOptions = {},
): BoardEditorStore {
  const userId = initial.user_id;
  const [boot] = useState(() => initialState(initial, persist));
  const [state, dispatch] = useReducer(boardEditorReducer, boot.state);
  const [restored, setRestored] = useState(boot.restored);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<BoardConflictBody | null>(null);
  const [response, setResponse] = useState<BoardResponse>(initial);
  const mine = useMyEntitlement();
  const entitlement = override ?? mine;

  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  });
  const dirty = isDirty(state);

  // Autosave the draft, debounced; a clean board clears it.
  useEffect(() => {
    if (!persist) return;
    if (!dirty) {
      clearDraft(userId);
      return;
    }
    const id = setTimeout(() => {
      writeDraft(userId, {
        version: state.version,
        board: state.draft,
        at: Date.now(),
      });
    }, AUTOSAVE_MS);
    return () => clearTimeout(id);
  }, [persist, dirty, userId, state.draft, state.version]);

  const drafts = useMemo(
    () =>
      state.draft.widgets
        .filter((widget) => !canShowWidget(widget.type, entitlement))
        .map((widget) => widget.id),
    [state.draft, entitlement],
  );

  const busy = useRef(false);
  const saveFrom = useCallback(
    async (board: ProfileBoard, version: number) => {
      if (busy.current) return null;
      busy.current = true;
      setSaveStatus("saving");
      setSaveError(null);
      try {
        const saved = await saveProfileBoard(board, version);
        dispatch({ type: "saved", board: saved.board, version: saved.version });
        if (persist) clearDraft(userId);
        setConflict(null);
        setRestored(false);
        setSaveStatus("saved");
        const next: BoardResponse = {
          ...response,
          board: saved.board,
          version: saved.version,
          drafts: saved.drafts,
          updated_at: saved.updated_at,
        };
        setResponse(next);
        return next;
      } catch (error) {
        const server = boardConflictOf(error);
        if (server) {
          setConflict(server);
          setSaveStatus("conflict");
        } else {
          setSaveError(saveErrorMessage(error));
          setSaveStatus("error");
        }
        return null;
      } finally {
        busy.current = false;
      }
    },
    [persist, response, userId],
  );

  const save = useCallback(() => {
    const current = stateRef.current;
    return saveFrom(current.draft, current.version);
  }, [saveFrom]);

  const takeTheirs = useCallback(() => {
    if (!conflict) return;
    dispatch({
      type: "load",
      board: conflict.board,
      version: conflict.version,
    });
    if (persist) clearDraft(userId);
    setResponse((prev) => ({
      ...prev,
      board: conflict.board,
      version: conflict.version,
    }));
    setConflict(null);
    setSaveStatus("idle");
  }, [conflict, persist, userId]);

  const keepMine = useCallback(() => {
    if (!conflict) return Promise.resolve(null);
    const draft = stateRef.current.draft;
    dispatch({
      type: "load",
      board: conflict.board,
      version: conflict.version,
      draft,
    });
    setConflict(null);
    return saveFrom(draft, conflict.version);
  }, [conflict, saveFrom]);

  const discard = useCallback(() => {
    dispatch({ type: "discard" });
    if (persist) clearDraft(userId);
    setRestored(false);
    setSaveError(null);
    setSaveStatus("idle");
  }, [persist, userId]);

  const forgetLocalDraft = useCallback(() => {
    if (persist) clearDraft(userId);
  }, [persist, userId]);

  return {
    state,
    dispatch,
    dirty,
    canUndo: state.past.length > 0,
    canRedo: state.future.length > 0,
    drafts,
    entitlement,
    canCustom: hasPerk(entitlement, "customWidgets"),
    response,
    saveStatus,
    saveError,
    conflict,
    save,
    takeTheirs,
    keepMine,
    discard,
    restored,
    dismissRestored: () => setRestored(false),
    dropRestored: discard,
    forgetLocalDraft,
  };
}

function saveErrorMessage(error: unknown): string {
  const status = (error as { status?: number } | null)?.status;
  if (status === 429) {
    const wait = (error as { retryAfterMs?: number }).retryAfterMs;
    const seconds = typeof wait === "number" ? Math.ceil(wait / 1000) : null;
    return seconds
      ? `That's a lot of saves. Try again in ${seconds} ${seconds === 1 ? "second" : "seconds"}.`
      : "That's a lot of saves. Wait a moment and try again.";
  }
  return describeApiError(error, "save your board");
}

// ── Image uploads ──────────────────────────────────────────────────────────

const IMAGE_TYPES = ["image/png", "image/jpeg", "image/webp", "image/gif"];

export interface WidgetAssetUpload {
  /** Uploads `file`; resolves to the asset name, or null with `error` set. */
  upload(file: File): Promise<string | null>;
  busy: boolean;
  error: string | null;
  clearError(): void;
  /** The plan's limit, as text ("8 MB"). */
  limitText: string;
  /** Animated images play (the plan has animatedProfile). */
  animated: boolean;
}

export function useWidgetAssetUpload(
  entitlement: Entitlement,
): WidgetAssetUpload {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const limit = clientUploadLimitFor(entitlement, "profileWidget");
  const limitText = formatUploadLimit(limit);

  const upload = useCallback(
    async (file: File) => {
      setError(null);
      if (file.type && !IMAGE_TYPES.includes(file.type)) {
        setError(
          "That isn't an image we can show. Use PNG, JPEG, WebP or GIF.",
        );
        return null;
      }
      if (file.size > limit) {
        setError(
          `That image is ${formatUploadSize(file.size)}. Your plan takes up to ${limitText}.`,
        );
        return null;
      }
      setBusy(true);
      try {
        const result = await uploadWidgetAsset(file);
        return result.asset;
      } catch (err) {
        const status = (err as { status?: number } | null)?.status;
        const message = (err as { message?: unknown } | null)?.message;
        // These routes write their refusals for people (animated needs
        // Loud, too large with the limit, not an image).
        if (
          (status === 403 || status === 413 || status === 415) &&
          typeof message === "string" &&
          message
        ) {
          setError(message);
        } else {
          setError(
            describeApiError(err, "upload that image", {
              uploadLimitBytes: limit,
            }),
          );
        }
        return null;
      } finally {
        setBusy(false);
      }
    },
    [limit, limitText],
  );

  return {
    upload,
    busy,
    error,
    clearError: () => setError(null),
    limitText,
    animated: hasPerk(entitlement, "animatedProfile"),
  };
}
