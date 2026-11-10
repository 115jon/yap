/**
 * "Add a widget": search, category tabs, one option per widget type with
 * its sentence, a check when it's on the board and the reason when it
 * can't be added. Custom comes first; on a free plan it carries the Loud
 * lock and adds as a draft only you see.
 *
 * Keyboard: the search field keeps focus (combobox), ↑/↓ choose, Enter
 * adds, Esc closes. A popover on desktop, a bottom drawer on phones.
 */

import {
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type RefObject,
} from "react";

import { Icon } from "@/components/ui/icon";
import { Kbd } from "@/components/ui/kbd";
import { Input } from "@/components/ui/input";
import { ResponsivePopover } from "@/components/ui/popover";
import { Tabs } from "@/components/ui/tabs";
import {
  MAX_WIDGETS,
  remainingOfType,
  WIDGET_CATEGORIES,
  WIDGET_CATEGORY_LABELS,
  WIDGET_SPECS,
  WIDGET_TYPES,
  type ProfileBoard,
  type WidgetCategory,
  type WidgetType,
} from "@/lib/profile-board";
import { DEADLOCK_ICON_PATH } from "@/lib/deadlock";
import { cn } from "@/lib/utils";

import { useBoardMedia } from "./board-context";
import { WIDGET_ICON } from "./board-format";
import { LoudLock } from "./editor-fields";

type PickerTab = "all" | WidgetCategory;

export interface PickerEntry {
  type: WidgetType;
  added: number;
  /** Why it can't be added, or null. */
  reason: string | null;
  locked: boolean;
}

/** Every type in picker order (Custom first) with what stops it. */
export function pickerEntries(
  board: ProfileBoard,
  canCustom: boolean,
): PickerEntry[] {
  const order: WidgetType[] = [
    "custom",
    "deadlock",
    ...WIDGET_TYPES.filter((type) => type !== "custom" && type !== "deadlock"),
  ];
  const full = board.widgets.length >= MAX_WIDGETS;
  return order.map((type) => {
    const spec = WIDGET_SPECS[type];
    const added = board.widgets.filter((w) => w.type === type).length;
    let reason: string | null = null;
    if (full) reason = `Your board is full: ${MAX_WIDGETS} of ${MAX_WIDGETS}`;
    else if (remainingOfType(board, type) === 0)
      reason =
        spec.max === 1
          ? "Already on your board"
          : `${spec.max} of ${spec.max} on your board`;
    return { type, added, reason, locked: !!spec.perk && !canCustom };
  });
}

function blurbFor(entry: PickerEntry): string {
  if (entry.locked) {
    return "Your own plate: a title, a picture and four lines. Goes live with Yap Loud; lay one out now as a draft only you see.";
  }
  return WIDGET_SPECS[entry.type].blurb;
}

export interface WidgetPickerProps {
  open: boolean;
  onOpenChange(open: boolean): void;
  board: ProfileBoard;
  canCustom: boolean;
  onAdd(type: WidgetType): void;
  anchor: RefObject<HTMLElement | null>;
  forceMode?: "popover" | "drawer";
}

export function WidgetPicker({
  open,
  onOpenChange,
  board,
  canCustom,
  onAdd,
  anchor,
  forceMode,
}: WidgetPickerProps) {
  const searchRef = useRef<HTMLInputElement | null>(null);
  return (
    <ResponsivePopover
      open={open}
      onOpenChange={onOpenChange}
      title="Add a widget"
      anchor={anchor}
      side="top"
      align="start"
      variant="rich"
      forceMode={forceMode}
      initialFocus={searchRef}
      className="w-[min(520px,var(--available-width))] max-w-[520px] p-0"
      drawerClassName="max-h-[85dvh]"
    >
      <PickerBody
        board={board}
        canCustom={canCustom}
        searchRef={searchRef}
        onAdd={(type) => {
          onAdd(type);
          onOpenChange(false);
        }}
        onClose={() => onOpenChange(false)}
      />
    </ResponsivePopover>
  );
}

