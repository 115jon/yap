import { describe, expect, it } from "vitest";

import { ambientColorFromPixels, fullSizeAvatarUrl } from "@/lib/ambient-color";

function pixels(...rgba: [number, number, number, number][]) {
  return Uint8ClampedArray.from(rgba.flat());
}

describe("ambientColorFromPixels", () => {
  it("weights colourful pixels over a white background", () => {
    // Three white pixels and one saturated magenta: the colour wins.
    const color = ambientColorFromPixels(
      pixels(
        [255, 255, 255, 255],
        [255, 255, 255, 255],
        [255, 255, 255, 255],
        [230, 20, 140, 255],
      ),
    );
    expect(color).toMatch(/^hsl\((\d+) (\d+)% (\d+)%\)$/);
    const [, hue, , light] = color!.match(/^hsl\((\d+) (\d+)% (\d+)%\)$/)!;
    expect(Number(hue)).toBeGreaterThan(300);
    expect(Number(light)).toBeGreaterThanOrEqual(35);
    expect(Number(light)).toBeLessThanOrEqual(50);
  });

  it("holds the lightness in the glow band for a black picture", () => {
    expect(ambientColorFromPixels(pixels([0, 0, 0, 255]))).toBe(
      "hsl(0 15% 35%)",
    );
  });

  it("is null when every pixel is transparent", () => {
    expect(ambientColorFromPixels(pixels([200, 10, 10, 0]))).toBeNull();
    expect(ambientColorFromPixels(new Uint8ClampedArray())).toBeNull();
  });
});

describe("fullSizeAvatarUrl", () => {
  it("raises a Google photo's size suffix", () => {
    expect(
      fullSizeAvatarUrl("https://lh3.googleusercontent.com/a/abc=s96-c"),
    ).toBe("https://lh3.googleusercontent.com/a/abc=s512-c");
    expect(
      fullSizeAvatarUrl(
        "https://lh3.googleusercontent.com/a/abc=s96-c-rg-br100",
      ),
    ).toBe("https://lh3.googleusercontent.com/a/abc=s512-c-rg-br100");
  });

  it("raises a Discord CDN size", () => {
    expect(
      fullSizeAvatarUrl("https://cdn.discordapp.com/avatars/1/a.png?size=64"),
    ).toBe("https://cdn.discordapp.com/avatars/1/a.png?size=512");
  });

  it("leaves our own uploads alone", () => {
    expect(fullSizeAvatarUrl("/api/avatars/u-1/up-1.gif")).toBe(
      "/api/avatars/u-1/up-1.gif",
    );
  });
});
