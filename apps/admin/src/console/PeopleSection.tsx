/**
 * People: every account in one dense table (a list of rows on phones),
 * searchable, filterable and sortable, with the person's panel docked on
 * the trailing side at lg+ (a sheet below it, a drawer on phones).
 *
 * Keys: "/" searches, j/k (or the arrows) move the highlighted row, Enter
 * opens it, Escape closes the panel.
 */

import { useEffect, useMemo, useRef, useState } from "react";

import { Avatar } from "@/components/ui/avatar";
import { EmptyState } from "@/components/ui/empty-state";
import { Num } from "@/components/ui/num";
import { Tabs } from "@/components/ui/tabs";
import { BadgeArt } from "@/components/ui/user-badges";
import { useIsMobile } from "@/hooks/useMediaQuery";
import { assetUrl } from "./admin-format";
import type { AdminPerson } from "../shared/admin-api";
import { getBadge } from "@/lib/badges";
import { effectiveTier } from "@/lib/entitlements";
import { cn } from "@/lib/utils";

import {
  isTypingTarget,
  RefreshButton,
  SectionToolbar,
  useAdminShell,
} from "./AdminConsole";
import {
  formatBytes,
  formatCount,
  formatDate,
  formatHours,
  parseTime,
} from "./admin-format";
import {
  Freshness,
  LoadError,
  PlanChip,
  PresenceWord,
  SearchField,
  SkeletonRows,
  SortHeader,
  useAdminNow,
  type SortDirection,
} from "./admin-ui";
import { PersonPanel } from "./PersonPanel";

type Filter = "all" | "online" | "loud" | "founder" | "free" | "suspended";

type SortKey =
  | "name"
  | "joined"
  | "seen"
  | "plan"
  | "messages"
  | "voice"
  | "stream"
  | "servers"
  | "storage";

const TIER_RANK = { founder: 2, loud: 1, free: 0 } as const;

function displayName(person: AdminPerson): string {
  return person.display_name?.trim() || person.username;
}

function sortValue(
  person: AdminPerson,
  key: SortKey,
  now: number,
): number | string {
  switch (key) {
    case "name":
      return displayName(person).toLowerCase();
    case "joined":
      return parseTime(person.created_at) ?? 0;
    case "seen":
      return parseTime(person.last_active_at) ?? 0;
    case "plan":
      return TIER_RANK[effectiveTier(person.plan, now)];
    case "messages":
      return person.messages_sent;
    case "voice":
      return person.voice_seconds;
    case "stream":
      return person.stream_seconds;
    case "servers":
      return person.servers_joined;
    case "storage":
      return person.storage_bytes;
  }
}

function matches(person: AdminPerson, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  return (
    person.username.toLowerCase().includes(q) ||
    (person.display_name ?? "").toLowerCase().includes(q) ||
    person.id.toLowerCase() === q ||
    person.badges.some((badge) =>
      (getBadge(badge.id)?.name ?? badge.id).toLowerCase().includes(q),
    )
  );
}

