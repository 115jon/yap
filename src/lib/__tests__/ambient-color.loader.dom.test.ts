// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { peekAmbientPicture, readAmbientPicture } from "@/lib/ambient-color";

/** Which loads succeed: by URL, then whether the load asked for CORS. */
let allow: (url: string, cors: boolean) => boolean;
const created: { url: string; cors: boolean }[] = [];

class FakeImage {
  crossOrigin: string | null = null;
  decoding = "auto";
  naturalWidth = 64;
  naturalHeight = 64;
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  set src(url: string) {
    const cors = this.crossOrigin === "anonymous";
    created.push({ url, cors });
    queueMicrotask(() => (allow(url, cors) ? this.onload : this.onerror)?.());
  }
}

describe("ambient-color loader", () => {
  beforeEach(() => {
    created.length = 0;
    vi.stubGlobal("Image", FakeImage);
    // jsdom has no 2D canvas: the colour and wash come back null.
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("falls back to a plain load when CORS is refused, and memoises it", async () => {
    allow = (_url, cors) => !cors;
    const picture = await readAmbientPicture("https://cdn.example/a.png");
    expect(picture).toEqual({ wash: null, color: null });
    expect(created).toEqual([
      { url: "https://cdn.example/a.png", cors: true },
      { url: "https://cdn.example/a.png", cors: false },
    ]);
    expect(peekAmbientPicture("https://cdn.example/a.png")).toBe(picture);
    // A second card reads the memo; nothing loads again.
    await readAmbientPicture("https://cdn.example/a.png");
    expect(created).toHaveLength(2);
  });

  it("shares one load between cards asking at the same time", async () => {
    allow = () => true;
    const [a, b] = await Promise.all([
      readAmbientPicture("/api/avatars/u/1.png"),
      readAmbientPicture("/api/avatars/u/1.png"),
    ]);
    expect(a).toBe(b);
    expect(created).toEqual([{ url: "/api/avatars/u/1.png", cors: true }]);
  });

  it("does not remember a picture that failed to load", async () => {
    allow = () => false;
    expect(await readAmbientPicture("/broken.png")).toBeNull();
    expect(peekAmbientPicture("/broken.png")).toBeUndefined();
    allow = () => true;
    expect(await readAmbientPicture("/broken.png")).not.toBeNull();
    expect(created.filter((c) => c.url === "/broken.png")).toHaveLength(3);
  });
});
