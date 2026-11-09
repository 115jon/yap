/**
 * The Rack in edit mode, shared by edit in place and the Studio: the real
 * BoardRack with every cell wrapped in edit chrome (drag handle, selection
 * ring, a small toolbar on the selected widget, inline text where it sits),
 * the "+ Add a widget" slot and its picker, and the keyboard model:
 *
 *   Tab to a widget, Space picks it up and drops it, arrows move it while
 *   it's up (←/↑ earlier, →/↓ later) and walk between widgets otherwise,
 *   [ and ] resize, Delete removes (Ctrl Z brings it back), Esc drops.
 *
 * Every move is announced through `announce` ("Mood picked up, position 2
 * of 7"). Pointer drag is dnd-kit on the handle. Previewing as Friend or
 * Anyone drops the chrome and the drafts and shows the board as they get it.
 */

import {
  DndContext,
  PointerSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  useEffect,
  useId,
  useRef,
  useState,
  type CSSProperties,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";

import { Icon } from "@/components/ui/icon";
import { IconButton } from "@/components/ui/IconButton";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { Popover, PopoverContent } from "@/components/ui/popover";
import { YapBark } from "@/components/ui/yap-bark";
import { YappyPose } from "@/components/ui/yappy-pose";
import {
  linkHost,
  MAX_CUSTOM_LINES,
  MAX_WIDGETS,
  TEXT_LIMITS,
  WIDGET_SPECS,
  type AnyProfileWidget,
  type BoardData,
  type BoardUserRef,
  type ProfileWidget,
  type WidgetConfigMap,
  type WidgetSize,
  type WidgetType,
} from "@/lib/profile-board";
import {
  SIZE_LABELS,
  steppedSize,
  type PreviewAudience,
} from "@/lib/profile-board-editor";
import { cn } from "@/lib/utils";

import { useBoardMedia } from "./board-context";
import { BoardRack, type BoardOwner } from "./ProfileBoard";
import {
  CharCount,
  clampText,
  trimLines,
  WidgetInspector,
} from "./editor-fields";
import type { BoardEditorStore } from "./useBoardEditor";
import { WidgetPlate } from "./WidgetPlate";
import { CountdownReadout } from "./widgets/Countdown";
import { WidgetImage } from "./widgets/shared";
import { WidgetPicker } from "./WidgetPicker";

export type Announce = (message: string) => void;

/** The name a widget is announced by: Custom's own label, else its type. */
export function widgetName(widget: AnyProfileWidget): string {
  if (widget.type === "custom" && widget.cfg.legend.trim()) {
    return widget.cfg.legend.trim();
  }
  return WIDGET_SPECS[widget.type].name;
}

/** The board as Friend or Anyone sees it: who may sign the guestbook. */
export function previewData(
  data: BoardData,
  audience: PreviewAudience,
): BoardData {
  if (audience === "me") return data;
  const book = data.guestbook ?? {
    enabled: true,
    notes: [],
    total: 0,
    can_post: false,
  };
  return { ...data, guestbook: { ...book, can_post: audience === "friend" } };
}

const PREVIEW_VIEWER: BoardUserRef = {
  id: "preview-viewer",
  username: "friend",
  display_name: "A friend",
  avatar_url: null,
};

export interface EditableRackProps {
  editor: BoardEditorStore;
  owner: BoardOwner;
  announce: Announce;
  now?: number;
  /**
   * "popover": "More options" opens the inspector on the widget (edit in
   * place). "side": the Studio shows it in its own pane; More options
   * calls `onInspect`.
   */
  inspector?: "popover" | "side";
  onInspect?(id: string): void;
  /** The picker is open (controlled, so fixtures can open it). */
  pickerOpen: boolean;
  onPickerOpenChange(open: boolean): void;
  pickerForceMode?: "popover" | "drawer";
  /** Leave the preview ("Back to editing"). */
  className?: string;
}

export function EditableRack({
  editor,
  owner,
  announce,
  now,
  inspector = "popover",
  onInspect,
  pickerOpen,
  onPickerOpenChange,
  pickerForceMode,
  className,
}: EditableRackProps) {
  const { state, dispatch } = editor;
  const widgets = state.draft.widgets;
  const preview = state.preview;
  const data = editor.response.data;
  const addRef = useRef<HTMLButtonElement | null>(null);
  const rackRef = useRef<HTMLDivElement | null>(null);
  const focusNext = useRef<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const [optionsFor, setOptionsFor] = useState<string | null>(null);
  const hintId = useId();
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
  );

  // After a remove or a keyboard walk, focus follows the selection.
  useEffect(() => {
    const id = focusNext.current;
    if (!id) return;
    focusNext.current = null;
    const cell = rackRef.current?.querySelector<HTMLElement>(
      `[data-edit-cell="${id}"]`,
    );
    cell?.focus();
  });

  const positionText = (index: number) =>
    `position ${index + 1} of ${widgets.length}`;

  const onCellKeyDown = (
    event: KeyboardEvent<HTMLDivElement>,
    widget: AnyProfileWidget,
    index: number,
  ) => {
    if (event.target !== event.currentTarget) return;
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const grabbed = state.grabbedId === widget.id;
    const name = widgetName(widget);
    const key = event.key;
    if (key === " " || key === "Spacebar") {
      event.preventDefault();
      if (grabbed) {
        dispatch({ type: "grab", id: null });
        announce(`${name} dropped, ${positionText(index)}.`);
      } else {
        dispatch({ type: "grab", id: widget.id });
        announce(
          `${name} picked up, ${positionText(index)}. Use the arrow keys to move it, Space to drop it.`,
        );
      }
      return;
    }
    if (
      key === "ArrowLeft" ||
      key === "ArrowRight" ||
      key === "ArrowUp" ||
      key === "ArrowDown"
    ) {
      event.preventDefault();
      const delta = key === "ArrowLeft" || key === "ArrowUp" ? -1 : 1;
      if (grabbed) {
        const to = Math.max(0, Math.min(widgets.length - 1, index + delta));
        if (to === index) {
          announce(
            `${name} is already ${to === 0 ? "first" : "last"}, ${positionText(index)}.`,
          );
          return;
        }
        dispatch({ type: "moveBy", id: widget.id, delta });
        announce(`${name} moved to ${positionText(to)}.`);
      } else {
        const next = widgets[index + delta];
        if (next) {
          dispatch({ type: "select", id: next.id });
          focusNext.current = next.id;
        }
      }
      return;
    }
    if (key === "[" || key === "]") {
      event.preventDefault();
      const direction = key === "]" ? 1 : -1;
      const size = steppedSize(widget.type, widget.size, direction);
      if (!size) {
        announce(
          `${name} is already ${SIZE_LABELS[widget.size]}, the ${direction > 0 ? "largest" : "smallest"} it goes.`,
        );
        return;
      }
      dispatch({ type: "resize", id: widget.id, size });
      announce(`${name} is now ${SIZE_LABELS[size]}.`);
      return;
    }
    if (key === "Delete" || key === "Backspace") {
      event.preventDefault();
      const neighbour =
        widgets[index + 1]?.id ?? widgets[index - 1]?.id ?? null;
      dispatch({ type: "remove", id: widget.id });
      focusNext.current = neighbour;
      announce(`${name} removed. Press Ctrl Z to undo.`);
      if (!neighbour) addRef.current?.focus();
      return;
    }
    if (key === "Escape" && grabbed) {
      event.preventDefault();
      event.stopPropagation();
      dispatch({ type: "grab", id: null });
      announce(`${name} dropped, ${positionText(index)}.`);
    }
  };

  const onDragEnd = (event: DragEndEvent) => {
    setOverId(null);
    const id = String(event.active.id);
    const over = event.over ? String(event.over.id) : null;
    if (!over || over === id) return;
    const to = widgets.findIndex((widget) => widget.id === over);
    const widget = widgets.find((item) => item.id === id);
    if (to < 0 || !widget) return;
    dispatch({ type: "move", id, index: to });
    dispatch({ type: "select", id });
    announce(`${widgetName(widget)} moved to ${positionText(to)}.`);
  };

  if (preview !== "me") {
    return (
      <div className={cn("flex flex-col gap-4", className)}>
        <PreviewBanner
          audience={preview}
          hidden={editor.drafts.length}
          onBack={() => dispatch({ type: "preview", audience: "me" })}
        />
        {/* A preview, not the real thing: nothing in it posts. */}
        <div
          data-slot="board-preview"
          onSubmitCapture={(event: FormEvent) => {
            event.preventDefault();
            event.stopPropagation();
          }}
        >
          <BoardRack
            widgets={widgets}
            drafts={editor.drafts}
            data={previewData(data, preview)}
            isOwner
            preview="anyone"
            owner={owner}
            viewer={PREVIEW_VIEWER}
            now={now}
          />
        </div>
      </div>
    );
  }

  const used = widgets.length;
  const full = used >= MAX_WIDGETS;
  const optionsWidget = widgets.find((widget) => widget.id === optionsFor);

  return (
    <div ref={rackRef} className={cn("flex flex-col", className)}>
      <p id={hintId} className="sr-only">
        Space picks up a widget. Arrow keys move it. Brackets resize it. Delete
        removes it.
      </p>
      <DndContext
        sensors={sensors}
        onDragStart={(event) =>
          dispatch({ type: "select", id: String(event.active.id) })
        }
        onDragOver={(event) =>
          setOverId(event.over ? String(event.over.id) : null)
        }
        onDragCancel={() => setOverId(null)}
        onDragEnd={onDragEnd}
      >
        <BoardRack
          widgets={widgets}
          drafts={editor.drafts}
          data={data}
          isOwner
          owner={owner}
          now={now}
          renderCell={(widget, _cell, content) => {
            const index = widgets.findIndex((item) => item.id === widget.id);
            const selected = state.selectedId === widget.id;
            return (
              <EditCell
                widget={widget}
                index={index}
                count={widgets.length}
                selected={selected}
                grabbed={state.grabbedId === widget.id}
                over={overId === widget.id}
                hintId={hintId}
                onSelect={() => {
                  if (state.selectedId !== widget.id)
                    dispatch({ type: "select", id: widget.id });
                }}
                onKeyDown={(event) => onCellKeyDown(event, widget, index)}
                onResize={(size) => {
                  dispatch({ type: "resize", id: widget.id, size });
                  announce(
                    `${widgetName(widget)} is now ${SIZE_LABELS[size]}.`,
                  );
                }}
                onRemove={() => {
                  dispatch({ type: "remove", id: widget.id });
                  announce(
                    `${widgetName(widget)} removed. Press Ctrl Z to undo.`,
                  );
                }}
                onMore={() =>
                  inspector === "side"
                    ? onInspect?.(widget.id)
                    : setOptionsFor(widget.id)
                }
                moreOpen={optionsFor === widget.id}
                optionsPopover={
                  inspector === "popover" && optionsWidget?.id === widget.id
                    ? (anchor) => (
                        <Popover
                          open
                          onOpenChange={(open) => {
                            if (!open) setOptionsFor(null);
                          }}
                        >
                          <PopoverContent
                            anchor={anchor}
                            aria-label={`${widgetName(widget)} options`}
                            side="bottom"
                            align="end"
                            variant="rich"
                            className="w-[min(380px,var(--available-width))] max-w-[380px] p-4"
                          >
                            <WidgetInspector
                              editor={editor}
                              widget={optionsWidget}
                              data={data}
                              variant="popover"
                              ground="ply-3"
                            />
                          </PopoverContent>
                        </Popover>
                      )
                    : undefined
                }
              >
                {selected ? (
                  <InlineBody
                    widget={widget}
                    editor={editor}
                    draft={editor.drafts.includes(widget.id)}
                    ownerId={owner.id}
                    now={now}
                    fallback={content}
                  />
                ) : (
                  content
                )}
              </EditCell>
            );
          }}
        />
      </DndContext>
      <div className="@container/rack mt-[22px] grid grid-cols-2 gap-x-3.5 @max-[540px]/rack:mt-5 @max-[540px]/rack:gap-x-2.5">
        <button
          ref={addRef}
          type="button"
          data-slot="board-add"
          aria-haspopup="dialog"
          aria-expanded={pickerOpen}
          onClick={() => onPickerOpenChange(true)}
          className={cn(
            "group/add flex h-[168px] flex-col items-center justify-center gap-2 rounded-[2px] border border-dashed border-line-strong text-ink-2 outline-none transition-colors duration-(--dur-1) @max-[540px]/rack:h-[151px]",
            "pointer-fine:hover:border-ink-3 pointer-fine:hover:bg-tint-1 focus-visible:border-accent-ink",
            pickerOpen && "border-ink-2 bg-tint-1",
          )}
        >
          <span className="grid size-10 place-items-center rounded-[2px] bg-tint-2 text-ink-1">
            <Icon name="add" size="md" />
          </span>
          <span className="font-display text-[15px] leading-5 font-bold text-ink-1">
            {full ? "Board is full" : "Add a widget"}
          </span>
          <span className="text-num-xs text-ink-3">
            {used} of {MAX_WIDGETS} used
          </span>
        </button>
      </div>
      <WidgetPicker
        open={pickerOpen}
        onOpenChange={(open) => {
          onPickerOpenChange(open);
          if (!open) addRef.current?.focus();
        }}
        board={state.draft}
        canCustom={editor.canCustom}
        anchor={addRef}
        forceMode={pickerForceMode}
        onAdd={(type) => {
          dispatch({ type: "add", widget: type });
          const locked = !!WIDGET_SPECS[type].perk && !editor.canCustom;
          announce(
            `${WIDGET_SPECS[type].name} added at the end${locked ? ", as a draft only you see" : ""}.`,
          );
        }}
      />
    </div>
  );
}

