/**
 * The Activity tab of the full profile: a timeline the server writes (games,
 * calls, streams, Listen Together, badges, servers joined), drawn on the
 * Talk Tree guide, one branch per day group.
 *
 * Others see what the owner shares (the server already filtered it). The
 * owner sees everything, can hide single events (the eye on each row; a
 * hidden row prints muted with "Hidden") and sets who sees what in "Who sees
 * your activity" below the timeline. Every change is optimistic and reverts
 * with an error in words when the save fails.
 *
 * Nothing here ticks or animates at rest: times are printed once.
 */

import { getAuthAssetUrl } from "@/lib/platform";
import * as React from "react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Icon, type IconName } from "@/components/ui/icon";
import { IconButton } from "@/components/ui/IconButton";
import { Led } from "@/components/ui/led";
import { Num } from "@/components/ui/num";
import { SegmentedControl } from "@/components/ui/segmented-control";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { TreeBranch, TreeLeaf } from "@/components/ui/tree";
import { BadgeArt } from "@/components/ui/user-badges";
import { describeApiError } from "@/lib/api-errors";
import { isBadgeId } from "@/lib/badges";
import {
  ACTIVITY_KINDS,
  DEFAULT_ACTIVITY_SETTINGS,
  type ActivityEvent,
  type ActivityKind,
  type ActivityPage,
  type ActivityRetentionDays,
  type ActivitySettings,
} from "@/lib/profile-board";
import {
  fetchProfileActivity,
  setActivityEventHidden,
  updateProfilePrefs,
} from "@/lib/profile-board-api";
import { cn } from "@/lib/utils";

import {
  ACTIVITY_FILTER_LABELS,
  ACTIVITY_FILTERS,
  activityText,
  activityWhen,
  describeActivity,
  groupActivity,
  withinRetention,
  type ActivityFilter,
  type ActivityGroup,
  type ActivityGroupKind,
} from "./activity-format";
import { ActivitySettingsSection } from "./activity-settings";

export interface ProfileActivityProps {
  userId: string;
  isOwner: boolean;
  /** The owner's display name, for copy ("Kiwi shares this with…"). */
  displayName: string;
  /** Fixtures: render this page instead of fetching. */
  fixture?: ActivityPage;
}

type Load =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready" };

const FILTER_ITEMS = ACTIVITY_FILTERS.map((value) => ({
  value,
  label: ACTIVITY_FILTER_LABELS[value],
}));

