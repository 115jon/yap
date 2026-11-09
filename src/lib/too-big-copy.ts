/**
 * The words for every "too big" upload card (toobig spec, 2026-11-09). The
 * native client prints the same strings, so change them in both places.
 *
 * Each situation picks Yappy's pose, a headline, one line, the size meter
 * (where the card has room) and its actions. `lead` is the part of the line
 * printed in ink-1 (a file name); `line` follows it directly.
 */

import {
  formatUploadLimit,
  formatUploadOverage,
  formatUploadSize,
} from "@/lib/upload-limits";

export type YappyPose = "stuffed" | "sad" | "squashed";

export type TooBigAction =
  | "remove"
  | "choose-another"
  | "remove-big"
  | "pick-another";

export const TOO_BIG_ACTION_LABELS: Record<TooBigAction, string> = {
  remove: "Remove",
  "choose-another": "Choose another",
  "remove-big": "Remove the big ones",
  "pick-another": "Pick another",
};

export type TooBigSituation =
  /** Composer drag-over, one file over the limit (before the drop). */
  | { kind: "drag-one"; name: string; size: number; limit: number }
  /** Composer drag-over, several files, some over. */
  | { kind: "drag-some"; over: number; fits: number; limit: number }
  /** Composer tray, one file over (after a drop, paste or the picker). */
  | { kind: "tray-one"; name: string; size: number; limit: number }
  /** Composer tray, several over. */
  | { kind: "tray-some"; over: number; fits: number; limit: number }
  | {
      kind: "avatar";
      size: number;
      limit: number;
      /** "GIF" for an image/gif, "image" otherwise. */
      isGif: boolean;
    }
  | { kind: "banner"; size: number; limit: number }
  | { kind: "soundboard"; size: number; limit: number }
  | { kind: "gif"; size: number; limit: number }
  | { kind: "vibe"; size: number; limit: number }
  | { kind: "camera-background"; size: number; limit: number }
  /**
   * The server answered 413 anyway (its limit changed). `serverLimit` is the
   * number from its message, or the client's limit when it named none.
   * `action` is "remove" in the composer tray, "pick-another" in a picker.
   */
  | {
      kind: "server";
      serverLimit: number;
      action: "remove" | "pick-another";
    };

export interface TooBigCopy {
  pose: YappyPose;
  headline: string;
  /** Printed in ink-1 before `line` (a file name), or "". */
  lead: string;
  line: string;
  /** A quiet hint under the line (drag-over only). */
  hint?: string;
  meter?: { size: number; limit: number };
  actions: TooBigAction[];
  /** The whole card as one sentence: its accessible name. */
  announcement: string;
}

const SEVERAL_HEADLINE = "A few of these won't fit";

function several(over: number, fits: number, limit: number): string {
  const first = `${over} files are over the ${formatUploadLimit(limit)} limit.`;
  // "The other 0 will still send" would be false comfort: say nothing.
  return fits > 0 ? `${first} The other ${fits} will still send.` : first;
}

function card(
  pose: YappyPose,
  headline: string,
  line: string,
  extra: Partial<Omit<TooBigCopy, "pose" | "headline" | "line">> = {},
): TooBigCopy {
  const lead = extra.lead ?? "";
  const hint = extra.hint;
  return {
    pose,
    headline,
    lead,
    line,
    hint,
    meter: extra.meter,
    actions: extra.actions ?? [],
    announcement:
      extra.announcement ??
      [headline + ".", lead + line, hint].filter(Boolean).join(" "),
  };
}

export function tooBigCopy(situation: TooBigSituation): TooBigCopy {
  switch (situation.kind) {
    case "drag-one": {
      const { name, size, limit } = situation;
      return card(
        "stuffed",
        "Too big to chew",
        ` is ${formatUploadSize(size)}. Files can be up to ${formatUploadLimit(limit)}.`,
        {
          lead: name,
          hint: "Drop a smaller file, or post a link to it instead.",
          meter: { size, limit },
        },
      );
    }
    case "drag-some":
      return card(
        "stuffed",
        SEVERAL_HEADLINE,
        several(situation.over, situation.fits, situation.limit),
      );
    case "tray-one": {
      const { name, size, limit } = situation;
      // The file name is the title; the line is the numbers.
      const sizeText = formatUploadSize(size);
      const overText = `${formatUploadOverage(size, limit)} over the ${formatUploadLimit(limit)} limit`;
      return card("stuffed", name, `${sizeText} · ${overText}`, {
        actions: ["remove", "choose-another"],
        announcement: `${name} is ${sizeText}, ${overText}.`,
      });
    }
    case "tray-some":
      return card(
        "stuffed",
        SEVERAL_HEADLINE,
        several(situation.over, situation.fits, situation.limit),
        { actions: ["remove-big"] },
      );
    case "avatar": {
      const { size, limit, isGif } = situation;
      return card(
        "sad",
        "Yappy can't carry that one",
        `This ${isGif ? "GIF" : "image"} is ${formatUploadSize(size)}. Avatars can be up to ${formatUploadLimit(limit)}.`,
        { meter: { size, limit }, actions: ["pick-another"] },
      );
    }
    case "banner": {
      const { size, limit } = situation;
      return card(
        "sad",
        "That banner's too heavy",
        `Banners can be up to ${formatUploadLimit(limit)}; this one is ${formatUploadSize(size)}.`,
        { meter: { size, limit }, actions: ["pick-another"] },
      );
    }
    case "soundboard":
      return squashed(situation, "That sound won't fit", "Sounds");
    case "gif":
      return squashed(situation, "That GIF won't fit", "GIFs");
    case "vibe":
      return squashed(situation, "That one won't fit", "Vibes");
    case "camera-background":
      return squashed(situation, "That picture won't fit", "Backgrounds");
    case "server":
      return card(
        "sad",
        "The server turned it away",
        `It says the limit is ${formatUploadLimit(situation.serverLimit)}.`,
        { actions: [situation.action] },
      );
  }
}

function squashed(
  { size, limit }: { size: number; limit: number },
  headline: string,
  things: string,
): TooBigCopy {
  return card(
    "squashed",
    headline,
    `${things} can be up to ${formatUploadLimit(limit)}; this one is ${formatUploadSize(size)}.`,
    { meter: { size, limit }, actions: ["pick-another"] },
  );
}

/**
 * The drop zone's oversize state for the files being dragged, or null when
 * none is known to be over `limit`. Browsers often hide sizes until the drop
 * (an empty `DataTransfer.files` during dragover); then this is null and the
 * normal overlay shows, and the tray handles it after the drop.
 */
export function dragOverSituation(
  files: ArrayLike<{ name: string; size: number }>,
  limit: number,
): TooBigSituation | null {
  const all = Array.from(files);
  const over = all.filter((file) => file.size > limit);
  const [only] = over;
  if (!only) return null;
  if (over.length === 1) {
    return { kind: "drag-one", name: only.name, size: only.size, limit };
  }
  return {
    kind: "drag-some",
    over: over.length,
    fits: all.length - over.length,
    limit,
  };
}
