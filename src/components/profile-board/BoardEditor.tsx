/**
 * Edit in place: "Edit board" on your own profile turns the Board tab into
 * this editor (ProfilePanel lazy-loads it). A toolbar row (Editing, undo,
 * redo, Preview as Me / Friend / Anyone, Open Studio, Discard, Done) over
 * the Rack with edit chrome, and the keys along the bottom.
 *
 * Done saves (if anything changed) and hands the saved board back through
 * `onExit`; Discard asks first only when there is something to lose. The
 * model, undo stack and save logic are `useBoardEditor`, shared with the
 * Studio; the Rack's chrome and keyboard are `EditableRack`.
 */

import { useLayoutEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { IconButton } from "@/components/ui/IconButton";
import { LedLabel } from "@/components/ui/led";
import { SegmentedControl } from "@/components/ui/segmented-control";
import type { Entitlement } from "@/lib/entitlements";
import type { BoardResponse } from "@/lib/profile-board";
import type { PreviewAudience } from "@/lib/profile-board-editor";
import { cn } from "@/lib/utils";
import { useChatStore } from "@/stores/chat-store";

import {
  EditableRack,
  editorKeyHandler,
  KeyHints,
  widgetName,
} from "./EditableRack";
import { DiscardDialog, EditorNotices, FinishDialog } from "./editor-notices";
import type { BoardOwner } from "./ProfileBoard";
import { useBoardEditor } from "./useBoardEditor";

export interface BoardEditorProps {
  /** The owner's board as last fetched (the editor saves from its version). */
  initial: BoardResponse;
  /** Done (saved) or Discard: back to viewing, with the latest board. */
  onExit(next: BoardResponse | null): void;
  /** "Open Studio": the full editor page. */
  onOpenStudio?(): void;
  /** The owner as the board prints them; defaults to the signed-in user. */
  owner?: BoardOwner;
  /** Plan override (fixtures, tests); defaults to the signed-in user's. */
  entitlement?: Entitlement;
  /** Frozen clock (fixtures, tests). */
  now?: number;
  /** Fixtures: start with this widget selected, or picked up. */
  initialSelectedId?: string;
  initialGrabbedId?: string;
  /** Fixtures: start with the picker open, in this form. */
  initialPickerOpen?: boolean;
  pickerForceMode?: "popover" | "drawer";
}

export const PREVIEW_ITEMS: { value: PreviewAudience; label: string }[] = [
  { value: "me", label: "Me" },
  { value: "friend", label: "Friend" },
  { value: "anyone", label: "Anyone" },
];

export function useBoardOwner(
  initial: BoardResponse,
  owner?: BoardOwner,
): BoardOwner {
  const user = useChatStore((state) => state.user);
  return (
    owner ?? {
      id: initial.user_id,
      name: user?.display_name?.trim() || user?.username || "You",
      avatarUrl: user?.avatar_url ?? null,
    }
  );
}

export default function BoardEditor({
  initial,
  onExit,
  onOpenStudio,
  owner: ownerProp,
  entitlement,
  now,
  initialSelectedId,
  initialGrabbedId,
  initialPickerOpen = false,
  pickerForceMode,
}: BoardEditorProps) {
  const editor = useBoardEditor(initial, { entitlement });
  const owner = useBoardOwner(initial, ownerProp);
  const [message, setMessage] = useState("");
  const [pickerOpen, setPickerOpen] = useState(initialPickerOpen);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [confirmFinish, setConfirmFinish] = useState(false);
  const exitAfterSave = useRef(false);
  const { state, dispatch } = editor;
  const saving = editor.saveStatus === "saving";

  // Fixtures open on a selected or picked-up widget.
  useLayoutEffect(() => {
    if (initialGrabbedId) {
      const index = state.draft.widgets.findIndex(
        (widget) => widget.id === initialGrabbedId,
      );
      const widget = state.draft.widgets[index];
      if (widget) {
        dispatch({ type: "grab", id: widget.id });
        setMessage(
          `${widgetName(widget)} picked up, position ${index + 1} of ${state.draft.widgets.length}. Use the arrow keys to move it, Space to drop it.`,
        );
      }
    } else if (initialSelectedId) {
      dispatch({ type: "select", id: initialSelectedId });
    }
    // Once, on open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const finish = async () => {
    if (!editor.dirty) {
      editor.forgetLocalDraft();
      onExit(editor.response);
      return;
    }
    exitAfterSave.current = true;
    const saved = await editor.save();
    if (saved) onExit(saved);
  };

  const discard = () => {
    if (editor.dirty) setConfirmDiscard(true);
    else {
      editor.forgetLocalDraft();
      onExit(null);
    }
  };

  const onKeyDown = editorKeyHandler(editor, {
    announce: setMessage,
    onEscape: () => setConfirmFinish(true),
  });

  return (
    <div
      data-slot="board-editor"
      onKeyDown={onKeyDown}
      className="@container/editor flex flex-col gap-4"
    >
      <div
        role="toolbar"
        aria-label="Board editing"
        data-slot="board-editor-toolbar"
        className="-mx-7 -mt-5 flex flex-wrap items-center gap-x-3 gap-y-2.5 border-b border-line px-7 py-3 @max-[540px]/panel:-mx-4 @max-[540px]/panel:-mt-4 @max-[540px]/panel:px-4"
      >
        <LedLabel state="talk">Editing</LedLabel>
        <div className="flex items-center">
          <IconButton
            icon="reset"
            size={32}
            variant="muted"
            aria-label="Undo"
            tooltip="Undo (Ctrl Z)"
            disabled={!editor.canUndo}
            onClick={() => dispatch({ type: "undo" })}
          />
          <IconButton
            icon="reset"
            size={32}
            variant="muted"
            aria-label="Redo"
            tooltip="Redo (Ctrl Shift Z)"
            iconClassName="-scale-x-100"
            disabled={!editor.canRedo}
            onClick={() => dispatch({ type: "redo" })}
          />
        </div>
        <span
          aria-hidden="true"
          className="h-6 w-px bg-line-strong @max-[600px]/editor:hidden"
        />
        <div className="flex items-center gap-2.5">
          <span
            aria-hidden="true"
            className="silkscreen text-ink-3 @max-[720px]/editor:hidden"
          >
            Preview as
          </span>
          <SegmentedControl
            label="Preview as"
            size="sm"
            items={PREVIEW_ITEMS}
            value={state.preview}
            onValueChange={(audience) =>
              dispatch({ type: "preview", audience })
            }
          />
        </div>
        <div className="ml-auto flex items-center gap-1 @max-[600px]/editor:ml-0 @max-[600px]/editor:w-full">
          {onOpenStudio ? (
            <IconButton
              icon="sliders"
              size={32}
              variant="muted"
              aria-label="Open Studio"
              tooltip="Open Studio"
              className="@max-[600px]/editor:mr-auto"
              onClick={onOpenStudio}
            />
          ) : null}
          <Button variant="ghost" size="sm" onClick={discard}>
            Discard
          </Button>
          <Button
            data-slot="board-done"
            variant="primary"
            size="sm"
            loading={saving}
            onClick={() => void finish()}
          >
            <Icon name="check" size="sm" />
            Done
          </Button>
        </div>
      </div>

      <EditorNotices
        editor={editor}
        onAfterKeepMine={(saved) => {
          if (saved && exitAfterSave.current) onExit(saved);
        }}
      />

      <EditableRack
        editor={editor}
        owner={owner}
        announce={setMessage}
        now={now}
        pickerOpen={pickerOpen}
        onPickerOpenChange={setPickerOpen}
        pickerForceMode={pickerForceMode}
      />

      <KeyHints
        message={message}
        grabbing={state.grabbedId !== null}
        className={cn(
          "-mx-7 -mb-7 border-t border-line px-7 py-2.5 @max-[540px]/panel:[&>span]:hidden @max-[540px]/panel:-mx-4 @max-[540px]/panel:-mb-6 @max-[540px]/panel:px-4",
          state.preview !== "me" && "[&>span]:hidden",
        )}
      />

      <DiscardDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        onDiscard={() => {
          editor.discard();
          onExit(null);
        }}
      />
      <FinishDialog
        open={confirmFinish}
        onOpenChange={setConfirmFinish}
        dirty={editor.dirty}
        onFinish={() => void finish()}
      />
    </div>
  );
}