// ── Preview banner ─────────────────────────────────────────────────────────

const AUDIENCE_TEXT: Record<Exclude<PreviewAudience, "me">, ReactNode> = {
  friend: (
    <>
      Previewing as{" "}
      <strong className="font-semibold text-ink-1">a friend</strong>
    </>
  ),
  anyone: (
    <>
      Previewing as{" "}
      <strong className="font-semibold text-ink-1">
        someone you don't know
      </strong>
    </>
  ),
};

function PreviewBanner({
  audience,
  hidden,
  onBack,
}: {
  audience: Exclude<PreviewAudience, "me">;
  hidden: number;
  onBack(): void;
}) {
  return (
    <div
      data-slot="preview-banner"
      role="status"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-[2px] bg-accent-tint px-3 py-2 text-ui-sm text-ink-2"
    >
      <Icon name="eye" size="sm" className="shrink-0 text-accent-ink" />
      <span className="min-w-0 flex-1 text-pretty">
        {AUDIENCE_TEXT[audience]}.{" "}
        {hidden > 0
          ? `${hidden} ${hidden === 1 ? "widget is" : "widgets are"} hidden from them.`
          : "They see every widget."}
      </span>
      <button
        type="button"
        onClick={onBack}
        className="shrink-0 font-display font-bold text-ink-1 underline decoration-2 underline-offset-4 outline-none pointer-fine:hover:text-accent-ink focus-visible:text-accent-ink"
      >
        Back to editing
      </button>
    </div>
  );
}

