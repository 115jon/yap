import { describe, expect, it } from "vitest";

import {
  dragOverSituation,
  TOO_BIG_ACTION_LABELS,
  tooBigCopy,
  type TooBigSituation,
} from "../too-big-copy";
import { MB, UPLOAD_LIMITS } from "../upload-limits";

const mb = (n: number) => Math.round(n * MB);

interface Want {
  pose: string;
  headline: string;
  text: string;
  actions: string[];
  meter: boolean;
}

/** Each situation in the spec's copy table, as the card prints it. */
const TABLE: [string, TooBigSituation, Want][] = [
  [
    "composer drag-over, one over",
    {
      kind: "drag-one",
      name: "raid-clip.mp4",
      size: mb(31.2),
      limit: UPLOAD_LIMITS.attachment,
    },
    {
      pose: "stuffed",
      headline: "Too big to chew",
      text: "raid-clip.mp4 is 31.2 MB. Files can be up to 25 MB.",
      actions: [],
      meter: true,
    },
  ],
  [
    "composer drag-over, several",
    { kind: "drag-some", over: 3, fits: 2, limit: UPLOAD_LIMITS.attachment },
    {
      pose: "stuffed",
      headline: "A few of these won't fit",
      text: "3 files are over the 25 MB limit. The other 2 will still send.",
      actions: [],
      meter: false,
    },
  ],
  [
    "composer tray, one over",
    {
      kind: "tray-one",
      name: "raid-clip.mp4",
      size: mb(31.2),
      limit: UPLOAD_LIMITS.attachment,
    },
    {
      pose: "stuffed",
      headline: "raid-clip.mp4",
      text: "31.2 MB · 6.2 MB over the 25 MB limit",
      actions: ["Remove", "Choose another"],
      meter: false,
    },
  ],
  [
    "composer tray, several over",
    { kind: "tray-some", over: 3, fits: 2, limit: UPLOAD_LIMITS.attachment },
    {
      pose: "stuffed",
      headline: "A few of these won't fit",
      text: "3 files are over the 25 MB limit. The other 2 will still send.",
      actions: ["Remove the big ones"],
      meter: false,
    },
  ],
  [
    "avatar GIF",
    {
      kind: "avatar",
      size: mb(20.4),
      limit: UPLOAD_LIMITS.avatar,
      isGif: true,
    },
    {
      pose: "sad",
      headline: "Yappy can't carry that one",
      text: "This GIF is 20.4 MB. Avatars can be up to 8 MB.",
      actions: ["Pick another"],
      meter: true,
    },
  ],
  [
    "avatar image",
    {
      kind: "avatar",
      size: mb(9.6),
      limit: UPLOAD_LIMITS.avatar,
      isGif: false,
    },
    {
      pose: "sad",
      headline: "Yappy can't carry that one",
      text: "This image is 9.6 MB. Avatars can be up to 8 MB.",
      actions: ["Pick another"],
      meter: true,
    },
  ],
  [
    "banner",
    { kind: "banner", size: mb(12), limit: UPLOAD_LIMITS.banner },
    {
      pose: "sad",
      headline: "That banner's too heavy",
      text: "Banners can be up to 8 MB; this one is 12.0 MB.",
      actions: ["Pick another"],
      meter: true,
    },
  ],
  [
    "soundboard",
    { kind: "soundboard", size: mb(63), limit: UPLOAD_LIMITS.soundboard },
    {
      pose: "squashed",
      headline: "That sound won't fit",
      text: "Sounds can be up to 50 MB; this one is 63.0 MB.",
      actions: ["Pick another"],
      meter: true,
    },
  ],
  [
    "GIF upload",
    { kind: "gif", size: mb(31.7), limit: UPLOAD_LIMITS.gif },
    {
      pose: "squashed",
      headline: "That GIF won't fit",
      text: "GIFs can be up to 25 MB; this one is 31.7 MB.",
      actions: ["Pick another"],
      meter: true,
    },
  ],
  [
    "vibe",
    { kind: "vibe", size: mb(40.2), limit: UPLOAD_LIMITS.vibe },
    {
      pose: "squashed",
      headline: "That one won't fit",
      text: "Vibes can be up to 25 MB; this one is 40.2 MB.",
      actions: ["Pick another"],
      meter: true,
    },
  ],
  [
    "camera background",
    {
      kind: "camera-background",
      size: mb(27.9),
      limit: UPLOAD_LIMITS.cameraBackground,
    },
    {
      pose: "squashed",
      headline: "That picture won't fit",
      text: "Backgrounds can be up to 25 MB; this one is 27.9 MB.",
      actions: ["Pick another"],
      meter: true,
    },
  ],
  [
    "server said no, in a picker",
    { kind: "server", serverLimit: 8 * MB, action: "pick-another" },
    {
      pose: "sad",
      headline: "The server turned it away",
      text: "It says the limit is 8 MB.",
      actions: ["Pick another"],
      meter: false,
    },
  ],
  [
    "server said no, in the tray",
    { kind: "server", serverLimit: 20 * MB, action: "remove" },
    {
      pose: "sad",
      headline: "The server turned it away",
      text: "It says the limit is 20 MB.",
      actions: ["Remove"],
      meter: false,
    },
  ],
];

