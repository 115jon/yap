import { describe, expect, it } from "vitest";

import { parseBoard, type ProfileBoard } from "@/lib/profile-board";
import {
  boardEditorReducer,
  createEditorState,
  isDirty,
  type BoardEditorAction,
  type BoardEditorState,
} from "@/lib/profile-board-editor";

const board = parseBoard({
  widgets: [
    { id: "mood-1", type: "mood", size: "S", cfg: { text: "hi" } },
    { id: "links-1", type: "links", size: "S" },
    { id: "voice-1", type: "voice_hours", size: "M" },
  ],
}).board as ProfileBoard;

function run(state: BoardEditorState, ...actions: BoardEditorAction[]) {
  return actions.reduce(boardEditorReducer, state);
}

const ids = (state: BoardEditorState) => state.draft.widgets.map((w) => w.id);

describe("boardEditorReducer", () => {
  it("moves by keyboard steps and clamps at the ends", () => {
    const state = run(
      createEditorState(board, 3),
      { type: "moveBy", id: "mood-1", delta: 1 },
      { type: "moveBy", id: "mood-1", delta: 5 },
    );
    expect(ids(state)).toEqual(["links-1", "voice-1", "mood-1"]);
    expect(isDirty(state)).toBe(true);
  });

  it("resizes only to sizes the type allows", () => {
    const state = run(
      createEditorState(board, 3),
      { type: "cycleSize", id: "mood-1", direction: 1 },
      { type: "cycleSize", id: "mood-1", direction: 1 },
    );
    // Mood is S or M.
    expect(state.draft.widgets[0].size).toBe("M");
    const refused = run(state, { type: "resize", id: "mood-1", size: "L" });
    expect(refused).toBe(state);
  });

  it("undoes and redoes, and a new change clears redo", () => {
    let state = run(
      createEditorState(board, 3),
      { type: "remove", id: "links-1" },
      { type: "undo" },
    );
    expect(ids(state)).toEqual(["mood-1", "links-1", "voice-1"]);
    state = run(state, { type: "redo" });
    expect(ids(state)).toEqual(["mood-1", "voice-1"]);
    state = run(
      state,
      { type: "undo" },
      { type: "moveBy", id: "mood-1", delta: 1 },
    );
    expect(state.future).toEqual([]);
  });

  it("makes a typing run one undo step", () => {
    const state = run(
      createEditorState(board, 3),
      { type: "updateCfg", id: "mood-1", cfg: { text: "h" }, coalesce: "text" },
      {
        type: "updateCfg",
        id: "mood-1",
        cfg: { text: "he" },
        coalesce: "text",
      },
      {
        type: "updateCfg",
        id: "mood-1",
        cfg: { text: "hey" },
        coalesce: "text",
      },
    );
    expect(state.past).toHaveLength(1);
    const undone = run(state, { type: "undo" });
    expect(undone.draft.widgets[0].cfg).toMatchObject({ text: "hi" });
  });

  it("refuses widgets past a type's limit or the board's", () => {
    const state = run(createEditorState(board, 3), {
      type: "add",
      widget: "mood",
    });
    expect(ids(state)).toHaveLength(3);
    const added = run(state, { type: "add", widget: "links", index: 0 });
    expect(added.draft.widgets[0].type).toBe("links");
    expect(added.selectedId).toBe(added.draft.widgets[0].id);
  });

  it("selects the neighbour after a remove", () => {
    const state = run(
      createEditorState(board, 3),
      { type: "select", id: "links-1" },
      { type: "remove", id: "links-1" },
    );
    expect(state.selectedId).toBe("voice-1");
  });

  it("keeps history across a save and resets on discard", () => {
    const edited = run(createEditorState(board, 3), {
      type: "remove",
      id: "mood-1",
    });
    const saved = run(edited, {
      type: "saved",
      board: edited.draft,
      version: 4,
    });
    expect(saved.version).toBe(4);
    expect(isDirty(saved)).toBe(false);
    expect(saved.past).toHaveLength(1);
    const discarded = run(
      saved,
      { type: "moveBy", id: "links-1", delta: 1 },
      {
        type: "discard",
      },
    );
    expect(ids(discarded)).toEqual(["links-1", "voice-1"]);
    expect(discarded.past).toEqual([]);
  });
});
