/**
 * Top games: the owner's most played over 7 days, 30 days or all time.
 * One series of magnitudes, so one neutral bar per game (ink-2 on a tint
 * track), never a hue per game; the hours carry the value as text.
 */

import { Num } from "@/components/ui/num";
import type { BoardGameTotal, TopGamesRange } from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import { formatHours } from "../board-format";
import { useBoardMedia } from "../board-context";
import { Plate, WidgetEmpty, WidgetImage, type WidgetProps } from "./shared";

const RANGE_LABEL: Record<TopGamesRange, string> = {
  "7d": "7 days",
  "30d": "30 days",
  all: "All time",
};

export function TopGamesWidget({
  widget,
  data,
  ctx,
}: WidgetProps<"top_games">) {
  const top = data.top_games;
  const range = top?.range ?? widget.cfg.range;
  const readout = <span className="silkscreen">{RANGE_LABEL[range]}</span>;
  const games = top?.games ?? [];

  if (games.length === 0) {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <WidgetEmpty
          icon="trophy"
          title="No games yet"
          hint="Sessions of 5 minutes or more count here."
        />
      </Plate>
    );
  }

  const max = Math.max(...games.map((game) => game.seconds), 1);
  const summary = top ? (
    <p className="truncate text-ui-xs text-ink-3">
      <Num className="text-ink-2">{formatHours(top.total_seconds)}</Num> across{" "}
      {top.game_count} {top.game_count === 1 ? "game" : "games"}
    </p>
  ) : null;

  if (widget.size === "S") {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <ol className="flex flex-col gap-2">
          {games.slice(0, 3).map((game, index) => (
            <GameRow
              key={game.name}
              game={game}
              rank={index + 1}
              max={max}
              bar={false}
            />
          ))}
        </ol>
      </Plate>
    );
  }

  if (widget.size === "M") {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <ol className="flex flex-col gap-3.5">
          {games.slice(0, 5).map((game, index) => (
            <GameRow
              key={game.name}
              game={game}
              rank={index + 1}
              max={max}
              bar
            />
          ))}
        </ol>
        <div className="mt-auto flex min-w-0 items-center gap-2 border-t border-line pt-2.5">
          {summary}
        </div>
      </Plate>
    );
  }

  return (
    <Plate widget={widget} ctx={ctx} readout={readout}>
      <div className="flex min-h-0 flex-1 gap-6">
        <ol className="flex min-w-0 flex-1 flex-col gap-2.5">
          {games.slice(0, 3).map((game, index) => (
            <GameRow
              key={game.name}
              game={game}
              rank={index + 1}
              max={max}
              bar
            />
          ))}
        </ol>
        {top ? (
          <div className="flex w-[34%] shrink-0 flex-col justify-between border-l border-line pl-4 @max-[420px]/cell:hidden">
            <p className="font-silk text-[32px] leading-9 font-semibold tracking-[-0.02em] text-ink-1 tabular-nums [font-stretch:90%]">
              {formatHours(top.total_seconds)}
            </p>
            <p className="text-ui-xs text-ink-3">
              across {top.game_count} {top.game_count === 1 ? "game" : "games"}
            </p>
            <p className="truncate text-ui-xs text-ink-3">
              Most played:{" "}
              <span className="font-medium text-ink-2">{games[0]?.name}</span>
            </p>
          </div>
        ) : null}
      </div>
    </Plate>
  );
}

function GameRow({
  game,
  rank,
  max,
  bar,
}: {
  game: BoardGameTotal;
  rank: number;
  max: number;
  bar: boolean;
}) {
  const media = useBoardMedia();
  return (
    <li className="flex min-w-0 items-center gap-2.5">
      <Num className="w-3 shrink-0 text-ink-3 @max-[200px]/cell:hidden">
        {rank}
      </Num>
      {game.icon_url ? (
        <WidgetImage
          src={media.url(game.icon_url)}
          alt=""
          className="size-7 shrink-0"
        />
      ) : (
        <span
          aria-hidden="true"
          className="flex size-7 shrink-0 items-center justify-center rounded-[2px] bg-ply-4 font-display text-[12px] font-extrabold text-ink-3"
        >
          {game.name.slice(0, 1).toUpperCase()}
        </span>
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span
          className="truncate text-ui-sm font-medium text-ink-1"
          title={game.name}
        >
          {game.name}
        </span>
        {bar ? (
          <span
            aria-hidden="true"
            className="block h-1 overflow-hidden rounded-[1px] bg-tint-2"
          >
            <span
              className={cn("block h-full bg-ink-2")}
              style={{ width: `${Math.max(2, (game.seconds / max) * 100)}%` }}
            />
          </span>
        ) : null}
      </div>
      <Num size="sm" className="shrink-0 text-ink-2">
        {formatHours(game.seconds)}
      </Num>
    </li>
  );
}