export function PeopleSection() {
  const { people, selectedUserId, openPerson, registerSearch } =
    useAdminShell();
  const now = useAdminNow();
  const isMobile = useIsMobile();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [sort, setSort] = useState<{ key: SortKey; dir: SortDirection }>({
    key: "seen",
    dir: "desc",
  });
  const [cursor, setCursor] = useState(0);

  const online = useMemo(
    () =>
      people.data?.online_user_ids
        ? new Set(people.data.online_user_ids)
        : null,
    [people.data],
  );
  const all = people.data?.people ?? [];

  const counts = useMemo(() => {
    const result: Record<Filter, number> = {
      all: all.length,
      online: 0,
      loud: 0,
      founder: 0,
      free: 0,
      suspended: 0,
    };
    for (const person of all) {
      const tier = effectiveTier(person.plan, now);
      result[tier] += 1;
      if (person.suspended_at) result.suspended += 1;
      if (online?.has(person.id)) result.online += 1;
    }
    return result;
    // The tier counts only move on a day boundary; `now` ticks every second.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [all, online]);

  const rows = useMemo(() => {
    const filtered = all.filter((person) => {
      if (!matches(person, query.trim())) return false;
      switch (filter) {
        case "all":
          return true;
        case "online":
          return !!online?.has(person.id);
        case "suspended":
          return !!person.suspended_at;
        default:
          return effectiveTier(person.plan, now) === filter;
      }
    });
    const dir = sort.dir === "asc" ? 1 : -1;
    return filtered.sort((a, b) => {
      const av = sortValue(a, sort.key, now);
      const bv = sortValue(b, sort.key, now);
      if (av < bv) return -dir;
      if (av > bv) return dir;
      return displayName(a).localeCompare(displayName(b));
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [all, query, filter, sort, online]);

  // Keep the keyboard cursor on the open person, or inside the list.
  useEffect(() => {
    if (selectedUserId) {
      const index = rows.findIndex((person) => person.id === selectedUserId);
      if (index >= 0) setCursor(index);
    } else {
      setCursor((value) => Math.min(value, Math.max(0, rows.length - 1)));
    }
  }, [rows, selectedUserId]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey
      )
        return;
      if (isTypingTarget(event.target)) return;
      if (
        event.target instanceof HTMLElement &&
        event.target.closest(
          '[role="dialog"],[role="alertdialog"],[data-admin-panel]',
        )
      ) {
        return;
      }
      if (rows.length === 0) return;
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault();
        setCursor((value) => {
          const next = Math.min(rows.length - 1, value + 1);
          if (selectedUserId) openPerson(rows[next]!.id);
          return next;
        });
      } else if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault();
        setCursor((value) => {
          const next = Math.max(0, value - 1);
          if (selectedUserId) openPerson(rows[next]!.id);
          return next;
        });
      } else if (event.key === "Enter") {
        event.preventDefault();
        openPerson(rows[cursor]?.id ?? null);
      } else if (event.key === "Escape" && selectedUserId) {
        openPerson(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [rows, cursor, selectedUserId, openPerson]);

  const onSort = (key: SortKey) =>
    setSort((current) =>
      current.key === key
        ? { key, dir: current.dir === "asc" ? "desc" : "asc" }
        : { key, dir: key === "name" ? "asc" : "desc" },
    );

  const filters: { value: Filter; label: string }[] = [
    { value: "all", label: "Everyone" },
    { value: "online", label: "Online" },
    { value: "founder", label: "Founder" },
    { value: "loud", label: "Loud" },
    { value: "free", label: "Free" },
    { value: "suspended", label: "Suspended" },
  ];

  const selected = all.find((person) => person.id === selectedUserId) ?? null;

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <section
        aria-label="Accounts"
        className="flex min-h-0 min-w-0 flex-1 flex-col"
      >
        <SectionToolbar>
          <SearchField
            label="Search accounts"
            placeholder="Search by name, @username, badge or id"
            value={query}
            onChange={setQuery}
            inputRef={registerSearch}
            className="w-full max-w-[360px] max-md:max-w-none"
          />
          <div className="ml-auto flex items-center gap-3 max-md:hidden">
            <Freshness
              updatedAt={people.updatedAt}
              error={people.error}
              staleAfterMs={120_000}
              now={now}
            />
            <RefreshButton onRefresh={() => void people.refresh()} />
          </div>
        </SectionToolbar>
        <div className="shrink-0 px-2 max-md:px-1">
          <Tabs
            label="Filter accounts"
            scrollable
            items={filters.map((item) => ({
              value: item.value,
              label: item.label,
              adornment: (
                <Num className="text-ink-3" aria-hidden="true">
                  {formatCount(counts[item.value])}
                </Num>
              ),
            }))}
            value={filter}
            onValueChange={setFilter}
          />
        </div>
        <div className="min-h-0 flex-1 overflow-auto overscroll-contain custom-scrollbar">
          {people.loading && !people.data ? (
            <SkeletonRows rows={10} />
          ) : people.error && !people.data ? (
            <LoadError
              error={people.error}
              what="accounts"
              onRetry={() => void people.refresh()}
            />
          ) : rows.length === 0 ? (
            <EmptyState
              title={query ? `No one matches “${query.trim()}”` : "No one here"}
              description={
                query
                  ? "Search reads display names, usernames, badge names and exact ids."
                  : "No account fits this filter right now."
              }
            />
          ) : isMobile ? (
            <PeopleList
              rows={rows}
              online={online}
              cursor={cursor}
              now={now}
              onOpen={openPerson}
            />
          ) : (
            <PeopleTable
              rows={rows}
              online={online}
              cursor={cursor}
              selectedId={selectedUserId}
              compact={!!selectedUserId}
              sort={sort}
              onSort={onSort}
              now={now}
              onOpen={(id) => openPerson(id)}
            />
          )}
        </div>
        <p className="shrink-0 border-t border-line px-4 py-2 text-ui-xs text-ink-3 max-md:hidden">
          <Num>{rows.length}</Num> of <Num>{all.length}</Num> accounts. j/k to
          move, Enter to open.
        </p>
      </section>
      <PersonPanel
        userId={selectedUserId}
        person={selected}
        online={selectedUserId ? (online?.has(selectedUserId) ?? null) : null}
        onClose={() => openPerson(null)}
      />
    </div>
  );
}

function PeopleTable({
  rows,
  online,
  cursor,
  selectedId,
  compact,
  sort,
  onSort,
  now,
  onOpen,
}: {
  rows: AdminPerson[];
  online: Set<string> | null;
  cursor: number;
  selectedId: string | null;
  /** The panel is open beside the table: drop the lesser columns. */
  compact: boolean;
  sort: { key: SortKey; dir: SortDirection };
  onSort: (key: SortKey) => void;
  now: number;
  onOpen: (id: string) => void;
}) {
  const rowRefs = useRef<(HTMLTableRowElement | null)[]>([]);
  useEffect(() => {
    rowRefs.current[cursor]?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  return (
    <table
      className={cn(
        "w-full border-separate border-spacing-0 text-left",
        compact ? "min-w-[720px]" : "min-w-[1080px]",
      )}
    >
      <caption className="sr-only">
        Accounts. Column headers sort; press Enter on a row to open it.
      </caption>
      <thead className="sticky top-0 z-10 bg-ply-0">
        <tr className="[&>th]:border-b [&>th]:border-line">
          <SortHeader
            column="name"
            label="Person"
            sort={sort}
            onSort={onSort}
            className="pl-2"
          />
          <SortHeader column="plan" label="Plan" sort={sort} onSort={onSort} />
          <th
            scope="col"
            className="silkscreen h-9 px-3 font-normal text-ink-3"
          >
            Badges
          </th>
          {compact ? null : (
            <SortHeader
              column="joined"
              label="Joined"
              sort={sort}
              onSort={onSort}
            />
          )}
          <SortHeader
            column="seen"
            label="Last seen"
            sort={sort}
            onSort={onSort}
          />
          <SortHeader
            column="messages"
            label="Msgs"
            sort={sort}
            onSort={onSort}
            align="end"
          />
          <SortHeader
            column="voice"
            label="Voice"
            sort={sort}
            onSort={onSort}
            align="end"
          />
          {compact ? null : (
            <>
              <SortHeader
                column="stream"
                label="Stream"
                sort={sort}
                onSort={onSort}
                align="end"
              />
              <SortHeader
                column="servers"
                label="Servers"
                sort={sort}
                onSort={onSort}
                align="end"
              />
              <SortHeader
                column="storage"
                label="Storage"
                sort={sort}
                onSort={onSort}
                align="end"
                className="pr-2"
              />
            </>
          )}
        </tr>
      </thead>
      <tbody>
        {rows.map((person, index) => {
          const isCursor = index === cursor;
          const isOpen = person.id === selectedId;
          const name = displayName(person);
          return (
            <tr
              key={person.id}
              ref={(node) => {
                rowRefs.current[index] = node;
              }}
              data-cursor={isCursor ? "" : undefined}
              aria-selected={isOpen || undefined}
              onClick={() => onOpen(person.id)}
              className={cn(
                "group/row h-11 cursor-pointer text-ui-row [&>td]:border-b [&>td]:border-line",
                isOpen
                  ? "bg-tint-2"
                  : isCursor
                    ? "bg-tint-1"
                    : "pointer-fine:hover:bg-tint-1",
              )}
            >
              <td className="relative max-w-[280px] py-0 pr-3 pl-2">
                {isCursor ? (
                  <span
                    aria-hidden="true"
                    className="absolute top-2 bottom-2 left-0 w-0.5 bg-accent-ink"
                  />
                ) : null}
                <button
                  type="button"
                  tabIndex={isCursor ? 0 : -1}
                  onClick={(event) => {
                    event.stopPropagation();
                    onOpen(person.id);
                  }}
                  className="flex w-full min-w-0 items-center gap-2.5 rounded-[2px] text-left outline-offset-2"
                >
                  <Avatar
                    src={person.avatar_url ? assetUrl(person.avatar_url) : null}
                    name={name}
                    size={24}
                    presence={
                      person.suspended_at
                        ? "dnd"
                        : online?.has(person.id)
                          ? "online"
                          : undefined
                    }
                    ring={isOpen ? "ply-2" : "ply-0"}
                  />
                  <span className="flex min-w-0 flex-col">
                    <span className="truncate font-medium text-ink-1">
                      {name}
                    </span>
                  </span>
                  <span className="truncate text-ui-xs text-ink-3">
                    @{person.username}
                  </span>
                </button>
              </td>
              <td className="px-3">
                <PlanChip plan={person.plan} now={now} />
              </td>
              <td className="px-3">
                <span className="flex items-center gap-0.5">
                  {person.badges.slice(0, 5).map((badge) => (
                    <BadgeArt key={badge.id} id={badge.id} size="sm" />
                  ))}
                  {person.badges.length > 5 ? (
                    <Num className="ml-1 text-ink-3">
                      +{person.badges.length - 5}
                    </Num>
                  ) : null}
                  <span className="sr-only">
                    {person.badges
                      .map((badge) => getBadge(badge.id)?.name)
                      .join(", ") || "No badges"}
                  </span>
                </span>
              </td>
              {compact ? null : (
                <td className="px-3 text-num-xs whitespace-nowrap text-ink-2">
                  {formatDate(person.created_at)}
                </td>
              )}
              <td className="px-3 whitespace-nowrap">
                <PresenceWord
                  online={online ? online.has(person.id) : null}
                  suspended={!!person.suspended_at}
                  lastSeen={person.last_active_at}
                  now={now}
                />
              </td>
              <td className="px-3 text-right text-num-xs text-ink-2">
                {formatCount(person.messages_sent)}
              </td>
              <td className="px-3 text-right text-num-xs text-ink-2">
                {formatHours(person.voice_seconds)}
              </td>
              {compact ? null : (
                <>
                  <td className="px-3 text-right text-num-xs text-ink-2">
                    {formatHours(person.stream_seconds)}
                  </td>
                  <td className="px-3 text-right text-num-xs text-ink-2">
                    {person.servers_joined}
                  </td>
                  <td className="py-0 pr-2 pl-3 text-right text-num-xs text-ink-2">
                    {formatBytes(person.storage_bytes)}
                  </td>
                </>
              )}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function PeopleList({
  rows,
  online,
  cursor,
  now,
  onOpen,
}: {
  rows: AdminPerson[];
  online: Set<string> | null;
  cursor: number;
  now: number;
  onOpen: (id: string) => void;
}) {
  return (
    <ul aria-label="Accounts" className="flex flex-col py-1">
      {rows.map((person, index) => {
        const name = displayName(person);
        return (
          <li key={person.id}>
            <button
              type="button"
              onClick={() => onOpen(person.id)}
              className={cn(
                "flex min-h-14 w-full min-w-0 items-center gap-3 px-3 py-2 text-left active:bg-tint-2",
                index === cursor && "bg-tint-1",
              )}
            >
              <Avatar
                src={person.avatar_url ? assetUrl(person.avatar_url) : null}
                name={name}
                size={40}
                presence={
                  person.suspended_at
                    ? "dnd"
                    : online?.has(person.id)
                      ? "online"
                      : undefined
                }
                ring="ply-0"
              />
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate text-ui-md font-medium text-ink-1">
                    {name}
                  </span>
                  <span className="flex shrink-0 items-center gap-0.5">
                    {person.badges.slice(0, 3).map((badge) => (
                      <BadgeArt key={badge.id} id={badge.id} size="sm" />
                    ))}
                  </span>
                </span>
                <span className="flex min-w-0 items-center gap-2">
                  <PlanChip
                    plan={person.plan}
                    now={now}
                    detail={false}
                    className="shrink-0"
                  />
                  <span className="truncate text-num-xs text-ink-3">
                    {formatCount(person.messages_sent)} msgs ·{" "}
                    {formatHours(person.voice_seconds)}
                  </span>
                </span>
              </span>
              <span className="shrink-0">
                <PresenceWord
                  online={online ? online.has(person.id) : null}
                  suspended={!!person.suspended_at}
                  lastSeen={person.last_active_at}
                  now={now}
                />
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
