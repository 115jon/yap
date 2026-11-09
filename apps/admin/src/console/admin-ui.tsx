/**
 * Small pieces the console's sections share: the stat readout, the plan
 * chip, the table header cell, the search field and the load/error states.
 * Built from the kit (Num, Led, Badge, Kbd, EmptyState); nothing new.
 */

import { useEffect, useRef, useState, type ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Icon } from "@/components/ui/icon";
import { FIELD_UNDERLINE } from "@/components/ui/input";
import { Kbd } from "@/components/ui/kbd";
import { Led, type LedState } from "@/components/ui/led";
import { Skeleton } from "@/components/ui/skeleton";
import { BadgeArt } from "@/components/ui/user-badges";
import { describeApiError } from "@/lib/api-errors";
import {
  effectiveTier,
  TIER_NAMES,
  type Entitlement,
} from "@/lib/entitlements";
import { cn } from "@/lib/utils";

import { useSecondTick } from "./admin-data";
import { formatAgo, formatUntil, useFixedNow } from "./admin-format";

/** The console's "now": pinned in fixtures, a 1 Hz tick otherwise. */
export function useAdminNow(): number {
  const fixed = useFixedNow();
  const tick = useSecondTick(fixed === null);
  return fixed ?? tick;
}

/** A labelled readout: silkscreen label over a Martian figure. */
export function Readout({
  label,
  value,
  detail,
  led,
  className,
}: {
  label: ReactNode;
  value: ReactNode;
  detail?: ReactNode;
  led?: LedState;
  className?: string;
}) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-1.5", className)}>
      <p className="silkscreen flex items-center gap-2 text-ink-3">
        {led ? <Led state={led} /> : null}
        <span className="truncate">{label}</span>
      </p>
      <p className="font-silk text-[32px] leading-8 font-semibold text-ink-1 tabular-nums [font-stretch:90%] max-md:text-[26px]">
        {value}
      </p>
      {detail ? (
        <p className="truncate text-ui-xs text-ink-3">{detail}</p>
      ) : null}
    </div>
  );
}

/** A row of readouts under a 2px ink rule (DitherReadouts' frame). */
export function ReadoutRow({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "grid grid-cols-2 gap-x-6 gap-y-5 border-t-2 border-ink-1 pt-3 sm:grid-cols-3 lg:grid-cols-5",
        className,
      )}
    >
      {children}
    </div>
  );
}

/** The plan as a chip: tier name, its badge art, and Loud's time left. */
export function PlanChip({
  plan,
  now,
  detail = true,
  className,
}: {
  plan: Entitlement;
  now: number;
  /** Loud's time left and "lapsed" beside the chip. */
  detail?: boolean;
  className?: string;
}) {
  const tier = effectiveTier(plan, now);
  const lapsed = plan.tier !== "free" && tier === "free";
  return (
    <span className={cn("inline-flex min-w-0 items-center gap-1.5", className)}>
      <Badge
        variant={tier === "free" ? "ghost" : "secondary"}
        className="gap-1.5"
      >
        {tier === "founder" || tier === "loud" ? (
          <BadgeArt id={tier} size="sm" className="-ml-0.5" />
        ) : null}
        {TIER_NAMES[tier]}
      </Badge>
      {!detail ? null : tier === "loud" ? (
        <span className="truncate text-num-xs text-ink-3">
          {formatUntil(plan.expires_at, now)}
        </span>
      ) : lapsed ? (
        <span className="truncate text-num-xs text-warning-ink">lapsed</span>
      ) : null}
    </span>
  );
}

