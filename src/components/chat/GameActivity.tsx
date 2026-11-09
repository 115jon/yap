/**
 * Game activity ("Playing X") on member rows and profile cards. The value
 * comes from presence (chat store `gameActivityByUserId`, read with
 * useGameActivity); it is absent for anyone offline, invisible or not
 * sharing their activity. This module does not import the store, so the
 * profile card can use it on pages without one.
 *
 * - `GameActivityText` is the one-line row subtext, in the custom status's
 *   place (Discord shows the activity ahead of a custom status).
 * - `GameActivityCard` is the profile card section: the game's icon, its
 *   name, Rich Presence details and state when present, and a live elapsed
 *   time. Only the card ticks (NumClock, paused while the app is hidden);
 *   rows never show a clock, so a long member list runs no timers.
 */

import { NumClock } from "@/components/ui/num";
import { gameActivityLabel } from "@/lib/game-activity";
import type { GameActivity } from "@/lib/gateway-protocol";
import { getAuthAssetUrl } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useState } from "react";

/** Row subtext: "Playing **Name**", truncated to one line. */
export function GameActivityText({
  activity,
  className,
}: {
  activity: GameActivity;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "block truncate text-ui-xs font-normal text-ink-3",
        className,
      )}
      title={gameActivityLabel(activity)}
    >
      Playing <span className="font-semibold text-ink-2">{activity.name}</span>
    </span>
  );
}

/** The game's 40 px icon; nothing at all when it has none or it fails. */
function GameIcon({ activity }: { activity: GameActivity }) {
  const [failed, setFailed] = useState(false);
  if (!activity.icon_url || failed) return null;
  return (
    <span
      aria-hidden="true"
      className="flex size-10 shrink-0 overflow-hidden rounded-avatar bg-tint-2"
    >
      <img
        src={getAuthAssetUrl(activity.icon_url)}
        alt=""
        width={40}
        height={40}
        loading="lazy"
        decoding="async"
        className="size-full object-cover"
        onError={() => setFailed(true)}
      />
    </span>
  );
}

/** The profile card body of the "Playing a game" section. */
export function GameActivityCard({ activity }: { activity: GameActivity }) {
  return (
    <div className="flex min-w-0 items-center gap-3">
      {/* Keyed by icon: a new icon gets a fresh try after a failed one. */}
      <GameIcon key={activity.icon_url ?? "none"} activity={activity} />
      <div className="flex min-w-0 flex-col">
        <span className="truncate text-ui-sm font-semibold text-ink-1">
          {activity.name}
        </span>
        {activity.details ? (
          <span className="truncate text-ui-xs text-ink-2">
            {activity.details}
          </span>
        ) : null}
        {activity.state ? (
          <span className="truncate text-ui-xs text-ink-2">
            {activity.state}
          </span>
        ) : null}
        <span className="flex items-center gap-1 text-ui-xs text-ink-2">
          <NumClock since={activity.started_at} size="xs" />
          elapsed
        </span>
      </div>
    </div>
  );
}
