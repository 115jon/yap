/**
 * The Profile studio: the board editor as a full page. A header (back,
 * save state, undo/redo, Discard, Save board), the widget library on the
 * left (grouped by category, with what's on the board ticked), the live
 * board in the middle (Preview as Me / Friend / Anyone at desktop or phone
 * width, with the same chrome and keys as edit in place), and the selected
 * widget's inspector on the right.
 *
 * Under 720px it is one column: Board · Add · Edit tabs switch between the
 * preview, the library and the inspector.
 *
 * Same store as edit in place (`useBoardEditor`): model, undo and saving
 * are shared; only the frame differs.
 */

import { useEffect, useId, useMemo, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { IconButton } from "@/components/ui/IconButton";
import { Input } from "@/components/ui/input";
import { LedLabel } from "@/components/ui/led";
import { SegmentedControl } from "@/components/ui/segmented-control";
import { Tabs } from "@/components/ui/tabs";
import { TreeBranch, TreeLeaf } from "@/components/ui/tree";
import type { Entitlement } from "@/lib/entitlements";
import {
  MAX_WIDGETS,
  WIDGET_CATEGORIES,
  WIDGET_CATEGORY_LABELS,
  WIDGET_SPECS,
  WIDGET_TYPES,
  type BoardResponse,
  type WidgetType,
} from "@/lib/profile-board";
import { fetchProfileBoard } from "@/lib/profile-board-api";
import { canAdd } from "@/lib/profile-board-editor";
import { cn } from "@/lib/utils";

import { PREVIEW_ITEMS, useBoardOwner } from "./BoardEditor";
import { WIDGET_ICON } from "./board-format";
import { EditableRack, editorKeyHandler, KeyHints } from "./EditableRack";
import { LoudLock, WidgetInspector } from "./editor-fields";
import { DiscardDialog, EditorNotices } from "./editor-notices";
import type { BoardOwner } from "./ProfileBoard";
import { useBoardEditor, type BoardEditorStore } from "./useBoardEditor";

export interface ProfileStudioProps {
  onClose(): void;
  /** /ui-fixtures and tests: edit this board instead of fetching. */
  fixture?: BoardResponse;
  /** Plan override (fixtures, tests). */
  entitlementOverride?: Entitlement;
  owner?: BoardOwner;
  now?: number;
  /** Fixtures: start with this widget selected. */
  initialSelectedId?: string;
  /** Fixtures: which pane a phone-width studio opens on. */
  initialPane?: StudioPane;
}

type StudioPane = "board" | "library" | "inspector";

export default function ProfileStudio(props: ProfileStudioProps) {
  const { fixture, onClose } = props;
  const [loaded, setLoaded] = useState<{
    board: BoardResponse | null;
    error: string | null;
    nonce: number;
  }>({ board: fixture ?? null, error: null, nonce: 0 });

  useEffect(() => {
    if (fixture) return;
    const controller = new AbortController();
    fetchProfileBoard("me", controller.signal).then(
      (board) => {
        if (!controller.signal.aborted)
          setLoaded((prev) => ({ ...prev, board, error: null }));
      },
      () => {
        if (!controller.signal.aborted)
          setLoaded((prev) => ({
            ...prev,
            error: "Couldn't load your board.",
          }));
      },
    );
    return () => controller.abort();
  }, [fixture, loaded.nonce]);

  const board = fixture ?? loaded.board;
  if (!board) {
    return (
      <div data-slot="profile-studio" className="flex h-full flex-col bg-ply-1">
        <StudioHeaderShell onBack={onClose} />
        <div className="grid flex-1 place-items-center p-6">
          {loaded.error ? (
            <div role="alert" className="flex flex-col items-center gap-3">
              <p className="text-ui-sm text-ink-2">{loaded.error}</p>
              <Button
                variant="secondary"
                size="sm"
                onClick={() =>
                  setLoaded((prev) => ({
                    ...prev,
                    error: null,
                    nonce: prev.nonce + 1,
                  }))
                }
              >
                <Icon name="refresh" size="sm" />
                Try again
              </Button>
            </div>
          ) : (
            <p role="status" className="text-ui-sm text-ink-3">
              Loading your board…
            </p>
          )}
        </div>
      </div>
    );
  }
  return <Studio {...props} initial={board} />;
}

function StudioHeaderShell({
  onBack,
  children,
}: {
  onBack(): void;
  children?: ReactNode;
}) {
  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-line bg-ply-1 px-3 @max-[720px]/studio:h-12 @max-[720px]/studio:px-2">
      <IconButton
        icon="back"
        size={40}
        aria-label="Back to your profile"
        onClick={onBack}
      />
      <h1 className="font-display text-[18px] leading-6 font-extrabold tracking-[-0.02em] text-ink-1 @max-[720px]/studio:sr-only">
        Profile studio
      </h1>
      {children}
    </header>
  );
}