// ── One cell's chrome ──────────────────────────────────────────────────────

function EditCell({
  widget,
  index,
  count,
  selected,
  grabbed,
  over,
  hintId,
  onSelect,
  onKeyDown,
  onResize,
  onRemove,
  onMore,
  moreOpen,
  optionsPopover,
  children,
}: {
  widget: AnyProfileWidget;
  index: number;
  count: number;
  selected: boolean;
  grabbed: boolean;
  over: boolean;
  hintId: string;
  onSelect(): void;
  onKeyDown(event: KeyboardEvent<HTMLDivElement>): void;
  onResize(size: WidgetSize): void;
  onRemove(): void;
  onMore(): void;
  moreOpen: boolean;
  optionsPopover?: (anchor: HTMLElement | null) => ReactNode;
  children: ReactNode;
}) {
  const {
    setNodeRef: setDragRef,
    setActivatorNodeRef,
    listeners,
    transform,
    isDragging: dragging,
  } = useDraggable({ id: widget.id });
  const { setNodeRef: setDropRef } = useDroppable({ id: widget.id });
  const [moreEl, setMoreEl] = useState<HTMLButtonElement | null>(null);
  const spec = WIDGET_SPECS[widget.type];
  const name = widgetName(widget);
  const style: CSSProperties | undefined = transform
    ? {
        transform: `translate3d(${transform.x}px, ${transform.y}px, 0)`,
      }
    : undefined;

  return (
    <div
      ref={setDropRef}
      data-edit-wrap={widget.id}
      className={cn(
        "group/cell relative h-full",
        (selected || dragging) && "z-10",
        dragging && "z-30",
      )}
    >
      <div
        ref={setDragRef}
        data-edit-cell={widget.id}
        data-selected={selected || undefined}
        data-grabbed={grabbed || undefined}
        tabIndex={0}
        role="group"
        aria-roledescription="widget"
        aria-label={`${name}, ${SIZE_LABELS[widget.size]}, position ${index + 1} of ${count}${grabbed ? ", picked up" : ""}`}
        aria-describedby={hintId}
        onFocus={(event) => {
          if (event.target === event.currentTarget) onSelect();
        }}
        onPointerDown={onSelect}
        onClickCapture={(event) => {
          // In edit mode a click selects; it never follows a widget's link.
          if ((event.target as HTMLElement).closest("a[href]")) {
            event.preventDefault();
          }
        }}
        onKeyDown={onKeyDown}
        style={style}
        className={cn(
          "relative h-full rounded-[2px] outline-none",
          "focus-visible:outline-2 focus-visible:outline-offset-[5px] focus-visible:outline-accent-ink",
          "[&>section]:transition-[border-color] [&>section]:duration-(--dur-1)",
          selected && "[&>section]:border-accent-ink",
          grabbed &&
            "outline-2 outline-offset-[5px] outline-dashed outline-accent-ink motion-safe:-translate-y-1 motion-safe:transition-transform",
          over && "[&>section]:border-ink-1 [&>section]:bg-tint-1",
          dragging && "cursor-grabbing opacity-90 shadow-float",
        )}
      >
        {children}
      </div>

      <button
        ref={setActivatorNodeRef}
        type="button"
        tabIndex={-1}
        aria-hidden="true"
        data-slot="drag-handle"
        {...listeners}
        className={cn(
          "absolute top-1/2 -left-2.5 z-20 grid h-7 w-4 -translate-y-1/2 cursor-grab touch-none place-items-center rounded-[2px] border border-line-strong bg-ply-3 text-ink-3",
          "pointer-fine:opacity-0 pointer-fine:group-hover/cell:opacity-100 pointer-fine:hover:text-ink-1",
          (selected || grabbed) && "pointer-fine:opacity-100 text-ink-1",
          "transition-opacity duration-(--dur-1)",
        )}
      >
        <Icon name="dragHandle" size="xs" />
      </button>

      {selected && !dragging ? (
        <div
          role="toolbar"
          aria-label={`${name} tools`}
          data-slot="widget-toolbar"
          className="absolute right-2 -bottom-[15px] z-20 flex h-7 items-center gap-0.5 rounded-[2px] border border-line-strong bg-ply-3 px-0.5 shadow-float"
        >
          {spec.sizes.length > 1
            ? spec.sizes.map((size) => (
                <button
                  key={size}
                  type="button"
                  aria-pressed={widget.size === size}
                  aria-label={SIZE_LABELS[size]}
                  title={SIZE_LABELS[size]}
                  onClick={() => onResize(size)}
                  className="silkscreen grid h-6 min-w-6 place-items-center rounded-[2px] px-1 text-ink-2 outline-none aria-pressed:bg-primary-fill aria-pressed:text-on-primary pointer-fine:hover:not-aria-pressed:bg-tint-2 focus-visible:ring-2 focus-visible:ring-accent-ink"
                >
                  {size}
                </button>
              ))
            : null}
          <span aria-hidden="true" className="mx-0.5 h-4 w-px bg-line-strong" />
          <IconButton
            ref={setMoreEl}
            icon="sliders"
            size={24}
            variant="ghost"
            aria-label={`More options for ${name}`}
            aria-haspopup="dialog"
            aria-expanded={moreOpen}
            onClick={onMore}
          />
          <IconButton
            icon="trash"
            size={24}
            variant="destructive"
            aria-label={`Remove ${name}`}
            onClick={onRemove}
          />
        </div>
      ) : null}
      {optionsPopover ? optionsPopover(moreEl) : null}
    </div>
  );
}

