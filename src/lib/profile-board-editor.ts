/**
 * The board editor's model: one pure reducer that both editors (edit in
 * place on the profile, and the Studio page) drive, so they share undo,
 * limits and keyboard semantics. No React and no DOM, so the native port
 * can mirror it action for action.
 *
 * History holds whole boards (a board is under 8 KB): every change pushes
 * the previous draft, except a run of edits to the same text field
 * (`coalesce`), which is one undo step, as in a text box.
 */

import {
  createWidget,
  MAX_WIDGETS,
  parseWidgetConfig,
  remainingOfType,
  WIDGET_SPECS,
  WIDGET_SIZES,
  type AnyProfileWidget,
  type ProfileBoard,
  type WidgetConfigMap,
  type WidgetSize,
  type WidgetType,
} from "@/lib/profile-board";

export const PREVIEW_AUDIENCES = ["me", "friend", "anyone"] as const;
/**
 * Who the editor previews as. Me: the owner (drafts and owner controls).
 * Friend: someone who can sign the guestbook. Anyone: someone who can open
 * the profile but not post (drafts hidden either way).
 */
export type PreviewAudience = (typeof PREVIEW_AUDIENCES)[number];

export const MAX_HISTORY = 100;

export interface BoardEditorState {
  /** The board as the server last confirmed it. */
  saved: ProfileBoard;
  /** The version `saved` came at; a save sends it. */
  version: number;
  draft: ProfileBoard;
  past: ProfileBoard[];
  future: ProfileBoard[];
  /** The selected widget (toolbar, inspector). */
  selectedId: string | null;
  /** The widget picked up with Space, moved with the arrow keys. */
  grabbedId: string | null;
  preview: PreviewAudience;
  /** The last coalescing key, so a typing run is one undo step. */
  lastCoalesce: string | null;
}

export type BoardEditorAction =
  | { type: "load"; board: ProfileBoard; version: number; draft?: ProfileBoard }
  | { type: "add"; widget: WidgetType; index?: number }
  | { type: "remove"; id: string }
  | { type: "duplicate"; id: string }
  | { type: "move"; id: string; index: number }
  | { type: "moveBy"; id: string; delta: number }
  | { type: "resize"; id: string; size: WidgetSize }
  | { type: "cycleSize"; id: string; direction: 1 | -1 }
  | {
      type: "updateCfg";
      id: string;
      cfg: Partial<WidgetConfigMap[WidgetType]>;
      /** Edits with the same key in a row are one undo step. */
      coalesce?: string;
    }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "discard" }
  | { type: "saved"; board: ProfileBoard; version: number }
  | { type: "select"; id: string | null }
  | { type: "grab"; id: string | null }
  | { type: "preview"; audience: PreviewAudience };

export function createEditorState(
  board: ProfileBoard,
  version: number,
): BoardEditorState {
  return {
    saved: board,
    version,
    draft: board,
    past: [],
    future: [],
    selectedId: null,
    grabbedId: null,
    preview: "me",
    lastCoalesce: null,
  };
}

/** Whether the draft differs from what the server has. */
export function isDirty(state: BoardEditorState): boolean {
  return boardKey(state.draft) !== boardKey(state.saved);
}

/** A stable string for comparing boards. */
export function boardKey(board: ProfileBoard): string {
  return JSON.stringify(board.widgets);
}

export function canAdd(board: ProfileBoard, type: WidgetType): boolean {
  return remainingOfType(board, type) > 0;
}

export function isBoardFull(board: ProfileBoard): boolean {
  return board.widgets.length >= MAX_WIDGETS;
}

function clampIndex(index: number, length: number): number {
  return Math.max(0, Math.min(length, Math.trunc(index)));
}

function commit(
  state: BoardEditorState,
  widgets: AnyProfileWidget[],
  extra: Partial<BoardEditorState> = {},
  coalesce: string | null = null,
): BoardEditorState {
  const draft: ProfileBoard = { v: 1, widgets };
  if (boardKey(draft) === boardKey(state.draft)) {
    return { ...state, ...extra };
  }
  const continuing = coalesce !== null && coalesce === state.lastCoalesce;
  const past = continuing
    ? state.past
    : [...state.past, state.draft].slice(-MAX_HISTORY);
  return {
    ...state,
    ...extra,
    draft,
    past,
    future: [],
    lastCoalesce: coalesce,
  };
}

function indexOf(state: BoardEditorState, id: string): number {
  return state.draft.widgets.findIndex((widget) => widget.id === id);
}