function SaveState({ editor }: { editor: BoardEditorStore }) {
  if (editor.saveStatus === "saving")
    return <LedLabel state="off">Saving…</LedLabel>;
  if (editor.saveStatus === "conflict" || editor.saveStatus === "error")
    return <LedLabel state="muted">Not saved</LedLabel>;
  if (editor.dirty) return <LedLabel state="idle">Unsaved</LedLabel>;
  return <LedLabel state="talk">Saved</LedLabel>;
}

function Studio({
  initial,
  onClose,
  entitlementOverride,
  owner: ownerProp,
  now,
  initialSelectedId,
  initialPane = "board",
}: ProfileStudioProps & { initial: BoardResponse }) {
  const editor = useBoardEditor(initial, { entitlement: entitlementOverride });
  const owner = useBoardOwner(initial, ownerProp);
  const { state, dispatch } = editor;
  const [message, setMessage] = useState("");
  const [pickerOpen, setPickerOpen] = useState(false);
  const [device, setDevice] = useState<"desktop" | "phone">("desktop");
  const [pane, setPane] = useState<StudioPane>(initialPane);
  const [confirmLeave, setConfirmLeave] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const idBase = useId();

  useEffect(() => {
    if (initialSelectedId) dispatch({ type: "select", id: initialSelectedId });
    // Once, on open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Closing the window with unsaved changes asks first (the draft is also
  // kept on this device).
  useEffect(() => {
    if (!editor.dirty) return;
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [editor.dirty]);

  // The selected widget scrolls into the preview (library adds, fixtures).
  useEffect(() => {
    const id = state.selectedId;
    if (!id) return;
    document
      .querySelector(`[data-edit-cell="${id}"]`)
      ?.scrollIntoView?.({ block: "nearest" });
  }, [state.selectedId]);

  const selected = state.draft.widgets.find(
    (widget) => widget.id === state.selectedId,
  );

  const leave = () => {
    if (editor.dirty) setConfirmLeave(true);
    else {
      editor.forgetLocalDraft();
      onClose();
    }
  };

  const onKeyDown = editorKeyHandler(editor, { announce: setMessage });

  const addOrSelect = (type: WidgetType) => {
    if (canAdd(state.draft, type)) {
      dispatch({ type: "add", widget: type });
      const locked = !!WIDGET_SPECS[type].perk && !editor.canCustom;
      setMessage(
        `${WIDGET_SPECS[type].name} added at the end${locked ? ", as a draft only you see" : ""}.`,
      );
      setPane("inspector");
      return;
    }
    const existing = state.draft.widgets.find((widget) => widget.type === type);
    if (existing) {
      dispatch({ type: "select", id: existing.id });
      setPane("inspector");
    }
  };

  return (
    <div
      data-slot="profile-studio"
      onKeyDown={onKeyDown}
      className="@container/studio flex h-full min-h-0 flex-col bg-ply-1 text-ink-1"
    >
      <StudioHeaderShell onBack={leave}>
        <span className="min-w-0 @max-[720px]/studio:[&_[data-slot=led-label]>span:last-child]:sr-only">
          <SaveState editor={editor} />
        </span>
        <div className="ml-auto flex items-center gap-1">
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
          <span
            aria-hidden="true"
            className="mx-2 h-6 w-px bg-line-strong @max-[720px]/studio:hidden"
          />
          <Button
            variant="ghost"
            size="sm"
            disabled={!editor.dirty}
            onClick={() => setConfirmDiscard(true)}
          >
            Discard
          </Button>
          <Button
            data-slot="studio-save"
            variant="primary"
            className="@max-[720px]/studio:h-8 @max-[720px]/studio:px-3 @max-[720px]/studio:text-[14px]"
            loading={editor.saveStatus === "saving"}
            disabled={!editor.dirty && !editor.conflict}
            onClick={() => void editor.save()}
          >
            Save board
          </Button>
        </div>
      </StudioHeaderShell>

      <div className="hidden border-b border-line px-3 @max-[720px]/studio:block">
        <Tabs
          label="Studio"
          idBase={idBase}
          items={[
            { value: "board" as StudioPane, label: "Board" },
            { value: "library" as StudioPane, label: "Add" },
            { value: "inspector" as StudioPane, label: "Edit" },
          ]}
          value={pane}
          onValueChange={setPane}
          className="border-b-0"
        />
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-[264px_minmax(0,1fr)_340px] @max-[1100px]/studio:grid-cols-[232px_minmax(0,1fr)_300px] @max-[720px]/studio:grid-cols-1">
        <aside
          aria-label="Widget library"
          className={cn(
            "flex min-h-0 flex-col border-r border-line bg-ply-1",
            pane !== "library" && "@max-[720px]/studio:hidden",
          )}
        >
          <Library editor={editor} onPick={addOrSelect} />
        </aside>

        <main
          aria-label="Board preview"
          className={cn(
            "flex min-h-0 min-w-0 flex-col bg-ply-0",
            pane !== "board" && "@max-[720px]/studio:hidden",
          )}
        >
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-line px-5 py-2.5 @max-[720px]/studio:px-3">
            <div className="flex items-center gap-2.5">
              <span
                aria-hidden="true"
                className="silkscreen text-ink-3 @max-[720px]/studio:hidden"
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
            <SegmentedControl
              label="Preview width"
              size="sm"
              className="ml-auto @max-[720px]/studio:hidden"
              items={[
                { value: "desktop" as const, label: "Desktop" },
                { value: "phone" as const, label: "Phone" },
              ]}
              value={device}
              onValueChange={setDevice}
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 py-6 @max-[720px]/studio:px-0 @max-[720px]/studio:py-0">
            <div
              data-slot="studio-frame"
              data-device={device}
              className={cn(
                "@container/panel mx-auto border border-line-strong bg-ply-3 px-7 pt-6 pb-4 @max-[720px]/studio:w-full! @max-[720px]/studio:border-0 @max-[720px]/studio:px-4",
                device === "phone" && "px-4",
              )}
              style={{
                width: device === "phone" ? 390 : 744,
                maxWidth: "100%",
              }}
            >
              <div className="flex flex-col gap-4">
                <EditorNotices editor={editor} />
                <EditableRack
                  editor={editor}
                  owner={owner}
                  announce={setMessage}
                  now={now}
                  inspector="side"
                  onInspect={(id) => {
                    dispatch({ type: "select", id });
                    setPane("inspector");
                  }}
                  pickerOpen={pickerOpen}
                  onPickerOpenChange={setPickerOpen}
                />
              </div>
            </div>
          </div>
          <KeyHints
            message={message}
            grabbing={state.grabbedId !== null}
            escHint={false}
            className={cn(
              "border-t border-line bg-ply-1 px-5 py-2 @max-[720px]/studio:hidden",
              state.preview !== "me" && "[&>span]:hidden",
            )}
          />
        </main>

        <aside
          aria-label="Widget settings"
          className={cn(
            "min-h-0 overflow-y-auto overscroll-contain border-l border-line bg-ply-1 p-5",
            pane !== "inspector" && "@max-[720px]/studio:hidden",
            "@max-[720px]/studio:border-l-0 @max-[720px]/studio:p-4",
          )}
        >
          {selected ? (
            <WidgetInspector
              key={selected.id}
              editor={editor}
              widget={selected}
              data={editor.response.data}
              ground="ply-1"
              onRemoved={() =>
                setMessage(`${WIDGET_SPECS[selected.type].name} removed.`)
              }
            />
          ) : (
            <div className="flex flex-col gap-2 pt-2">
              <p className="font-display text-[17px] leading-[22px] font-bold text-ink-1">
                Pick a widget to edit it
              </p>
              <p className="text-ui-sm text-pretty text-ink-3">
                Click one on the board, or add one from the library. Its text,
                pictures and size are set here.
              </p>
            </div>
          )}
        </aside>
      </div>

      <DiscardDialog
        open={confirmLeave}
        onOpenChange={setConfirmLeave}
        title="Leave without saving?"
        confirmLabel="Discard and leave"
        onDiscard={() => {
          editor.discard();
          onClose();
        }}
      />
      <DiscardDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        onDiscard={editor.discard}
      />
    </div>
  );
}

function Library({
  editor,
  onPick,
}: {
  editor: BoardEditorStore;
  onPick(type: WidgetType): void;
}) {
  const [query, setQuery] = useState("");
  const widgets = editor.state.draft.widgets;
  const used = widgets.length;
  const groups = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return WIDGET_CATEGORIES.map((category) => ({
      category,
      types: WIDGET_TYPES.filter((type) => {
        const spec = WIDGET_SPECS[type];
        if (spec.category !== category) return false;
        return (
          !needle ||
          spec.name.toLowerCase().includes(needle) ||
          spec.blurb.toLowerCase().includes(needle)
        );
      }),
    })).filter((group) => group.types.length > 0);
  }, [query]);

  return (
    <>
      <div className="p-3">
        <label className="relative block">
          <span className="sr-only">Find a widget</span>
          <Icon
            name="search"
            size="sm"
            className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ink-3"
          />
          <Input
            value={query}
            placeholder="Find a widget"
            className="pl-9"
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </div>
      <nav
        aria-label="Widgets by category"
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-2 pb-3"
      >
        {groups.length === 0 ? (
          <p className="px-2 py-4 text-ui-sm text-ink-3">
            No widget matches “{query}”.
          </p>
        ) : null}
        {groups.map(({ category, types }) => {
          const onBoard = widgets.filter(
            (widget) => WIDGET_SPECS[widget.type].category === category,
          ).length;
          return (
            <TreeBranch
              key={category}
              rowHeight={32}
              animateEnter={false}
              className="mt-3 first:mt-1"
              header={
                <div className="flex h-6 items-center justify-between px-2">
                  <span className="silkscreen text-ink-3">
                    {WIDGET_CATEGORY_LABELS[category]}
                  </span>
                  {onBoard > 0 ? (
                    <span className="text-num-xs text-ink-3">{onBoard}</span>
                  ) : null}
                </div>
              }
            >
              {types.map((type) => {
                const spec = WIDGET_SPECS[type];
                const added = widgets.some((widget) => widget.type === type);
                const locked = !!spec.perk && !editor.canCustom;
                const current =
                  !!editor.state.selectedId &&
                  widgets.find((w) => w.id === editor.state.selectedId)
                    ?.type === type;
                return (
                  <TreeLeaf key={type} current={current}>
                    <button
                      type="button"
                      data-library-type={type}
                      onClick={() => onPick(type)}
                      title={spec.blurb}
                      className={cn(
                        "flex h-8 min-w-0 flex-1 items-center gap-2 rounded-[2px] px-2 text-left text-ui-row font-medium text-ink-2 outline-none",
                        "pointer-fine:hover:bg-tint-1 pointer-fine:hover:text-ink-1 focus-visible:ring-2 focus-visible:ring-accent-ink",
                        current && "bg-tint-2 text-ink-1",
                      )}
                    >
                      <Icon
                        name={WIDGET_ICON[type]}
                        size="sm"
                        className="shrink-0"
                      />
                      <span className="min-w-0 flex-1 truncate">
                        {spec.name}
                      </span>
                      {locked ? <LoudLock /> : null}
                      {added ? (
                        <Icon
                          name="check"
                          size="sm"
                          className="shrink-0 text-ink-2"
                          aria-label="On your board"
                        />
                      ) : null}
                    </button>
                  </TreeLeaf>
                );
              })}
            </TreeBranch>
          );
        })}
      </nav>
      <div className="border-t border-line px-4 py-3">
        <div className="flex items-center justify-between">
          <span className="silkscreen text-ink-3">Board</span>
          <span
            className={cn(
              "text-num-xs",
              used >= MAX_WIDGETS ? "text-warning-ink" : "text-ink-2",
            )}
          >
            {used} / {MAX_WIDGETS}
          </span>
        </div>
        <meter
          min={0}
          max={MAX_WIDGETS}
          value={used}
          aria-label="Widgets on your board"
          className="sr-only"
        />
        <div aria-hidden="true" className="mt-2 h-1.5 bg-tint-2">
          <div
            className="h-full origin-left bg-ink-2"
            style={{ transform: `scaleX(${used / MAX_WIDGETS})` }}
          />
        </div>
      </div>
    </>
  );
}
