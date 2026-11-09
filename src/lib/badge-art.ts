/**
 * Badge pixel art: one 16 x 16 cell drawing per badge, printed in the
 * brand's Bayer dither like Yappy (components/ui/yap-bark). Pure data, no
 * React or DOM, so the web and the phone app draw the same cells.
 *
 * Each drawing is 16 rows of 16 characters:
 *   "#"  body, lit through the Bayer dither (dense top left, thinning
 *        toward the bottom right, as the mark is lit)
 *   "@"  edge, always lit
 *   "+"  detail in the second ink (silkscreen white)
 *   "."  empty
 * `yap-team` is Yappy himself: the 32-cell master (yap-geometry) sampled
 * two to one, open jaw, with the edge cells as "@".
 */

import type { BadgeId } from "@/lib/badges";

export const BADGE_CELLS = 16;

export type BadgeInk = "body" | "edge" | "detail";

const FOUNDER = [
  "................",
  "................",
  ".......++.......",
  ".@.....++.....@.",
  ".@@....@@....@@.",
  ".@#@..@##@..@#@.",
  ".@##@@####@@##@.",
  ".@############@.",
  ".@############@.",
  ".@############@.",
  ".@############@.",
  ".@@@@@@@@@@@@@@.",
  ".@@+@@@++@@@+@@.",
  ".@@@@@@@@@@@@@@.",
  "................",
  "................",
];

const LOUD = [
  "................",
  "...........+....",
  "......@@....+...",
  ".....@#@.+...+..",
  "....@##@..+...+.",
  "@@@@###@..+...+.",
  "@######@...+...+",
  "@######@...+...+",
  "@######@...+...+",
  "@######@...+...+",
  "@@@@###@..+...+.",
  "....@##@..+...+.",
  ".....@#@.+...+..",
  "......@@....+...",
  "...........+....",
  "................",
];

const BUG_HUNTER = [
  "................",
  "....+......+....",
  ".....+....+.....",
  "......@@@@......",
  ".....@@@@@@.....",
  "+...@##@@##@...+",
  ".+.@###@@###@.+.",
  "..+@###@@###@+..",
  "...@###@@###@...",
  "+++@###@@###@+++",
  "...@###@@###@...",
  "..+@###@@###@+..",
  ".+..@##@@##@..+.",
  "+....@@@@@@....+",
  "................",
  "................",
];

const EARLY_YAPPER = [
  "................",
  "................",
  ".......++.......",
  "..+....++....+..",
  "...+........+...",
  "......@@@@......",
  "....@@####@@....",
  "...@########@...",
  "..@##########@..",
  "..@##########@..",
  ".@############@.",
  "++++++++++++++++",
  "................",
  "...++++++++++...",
  "................",
  "......++++......",
];

const SERVER_OWNER = [
  "................",
  "................",
  ".@@..@@..@@..@@.",
  ".@#@@#@@@@#@@#@.",
  ".@############@.",
  ".@############@.",
  ".@###++##++###@.",
  ".@###++##++###@.",
  ".@############@.",
  ".@#####++#####@.",
  ".@####++++####@.",
  ".@####++++####@.",
  ".@####++++####@.",
  ".@@@@@@@@@@@@@@.",
  "................",
  "................",
];

const VOICE_REGULAR = [
  "................",
  "......@@@@......",
  ".....@####@.....",
  ".....@#++#@.....",
  ".....@####@.....",
  ".....@#++#@.....",
  ".....@####@.....",
  "...+.@####@.+...",
  "...+.@####@.+...",
  "...+..@@@@..+...",
  "....+......+....",
  ".....++++++.....",
  ".......@@.......",
  ".......@@.......",
  ".....@@@@@@.....",
  "................",
];

const NIGHT_OWL = [
  "................",
  "...........+....",
  ".....@@...+++...",
  "...@@@.....+....",
  "..@@#@..........",
  "..@#@.......+...",
  ".@##@......+++..",
  ".@###@......+...",
  ".@###@..........",
  ".@###@@.........",
  ".@####@@........",
  "..@####@@@..@...",
  "..@@######@@@...",
  "...@@#####@@....",
  ".....@@@@@......",
  "................",
];

/** Yappy on 16 cells (see the header). */
const YAP_TEAM = [
  "................",
  ".@..............",
  ".@@.............",
  ".@.@............",
  ".@@#@...........",
  ".@###@@@@@@.....",
  ".@#####@###@@@@.",
  ".@####@.@######@",
  ".@#####@#######@",
  ".@#########@@@@@",
  ".@########@.....",
  ".@#########@@@..",
  "..@######@@@@@..",
  "..@#####@.......",
  "..@#####@.......",
  "..@@@@@@@.......",
];

export const BADGE_ART: Record<BadgeId, readonly string[]> = {
  founder: FOUNDER,
  loud: LOUD,
  "yap-team": YAP_TEAM,
  "bug-hunter": BUG_HUNTER,
  "early-yapper": EARLY_YAPPER,
  "server-owner": SERVER_OWNER,
  "voice-regular": VOICE_REGULAR,
  "night-owl": NIGHT_OWL,
};

const INK_OF = {
  "#": "body",
  "@": "edge",
  "+": "detail",
} as const satisfies Record<string, BadgeInk>;

/** The ink at cell (`x`, `y`) of `badge`'s drawing, or null when empty. */
export function badgeCellInk(
  badge: BadgeId,
  x: number,
  y: number,
): BadgeInk | null {
  const cell = BADGE_ART[badge][y]?.[x];
  return cell !== undefined && cell in INK_OF
    ? INK_OF[cell as keyof typeof INK_OF]
    : null;
}

/**
 * The light across a badge, as `yapLight` lights the mark: dense at the top
 * left, thinning toward the bottom right. Coverage 0..1 for `bayerLit`.
 */
export function badgeLight(x: number, y: number): number {
  const d = (x + y) / (2 * (BADGE_CELLS - 1));
  return 0.98 - d * 0.34;
}