export function PickerBody({
  board,
  canCustom,
  onAdd,
  onClose,
  searchRef,
}: {
  board: ProfileBoard;
  canCustom: boolean;
  onAdd(type: WidgetType): void;
  onClose(): void;
  searchRef?: RefObject<HTMLInputElement | null>;
}) {
  const idBase = useId();
  const listId = `${idBase}-list`;
  const [query, setQuery] = useState("");
  const [tab, setTab] = useState<PickerTab>("all");
  const [active, setActive] = useState(0);

  const entries = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return pickerEntries(board, canCustom).filter((entry) => {
      const spec = WIDGET_SPECS[entry.type];
      if (tab !== "all" && spec.category !== tab) return false;
      if (!needle) return true;
      return (
        spec.name.toLowerCase().includes(needle) ||
        spec.blurb.toLowerCase().includes(needle) ||
        WIDGET_CATEGORY_LABELS[spec.category].toLowerCase().includes(needle)
      );
    });
  }, [board, canCustom, query, tab]);

  const current = Math.min(active, Math.max(0, entries.length - 1));
  const optionId = (type: WidgetType) => `${idBase}-opt-${type}`;

  const choose = (entry: PickerEntry | undefined) => {
    if (!entry || entry.reason) return;
    onAdd(entry.type);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (entries.length === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next = (current + step + entries.length) % entries.length;
      setActive(next);
      document
        .getElementById(optionId(entries[next]!.type))
        ?.scrollIntoView?.({ block: "nearest" });
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(entries[current]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  };

  const activeEntry = entries[current];
  const used = board.widgets.length;
  const media = useBoardMedia();

  return (
    <div
      data-slot="widget-picker"
      className="flex max-h-[min(560px,80dvh)] min-h-0 flex-col"
    >
      <div className="flex flex-col gap-3 px-4 pt-4">
        <div className="flex items-baseline justify-between gap-3">
          <h2 className="font-display text-[20px] leading-6 font-bold text-ink-1">
            Add a widget
          </h2>
          <span
            className={cn(
              "text-num-xs",
              used >= MAX_WIDGETS ? "text-warning-ink" : "text-ink-3",
            )}
          >
            {used} / {MAX_WIDGETS}
          </span>
        </div>
        <label className="relative block">
          <span className="sr-only">Search widgets</span>
          <Icon
            name="search"
            size="sm"
            className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ink-3"
          />
          <Input
            ref={searchRef}
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={
              activeEntry ? optionId(activeEntry.type) : undefined
            }
            value={query}
            placeholder="Search widgets"
            className="pl-9"
            onChange={(event) => {
              setQuery(event.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
          />
        </label>
        <Tabs
          label="Widget categories"
          scrollable
          items={[
            { value: "all" as PickerTab, label: "All" },
            ...WIDGET_CATEGORIES.map((category) => ({
              value: category as PickerTab,
              label: WIDGET_CATEGORY_LABELS[category],
            })),
          ]}
          value={tab}
          onValueChange={(next) => {
            setTab(next);
            setActive(0);
          }}
        />
      </div>
      {used >= MAX_WIDGETS ? (
        <p
          role="status"
          className="mx-4 mt-3 text-ui-sm text-pretty text-ink-2"
        >
          Your board is full. Remove a widget to add another.
        </p>
      ) : null}
      <ul
        id={listId}
        role="listbox"
        aria-label="Widgets"
        className="grid min-h-0 flex-1 grid-cols-2 gap-2 overflow-y-auto overscroll-contain p-4 @container max-[520px]:grid-cols-1"
      >
        {entries.length === 0 ? (
          <li className="col-span-full py-6 text-center text-ui-sm text-ink-3">
            No widget matches “{query}”.
          </li>
        ) : null}
        {entries.map((entry, index) => {
          const spec = WIDGET_SPECS[entry.type];
          const on = index === current;
          const disabled = !!entry.reason;
          return (
            <li
              key={entry.type}
              id={optionId(entry.type)}
              role="option"
              aria-selected={on}
              aria-disabled={disabled || undefined}
              data-type={entry.type}
              onPointerMove={() => setActive(index)}
              onClick={() => choose(entry)}
              className={cn(
                "relative flex min-w-0 cursor-default items-start gap-3 rounded-[2px] border border-line-strong p-3 transition-colors duration-(--dur-1)",
                (entry.type === "custom" || entry.type === "deadlock") &&
                  "col-span-full",
                on && "bg-tint-1 border-ink-2",
                disabled && "opacity-60",
              )}
            >
              {entry.type === "deadlock" ? (
                <span className="size-10 shrink-0 overflow-hidden rounded-[2px] bg-black">
                  <img
                    src={media.url(DEADLOCK_ICON_PATH)}
                    alt=""
                    className="size-full object-cover"
                    draggable={false}
                  />
                </span>
              ) : (
                <span className="grid size-10 shrink-0 place-items-center rounded-[2px] border border-line-strong text-ink-2">
                  <Icon
                    name={entry.locked ? "lock" : WIDGET_ICON[entry.type]}
                    size="md"
                  />
                </span>
              )}
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate font-display text-[15px] leading-5 font-bold text-ink-1">
                    {spec.name}
                  </span>
                  {entry.locked ? <LoudLock /> : null}
                  {entry.type === "deadlock" ? (
                    <>
                      <span className="silkscreen rounded-[2px] bg-tint-2 px-1 text-ink-3">
                        New
                      </span>
                      {entry.added === 0 ? (
                        <span className="silkscreen ml-auto inline-flex shrink-0 items-center gap-1 text-ink-3">
                          <Icon name="steam" size="xs" />
                          Needs Steam
                        </span>
                      ) : null}
                    </>
                  ) : null}
                  {entry.added > 0 ? (
                    <Icon
                      name="check"
                      size="sm"
                      className="ml-auto shrink-0 text-ink-2"
                      aria-label="On your board"
                    />
                  ) : null}
                </span>
                <span className="text-ui-xs text-pretty text-ink-3">
                  {blurbFor(entry)}
                </span>
                {entry.reason ? (
                  <span
                    data-slot="picker-reason"
                    className="mt-1 text-ui-xs font-medium text-ink-2"
                  >
                    {entry.reason}
                  </span>
                ) : null}
              </span>
            </li>
          );
        })}
      </ul>
      <div className="flex items-center gap-2 border-t border-line px-4 py-2.5 text-ui-xs text-ink-3 pointer-coarse:hidden">
        <Kbd size="sm">↑</Kbd>
        <Kbd size="sm">↓</Kbd>
        <span>choose</span>
        <Kbd size="sm" className="ml-2">
          Enter
        </Kbd>
        <span>add</span>
        <Kbd size="sm" className="ml-2">
          Esc
        </Kbd>
        <span>close</span>
        {activeEntry ? (
          <span className="ml-auto text-num-xs">
            Sizes {WIDGET_SPECS[activeEntry.type].sizes.join(", ")}
          </span>
        ) : null}
      </div>
    </div>
  );
}
