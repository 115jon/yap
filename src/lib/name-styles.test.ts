import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import {
  ditherFadeColumns,
  ditherFadeCoverage,
  ditherFadeLit,
  getDitherFadeImage,
} from "./name-dither";
import {
  CLASSIC_DISPLAY_NAME_EFFECTS,
  CLASSIC_DISPLAY_NAME_FONTS,
  createRandomDisplayNameStyle,
  DISPLAY_NAME_EFFECT_OPTIONS,
  DISPLAY_NAME_FONT_OPTIONS,
  displayNameStyleHasMotion,
  getDisplayNameEffectOverlayStyle,
  getDisplayNameEffectStyle,
  isYapDisplayNameEffect,
  normalizeDisplayNameStyle,
  resolveReadableDisplayNameStyle,
  serializeDisplayNameStyle,
  usesYapDisplayNameStyle,
  YAP_DISPLAY_NAME_EFFECTS,
  YAP_DISPLAY_NAME_FONTS,
  type DisplayNameStyle,
} from "./profile-customization";

const base: DisplayNameStyle = {
  font: "gg-sans",
  effect: "solid",
  primaryColor: "#F8FAFC",
  secondaryColor: "#FB7185",
};

describe("display name style sanitizer", () => {
  it("accepts every Yap effect and font", () => {
    for (const effect of YAP_DISPLAY_NAME_EFFECTS) {
      for (const font of YAP_DISPLAY_NAME_FONTS) {
        expect(normalizeDisplayNameStyle({ ...base, effect, font })).toEqual({
          ...base,
          effect,
          font,
        });
      }
    }
  });

  it("keeps every Classic value valid and serializes old styles unchanged", () => {
    for (const effect of CLASSIC_DISPLAY_NAME_EFFECTS) {
      for (const font of CLASSIC_DISPLAY_NAME_FONTS) {
        const stored = JSON.stringify({
          font,
          effect,
          primaryColor: "#F8FAFC",
          secondaryColor: "#FB7185",
        });
        // A style saved before this change round-trips byte for byte.
        expect(
          serializeDisplayNameStyle(normalizeDisplayNameStyle(stored)),
        ).toBe(stored);
      }
    }
  });

  it("keeps wave only when it is exactly true", () => {
    expect(normalizeDisplayNameStyle({ ...base, wave: true })?.wave).toBe(true);
    for (const wave of [false, "true", 1, null, {}]) {
      const style = normalizeDisplayNameStyle({ ...base, wave });
      expect(style).not.toBeNull();
      expect(style).not.toHaveProperty("wave");
    }
    expect(
      serializeDisplayNameStyle({ ...base, effect: "holo", wave: true }),
    ).toBe(JSON.stringify({ ...base, effect: "holo", wave: true }));
  });

  it("rejects unknown effects and fonts and drops unknown keys", () => {
    expect(
      normalizeDisplayNameStyle({ ...base, effect: "rainbow" }),
    ).toBeNull();
    expect(
      normalizeDisplayNameStyle({ ...base, font: "comic-sans" }),
    ).toBeNull();
    expect(
      normalizeDisplayNameStyle({ ...base, effect: "holo", url: "x" }),
    ).not.toHaveProperty("url");
  });

  it("lists every effect and font once, grouped", () => {
    const effects = DISPLAY_NAME_EFFECT_OPTIONS.map((o) => o.id);
    expect(new Set(effects).size).toBe(effects.length);
    expect(effects).toEqual([
      ...CLASSIC_DISPLAY_NAME_EFFECTS,
      ...YAP_DISPLAY_NAME_EFFECTS,
    ]);
    const fonts = DISPLAY_NAME_FONT_OPTIONS.map((o) => o.id);
    expect(fonts).toEqual([
      ...CLASSIC_DISPLAY_NAME_FONTS,
      ...YAP_DISPLAY_NAME_FONTS,
    ]);
    for (const option of DISPLAY_NAME_EFFECT_OPTIONS) {
      expect(option.group).toBe(
        isYapDisplayNameEffect(option.id) ? "yap" : "classic",
      );
      // A Yap effect names a face that exists.
      if (option.font) expect(fonts).toContain(option.font);
    }
  });

  it("knows which styles go beyond Classic", () => {
    expect(usesYapDisplayNameStyle(base)).toBe(false);
    expect(usesYapDisplayNameStyle({ ...base, effect: "ember" })).toBe(true);
    expect(usesYapDisplayNameStyle({ ...base, font: "vt323" })).toBe(true);
    expect(usesYapDisplayNameStyle({ ...base, wave: true })).toBe(true);
  });

  it("marks motion for animated effects and Wave", () => {
    for (const effect of [
      "holo",
      "pixel",
      "ember",
      "gradient",
      "neon",
      "pop",
    ] as const)
      expect(displayNameStyleHasMotion({ ...base, effect })).toBe(true);
    for (const effect of [
      "dither",
      "chrome",
      "crt",
      "glitch",
      "sticker",
      "candlelit",
      "solid",
      "toon",
    ] as const)
      expect(displayNameStyleHasMotion({ ...base, effect })).toBe(false);
    expect(displayNameStyleHasMotion({ ...base, wave: true })).toBe(true);
  });

  it("hands Yap effects their inks only (the stylesheet paints them)", () => {
    for (const effect of YAP_DISPLAY_NAME_EFFECTS) {
      const style = getDisplayNameEffectStyle(
        { ...base, effect },
        { animate: true },
      ) as Record<string, unknown>;
      expect(style).toEqual({ "--nm-a": "#F8FAFC", "--nm-b": "#FB7185" });
      expect(getDisplayNameEffectOverlayStyle({ ...base, effect })).toBeNull();
    }
  });

  it("keeps Sticker's fill on a light ground and nudges its outline", () => {
    const sticker = resolveReadableDisplayNameStyle(
      {
        ...base,
        effect: "sticker",
        primaryColor: "#FFFFFF",
        secondaryColor: "#FFF0F5",
      },
      { backgroundColor: "#FFFFFF", fallbackTextColor: "#111111" },
    );
    expect(sticker?.primaryColor).toBe("#FFFFFF");
    expect(sticker?.secondaryColor).not.toBe("#FFF0F5");
    const solid = resolveReadableDisplayNameStyle(
      { ...base, primaryColor: "#FFFFFF" },
      { backgroundColor: "#FFFFFF", fallbackTextColor: "#111111" },
    );
    expect(solid?.primaryColor).not.toBe("#FFFFFF");
  });

  it("only surprises with Yap styles when they are allowed", () => {
    for (let i = 0; i < 200; i++) {
      const style = createRandomDisplayNameStyle();
      expect(usesYapDisplayNameStyle(style)).toBe(false);
      expect(normalizeDisplayNameStyle(style)).toEqual(style);
    }
    for (let i = 0; i < 200; i++) {
      const style = createRandomDisplayNameStyle({ includeYap: true });
      expect(normalizeDisplayNameStyle(style)).toEqual(style);
    }
  });
});

