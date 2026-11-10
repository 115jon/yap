/**
 * Deadlock: the owner's career (rank, hero pool, the last ten matches, the
 * rank over 30 days) from the server's digest of deadlock-api.com, and what
 * they are doing in the game right now from presence.
 *
 * Live beats career only while live: a Deadlock activity swaps the plate
 * to the live variant (in match, in queue, in the Hideout when the desktop
 * app reports a phase; "Playing Deadlock" with its uptime otherwise) and it
 * returns to the owner's at-rest choice when the game closes. Nothing
 * moves at rest: the only clock is the live one, ticking in its own leaf
 * while the app is visible and focused (NumClock).
 *
 * States: not linked (owner: Link Steam; others never receive the plate),
 * loading (the first fetch, static blocks), no matches and deleted at
 * source (owner only), placement, stale (the last copy, its age in the
 * legend) and lapsed (no match in 30 days: all-time numbers, badge dimmed).
 *
 * Game art keeps its own colours (like user media); everything else is
 * tokens, so the plate follows the theme. All art comes from Yap's mirror.
 */

import {
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";

import { openSettings } from "@/components/chat/settings/settings-links";
import { Button } from "@/components/ui/button";
import { bayerLit, cellMask } from "@/components/ui/dither";
import { Icon } from "@/components/ui/icon";
import { LedLabel, Led } from "@/components/ui/led";
import { Num, NumClock } from "@/components/ui/num";
import { useBoxSize } from "@/components/ui/use-dither-geometry";
import { useGameActivity } from "@/hooks/useGameActivity";
import {
  DEADLOCK_CREDIT,
  deadlockAssetPath,
  heroAssetKey,
  isDeadlockActivity,
  itemAssetKey,
  kdaRatio,
  rankAssetKey,
  rankName,
  tierName,
  winRate,
  type DeadlockBoardData,
  type DeadlockDigest,
  type DeadlockHeroInfo,
  type DeadlockMatch,
  type DeadlockRange,
} from "@/lib/deadlock";
import type { GameActivity } from "@/lib/gateway-protocol";
import type { ProfileWidget } from "@/lib/profile-board";
import { cn } from "@/lib/utils";
import { useChatStore } from "@/stores/chat-store";

import type { BoardCtx } from "../board-format";
import { formatAge } from "../board-format";
import { useBoardMedia } from "../board-context";
import { Plate, WidgetTitle, type WidgetProps } from "./shared";

const DAY_S = 86_400;
const LAPSED_S = 30 * DAY_S;

const RANGE_SHORT: Record<DeadlockRange, string> = {
  "7d": "7d",
  "30d": "30d",
  season: "Season",
};
const RANGE_LONG: Record<DeadlockRange, string> = {
  "7d": "7 days",
  "30d": "30 days",
  season: "Season",
};
const MODE_LABEL = {
  ranked: "Ranked",
  unranked: "Unranked",
  brawl: "Street Brawl",
  bots: "Bots",
  private: "Private",
  other: "Match",
} as const;

type Live = NonNullable<GameActivity["game"]>;

// ── Formatting ─────────────────────────────────────────────────────────────

/** "35.4k" with the "k" small. */
function Thousands({ value }: { value: number }) {
  if (value < 1000) return <>{Math.round(value)}</>;
  return (
    <>
      {(value / 1000).toFixed(1)}
      <small className="ml-0.5 text-[11px] font-medium text-ink-3">k</small>
    </>
  );
}

function thousandsText(value: number): string {
  return value < 1000
    ? String(Math.round(value))
    : `${(value / 1000).toFixed(1)}k`;
}

/** "30:12". */
function clockText(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

function seasonShort(name: string | undefined): string {
  return (name ?? "Season").replace(/^Beta\s+/i, "");
}

function heroOf(digest: DeadlockDigest | null, id: number): DeadlockHeroInfo {
  return (
    digest?.heroes[String(id)] ?? {
      key: "",
      name: `Hero ${id}`,
      color: "#5A5A5A",
    }
  );
}

function heroByKey(
  digest: DeadlockDigest | null,
  key: string | undefined,
): DeadlockHeroInfo | null {
  if (!key) return null;
  const known = Object.values(digest?.heroes ?? {}).find(
    (hero) => hero.key === key,
  );
  return (
    known ?? {
      key,
      name: key
        .split("_")
        .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
        .join(" "),
      color: "#5A5A5A",
    }
  );
}

function joinNames(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
}

// ── Pieces ─────────────────────────────────────────────────────────────────

function RankBadge({
  badge,
  size = "lg",
  dim = false,
  alt,
}: {
  badge: number;
  size?: "lg" | "sm" | "xs";
  dim?: boolean;
  alt?: string;
}) {
  const media = useBoardMedia();
  const [failed, setFailed] = useState(false);
  const box =
    size === "lg"
      ? "h-16 w-20"
      : size === "sm"
        ? "h-10 w-[50px]"
        : "h-6 w-[30px]";
  if (failed) {
    return (
      <span
        role="img"
        aria-label={alt ?? `${rankName(badge)} rank badge`}
        className={cn("block shrink-0 rounded-[2px] bg-ply-4", box)}
      />
    );
  }
  return (
    <img
      src={media.url(deadlockAssetPath(rankAssetKey(badge)))}
      alt={alt ?? `${rankName(badge)} rank badge`}
      loading="lazy"
      decoding="async"
      draggable={false}
      onError={() => setFailed(true)}
      className={cn("block shrink-0 object-contain", box, dim && "opacity-60")}
    />
  );
}

function HeroIcon({
  hero,
  className,
  alt,
}: {
  hero: DeadlockHeroInfo;
  className?: string;
  alt?: string;
}) {
  const media = useBoardMedia();
  const [failed, setFailed] = useState(false);
  const box = cn(
    "block size-7 shrink-0 rounded-[2px] bg-ply-4 object-cover",
    className,
  );
  if (failed || !hero.key) {
    return (
      <span
        role={alt ? "img" : undefined}
        aria-label={alt || undefined}
        aria-hidden={alt ? undefined : true}
        className={cn(
          box,
          "grid place-items-center font-display text-[11px] font-bold text-ink-3",
        )}
      >
        {hero.name.charAt(0)}
      </span>
    );
  }
  return (
    <img
      src={media.url(deadlockAssetPath(heroAssetKey(hero.key, "sm")))}
      alt={alt ?? ""}
      loading="lazy"
      decoding="async"
      draggable={false}
      onError={() => setFailed(true)}
      className={box}
    />
  );
}

/** The hero art band: the hero's colour, a faint grille, the card on the right. */
function Band({
  hero,
  height,
  className,
  children,
}: {
  hero: DeadlockHeroInfo;
  height: number;
  className?: string;
  children?: ReactNode;
}) {
  const media = useBoardMedia();
  const [failed, setFailed] = useState(false);
  return (
    <div
      data-slot="deadlock-band"
      className={cn(
        "relative shrink-0 overflow-hidden rounded-[2px]",
        "bg-[linear-gradient(140deg,color-mix(in_srgb,var(--hc)_78%,#000)_0%,color-mix(in_srgb,var(--hc)_30%,#000)_100%)]",
        "before:absolute before:inset-0 before:bg-[repeating-linear-gradient(90deg,rgb(255_255_255/.04)_0_1px,transparent_1px_6px)] before:content-['']",
        className,
      )}
      style={{ ["--hc" as string]: hero.color, height }}
    >
      {hero.key && !failed ? (
        <img
          src={media.url(deadlockAssetPath(heroAssetKey(hero.key, "card")))}
          alt=""
          aria-hidden="true"
          loading="lazy"
          decoding="async"
          draggable={false}
          onError={() => setFailed(true)}
          className="absolute -top-2 right-1.5 w-auto drop-shadow-[0_6px_10px_rgb(0_0_0/.35)]"
          style={{ height: Math.round(height * 1.45) }}
        />
      ) : null}
      {children}
    </div>
  );
}

/** A dark tag on the art (game media keeps its own colours). */
function ArtTag({
  children,
  led = false,
}: {
  children: ReactNode;
  led?: boolean;
}) {
  return (
    <span className="absolute top-2.5 left-2.5 inline-flex h-[22px] items-center gap-1.5 rounded-[2px] bg-black/60 px-[7px] text-[#f2f2f3]">
      {led ? <Led state="talk" /> : null}
      <span className="silkscreen">{children}</span>
    </span>
  );
}

function ArtClock({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="absolute bottom-2.5 left-3 font-silk text-[30px] leading-[30px] font-semibold tracking-[-0.02em] text-[#f2f2f3] tabular-nums [font-stretch:90%] [text-shadow:0_1px_8px_rgb(0_0_0/.5)]">
      <small className="mb-1 block font-silk text-[11px] leading-[14px] font-medium tracking-[0.08em] text-[#d8d8dc] uppercase [text-shadow:none]">
        {label}
      </small>
      {children}
    </div>
  );
}

function Result({ win }: { win: boolean }) {
  return (
    <span
      className={cn(
        "inline-flex h-5 items-center rounded-[2px] px-1.5 font-silk text-[11px] font-semibold tracking-[0.08em] uppercase [font-stretch:90%]",
        win
          ? "bg-ink-1 text-ply-0"
          : "text-ink-2 shadow-[inset_0_0_0_1px_var(--line-strong)]",
      )}
    >
      {win ? "Win" : "Loss"}
    </span>
  );
}

function Kda({
  match,
  className,
}: {
  match: DeadlockMatch;
  className?: string;
}) {
  const slash = <i className="mx-px font-normal text-ink-3 not-italic">/</i>;
  return (
    <span
      className={cn(
        "font-silk text-[28px] leading-[30px] font-semibold tracking-[-0.02em] whitespace-nowrap text-ink-1 tabular-nums [font-stretch:90%]",
        className,
      )}
      aria-label={`${match.k} kills, ${match.d} deaths, ${match.a} assists`}
    >
      {match.k}
      {slash}
      {match.d}
      {slash}
      {match.a}
    </span>
  );
}

function RankTitle({
  badge,
  className,
}: {
  badge: number;
  className?: string;
}) {
  return (
    <p
      className={cn(
        "font-display text-[16px] leading-5 font-bold tracking-[-0.01em] whitespace-nowrap text-ink-1",
        className,
      )}
    >
      {rankName(badge)}
    </p>
  );
}

function Readout({ children }: { children: ReactNode }) {
  return <span className="silkscreen">{children}</span>;
}

function Caption({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <span className={cn("truncate text-ui-xs text-ink-3", className)}>
      {children}
    </span>
  );
}

function Big({ value, unit }: { value: ReactNode; unit?: string }) {
  return (
    <span className="font-silk text-[28px] leading-[30px] font-semibold tracking-[-0.02em] text-ink-1 tabular-nums [font-stretch:90%]">
      {value}
      {unit ? (
        <small className="ml-px text-[13px] font-medium tracking-normal text-ink-3">
          {unit}
        </small>
      ) : null}
    </span>
  );
}

/** Label over value; values in mono. */
function Stat({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <dt className="silkscreen text-ink-3">{label}</dt>
      <dd className="font-silk text-[18px] leading-[22px] font-semibold text-ink-1 tabular-nums [font-stretch:90%]">
        {children}
      </dd>
    </div>
  );
}

function ZoneHead({ label, aside }: { label: string; aside?: ReactNode }) {
  return (
    <div className="flex h-3.5 items-center gap-2 text-ink-3">
      <span className="silkscreen">{label}</span>
      {aside !== undefined ? (
        <Num size="sm" className="ml-auto text-ink-3">
          {aside}
        </Num>
      ) : null}
    </div>
  );
}

/** Hero pool: one series, one colour (accent ink), value as text. */
function Pool({
  digest,
  range,
  rows = 5,
}: {
  digest: DeadlockDigest;
  range: DeadlockRange;
  rows?: number;
}) {
  const pool = digest.ranges[range].pool.slice(0, rows);
  const max = Math.max(1, ...pool.map((entry) => entry.matches));
  return (
    <ol
      aria-label={`Hero pool, ${RANGE_LONG[range]}`}
      className="flex flex-col gap-1"
    >
      {pool.map((entry) => {
        const hero = heroOf(digest, entry.hero);
        const rate = winRate(entry.wins, entry.matches);
        return (
          <li
            key={entry.hero}
            title={`${hero.name} · ${entry.matches} matches · ${rate}% won`}
            className="grid h-5 grid-cols-[20px_68px_minmax(0,1fr)_24px_34px] items-center gap-2 rounded-[1px] pointer-fine:hover:bg-tint-1"
          >
            <HeroIcon hero={hero} className="size-5" />
            <span className="truncate text-[12px] leading-4 text-ink-1">
              {hero.name}
            </span>
            <span aria-hidden="true" className="relative block h-1.5">
              <i
                className="absolute inset-y-0 left-0 rounded-r-[2px] bg-accent-ink"
                style={{ width: `${(entry.matches / max) * 100}%` }}
              />
            </span>
            <Num size="sm" className="text-right text-ink-2">
              {entry.matches}
              <span className="sr-only"> matches,</span>
            </Num>
            <Num size="sm" className="text-right text-ink-3">
              {rate}%<span className="sr-only"> won</span>
            </Num>
          </li>
        );
      })}
    </ol>
  );
}

function matchTip(digest: DeadlockDigest, match: DeadlockMatch, now: number) {
  const hero = heroOf(digest, match.hero);
  return `${match.win ? "Win" : "Loss"} · ${hero.name} · ${match.k}/${match.d}/${match.a} · ${clockText(match.duration)} · ${formatAge(new Date((match.at + match.duration) * 1000).toISOString(), now)}`;
}

/** The last ten as hero faces: win = solid key + W, loss = dim face + L. */
function Strip({ digest, now }: { digest: DeadlockDigest; now: number }) {
  return (
    <ol
      aria-label="Last 10 matches, newest first"
      className="grid grid-cols-[repeat(5,28px)] content-start items-start gap-1.5"
    >
      {digest.last10.map((match) => {
        const hero = heroOf(digest, match.hero);
        return (
          <li
            key={match.id}
            title={matchTip(digest, match, now)}
            className="flex flex-col items-center gap-[3px]"
          >
            <HeroIcon
              hero={hero}
              className={cn(!match.win && "opacity-55 saturate-[.4]")}
            />
            <WL win={match.win} />
            <span className="sr-only">
              {match.win ? "Win" : "Loss"} as {hero.name}, {match.k}/{match.d}/
              {match.a}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

function WL({ win, className }: { win: boolean; className?: string }) {
  return (
    <b
      aria-hidden="true"
      className={cn(
        "flex h-3.5 w-7 items-center justify-center rounded-[1px] font-silk text-[11px] leading-3 font-semibold [font-stretch:90%]",
        win
          ? "bg-ink-1 text-ply-0"
          : "text-ink-3 shadow-[inset_0_0_0_1px_var(--line-strong)]",
        className,
      )}
    >
      {win ? "W" : "L"}
    </b>
  );
}

/** The last ten as letters only (M). */
function Letters({ digest, now }: { digest: DeadlockDigest; now: number }) {
  return (
    <ol
      aria-label="Last 10 matches, newest first"
      className="grid grid-cols-10 gap-[3px]"
    >
      {digest.last10.map((match) => (
        <li key={match.id} title={matchTip(digest, match, now)}>
          <WL win={match.win} className="h-[18px] w-full" />
          <span className="sr-only">
            {match.win ? "Win" : "Loss"} as {heroOf(digest, match.hero).name}
          </span>
        </li>
      ))}
    </ol>
  );
}

function record(matches: DeadlockMatch[]) {
  const wins = matches.filter((match) => match.win).length;
  return `${wins}–${matches.length - wins}`;
}

// ── Rank history: the sanctioned dither ─────────────────────────────────────

const CELL = 2;

/** Badge → a continuous ladder index (tier × 6 + division). */
function ladder(badge: number): number {
  return Math.floor(badge / 10) * 6 + (badge % 10) - 1;
}

/** The rank history's still mask (see RankHistory), or null with no rank. */
function rankMask(
  days: Array<number | null>,
  cols: number,
  rows: number,
  seasonIndex: number,
): string | null {
  const known = days.filter((day): day is number => !!day && day > 0);
  if (cols <= 0 || known.length === 0) return null;
  const lo = Math.min(...known.map(ladder)) - 1;
  const hi = Math.max(...known.map(ladder)) + 1;
  const scale = (rung: number) =>
    Math.round(rows - 2 - ((rung - lo) / Math.max(1, hi - lo)) * (rows - 5));
  const topTier = Math.floor(Math.max(...known) / 10);
  const boundary = ladder(topTier * 10 + 1) - 0.5;
  const step = cols / days.length;
  const levels = days.map((day) =>
    day && day > 0 ? scale(ladder(day)) : null,
  );
  const boundaryY = boundary > lo ? scale(boundary) : null;
  const seasonX =
    seasonIndex > 0 && seasonIndex < days.length
      ? Math.round(seasonIndex * step) - 1
      : null;
  return cellMask(cols, rows, (x, yy) => {
    const day = Math.min(days.length - 1, Math.floor(x / step));
    const level = levels[day];
    if (seasonX !== null && x === seasonX && yy % 2 === 0) return true;
    if (boundaryY !== null && yy === boundaryY && x % 2 === 0) return true;
    if (level === null || level === undefined) return false;
    if (yy === level) return true;
    // The step between days: a vertical run at the day's first column.
    const prev = day > 0 ? levels[day - 1] : null;
    if (
      prev !== null &&
      prev !== undefined &&
      x === Math.round(day * step) &&
      yy >= Math.min(prev, level) &&
      yy <= Math.max(prev, level)
    ) {
      return true;
    }
    return yy > level && bayerLit(x, yy, 0.42 - ((yy - level) / rows) * 0.3);
  });
}

/**
 * The rank at the end of each of the last 30 days: a dithered area under a
 * crisp step line in the accent ink, a dotted marker on the season reset,
 * the tier boundary as a hairline and the tier badges in the gutter. One
 * still mask per size and data (nothing runs at rest); hover names a day.
 */
export function RankHistory({
  digest,
  height,
}: {
  digest: DeadlockDigest;
  height: number;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const box = useBoxSize(hostRef);
  const [hover, setHover] = useState<number | null>(null);
  const days = digest.rank_days;
  const start = Date.parse(`${digest.rank_days_start}T00:00:00Z`);
  const known = days.filter((day): day is number => !!day && day > 0);
  const cols = box ? Math.floor(box.width / CELL) : 0;
  const rows = Math.max(8, Math.floor(height / CELL));
  const seasonIndex =
    digest.season && Number.isFinite(start)
      ? Math.floor((digest.season.start * 1000 - start) / 86_400_000)
      : -1;
  const mask = useMemo(
    () => rankMask(days, cols, rows, seasonIndex),
    [days, cols, rows, seasonIndex],
  );

  const dayLabel = (index: number) =>
    Number.isFinite(start)
      ? new Date(start + index * 86_400_000).toLocaleDateString(undefined, {
          day: "numeric",
          month: "short",
          timeZone: "UTC",
        })
      : `Day ${index + 1}`;
  const first = known.length ? days.find((day) => day && day > 0)! : 0;
  const last = known.length
    ? [...days].reverse().find((day) => day && day > 0)!
    : 0;
  const tiers = [...new Set(known.map((badge) => Math.floor(badge / 10)))]
    .sort((a, b) => b - a)
    .slice(0, 2);

  return (
    <div className="flex flex-col gap-1">
      <div className="flex gap-1">
        <div
          ref={hostRef}
          className="relative min-w-0 flex-1"
          style={{ height: rows * CELL }}
          onPointerMove={(event) => {
            if (!box || event.pointerType === "touch") return;
            const x =
              event.clientX - event.currentTarget.getBoundingClientRect().left;
            const index = Math.floor((x / box.width) * days.length);
            setHover(index >= 0 && index < days.length ? index : null);
          }}
          onPointerLeave={() => setHover(null)}
        >
          {hover !== null && box ? (
            <span
              aria-hidden="true"
              className="absolute inset-y-0 bg-tint-2"
              style={{
                left: (hover / days.length) * box.width,
                width: Math.max(2, box.width / days.length),
              }}
            />
          ) : null}
          {mask ? (
            <span
              aria-hidden="true"
              data-dither=""
              data-slot="rank-history"
              className="rm-dither absolute top-0 left-0"
              style={{
                width: cols * CELL,
                height: rows * CELL,
                backgroundColor: "var(--accent-ink)",
                maskImage: mask,
                WebkitMaskImage: mask,
                maskSize: "100% 100%",
                WebkitMaskSize: "100% 100%",
                maskRepeat: "no-repeat",
                WebkitMaskRepeat: "no-repeat",
              }}
            />
          ) : null}
          {seasonIndex > 0 && seasonIndex < days.length && box ? (
            <span
              aria-hidden="true"
              className="absolute top-0 text-num-xs leading-3 text-ink-3"
              style={{ left: (seasonIndex / days.length) * box.width + 4 }}
            >
              {seasonShort(digest.season?.name).replace(/^Season\s+/i, "S")}
            </span>
          ) : null}
          <table className="sr-only">
            <caption>
              Rank at the end of each day, last 30 days: from {rankName(first)}{" "}
              to {rankName(last)}
            </caption>
            <tbody>
              {days.map((day, index) => (
                <tr key={index}>
                  <th scope="row">{dayLabel(index)}</th>
                  <td>
                    {day === null
                      ? "No rank"
                      : day === 0
                        ? "Placement"
                        : rankName(day)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div
          aria-hidden="true"
          className="flex w-[22px] shrink-0 flex-col justify-between py-0.5"
        >
          {tiers.map((tier) => (
            <RankBadge key={tier} badge={tier * 10} size="xs" alt="" />
          ))}
        </div>
      </div>
      <div aria-hidden="true" className="mr-[26px] flex justify-between">
        {hover !== null ? (
          <span className="text-num-xs text-ink-1">
            {dayLabel(hover)} ·{" "}
            {days[hover] === null
              ? "No rank"
              : days[hover] === 0
                ? "Placement"
                : rankName(days[hover]!)}
          </span>
        ) : (
          <>
            <span className="text-num-xs text-ink-3">{dayLabel(0)}</span>
            <span className="text-num-xs text-ink-3">Today</span>
          </>
        )}
      </div>
    </div>
  );
}

// ── Party ──────────────────────────────────────────────────────────────────

interface PartyMember {
  id: string;
  name: string;
  avatar: string | null;
}

/** Party members the viewer knows (shares a server, a DM or friendship). */
function useParty(ids: readonly string[] | undefined): PartyMember[] {
  const members = useChatStore((state) => state.members);
  const byServer = useChatStore((state) => state.membersByServerId);
  const relationships = useChatStore((state) => state.relationships);
  const dms = useChatStore((state) => state.dmChannels);
  return useMemo(() => {
    if (!ids || ids.length === 0) return [];
    const found = new Map<string, PartyMember>();
    const add = (user: {
      id: string;
      username: string;
      display_name?: string | null;
      avatar_url?: string | null;
    }) => {
      if (!ids.includes(user.id) || found.has(user.id)) return;
      found.set(user.id, {
        id: user.id,
        name: user.display_name?.trim() || user.username,
        avatar: user.avatar_url ?? null,
      });
    };
    for (const member of members) add(member.user);
    for (const list of Object.values(byServer)) {
      for (const member of list) add(member.user);
    }
    for (const relationship of relationships) {
      const user = (relationship as { user?: Parameters<typeof add>[0] }).user;
      if (user) add(user);
    }
    for (const dm of dms) add(dm.recipient);
    return ids.flatMap((id) => {
      const member = found.get(id);
      return member ? [member] : [];
    });
  }, [ids, members, byServer, relationships, dms]);
}

function PartyStack({ party }: { party: PartyMember[] }) {
  const media = useBoardMedia();
  return (
    <span aria-hidden="true" className="flex shrink-0">
      {party.slice(0, 3).map((member, index) => (
        <span
          key={member.id}
          className={cn(
            "grid size-4 place-items-center overflow-hidden rounded-[2px] bg-ply-4 font-display text-[9px] font-extrabold text-ink-1 ring-2 ring-ply-2",
            index > 0 && "-ml-0.5",
          )}
        >
          {member.avatar ? (
            <img
              src={media.url(member.avatar)}
              alt=""
              className="size-full object-cover"
            />
          ) : (
            member.name.charAt(0).toUpperCase()
          )}
        </span>
      ))}
    </span>
  );
}

function PartyLine({
  party,
  where,
  className,
}: {
  party: PartyMember[];
  where?: string;
  className?: string;
}) {
  if (party.length === 0) return null;
  return (
    <p
      className={cn(
        "flex min-w-0 items-center gap-2 text-[12px] leading-4 text-ink-2",
        className,
      )}
    >
      <PartyStack party={party} />
      <span className="truncate">
        With {joinNames(party.map((member) => member.name))}
        {where ? `, ${where}` : ""}
      </span>
    </p>
  );
}

// ── The plate ──────────────────────────────────────────────────────────────

type Props = WidgetProps<"deadlock">;

export function DeadlockWidget({ widget, data, ctx }: Props) {
  const activity = useGameActivity(ctx.ownerId);
  const deadlock = data.deadlock;
  const live = isDeadlockActivity(activity ?? null) ? activity! : null;

  if (!deadlock) return <LoadingPlate widget={widget} ctx={ctx} />;

  if (deadlock.state === "not_linked") {
    return ctx.isOwner ? <NotLinkedPlate widget={widget} ctx={ctx} /> : null;
  }
  if (deadlock.state === "protected" || deadlock.state === "empty") {
    if (!ctx.isOwner) return null;
    return (
      <OwnerNotice
        widget={widget}
        ctx={ctx}
        state={deadlock.state}
        persona={deadlock.persona ?? null}
      />
    );
  }
  if (deadlock.state === "pending" || !deadlock.digest) {
    return <LoadingPlate widget={widget} ctx={ctx} />;
  }
  return (
    <CareerPlate
      widget={widget}
      ctx={ctx}
      data={deadlock}
      digest={deadlock.digest}
      live={live}
    />
  );
}

/** The plate with the credit as its tooltip and in the accessibility tree. */
function DeadlockPlate({
  widget,
  ctx,
  readout,
  readoutLive,
  className,
  children,
}: {
  widget: ProfileWidget<"deadlock">;
  ctx: BoardCtx;
  readout?: ReactNode;
  readoutLive?: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <Plate
      widget={widget}
      ctx={ctx}
      readout={readout}
      readoutLive={readoutLive}
      className={className}
    >
      <div title={DEADLOCK_CREDIT} className="contents">
        {children}
        <p className="sr-only">{DEADLOCK_CREDIT}</p>
      </div>
    </Plate>
  );
}

function OnlyYou() {
  return (
    <span className="silkscreen inline-flex items-center gap-1.5 text-ink-3">
      <Icon name="eyeOff" size="xs" />
      Only you
    </span>
  );
}

function NotLinkedPlate({
  widget,
  ctx,
}: {
  widget: ProfileWidget<"deadlock">;
  ctx: BoardCtx;
}) {
  return (
    <DeadlockPlate
      widget={widget}
      ctx={ctx}
      readout={<OnlyYou />}
      readoutLive
      className="border-dashed border-line-strong bg-transparent"
    >
      <div
        data-slot="deadlock-not-linked"
        className="flex flex-1 flex-col items-start justify-center gap-1.5"
      >
        <WidgetTitle>Show your Deadlock stats</WidgetTitle>
        <p className="max-w-[46ch] text-[13px] leading-[18px] text-pretty text-ink-2">
          Link the Steam account you play on. Friends see your rank, heroes and
          live matches.
        </p>
        <Button
          variant="secondary"
          size="sm"
          className="pointer-events-auto mt-1"
          onClick={() => openSettings({ panel: "connections" })}
        >
          <Icon name="steam" size="sm" />
          Link Steam
        </Button>
      </div>
    </DeadlockPlate>
  );
}

function OwnerNotice({
  widget,
  ctx,
  state,
  persona,
}: {
  widget: ProfileWidget<"deadlock">;
  ctx: BoardCtx;
  state: "empty" | "protected";
  persona: string | null;
}) {
  return (
    <DeadlockPlate
      widget={widget}
      ctx={ctx}
      readout={state === "empty" ? <Readout>Linked</Readout> : <OnlyYou />}
      readoutLive
    >
      <div
        data-slot={`deadlock-${state}`}
        className="flex flex-1 flex-col items-start justify-center gap-1.5"
      >
        <WidgetTitle>
          {state === "empty"
            ? "No matches yet"
            : "Stats turned off at the source"}
        </WidgetTitle>
        <p className="max-w-[46ch] text-[13px] leading-[18px] text-pretty text-ink-2">
          {state === "empty" ? (
            <>
              Play a match
              {persona ? (
                <>
                  {" "}
                  on{" "}
                  <Num size="sm" className="text-ink-2">
                    {persona}
                  </Num>
                </>
              ) : null}{" "}
              and it shows here within the hour.
            </>
          ) : (
            "This Steam account asked deadlock-api.com to delete its data, so there is nothing to show. Live matches still work on desktop."
          )}
        </p>
      </div>
    </DeadlockPlate>
  );
}

function Sk({
  className,
  style,
}: {
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <span
      aria-hidden="true"
      className={cn("block rounded-[1px] bg-tint-2", className)}
      style={style}
    />
  );
}

/** First fetch only: static blocks, no shimmer. */
function LoadingPlate({
  widget,
  ctx,
}: {
  widget: ProfileWidget<"deadlock">;
  ctx: BoardCtx;
}) {
  const readout = <Sk className="h-2 w-16" />;
  const body =
    widget.size === "S" ? (
      <>
        <div className="flex gap-3.5">
          <Sk className="h-16 w-20" />
          <div className="flex flex-1 flex-col gap-2">
            <Sk className="h-3.5 w-[70%]" />
            <Sk className="h-6 w-[40%]" />
          </div>
        </div>
        <div className="mt-auto">
          <Sk className="h-2.5 w-1/2 bg-tint-1" />
        </div>
      </>
    ) : widget.size === "M" ? (
      <>
        <div className="flex gap-3">
          <Sk className="h-10 w-[50px]" />
          <div className="flex flex-1 flex-col gap-2">
            <Sk className="h-5 w-[60%]" />
            <Sk className="h-2.5 w-[40%] bg-tint-1" />
          </div>
        </div>
        <div className="grid grid-cols-4 gap-2">
          {Array.from({ length: 4 }, (_, i) => (
            <Sk key={i} className="h-9" />
          ))}
        </div>
        <div className="flex flex-col gap-[9px]">
          {Array.from({ length: 5 }, (_, i) => (
            <Sk key={i} className="h-3.5" />
          ))}
        </div>
        <Sk className="mt-auto h-[18px] bg-tint-1" />
      </>
    ) : (
      <div className="flex min-h-0 flex-1 items-stretch gap-[18px]">
        <div className="flex w-[232px] shrink-0 flex-col gap-[9px] @max-[560px]/cell:hidden">
          {Array.from({ length: 5 }, (_, i) => (
            <Sk key={i} className="h-3.5" />
          ))}
        </div>
        <span className="w-px self-stretch bg-line @max-[560px]/cell:hidden" />
        <div className="grid grid-cols-[repeat(5,28px)] content-start gap-1.5">
          {Array.from({ length: 10 }, (_, i) => (
            <Sk key={i} className="h-[45px] w-7" />
          ))}
        </div>
        <span className="w-px self-stretch bg-line" />
        <div className="flex flex-1 flex-col gap-2">
          <Sk className="h-5 w-[60%]" />
          <Sk className="flex-1 bg-tint-1" />
        </div>
      </div>
    );
  return (
    <DeadlockPlate widget={widget} ctx={ctx} readout={readout} readoutLive>
      <div data-slot="deadlock-loading" aria-busy="true" className="contents">
        <span className="sr-only">Loading Deadlock stats</span>
        {body}
      </div>
    </DeadlockPlate>
  );
}

function CareerPlate({
  widget,
  ctx,
  data,
  digest,
  live,
}: {
  widget: ProfileWidget<"deadlock">;
  ctx: BoardCtx;
  data: DeadlockBoardData;
  digest: DeadlockDigest;
  live: GameActivity | null;
}) {
  const nowS = Math.floor(ctx.at / 1000);
  const range = widget.cfg.range;
  const stats = digest.ranges[range];
  const lapsed =
    digest.last_played_at !== null && nowS - digest.last_played_at > LAPSED_S;
  const badge = digest.rank?.badge ?? 0;
  const placing = badge === 0 && digest.rank?.calibration_left != null;
  const party = useParty(
    widget.cfg.show_party ? live?.game?.party_user_ids : undefined,
  );

  const stale =
    data.stale && data.fetched_at ? (
      <span
        className="silkscreen inline-flex items-center gap-1.5"
        title="deadlock-api.com did not answer. Showing the last copy."
      >
        <Icon name="cloudOff" size="xs" />
        {formatAge(data.fetched_at, ctx.at)} old
      </span>
    ) : null;

  if (live) {
    return (
      <LivePlate
        widget={widget}
        ctx={ctx}
        digest={digest}
        activity={live}
        party={party}
        stale={stale}
      />
    );
  }

  const lastMatch = digest.last_match;
  const endedAt = lastMatch
    ? new Date((lastMatch.at + lastMatch.duration) * 1000).toISOString()
    : null;

  if (lapsed) {
    return (
      <DeadlockPlate
        widget={widget}
        ctx={ctx}
        readout={
          stale ??
          (digest.last_played_at ? (
            <Readout>
              Played{" "}
              {Math.floor((ctx.at / 1000 - digest.last_played_at) / DAY_S)}d ago
            </Readout>
          ) : null)
        }
      >
        <LapsedBody digest={digest} />
      </DeadlockPlate>
    );
  }

  const careerReadout =
    stale ??
    (placing ? (
      <Readout>{seasonShort(digest.season?.name)}</Readout>
    ) : (
      <Readout>
        All modes ·{" "}
        {widget.size === "L" ? RANGE_LONG[range] : RANGE_SHORT[range]}
      </Readout>
    ));

  if (widget.cfg.at_rest === "last_match" && lastMatch && widget.size !== "L") {
    return (
      <DeadlockPlate
        widget={widget}
        ctx={ctx}
        readout={
          stale ??
          (endedAt ? (
            <Readout>
              {widget.size === "S" ? "Played " : ""}
              {formatAge(endedAt, ctx.at)} ago
            </Readout>
          ) : null)
        }
      >
        {widget.size === "S" ? (
          <LastMatchSmall digest={digest} stats={stats} />
        ) : (
          <LastMatchMedium digest={digest} />
        )}
      </DeadlockPlate>
    );
  }

  return (
    <DeadlockPlate widget={widget} ctx={ctx} readout={careerReadout}>
      {widget.size === "S" ? (
        placing ? (
          <PlacementBody digest={digest} />
        ) : (
          <CareerSmall digest={digest} range={range} />
        )
      ) : widget.size === "M" ? (
        <CareerMedium digest={digest} range={range} now={ctx.at} />
      ) : (
        <CareerLarge digest={digest} range={range} now={ctx.at} />
      )}
    </DeadlockPlate>
  );
}

function mainHero(digest: DeadlockDigest, range: DeadlockRange) {
  const top = digest.ranges[range].pool[0];
  return top ? heroOf(digest, top.hero) : null;
}

function CareerSmall({
  digest,
  range,
}: {
  digest: DeadlockDigest;
  range: DeadlockRange;
}) {
  const stats = digest.ranges[range];
  const main = mainHero(digest, range);
  const badge = digest.rank?.badge ?? 0;
  return (
    <>
      <div className="flex items-center gap-3.5">
        <RankBadge badge={badge} />
        <div className="min-w-0 flex-1">
          <RankTitle badge={badge} />
          <div className="mt-1 flex items-baseline gap-1.5">
            <Big value={winRate(stats.wins, stats.matches)} unit="%" />
            <Caption>won</Caption>
          </div>
        </div>
      </div>
      <div className="mt-auto flex min-w-0 items-center gap-2">
        <Num size="sm" className="text-ink-1">
          {stats.matches}
        </Num>
        <Caption>matches</Caption>
        {main ? (
          <span className="ml-auto flex min-w-0 items-center gap-2 text-ui-xs text-ink-3">
            <HeroIcon hero={main} className="size-5" />
            <span className="truncate">{main.name} main</span>
          </span>
        ) : null}
      </div>
    </>
  );
}

function PlacementBody({ digest }: { digest: DeadlockDigest }) {
  const total = digest.season?.calibration_matches ?? 0;
  const left = digest.rank?.calibration_left ?? 0;
  const played = Math.max(0, total - left);
  const seasonStart = digest.season?.start ?? 0;
  const results = digest.last10
    .filter((match) => match.mode === "ranked" && match.at >= seasonStart)
    .reverse();
  return (
    <>
      <div className="flex items-center gap-3.5">
        <RankBadge badge={0} alt="Unranked badge" />
        <div className="min-w-0 flex-1">
          <p className="font-display text-[16px] leading-5 font-bold tracking-[-0.01em] text-ink-1">
            Placement
          </p>
          <Caption className="block">
            {left} of {total} matches left
          </Caption>
          {total > 0 ? (
            <div className="mt-2 flex gap-[3px]" aria-hidden="true">
              {Array.from({ length: total }, (_, index) => {
                const result = index < played ? results[index] : undefined;
                return (
                  <span
                    key={index}
                    className={cn(
                      "h-1.5 w-3.5 rounded-[1px]",
                      index >= played
                        ? "shadow-[inset_0_0_0_1px_var(--line-strong)]"
                        : result && !result.win
                          ? "bg-ink-3"
                          : "bg-ink-1",
                    )}
                  />
                );
              })}
            </div>
          ) : null}
        </div>
      </div>
      {digest.previous_badge ? (
        <div className="mt-auto flex items-center gap-2">
          <Caption>Last season</Caption>
          <Num size="sm" className="text-ink-2">
            {rankName(digest.previous_badge)}
          </Num>
        </div>
      ) : null}
    </>
  );
}

function LapsedBody({ digest }: { digest: DeadlockDigest }) {
  const badge = digest.rank?.badge || digest.previous_badge || 0;
  const lastRanked = digest.last_ranked_at
    ? new Date(digest.last_ranked_at * 1000).toLocaleDateString(undefined, {
        day: "numeric",
        month: "short",
      })
    : null;
  return (
    <>
      <div className="flex items-center gap-3.5">
        <RankBadge
          badge={Math.floor(badge / 10) * 10}
          size="sm"
          dim
          alt={`${tierName(badge)} badge`}
        />
        <div className="min-w-0 flex-1">
          <RankTitle badge={badge} />
          {lastRanked ? (
            <Caption className="block">
              Last ranked {lastRanked}
              {digest.season ? ` · ${seasonShort(digest.season.name)}` : ""}
            </Caption>
          ) : null}
        </div>
      </div>
      <div className="mt-auto flex items-center gap-2">
        <Caption>All time</Caption>
        <Num size="sm" className="ml-auto text-ink-2">
          {digest.all_time.matches} matches ·{" "}
          {winRate(digest.all_time.wins, digest.all_time.matches)}% won
        </Num>
      </div>
    </>
  );
}

function LastMatchSmall({
  digest,
  stats,
}: {
  digest: DeadlockDigest;
  stats: DeadlockDigest["ranges"][DeadlockRange];
}) {
  const match = digest.last_match!;
  const hero = heroOf(digest, match.hero);
  const badge = digest.rank?.badge ?? 0;
  return (
    <>
      <div className="flex items-center gap-3.5">
        <RankBadge badge={badge} size="sm" />
        <div className="min-w-0 flex-1">
          <RankTitle badge={badge} />
          <Caption className="block">
            {winRate(stats.wins, stats.matches)}% won · {stats.matches} matches
          </Caption>
        </div>
      </div>
      <div className="mt-auto flex items-center gap-2.5 border-t border-line pt-2.5">
        <HeroIcon hero={hero} />
        <div className="min-w-0 flex-1">
          <Caption className="block">
            Last match · {hero.name} · {clockText(match.duration)}
          </Caption>
          <div className="mt-0.5 flex items-center gap-2">
            <Result win={match.win} />
            <Num size="sm" className="text-ink-2">
              {match.k}/{match.d}/{match.a}
            </Num>
            <Num className="ml-auto text-ink-3">
              {thousandsText(match.souls)} souls
            </Num>
          </div>
        </div>
      </div>
    </>
  );
}

function LastMatchMedium({ digest }: { digest: DeadlockDigest }) {
  const media = useBoardMedia();
  const match = digest.last_match!;
  const hero = heroOf(digest, match.hero);
  const mode = MODE_LABEL[match.mode];
  return (
    <>
      <Band hero={hero} height={120}>
        <ArtTag>
          {match.win ? "Win" : "Loss"} · {mode}
        </ArtTag>
      </Band>
      <div className="flex items-end justify-between gap-2">
        <div className="min-w-0">
          <WidgetTitle>{hero.name}</WidgetTitle>
          <Caption className="block">
            {clockText(match.duration)}
            {match.badge ? ` · ${rankName(match.badge)}` : ` · ${mode}`}
          </Caption>
        </div>
        <Kda match={match} />
      </div>
      <dl className="grid grid-cols-3 gap-2">
        <Stat label="Souls">
          <Thousands value={match.souls} />
        </Stat>
        <Stat label="Last hits">{match.last_hits}</Stat>
        {match.delta !== null ? (
          <Stat label="Rank">
            {match.delta > 0 ? "+" : ""}
            {match.delta}
          </Stat>
        ) : (
          <Stat label="Mode">{mode}</Stat>
        )}
        {match.hero_damage !== null ? (
          <Stat label="Hero dmg">
            <Thousands value={match.hero_damage} />
          </Stat>
        ) : null}
        {match.objective_damage !== null ? (
          <Stat label="Obj. dmg">
            <Thousands value={match.objective_damage} />
          </Stat>
        ) : null}
        {match.level !== null ? <Stat label="Level">{match.level}</Stat> : null}
      </dl>
      {match.items.length > 0 ? (
        <div className="mt-auto flex flex-col gap-1.5">
          <span className="silkscreen text-ink-3">Final build</span>
          <ul className="flex gap-1">
            {match.items.map((id) => {
              const name = digest.items[String(id)]?.name ?? "Item";
              return (
                <li key={id} title={name}>
                  <img
                    src={media.url(deadlockAssetPath(itemAssetKey(id)))}
                    alt={name}
                    loading="lazy"
                    decoding="async"
                    draggable={false}
                    className="block size-9 rounded-[2px] bg-ply-4"
                  />
                </li>
              );
            })}
          </ul>
        </div>
      ) : null}
    </>
  );
}

function CareerMedium({
  digest,
  range,
  now,
}: {
  digest: DeadlockDigest;
  range: DeadlockRange;
  now: number;
}) {
  const stats = digest.ranges[range];
  const badge = digest.rank?.badge ?? 0;
  const week = digest.ranges["7d"].rank_delta;
  const placing = badge === 0 && digest.rank?.calibration_left != null;
  return (
    <>
      <div className="flex items-center gap-3">
        <RankBadge badge={badge} size="sm" />
        <div className="min-w-0">
          <p className="font-display text-[20px] leading-6 font-bold tracking-[-0.01em] whitespace-nowrap text-ink-1">
            {placing ? "Placement" : rankName(badge)}
          </p>
          <Caption className="block">
            {digest.season ? seasonShort(digest.season.name) : "Deadlock"}
            {placing
              ? ` · ${digest.rank?.calibration_left} placement matches left`
              : week !== 0
                ? ` · ${week > 0 ? "+" : ""}${week} this week`
                : ""}
          </Caption>
        </div>
      </div>
      <dl className="grid grid-cols-4 gap-2">
        <Stat label="Won">
          {winRate(stats.wins, stats.matches)}
          <small className="ml-0.5 text-[11px] font-medium text-ink-3">%</small>
        </Stat>
        <Stat label="KDA">{kdaRatio(stats).toFixed(1)}</Stat>
        <Stat label="Games">{stats.matches}</Stat>
        <Stat label="Hours">{Math.round(stats.seconds / 3600)}</Stat>
      </dl>
      {stats.pool.length > 0 ? (
        <div className="flex min-w-0 flex-col gap-1">
          <ZoneHead label="Hero pool" aside="games · won" />
          <Pool digest={digest} range={range} />
        </div>
      ) : null}
      {digest.last10.length > 0 ? (
        <div className="mt-auto flex min-w-0 flex-col gap-2">
          <ZoneHead
            label={`Last ${digest.last10.length}`}
            aside={record(digest.last10)}
          />
          <Letters digest={digest} now={now} />
        </div>
      ) : null}
    </>
  );
}

function CareerLarge({
  digest,
  range,
  now,
}: {
  digest: DeadlockDigest;
  range: DeadlockRange;
  now: number;
}) {
  const stats = digest.ranges[range];
  const badge = digest.rank?.badge ?? 0;
  // Points moved this week, as on the M plate.
  const delta = digest.ranges["7d"].rank_delta;
  return (
    <div className="flex min-h-0 flex-1 items-stretch gap-[18px]">
      <div className="flex w-[232px] shrink-0 flex-col gap-2 @max-[560px]/cell:hidden">
        <ZoneHead
          label="Hero pool"
          aside={`${stats.matches} · ${winRate(stats.wins, stats.matches)}%`}
        />
        <Pool digest={digest} range={range} />
      </div>
      <span className="w-px shrink-0 self-stretch bg-line @max-[560px]/cell:hidden" />
      <div className="flex shrink-0 flex-col gap-2">
        <ZoneHead
          label={`Last ${digest.last10.length}`}
          aside={record(digest.last10)}
        />
        <Strip digest={digest} now={now} />
      </div>
      <span className="w-px shrink-0 self-stretch bg-line" />
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex h-7 items-center gap-2">
          <RankBadge badge={badge} size="xs" />
          <span className="truncate font-display text-[14px] leading-5 font-bold text-ink-1">
            {badge ? rankName(badge) : "Placement"}
          </span>
          {delta !== 0 ? (
            <Num className="ml-auto text-ink-2">
              {delta > 0 ? "+" : ""}
              {delta}
            </Num>
          ) : null}
        </div>
        <RankHistory digest={digest} height={76} />
      </div>
    </div>
  );
}

// ── Live ───────────────────────────────────────────────────────────────────

function phaseOf(game: Live | undefined) {
  switch (game?.phase) {
    case "in_match":
    case "match_intro":
      return "match" as const;
    case "queue":
      return "queue" as const;
    case "hideout":
    case "menu":
    case "post_match":
    case "spectating":
      return "hideout" as const;
    default:
      return "playing" as const;
  }
}

function LivePlate({
  widget,
  ctx,
  digest,
  activity,
  party,
  stale,
}: {
  widget: ProfileWidget<"deadlock">;
  ctx: BoardCtx;
  digest: DeadlockDigest;
  activity: GameActivity;
  party: PartyMember[];
  stale: ReactNode;
}) {
  const game = activity.game;
  const phase = phaseOf(game);
  const hero = heroByKey(digest, game?.hero);
  const mode = game?.mode ? MODE_LABEL[game.mode] : null;
  const badge = digest.rank?.badge ?? 0;
  const since =
    phase === "match" || phase === "queue"
      ? (game?.phase_started_at ?? activity.started_at)
      : activity.started_at;
  const clock = (size: "art" | "sm") =>
    size === "art" ? (
      <NumClock
        since={since}
        now={ctx.now}
        className="text-inherit [font:inherit]"
      />
    ) : (
      <NumClock
        since={since}
        now={ctx.now}
        className="font-silk text-[22px] leading-6 font-semibold tracking-[-0.02em] text-ink-1 [font-stretch:90%]"
      />
    );

  const led =
    phase === "match" ? (
      <LedLabel state="talk">In match</LedLabel>
    ) : phase === "queue" ? (
      <LedLabel state="idle">In queue</LedLabel>
    ) : phase === "hideout" ? (
      <LedLabel state="idle">In game</LedLabel>
    ) : (
      <LedLabel state="talk">Playing</LedLabel>
    );
  const readout = led;

  const today = digest.last10.filter(
    (match) =>
      match.at >= Math.floor(ctx.at / 1000) - 86_400 &&
      new Date(match.at * 1000).toDateString() ===
        new Date(ctx.at).toDateString(),
  );
  const heroStats = hero
    ? digest.ranges["30d"].pool.find(
        (entry) => heroOf(digest, entry.hero).key === hero.key,
      )
    : undefined;
  const last = digest.last_match;
  const partyNames = party.length > 0;

  const title =
    phase === "match"
      ? hero
        ? `In match as ${hero.name}`
        : "In a match"
      : phase === "queue"
        ? "Looking for a match"
        : phase === "hideout"
          ? "In the Hideout"
          : "Playing Deadlock";
  const subtitle =
    phase === "match"
      ? [mode, badge ? rankName(badge) : null].filter(Boolean).join(" · ")
      : phase === "queue"
        ? [mode, hero ? `${hero.name} picked in the Hideout` : null]
            .filter(Boolean)
            .join(" · ")
        : phase === "hideout"
          ? `Between matches${hero ? ` · as ${hero.name}` : ""}`
          : badge
            ? rankName(badge)
            : "Career stats below";
  const clockUnit =
    phase === "queue" ? "in queue" : phase === "match" ? null : "in game";

  if (widget.size === "S" || (widget.size === "M" && !hero)) {
    const footer =
      phase === "hideout" && last ? (
        <>
          <Caption>Last match</Caption>
          <Result win={last.win} />
          <Num size="sm" className="text-ink-2">
            {last.k}/{last.d}/{last.a}
          </Num>
        </>
      ) : partyNames ? (
        <>
          <PartyStack party={party} />
          <Caption className="text-ink-2">
            {phase === "queue"
              ? `Party of ${party.length + 1} with ${joinNames(party.map((m) => m.name))}`
              : `With ${joinNames(party.map((m) => m.name))}`}
          </Caption>
        </>
      ) : (
        <>
          <Num size="sm" className="text-ink-1">
            {digest.ranges["30d"].matches}
          </Num>
          <Caption>matches in 30 days</Caption>
        </>
      );
    return (
      <DeadlockPlate
        widget={widget}
        ctx={ctx}
        readout={stale ?? readout}
        readoutLive
      >
        <div
          data-slot="deadlock-live"
          data-phase={phase}
          className="flex items-center gap-3.5"
        >
          {hero ? (
            <HeroIcon hero={hero} className="size-14" alt={hero.name} />
          ) : (
            <RankBadge badge={badge} size="sm" />
          )}
          <div className="min-w-0 flex-1">
            <WidgetTitle className="truncate">{title}</WidgetTitle>
            {subtitle ? <Caption className="block">{subtitle}</Caption> : null}
            <div className="mt-1.5 flex items-baseline gap-1.5">
              {clock("sm")}
              {clockUnit ? <Caption>{clockUnit}</Caption> : null}
            </div>
          </div>
        </div>
        {widget.size === "M" ? (
          <>
            {digest.ranges["30d"].pool.length > 0 ? (
              <div className="flex min-w-0 flex-col gap-1">
                <ZoneHead label="Hero pool" aside="games · won" />
                <Pool digest={digest} range="30d" />
              </div>
            ) : null}
            {digest.last10.length > 0 ? (
              <div className="mt-auto flex min-w-0 flex-col gap-2">
                <ZoneHead
                  label={`Last ${digest.last10.length}`}
                  aside={record(digest.last10)}
                />
                <Letters digest={digest} now={ctx.at} />
              </div>
            ) : null}
          </>
        ) : (
          <div className="mt-auto flex min-w-0 items-center gap-2">
            {footer}
          </div>
        )}
      </DeadlockPlate>
    );
  }

  if (widget.size === "M") {
    return (
      <DeadlockPlate
        widget={widget}
        ctx={ctx}
        readout={stale ?? readout}
        readoutLive
      >
        <div data-slot="deadlock-live" data-phase={phase} className="contents">
          <Band hero={hero!} height={150}>
            {mode ? <ArtTag led={phase === "match"}>{mode}</ArtTag> : null}
            <ArtClock
              label={
                phase === "match"
                  ? "Match time"
                  : phase === "queue"
                    ? "In queue"
                    : "In game"
              }
            >
              {clock("art")}
            </ArtClock>
          </Band>
          <div>
            <WidgetTitle>{hero!.name}</WidgetTitle>
            <Caption className="block">
              {phase === "match"
                ? subtitle
                : phase === "queue"
                  ? "Looking for a match"
                  : "In the Hideout"}
            </Caption>
          </div>
          <PartyLine party={party} />
          <div className="mt-auto flex flex-col gap-1.5 border-t border-line pt-2.5">
            {today.length > 0 ? (
              <div className="flex items-center justify-between gap-2">
                <Caption>Today</Caption>
                <span className="flex items-center gap-1.5">
                  <span className="flex items-center gap-1.5 @max-[260px]/cell:hidden">
                    {today
                      .slice(0, 4)
                      .reverse()
                      .map((match) => (
                        <Result key={match.id} win={match.win} />
                      ))}
                  </span>
                  <Num size="sm" className="ml-1 text-ink-2">
                    {record(today)}
                  </Num>
                </span>
              </div>
            ) : null}
            {heroStats ? (
              <div className="flex items-center justify-between gap-2">
                <Caption className="@max-[260px]/cell:hidden">
                  {hero!.name}, 30 days
                </Caption>
                <Num size="sm" className="truncate text-ink-2">
                  {heroStats.matches} matches ·{" "}
                  {winRate(heroStats.wins, heroStats.matches)}% won
                </Num>
              </div>
            ) : (
              <div className="flex items-center justify-between gap-2">
                <Caption>30 days</Caption>
                <Num size="sm" className="text-ink-2">
                  {digest.ranges["30d"].matches} matches
                </Num>
              </div>
            )}
          </div>
        </div>
      </DeadlockPlate>
    );
  }

  // L: live art on the left; the career strip stays, so the plate keeps its shape.
  return (
    <DeadlockPlate
      widget={widget}
      ctx={ctx}
      readout={stale ?? readout}
      readoutLive
    >
      <div
        data-slot="deadlock-live"
        data-phase={phase}
        className="flex min-h-0 flex-1 items-stretch gap-[18px]"
      >
        {hero ? (
          <div className="flex w-[230px] shrink-0 @max-[560px]/cell:hidden">
            <Band hero={hero} height={134} className="flex-1">
              {mode ? <ArtTag led={phase === "match"}>{mode}</ArtTag> : null}
              <ArtClock label={hero ? `As ${hero.name}` : "In game"}>
                {clock("art")}
              </ArtClock>
            </Band>
          </div>
        ) : null}
        <div className="flex min-w-0 flex-1 flex-col justify-between gap-2">
          <div>
            <WidgetTitle className="truncate text-[18px] leading-[22px]">
              {title}
            </WidgetTitle>
            {partyNames ? (
              <PartyLine party={party} className="mt-1" />
            ) : (
              <Caption className="mt-1 block">
                {subtitle}
                {!hero ? <> · {clock("sm")}</> : null}
              </Caption>
            )}
          </div>
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2 border-t border-line pt-2.5">
              <RankBadge badge={badge} size="xs" />
              <span className="truncate font-display text-[14px] leading-5 font-bold text-ink-1">
                {badge ? rankName(badge) : "Placement"}
              </span>
              {today.length > 0 ? (
                <>
                  <Caption className="ml-auto">Today</Caption>
                  <Num size="sm" className="text-ink-2">
                    {record(today)}
                  </Num>
                </>
              ) : null}
            </div>
            {heroStats && hero ? (
              <div className="flex items-center gap-2">
                <Caption>{hero.name}, 30 days</Caption>
                <Num size="sm" className="ml-auto text-ink-2">
                  {heroStats.matches} ·{" "}
                  {winRate(heroStats.wins, heroStats.matches)}% won
                </Num>
              </div>
            ) : null}
          </div>
        </div>
        <span className="w-px shrink-0 self-stretch bg-line" />
        <div className="flex shrink-0 flex-col gap-2">
          <ZoneHead
            label={`Last ${digest.last10.length}`}
            aside={record(digest.last10)}
          />
          <Strip digest={digest} now={ctx.at} />
        </div>
      </div>
    </DeadlockPlate>
  );
}