// ── Inline editing ─────────────────────────────────────────────────────────

/** Inputs that read as the text they edit: no box, a dashed rule. */
const INLINE =
  "w-full min-w-0 rounded-none border-0 border-b border-dashed border-line-field bg-transparent p-0 outline-none transition-colors duration-(--dur-1) placeholder:text-ink-3 pointer-fine:hover:border-ink-3 focus:border-solid focus:border-accent-ink";

const INLINE_TYPES = new Set<WidgetType>([
  "mood",
  "countdown",
  "custom",
  "links",
  "photos",
]);

function InlineInput({
  label,
  value,
  max,
  onChange,
  className,
  placeholder,
}: {
  label: string;
  value: string;
  max: number;
  onChange(value: string): void;
  className?: string;
  placeholder?: string;
}) {
  return (
    <span className="relative flex min-w-0 items-end gap-1.5">
      <input
        aria-label={label}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(clampText(event.target.value, max))}
        onKeyDown={(event) => {
          // Esc hands focus back to the widget, so Esc again finishes.
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            (
              event.currentTarget.closest("[data-edit-cell]") as HTMLElement
            )?.focus();
          }
        }}
        className={cn(INLINE, className)}
      />
      <CharCount value={value} max={max} near className="shrink-0 pb-0.5" />
    </span>
  );
}