describe("Dither fade tile", () => {
  it("ramps the second ink from none to all, left to right", () => {
    const cols = ditherFadeColumns(10);
    expect(ditherFadeCoverage(0, cols)).toBe(0);
    expect(ditherFadeCoverage(cols - 1, cols)).toBe(1);
    const litInColumn = (x: number) =>
      Array.from({ length: 8 }, (_, y) => ditherFadeLit(x, y, cols)).filter(
        Boolean,
      ).length;
    expect(litInColumn(0)).toBe(0);
    expect(litInColumn(cols - 1)).toBe(8);
    // Coverage never falls going right (over whole 8-column Bayer periods).
    let previous = -1;
    for (let x = 0; x + 8 <= cols; x += 8) {
      let lit = 0;
      for (let dx = 0; dx < 8; dx++) lit += litInColumn(x + dx);
      expect(lit).toBeGreaterThanOrEqual(previous);
      previous = lit;
    }
  });

  it("sizes the tile to the name and caches it", () => {
    expect(ditherFadeColumns(1)).toBe(24);
    expect(ditherFadeColumns(10)).toBe(60);
    expect(ditherFadeColumns(500)).toBe(256);
    const a = getDitherFadeImage("#F472B6", "#38BDF8", 8);
    expect(a).toMatch(/^url\("data:image\/svg\+xml,/);
    expect(decodeURIComponent(a)).toContain("fill='#F472B6'");
    expect(decodeURIComponent(a)).toContain("fill='#38BDF8'");
    expect(getDitherFadeImage("#F472B6", "#38BDF8", 8)).toBe(a);
  });
});

describe("name-styles.css contract", () => {
  const css = readFileSync(
    resolve(__dirname, "../styles/name-styles.css"),
    "utf8",
  );

  it("paints every Yap effect", () => {
    for (const effect of YAP_DISPLAY_NAME_EFFECTS)
      expect(css).toContain(`.rm-name[data-name-effect="${effect}"]`);
  });

  it("only animates while a name plays, and never under reduced motion", () => {
    // Every Yap animation hangs off [data-name-play]: the root's list...
    expect(css).toMatch(
      /\.rm-name\[data-name-play\]\s*\{\s*animation:\s*var\(--nm-root-anim, none\),\s*var\(--nm-bob-anim, none\);/,
    );
    // ...and the letters' wave.
    expect(css).toMatch(
      /\.rm-name\[data-wave\]\[data-split\]\[data-name-play\] \.rm-name-ch \{\s*animation: rm-name-wave/,
    );
    const direct = [...css.matchAll(/^\s*animation:\s*([^;]+);/gm)].map((m) =>
      m[1]!.trim(),
    );
    // No other `animation:` declaration starts motion by itself.
    expect(
      direct.every(
        (v) =>
          v.startsWith("var(") ||
          v.startsWith("rm-name-wave") ||
          v.startsWith("none"),
      ),
    ).toBe(true);
    expect(css).toMatch(
      /@media \(prefers-reduced-motion: reduce\)\s*\{\s*\.rm-name,\s*\.rm-name \* \{\s*animation: none !important;/,
    );
    for (const name of [
      "rm-name-holo",
      "rm-name-pixel",
      "rm-name-ember",
      "rm-name-wave",
      "rm-name-bob",
    ])
      expect(css).toContain(`@keyframes ${name}`);
  });

  it("gives every name room for its paint (padding taken back by margin)", () => {
    expect(css).toMatch(
      /\.rm-name \{[^}]*padding: var\(--nm-by\) var\(--nm-bx\);[^}]*margin: calc\(var\(--nm-by\) \* -1\) calc\(var\(--nm-bx\) \* -1\);[^}]*max-width: calc\(100% \+ 2 \* var\(--nm-bx\)\);/s,
    );
  });

  it("keeps each effect's reach inside its bleed", () => {
    const bleed = (effect: string) => {
      const block = css.match(
        new RegExp(
          `\\.rm-name\\[data-name-effect="${effect}"\\][^{]*\\{([^}]*--nm-bleed-y[^}]*)\\}`,
        ),
      )?.[1];
      const y = Number(block?.match(/--nm-bleed-y:\s*([\d.]+)em/)?.[1] ?? 0.22);
      const x = Number(block?.match(/--nm-bleed-x:\s*([\d.]+)em/)?.[1] ?? 0.16);
      return { x, y };
    };
    // Paint reach in em (offset + blur) per effect, from the stylesheet's
    // own values; see name-styles-spec.md.
    const reach: Record<string, { x: number; y: number }> = {
      dither: { x: 0.02, y: 0.02 },
      holo: { x: 0.02, y: 0.02 },
      chrome: { x: 0.03, y: 0.07 },
      pixel: { x: 0.02, y: 0.1 },
      crt: { x: 0.21, y: 0.16 },
      glitch: { x: 0.14, y: 0.02 },
      ember: { x: 0.22, y: 0.27 },
      sticker: { x: 0.13, y: 0.15 },
      candlelit: { x: 0.28, y: 0.28 },
    };
    for (const [effect, r] of Object.entries(reach)) {
      const b = bleed(effect);
      expect(b.x, effect).toBeGreaterThanOrEqual(r.x);
      expect(b.y, effect).toBeGreaterThanOrEqual(r.y);
    }
  });
});