describe("tooBigCopy", () => {
  it.each(TABLE)("%s", (_label, situation, want) => {
    const copy = tooBigCopy(situation);
    expect(copy.pose).toBe(want.pose);
    expect(copy.headline).toBe(want.headline);
    expect(copy.lead + copy.line).toBe(want.text);
    expect(copy.actions.map((a) => TOO_BIG_ACTION_LABELS[a])).toEqual(
      want.actions,
    );
    expect(Boolean(copy.meter)).toBe(want.meter);
    expect(copy.announcement).not.toContain("too large");
  });

  it("bolds the file name and adds the drag hint", () => {
    const copy = tooBigCopy(TABLE[0]![1]);
    expect(copy.lead).toBe("raid-clip.mp4");
    expect(copy.hint).toBe(
      "Drop a smaller file, or post a link to it instead.",
    );
    expect(copy.announcement).toBe(
      "Too big to chew. raid-clip.mp4 is 31.2 MB. Files can be up to 25 MB. Drop a smaller file, or post a link to it instead.",
    );
    expect(copy.meter).toEqual({
      size: mb(31.2),
      limit: UPLOAD_LIMITS.attachment,
    });
  });

  it("speaks the tray card as one sentence", () => {
    expect(tooBigCopy(TABLE[2]![1]).announcement).toBe(
      "raid-clip.mp4 is 31.2 MB, 6.2 MB over the 25 MB limit.",
    );
  });

  it("drops 'The other 0 will still send' when nothing else fits", () => {
    expect(
      tooBigCopy({
        kind: "tray-some",
        over: 2,
        fits: 0,
        limit: UPLOAD_LIMITS.attachment,
      }).line,
    ).toBe("2 files are over the 25 MB limit.");
  });
});

describe("dragOverSituation", () => {
  const limit = UPLOAD_LIMITS.attachment;

  it("is null when sizes are hidden or everything fits", () => {
    expect(dragOverSituation([], limit)).toBeNull();
    expect(dragOverSituation([{ name: "a", size: 1 }], limit)).toBeNull();
  });

  it("names the one file over", () => {
    expect(
      dragOverSituation(
        [
          { name: "a", size: 1 },
          { name: "big.mp4", size: mb(30) },
        ],
        limit,
      ),
    ).toEqual({ kind: "drag-one", name: "big.mp4", size: mb(30), limit });
  });

  it("counts several over and the rest", () => {
    expect(
      dragOverSituation(
        [
          { name: "a", size: 1 },
          { name: "b", size: mb(30) },
          { name: "c", size: mb(40) },
        ],
        limit,
      ),
    ).toEqual({ kind: "drag-some", over: 2, fits: 1, limit });
  });
});