function InlineBody({
  widget,
  editor,
  draft,
  ownerId,
  now,
  fallback,
}: {
  widget: AnyProfileWidget;
  editor: BoardEditorStore;
  draft: boolean;
  ownerId: string;
  now?: number;
  fallback: ReactNode;
}) {
  const media = useBoardMedia();
  if (!INLINE_TYPES.has(widget.type)) return <>{fallback}</>;
  const set = (cfg: Partial<WidgetConfigMap[WidgetType]>, coalesce: string) =>
    editor.dispatch({ type: "updateCfg", id: widget.id, cfg, coalesce });

  const plate = (body: ReactNode, legend?: string) => (
    <WidgetPlate
      type={widget.type}
      size={widget.size}
      legend={legend}
      draft={draft}
      bodyClassName="gap-2"
    >
      {body}
    </WidgetPlate>
  );

  switch (widget.type) {
    case "mood": {
      const cfg = (widget as ProfileWidget<"mood">).cfg;
      const big = widget.size === "M";
      return plate(
        <div
          className={cn(
            "flex min-h-0 flex-1 gap-3.5",
            big
              ? "flex-col justify-center"
              : "items-center @max-[200px]/cell:flex-col @max-[200px]/cell:items-start @max-[200px]/cell:gap-2",
          )}
        >
          <span className="shrink-0">
            {cfg.pose === "bark" ? (
              <YapBark cell={big ? 3 : 2} />
            ) : (
              <YappyPose
                pose={cfg.pose}
                cell={
                  cfg.pose === "stuffed"
                    ? ((big ? 3 : 2) * 32) / 40
                    : big
                      ? 3
                      : 2
                }
              />
            )}
          </span>
          <span className="w-full min-w-0 flex-1">
            <InlineInput
              label="Mood"
              value={cfg.text}
              max={TEXT_LIMITS.mood}
              placeholder="How's it going?"
              onChange={(text) => set({ text }, "text")}
              className={cn(
                "font-display font-bold tracking-[-0.01em] text-ink-1",
                big
                  ? "text-[22px] leading-[26px]"
                  : "text-[17px] leading-[21px]",
              )}
            />
          </span>
        </div>,
      );
    }
    case "countdown": {
      const cfg = (widget as ProfileWidget<"countdown">).cfg;
      const target = cfg.at ? Date.parse(cfg.at) : NaN;
      return plate(
        <>
          <InlineInput
            label="Countdown title"
            value={cfg.title}
            max={TEXT_LIMITS.countdownTitle}
            placeholder="What are you counting down to?"
            onChange={(title) => set({ title }, "title")}
            className="font-display text-[16px] leading-5 font-bold tracking-[-0.01em] text-ink-1"
          />
          {Number.isFinite(target) ? (
            <CountdownReadout
              target={target}
              now={now}
              big={widget.size === "M"}
            />
          ) : (
            <p className="text-ui-sm text-ink-3">
              Set the date in More options.
            </p>
          )}
        </>,
      );
    }
    case "custom": {
      const cfg = (widget as ProfileWidget<"custom">).cfg;
      const lines = Array.from(
        { length: MAX_CUSTOM_LINES },
        (_, index) => cfg.lines[index] ?? "",
      );
      const shown = widget.size === "S" ? lines.slice(0, 2) : lines;
      return plate(
        <div className="flex min-h-0 flex-1 gap-4">
          {cfg.asset && widget.size === "L" ? (
            <WidgetImage
              src={media.asset(ownerId, cfg.asset)}
              alt=""
              gif={cfg.asset.endsWith(".gif")}
              className="aspect-[4/3] h-full shrink-0"
            />
          ) : null}
          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
            <InlineInput
              label="Custom title"
              value={cfg.title}
              max={TEXT_LIMITS.title}
              placeholder="Title"
              onChange={(title) => set({ title }, "title")}
              className="font-display text-[17px] leading-[22px] font-bold tracking-[-0.01em] text-ink-1"
            />
            {shown.map((line, index) => (
              <span key={index} className="flex min-w-0 items-baseline gap-2">
                <span
                  aria-hidden="true"
                  className="h-px w-1.5 shrink-0 -translate-y-1 bg-ink-3"
                />
                <span className="min-w-0 flex-1">
                  <InlineInput
                    label={`Line ${index + 1}`}
                    value={line}
                    max={TEXT_LIMITS.line}
                    placeholder={index === 0 ? "A short line" : undefined}
                    onChange={(value) => {
                      const next = [...lines];
                      next[index] = value;
                      set({ lines: trimLines(next) }, `line-${index}`);
                    }}
                    className="text-ui-sm text-ink-2"
                  />
                </span>
              </span>
            ))}
          </div>
        </div>,
        cfg.legend,
      );
    }
    case "links": {
      const cfg = (widget as ProfileWidget<"links">).cfg;
      if (cfg.links.length === 0) return <>{fallback}</>;
      const limit = widget.size === "S" ? 3 : 5;
      return plate(
        <ul aria-label="Links" className="flex flex-col gap-1">
          {cfg.links.slice(0, limit).map((link, index) => (
            <li key={index} className="flex h-8 min-w-0 items-center gap-2.5">
              <Icon name="link" size="sm" className="shrink-0 text-ink-3" />
              <span className="min-w-0 flex-1">
                <InlineInput
                  label={`Link ${index + 1} label`}
                  value={link.label}
                  max={TEXT_LIMITS.linkLabel}
                  onChange={(label) =>
                    set(
                      {
                        links: cfg.links.map((item, i) =>
                          i === index ? { ...item, label } : item,
                        ),
                      },
                      `link-${index}-label`,
                    )
                  }
                  className="text-ui-row font-medium text-ink-1"
                />
              </span>
              <span className="max-w-[40%] shrink truncate text-num-xs text-ink-3 @max-[200px]/cell:hidden">
                {linkHost(link.url)}
              </span>
            </li>
          ))}
        </ul>,
      );
    }
    case "photos": {
      const cfg = (widget as ProfileWidget<"photos">).cfg;
      if (cfg.photos.length === 0 || widget.size === "S")
        return <>{fallback}</>;
      const shown = cfg.photos.slice(0, 4);
      return plate(
        <>
          <ul
            aria-label="Photos"
            className="grid min-h-0 flex-1 gap-1"
            style={{
              gridTemplateColumns: `repeat(${Math.min(shown.length, 2)}, minmax(0, 1fr))`,
            }}
          >
            {shown.map((photo) => (
              <li key={photo.asset} className="min-h-0 min-w-0">
                <WidgetImage
                  src={media.asset(ownerId, photo.asset)}
                  alt={photo.alt}
                  gif={photo.asset.endsWith(".gif")}
                  className="size-full"
                />
              </li>
            ))}
          </ul>
          <InlineInput
            label="Caption"
            value={cfg.caption}
            max={TEXT_LIMITS.caption}
            placeholder="Add a caption"
            onChange={(caption) => set({ caption }, "caption")}
            className="text-ui-xs text-ink-2"
          />
        </>,
      );
    }
    default:
      return <>{fallback}</>;
  }
}