/** Online / offline / suspended as an LED word. */
export function PresenceWord({
  online,
  suspended,
  lastSeen,
  now,
}: {
  online: boolean | null;
  suspended: boolean;
  lastSeen: string | null;
  now: number;
}) {
  if (suspended) {
    return (
      <span className="silkscreen inline-flex items-center gap-2 text-danger-ink">
        <Led state="muted" />
        Suspended
      </span>
    );
  }
  if (online) {
    return (
      <span className="silkscreen inline-flex items-center gap-2 text-success-ink">
        <Led state="talk" />
        Online
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-2 text-num-xs text-ink-3">
      <Led state="off" />
      {formatAgo(lastSeen, now)}
    </span>
  );
}

export type SortDirection = "asc" | "desc";

/** A sortable column header: the whole cell is the key, aria-sort on the th. */
export function SortHeader<K extends string>({
  column,
  label,
  sort,
  onSort,
  align = "start",
  className,
}: {
  column: K;
  label: string;
  sort: { key: K; dir: SortDirection };
  onSort: (key: K) => void;
  align?: "start" | "end";
  className?: string;
}) {
  const active = sort.key === column;
  return (
    <th
      scope="col"
      aria-sort={
        active ? (sort.dir === "asc" ? "ascending" : "descending") : "none"
      }
      className={cn("h-9 px-0 font-normal", className)}
    >
      <button
        type="button"
        onClick={() => onSort(column)}
        className={cn(
          "silkscreen flex h-9 w-full items-center gap-1 px-3 whitespace-nowrap outline-offset-[-2px] pointer-fine:hover:text-ink-1",
          align === "end" ? "justify-end" : "justify-start",
          active ? "text-ink-1" : "text-ink-3",
        )}
      >
        {label}
        <Icon
          name={active && sort.dir === "asc" ? "chevronUp" : "chevronDown"}
          size="xs"
          className={cn("shrink-0", active ? "opacity-100" : "opacity-0")}
        />
      </button>
    </th>
  );
}

/** The C1 underline search field with its "/" key cap. */
export function SearchField({
  value,
  onChange,
  placeholder,
  label,
  inputRef,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  label: string;
  inputRef?: (input: HTMLInputElement | null) => void;
  className?: string;
}) {
  const local = useRef<HTMLInputElement | null>(null);
  const [focused, setFocused] = useState(false);
  return (
    <div
      className={cn(
        FIELD_UNDERLINE,
        "flex h-10 min-w-0 items-center gap-2",
        className,
      )}
    >
      <Icon name="search" size="sm" className="shrink-0 text-ink-3" />
      <input
        ref={(node) => {
          local.current = node;
          inputRef?.(node);
        }}
        type="search"
        aria-label={label}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            if (value) onChange("");
            else local.current?.blur();
          }
          if (event.key === "Enter" || event.key === "ArrowDown") {
            local.current?.blur();
          }
        }}
        className="rm-search-input h-full min-w-0 flex-1 bg-transparent text-ui-row text-ink-1 outline-none placeholder:text-ink-3 max-md:text-ui-md"
      />
      {!focused && !value ? (
        <Kbd
          size="sm"
          variant="outline"
          className="hidden pointer-fine:inline-flex"
        >
          /
        </Kbd>
      ) : null}
    </div>
  );
}

/** "Updated 4 s ago" with a light: talk while fresh, idle once stale, muted on error. */
export function Freshness({
  updatedAt,
  error,
  staleAfterMs,
  now,
}: {
  updatedAt: number | null;
  error: unknown;
  staleAfterMs: number;
  now: number;
}) {
  const stale = updatedAt === null || now - updatedAt > staleAfterMs;
  const state: LedState = error ? "muted" : stale ? "idle" : "talk";
  return (
    <span
      role="status"
      aria-live="polite"
      className={cn(
        "inline-flex items-center gap-2 text-num-xs whitespace-nowrap",
        error ? "text-danger-ink" : "text-ink-3",
      )}
    >
      <Led state={state} />
      {error
        ? "Refresh failed"
        : updatedAt === null
          ? "Loading"
          : `Updated ${formatAgo(updatedAt, now)}`}
    </span>
  );
}

/** Static skeleton rows for a table (no shimmer: nothing loops at rest). */
export function SkeletonRows({
  rows = 8,
  className,
}: {
  rows?: number;
  className?: string;
}) {
  return (
    <div
      aria-hidden="true"
      className={cn("flex flex-col gap-0.5 p-2", className)}
    >
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="flex h-11 items-center gap-3 px-2">
          <Skeleton className="size-8 rounded-avatar" />
          <Skeleton className="h-3 w-40 rounded-[2px]" />
          <Skeleton className="ml-auto h-3 w-16 rounded-[2px]" />
          <Skeleton className="h-3 w-12 rounded-[2px] max-md:hidden" />
          <Skeleton className="h-3 w-12 rounded-[2px] max-md:hidden" />
        </div>
      ))}
    </div>
  );
}

/** A section that failed to load: what failed and the one way back. */
export function LoadError({
  error,
  what,
  onRetry,
}: {
  error: unknown;
  what: string;
  onRetry: () => void;
}) {
  return (
    <EmptyState
      title={`Couldn't load ${what}`}
      description={describeApiError(error, `load ${what}`)}
      action={
        <Button variant="secondary" onClick={onRetry}>
          Try again
        </Button>
      }
    />
  );
}

/** Focus follows a row the keyboard selected, without scrolling the page. */
export function useScrollIntoView(
  ref: React.RefObject<HTMLElement | null>,
  active: boolean,
) {
  useEffect(() => {
    if (active) ref.current?.scrollIntoView({ block: "nearest" });
  }, [active, ref]);
}