export default function ProfileActivity({
  userId,
  isOwner,
  displayName,
  fixture,
}: ProfileActivityProps) {
  const [filter, setFilter] = React.useState<ActivityFilter>("all");
  const [load, setLoad] = React.useState<Load>(
    fixture ? { status: "ready" } : { status: "loading" },
  );
  const [events, setEvents] = React.useState<ActivityEvent[]>(
    () => fixture?.events ?? [],
  );
  const [cursor, setCursor] = React.useState<string | null>(
    fixture?.next_cursor ?? null,
  );
  const [more, setMore] = React.useState<"idle" | "loading" | "error">("idle");
  const [settings, setSettings] = React.useState<ActivitySettings | null>(
    fixture?.settings ?? null,
  );
  const [settingsError, setSettingsError] = React.useState<string | null>(null);
  const [reload, setReload] = React.useState(0);
  const savedSettings = React.useRef<ActivitySettings | null>(
    fixture?.settings ?? null,
  );
  const saveSeq = React.useRef(0);
  const settingsRef = React.useRef<HTMLElement>(null);
  // "Now" for the day groups: fixed per mount, so nothing re-renders on a clock.
  const [now] = React.useState(() => Date.now());

  const kind: ActivityKind | null = filter === "all" ? null : filter;

  // ── Load the first page (again on a filter change or a retry) ─────────
  React.useEffect(() => {
    if (fixture) {
      setEvents(
        kind ? fixture.events.filter((e) => e.kind === kind) : fixture.events,
      );
      setCursor(fixture.next_cursor);
      return;
    }
    const controller = new AbortController();
    setLoad({ status: "loading" });
    setMore("idle");
    fetchProfileActivity(userId, { kind, signal: controller.signal })
      .then((page) => {
        if (controller.signal.aborted) return;
        setEvents(page.events);
        setCursor(page.next_cursor);
        if (page.settings && savedSettings.current === null) {
          savedSettings.current = page.settings;
          setSettings(page.settings);
        }
        setLoad({ status: "ready" });
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setLoad({
          status: "error",
          message: describeApiError(error, "load this activity"),
        });
      });
    return () => controller.abort();
  }, [userId, kind, fixture, reload]);

  const loadMore = React.useCallback(() => {
    if (!cursor || fixture) return;
    setMore("loading");
    fetchProfileActivity(userId, { kind, cursor })
      .then((page) => {
        setEvents((current) => {
          const seen = new Set(current.map((e) => e.id));
          return [...current, ...page.events.filter((e) => !seen.has(e.id))];
        });
        setCursor(page.next_cursor);
        setMore("idle");
      })
      .catch(() => setMore("error"));
  }, [cursor, fixture, userId, kind]);

  // ── Owner: hide or show one event ─────────────────────────────────────
  const toggleHidden = React.useCallback(
    (event: ActivityEvent) => {
      const hidden = !event.hidden;
      const apply = (value: boolean) =>
        setEvents((current) =>
          current.map((e) => (e.id === event.id ? { ...e, hidden: value } : e)),
        );
      apply(hidden);
      if (fixture) return;
      setActivityEventHidden(event.id, hidden).catch((error: unknown) => {
        apply(!hidden);
        toast.error(
          hidden
            ? "Couldn't hide that event"
            : "Couldn't show that event again",
          {
            id: `activity-hide-${event.id}`,
            description: describeApiError(error, "change this event"),
          },
        );
      });
    },
    [fixture],
  );

  // ── Owner: who sees what, and for how long ────────────────────────────
  const saveSettings = React.useCallback(
    (next: ActivitySettings, patch: Partial<ActivitySettings>) => {
      setSettings(next);
      setSettingsError(null);
      if (fixture) {
        savedSettings.current = next;
        return;
      }
      const seq = ++saveSeq.current;
      updateProfilePrefs({ activity: patch })
        .then((prefs) => {
          savedSettings.current = prefs.activity;
          if (seq === saveSeq.current) setSettings(prefs.activity);
        })
        .catch((error: unknown) => {
          if (seq !== saveSeq.current) return;
          setSettings(savedSettings.current);
          setSettingsError(
            `${describeApiError(error, "save who sees your activity")} Your settings are back to how they were.`,
          );
        });
    },
    [fixture],
  );

  const current = settings ?? DEFAULT_ACTIVITY_SETTINGS;
  const onToggleKind = (toggled: ActivityKind, shown: boolean) => {
    const hidden = ACTIVITY_KINDS.filter((k) =>
      k === toggled ? !shown : current.hidden_kinds.includes(k),
    );
    saveSettings(
      { ...current, hidden_kinds: hidden },
      { hidden_kinds: hidden },
    );
  };
  const onRetention = (days: ActivityRetentionDays) => {
    if (days === current.retention_days) return;
    saveSettings(
      { ...current, retention_days: days },
      { retention_days: days },
    );
  };

  // A shorter retention drops what the server will no longer return.
  const visible = React.useMemo(
    () =>
      isOwner && settings
        ? withinRetention(events, settings.retention_days, now)
        : events,
    [events, isOwner, settings, now],
  );
  const groups = React.useMemo(
    () => groupActivity(visible, now),
    [visible, now],
  );

  const showSettings = isOwner && settings !== null;
  const hiddenKinds = current.hidden_kinds.length;

  return (
    <div data-slot="profile-activity" className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
        <SegmentedControl
          label="Show"
          size="sm"
          items={FILTER_ITEMS}
          value={filter}
          onValueChange={setFilter}
        />
        {isOwner && showSettings ? (
          <Button
            variant="link"
            size="sm"
            className="text-ui-sm"
            onClick={() => {
              settingsRef.current?.scrollIntoView({
                block: "start",
                behavior: prefersReducedMotion() ? "auto" : "smooth",
              });
              settingsRef.current?.focus({ preventScroll: true });
            }}
          >
            {hiddenKinds === 0
              ? "Others see everything"
              : `${hiddenKinds} ${hiddenKinds === 1 ? "kind" : "kinds"} only you see`}
          </Button>
        ) : null}
      </div>

      {load.status === "loading" ? (
        <ActivitySkeleton />
      ) : load.status === "error" ? (
        <EmptyState
          size="sm"
          motif="empty"
          title="Activity didn't load"
          description={load.message}
          action={
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setReload((n) => n + 1)}
            >
              <Icon name="refresh" size="sm" />
              Try again
            </Button>
          }
        />
      ) : groups.length === 0 ? (
        <ActivityEmpty
          isOwner={isOwner}
          displayName={displayName}
          filter={filter}
        />
      ) : (
        <div className="flex flex-col gap-4">
          {groups.map((group) => (
            <ActivityDay
              key={group.key}
              group={group}
              isOwner={isOwner}
              onToggleHidden={toggleHidden}
            />
          ))}
          {cursor ? (
            <div className="flex flex-col items-start gap-2 ps-3.5">
              <Button
                variant="secondary"
                size="sm"
                loading={more === "loading"}
                onClick={loadMore}
              >
                Show more
              </Button>
              {more === "error" ? (
                <p role="alert" className="text-ui-xs text-danger-ink">
                  Couldn't load more. Try again.
                </p>
              ) : null}
            </div>
          ) : null}
        </div>
      )}

      {showSettings ? (
        <ActivitySettingsSection
          ref={settingsRef}
          className="mt-2"
          settings={current}
          error={settingsError}
          onToggleKind={onToggleKind}
          onRetention={onRetention}
        />
      ) : null}
    </div>
  );
}