// ── Key hints ──────────────────────────────────────────────────────────────

/**
 * The bar under the board: the keys, or, while a widget is picked up or
 * just moved, what happened (the same words the screen reader hears).
 */
export function KeyHints({
  message,
  grabbing,
  escHint = true,
  className,
}: {
  message: string;
  grabbing: boolean;
  /** Show "Esc done" (edit in place; the Studio has no Esc to finish). */
  escHint?: boolean;
  className?: string;
}) {
  return (
    <div
      data-slot="key-hints"
      className={cn(
        "flex min-h-11 flex-wrap items-center gap-x-4 gap-y-2 text-ui-xs text-ink-3",
        className,
        // Touch has no keys to hint: with nothing picked up the bar is only
        // the live region, kept for screen readers but taking no room.
        !grabbing && "pointer-coarse:sr-only",
      )}
    >
      <p
        role="status"
        aria-live="polite"
        aria-atomic="true"
        data-slot="editor-announcer"
        className={cn(
          grabbing
            ? "flex min-w-0 flex-1 items-center gap-2 text-ui-sm font-medium text-pretty text-ink-1"
            : "sr-only",
        )}
      >
        {grabbing ? (
          <Icon name="move" size="sm" className="shrink-0 text-accent-ink" />
        ) : null}
        {message}
      </p>
      {grabbing ? (
        <span className="ml-auto flex items-center gap-4 pointer-coarse:hidden">
          <Hint keys={["←", "→"]} label="move" />
          <Hint keys={["Space"]} label="drop" />
        </span>
      ) : (
        <span className="flex w-full flex-wrap items-center gap-x-4 gap-y-2 pointer-coarse:hidden">
          <Hint keys={["Space"]} label="pick up" />
          <Hint keys={["←", "→"]} label="move" />
          <Hint keys={["[", "]"]} label="resize" />
          <Hint keys={["Del"]} label="remove" />
          <Hint keys={["Ctrl", "Z"]} label="undo" />
          {escHint ? (
            <Hint keys={["Esc"]} label="done" className="ml-auto" />
          ) : null}
        </span>
      )}
    </div>
  );
}

