import { memo, useMemo, useState } from "react";
import { Pressable, View } from "react-native";
import Svg, { G, Rect } from "react-native-svg";

import { RADIUS, useTheme } from "@/theme";
import { Text } from "@/ui";

import {
  BADGE_CELLS,
  BADGES,
  badgeRuns,
  type BadgeId,
  type UserBadge,
} from "./badges";
import type { ProfileColors } from "./style";

const SIZE = 24;

const earnedOn = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  year: "numeric",
});

/** "Earned Nov 9, 2026", or nothing when the date is missing or unreadable. */
function earnedLabel(awardedAt: string): string | null {
  const date = new Date(awardedAt);
  if (!awardedAt || Number.isNaN(date.getTime())) return null;
  return `Earned ${earnedOn.format(date)}`;
}

/** One badge's 16 x 16 pixel art: body and edge in `ink`, detail in `detail`. */
const BadgeArt = memo(function BadgeArt({
  id,
  ink,
  detail,
}: {
  id: BadgeId;
  ink: string;
  detail: string;
}) {
  const runs = useMemo(() => badgeRuns(id), [id]);
  return (
    <Svg
      width={SIZE}
      height={SIZE}
      viewBox={`0 0 ${BADGE_CELLS} ${BADGE_CELLS}`}
    >
      <G fill={ink}>
        {runs
          .filter((run) => run.ink !== "detail")
          .map((run) => (
            <Rect
              key={`${run.x},${run.y}`}
              x={run.x}
              y={run.y}
              width={run.width}
              height={1}
            />
          ))}
      </G>
      <G fill={detail}>
        {runs
          .filter((run) => run.ink === "detail")
          .map((run) => (
            <Rect
              key={`${run.x},${run.y}`}
              x={run.x}
              y={run.y}
              width={run.width}
              height={1}
            />
          ))}
      </G>
    </Svg>
  );
});

/**
 * A person's badges as a wrapping row of pixel-art marks. Pressing one names
 * it underneath (name, what it is for, when it was earned); pressing it
 * again, or another, moves or clears the caption.
 */
export function BadgeRow({
  badges,
  colors,
}: {
  badges: readonly UserBadge[];
  colors: ProfileColors;
}) {
  const theme = useTheme();
  const [picked, setPicked] = useState<BadgeId | null>(null);
  if (badges.length === 0) return null;
  const open = badges.find((badge) => badge.id === picked) ?? null;
  const earned = open ? earnedLabel(open.awarded_at) : null;
  return (
    <View style={{ marginTop: 10 }}>
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        {badges.map((badge) => {
          const info = BADGES[badge.id];
          return (
            <Pressable
              key={badge.id}
              accessibilityRole="button"
              accessibilityLabel={`${info.name}: ${info.description}`}
              accessibilityState={{ selected: picked === badge.id }}
              hitSlop={4}
              onPress={() => setPicked(picked === badge.id ? null : badge.id)}
              onLongPress={() => setPicked(badge.id)}
              style={({ pressed }) => ({
                borderRadius: RADIUS,
                opacity: pressed ? 0.7 : 1,
              })}
            >
              <BadgeArt
                id={badge.id}
                ink={theme.accentInk}
                detail={colors.ink}
              />
            </Pressable>
          );
        })}
      </View>
      {open ? (
        <Text
          variant="row"
          accessibilityLiveRegion="polite"
          style={{
            marginTop: 6,
            fontSize: 13,
            lineHeight: 18,
            color: colors.ink2,
          }}
        >
          <Text
            variant="row"
            style={{ fontSize: 13, lineHeight: 18, color: colors.ink }}
          >
            {BADGES[open.id].name}
          </Text>
          {` · ${BADGES[open.id].description}`}
          {earned ? ` · ${earned}` : ""}
        </Text>
      ) : null}
    </View>
  );
}