function prefersReducedMotion() {
  return (
    typeof window !== "undefined" &&
    !!window.matchMedia &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

// ── One day ──────────────────────────────────────────────────────────────

function ActivityDay({
  group,
  isOwner,
  onToggleHidden,
}: {
  group: ActivityGroup;
  isOwner: boolean;
  onToggleHidden: (event: ActivityEvent) => void;
}) {
  const headingId = React.useId();
  const count = group.events.length;
  return (
    <section aria-labelledby={headingId}>
      <TreeBranch
        guideX={6}
        rowHeight={56}
        header={
          <h3
            id={headingId}
            className="silkscreen flex h-6 items-center gap-2 text-ink-3"
          >
            <span>{group.label}</span>
            <Num className="ms-auto text-ink-3">
              {count}
              <span className="sr-only">
                {count === 1 ? " event" : " events"}
              </span>
            </Num>
          </h3>
        }
        childrenClassName="mt-1 gap-0.5"
      >
        {group.events.map((event) => (
          <ActivityRow
            key={event.id}
            event={event}
            groupKind={group.kind}
            isOwner={isOwner}
            onToggleHidden={onToggleHidden}
          />
        ))}
      </TreeBranch>
    </section>
  );
}

// ── One event ────────────────────────────────────────────────────────────

const KIND_ICON: Record<
  Exclude<ActivityKind, "badge" | "server_join">,
  IconName
> = {
  game: "gamepad",
  voice: "mic",
  stream: "stream",
  listen: "music",
};

function ActivityThumb({ event }: { event: ActivityEvent }) {
  if (event.kind === "badge") {
    return event.ref && isBadgeId(event.ref) ? (
      <BadgeArt id={event.ref} size="md" />
    ) : (
      <Icon name="trophy" size="lg" />
    );
  }
  if (event.kind === "server_join") {
    const name = event.data.server_name?.trim();
    return name ? (
      <span className="font-display text-[17px] leading-none font-extrabold text-ink-1">
        {Array.from(name)[0]?.toUpperCase()}
      </span>
    ) : (
      <Icon name="server" size="lg" />
    );
  }
  if (event.kind === "game" && event.data.icon_url) {
    return <GameIcon src={event.data.icon_url} />;
  }
  return <Icon name={KIND_ICON[event.kind]} size="lg" />;
}

function GameIcon({ src }: { src: string }) {
  const [failed, setFailed] = React.useState(false);
  if (failed) return <Icon name="gamepad" size="lg" />;
  return (
    <img
      src={getAuthAssetUrl(src)}
      alt=""
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      draggable={false}
      onError={() => setFailed(true)}
      className="size-full object-cover"
    />
  );
}

const ActivityRow = React.memo(function ActivityRow({
  event,
  groupKind,
  isOwner,
  onToggleHidden,
}: {
  event: ActivityEvent;
  groupKind: ActivityGroupKind;
  isOwner: boolean;
  onToggleHidden: (event: ActivityEvent) => void;
}) {
  const description = describeActivity(event);
  const when = activityWhen(event, groupKind);
  const hidden = isOwner && event.hidden;
  const sentence = activityText(description);
  return (
    <TreeLeaf
      data-kind={event.kind}
      data-hidden={hidden ? "" : undefined}
      node={
        <span className="flex h-10 items-center">
          <Led state="off" />
        </span>
      }
      className="group/ev"
      rowClassName={cn(
        "h-auto min-h-14 items-start gap-3 rounded-[2px] py-2 pe-2",
        "pointer-fine:hover:bg-tint-1",
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-[2px] bg-tint-2 text-ink-2",
          hidden && "opacity-50",
        )}
      >
        <ActivityThumb event={event} />
      </span>
      <span className="flex min-h-10 min-w-0 flex-1 flex-col justify-center">
        <span
          className={cn(
            "text-ui-row text-pretty",
            hidden ? "text-ink-3" : "text-ink-1",
          )}
        >
          {description.runs.map((run, i) =>
            run.strong ? (
              <strong key={i} className="font-semibold">
                {run.text}
              </strong>
            ) : (
              <React.Fragment key={i}>{run.text}</React.Fragment>
            ),
          )}
        </span>
        <span className="text-ui-xs text-ink-3">
          {hidden ? (
            <span className="silkscreen me-1.5 inline-flex items-center gap-1 align-[1px] text-ink-2">
              <Icon name="eyeOff" size="xs" />
              Hidden
            </span>
          ) : null}
          {[description.detail, when].filter(Boolean).join(" · ")}
        </span>
      </span>
      <span className="flex h-10 shrink-0 items-center gap-1">
        {description.readout ? (
          <Num size="sm" className={hidden ? "text-ink-3" : "text-ink-2"}>
            {description.readout}
          </Num>
        ) : null}
        {isOwner ? (
          <IconButton
            size={28}
            variant="muted"
            icon={hidden ? "eyeOff" : "eye"}
            aria-pressed={hidden}
            aria-label={
              hidden
                ? `Show to others: ${sentence}`
                : `Hide from others: ${sentence}`
            }
            onClick={() => onToggleHidden(event)}
            className={cn(
              !hidden &&
                "pointer-fine:opacity-0 pointer-fine:group-hover/ev:opacity-100 pointer-fine:focus-visible:opacity-100",
              "transition-opacity duration-(--dur-1)",
            )}
          />
        ) : null}
      </span>
    </TreeLeaf>
  );
});

// ── Loading and empty ────────────────────────────────────────────────────

function ActivitySkeleton() {
  return (
    <div role="status" className="flex flex-col gap-2">
      <span className="sr-only">Loading activity</span>
      <Skeleton className="h-3 w-20 rounded-[2px]" />
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="flex h-14 items-center gap-3 ps-3.5">
          <Skeleton className="size-1.5 rounded-full" />
          <Skeleton className="size-10 rounded-[2px]" />
          <span className="flex flex-1 flex-col gap-1.5">
            <Skeleton
              className={cn("h-3 rounded-[2px]", i % 2 ? "w-1/2" : "w-2/3")}
            />
            <Skeleton className="h-2.5 w-24 rounded-[2px]" />
          </span>
        </div>
      ))}
    </div>
  );
}

const FILTER_EMPTY_TITLE: Record<Exclude<ActivityFilter, "all">, string> = {
  game: "No games yet",
  voice: "No calls yet",
  badge: "No badges yet",
};

function ActivityEmpty({
  isOwner,
  displayName,
  filter,
}: {
  isOwner: boolean;
  displayName: string;
  filter: ActivityFilter;
}) {
  if (filter !== "all") {
    const title = FILTER_EMPTY_TITLE[filter];
    return (
      <EmptyState
        size="sm"
        title={title}
        description={
          isOwner
            ? "They show up here once they happen."
            : `${displayName} hasn't shared any yet.`
        }
      />
    );
  }
  return isOwner ? (
    <EmptyState
      size="sm"
      title="Nothing yet"
      description="Games, calls, badges and servers you join show up here."
    />
  ) : (
    <EmptyState
      size="sm"
      title="Nothing to show yet"
      description={`When ${displayName} plays, calls or earns a badge and shares it, it shows up here.`}
    />
  );
}