function Hint({
  keys,
  label,
  className,
}: {
  keys: string[];
  label: string;
  className?: string;
}) {
  return (
    <span className={cn("inline-flex items-center gap-1.5", className)}>
      <KbdGroup>
        {keys.map((key) => (
          <Kbd key={key} size="sm">
            {key}
          </Kbd>
        ))}
      </KbdGroup>
      {label}
    </span>
  );
}

/**
 * Undo and redo for an editor's root (Ctrl/Cmd Z, Ctrl Shift Z, Ctrl Y),
 * and Esc when nothing nearer took it.
 */
export function editorKeyHandler(
  editor: BoardEditorStore,
  { onEscape, announce }: { onEscape?(): void; announce: Announce },
) {
  return (event: KeyboardEvent<HTMLElement>) => {
    if (event.defaultPrevented) return;
    const mod = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    if (mod && !event.altKey && (key === "z" || key === "y")) {
      event.preventDefault();
      const redo = key === "y" || (key === "z" && event.shiftKey);
      if (redo) {
        if (!editor.canRedo) return;
        editor.dispatch({ type: "redo" });
        announce("Redone.");
      } else {
        if (!editor.canUndo) return;
        editor.dispatch({ type: "undo" });
        announce("Undone.");
      }
      return;
    }
    if (event.key === "Escape" && onEscape) {
      const target = event.target as HTMLElement;
      if (target.closest("[data-slot=popover-content],[role=dialog]")) return;
      event.preventDefault();
      onEscape();
    }
  };
}