export function boardEditorReducer(
  state: BoardEditorState,
  action: BoardEditorAction,
): BoardEditorState {
  const widgets = state.draft.widgets;
  switch (action.type) {
    case "load": {
      const next = createEditorState(action.board, action.version);
      return action.draft
        ? { ...next, draft: action.draft, preview: state.preview }
        : { ...next, preview: state.preview };
    }
    case "add": {
      if (!canAdd(state.draft, action.widget)) return state;
      const widget = createWidget(action.widget) as AnyProfileWidget;
      const index = clampIndex(action.index ?? widgets.length, widgets.length);
      const next = [...widgets];
      next.splice(index, 0, widget);
      return commit(state, next, { selectedId: widget.id, grabbedId: null });
    }
    case "remove": {
      const index = indexOf(state, action.id);
      if (index < 0) return state;
      const next = widgets.filter((widget) => widget.id !== action.id);
      // Selection moves to the neighbour, so Delete can be pressed again.
      const neighbour = next[Math.min(index, next.length - 1)]?.id ?? null;
      return commit(state, next, {
        selectedId:
          state.selectedId === action.id ? neighbour : state.selectedId,
        grabbedId: state.grabbedId === action.id ? null : state.grabbedId,
      });
    }
    case "duplicate": {
      const index = indexOf(state, action.id);
      if (index < 0) return state;
      const source = widgets[index];
      if (!canAdd(state.draft, source.type)) return state;
      const copy = {
        ...createWidget(source.type, source.size),
        cfg: structuredClone(source.cfg),
      } as AnyProfileWidget;
      const next = [...widgets];
      next.splice(index + 1, 0, copy);
      return commit(state, next, { selectedId: copy.id });
    }
    case "move": {
      const from = indexOf(state, action.id);
      if (from < 0) return state;
      const next = [...widgets];
      const [widget] = next.splice(from, 1);
      next.splice(clampIndex(action.index, next.length), 0, widget);
      return commit(state, next);
    }
    case "moveBy": {
      const from = indexOf(state, action.id);
      if (from < 0) return state;
      return boardEditorReducer(state, {
        type: "move",
        id: action.id,
        index: from + action.delta,
      });
    }
    case "resize": {
      const index = indexOf(state, action.id);
      if (index < 0) return state;
      const widget = widgets[index];
      if (!WIDGET_SPECS[widget.type].sizes.includes(action.size)) return state;
      const next = [...widgets];
      next[index] = { ...widget, size: action.size } as AnyProfileWidget;
      return commit(state, next);
    }
    case "cycleSize": {
      const index = indexOf(state, action.id);
      if (index < 0) return state;
      const widget = widgets[index];
      const sizes = WIDGET_SPECS[widget.type].sizes;
      const at = sizes.indexOf(widget.size);
      const size = sizes[at + action.direction];
      if (!size) return state;
      return boardEditorReducer(state, { type: "resize", id: action.id, size });
    }
    case "updateCfg": {
      const index = indexOf(state, action.id);
      if (index < 0) return state;
      const widget = widgets[index];
      const next = [...widgets];
      // Kept as typed, not cleaned: trimming while someone types would eat
      // their spaces. The server cleans on save (parseBoard).
      next[index] = {
        ...widget,
        cfg: { ...widget.cfg, ...action.cfg },
      } as AnyProfileWidget;
      return commit(
        state,
        next,
        {},
        action.coalesce ? `${action.id}:${action.coalesce}` : null,
      );
    }
    case "undo": {
      const previous = state.past.at(-1);
      if (!previous) return state;
      return {
        ...state,
        draft: previous,
        past: state.past.slice(0, -1),
        future: [state.draft, ...state.future].slice(0, MAX_HISTORY),
        lastCoalesce: null,
        grabbedId: null,
        selectedId: keepSelection(previous, state.selectedId),
      };
    }
    case "redo": {
      const [next, ...rest] = state.future;
      if (!next) return state;
      return {
        ...state,
        draft: next,
        past: [...state.past, state.draft].slice(-MAX_HISTORY),
        future: rest,
        lastCoalesce: null,
        grabbedId: null,
        selectedId: keepSelection(next, state.selectedId),
      };
    }
    case "discard":
      return {
        ...createEditorState(state.saved, state.version),
        preview: state.preview,
      };
    case "saved":
      // The server's normalised copy becomes both saved and draft; history
      // stays, so a save never costs the undo stack.
      return {
        ...state,
        saved: action.board,
        version: action.version,
        draft: action.board,
        lastCoalesce: null,
        selectedId: keepSelection(action.board, state.selectedId),
        grabbedId: null,
      };
    case "select":
      return { ...state, selectedId: action.id, lastCoalesce: null };
    case "grab":
      return {
        ...state,
        grabbedId: action.id,
        selectedId: action.id ?? state.selectedId,
        lastCoalesce: null,
      };
    case "preview":
      return { ...state, preview: action.audience };
  }
}

function keepSelection(board: ProfileBoard, id: string | null): string | null {
  return id && board.widgets.some((widget) => widget.id === id) ? id : null;
}

/** The next size up or down for `size` among `type`'s sizes, or null. */
export function steppedSize(
  type: WidgetType,
  size: WidgetSize,
  direction: 1 | -1,
): WidgetSize | null {
  const sizes = WIDGET_SPECS[type].sizes;
  return sizes[sizes.indexOf(size) + direction] ?? null;
}

export const SIZE_LABELS: Record<WidgetSize, string> = {
  S: "Small",
  M: "Medium",
  L: "Large",
};

export { WIDGET_SIZES };

/**
 * A draft read back from local storage: cleaned like a save would clean
 * it, so a corrupted or old draft can never break the editor.
 */
export function reviveDraftWidgets(widgets: AnyProfileWidget[]) {
  return widgets.map(
    (widget) =>
      ({
        ...widget,
        cfg: parseWidgetConfig(widget.type, widget.cfg),
      }) as AnyProfileWidget,
  );
}
