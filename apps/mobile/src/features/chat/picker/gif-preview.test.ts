import { describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api", () => ({
  apiFetch: vi.fn(),
  apiJson: vi.fn(),
  ApiError: class extends Error {},
}));

const { gifPreview } = await import("./api");
type Gif = import("./api").Gif;

const asset = (url: string, contentType: string, sizeBytes: number) => ({
  url,
  width: 220,
  height: 200,
  sizeBytes,
  contentType,
});

const gif = (over: Partial<Gif>): Gif => ({
  id: "1",
  title: "cat",
  provider: "klipy",
  preview: asset("https://m.test/tiny.mp4", "video/mp4", 60_000),
  send: asset("https://m.test/full.gif", "image/gif", 22_700_000),
  sourceUrl: "https://m.test/full.gif",
  aspectRatio: 1.1,
  ...over,
});

describe("gifPreview", () => {
  it("draws the server's thumb", () => {
    const thumb = asset("https://m.test/tiny.gif", "image/gif", 180_000);
    expect(gifPreview(gif({ thumb }))).toEqual({
      kind: "image",
      url: thumb.url,
    });
  });

  it("plays the small video instead of a heavy sent GIF", () => {
    expect(gifPreview(gif({}))).toEqual({
      kind: "video",
      url: "https://m.test/tiny.mp4",
    });
  });

  it("uses a light sent GIF when there is no thumb", () => {
    const send = asset("https://m.test/small.gif", "image/gif", 400_000);
    expect(gifPreview(gif({ send }))).toEqual({ kind: "image", url: send.url });
  });

  it("does not trust a sent GIF of unknown size", () => {
    const send = asset("https://m.test/full.gif", "image/gif", 0);
    expect(gifPreview(gif({ send })).kind).toBe("video");
  });
});
