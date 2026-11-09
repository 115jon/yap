import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api-client", () => ({ apiPost: vi.fn() }));

import {
  findPrefetchableUrls,
  prefetchEmbeds,
  resetEmbedPrefetchState,
} from "../embed-prefetch";

beforeEach(() => {
  resetEmbedPrefetchState();
});

describe("findPrefetchableUrls", () => {
  it("takes every URL from pasted text", () => {
    expect(
      findPrefetchableUrls("https://www.instagram.com/reel/DfajF1zvsbl/", {
        pasted: true,
      }),
    ).toEqual(["https://www.instagram.com/reel/DfajF1zvsbl/"]);
  });

  it("leaves a URL that is still being typed alone", () => {
    expect(findPrefetchableUrls("look https://youtu")).toEqual([]);
    expect(findPrefetchableUrls("look https://youtu.be/dQw4w9WgXcQ ")).toEqual([
      "https://youtu.be/dQw4w9WgXcQ",
    ]);
  });

  it("ignores links in code and trims trailing punctuation", () => {
    expect(
      findPrefetchableUrls("`https://a.example/x` (https://b.example/y). ok"),
    ).toEqual(["https://b.example/y"]);
  });
});

describe("prefetchEmbeds", () => {
  it("asks for each URL once per session", async () => {
    const post = vi.fn(async () => {});
    expect(await prefetchEmbeds(["https://a.example/1"], post)).toEqual([
      "https://a.example/1",
    ]);
    expect(
      await prefetchEmbeds(
        ["https://a.example/1", "https://a.example/2"],
        post,
      ),
    ).toEqual(["https://a.example/2"]);
    expect(post).toHaveBeenCalledTimes(2);
    expect(post).toHaveBeenLastCalledWith("/api/embeds/prefetch", {
      urls: ["https://a.example/2"],
    });
  });

  it("never rejects when the server does", async () => {
    const post = vi.fn(async () => {
      throw new Error("offline");
    });
    await expect(
      prefetchEmbeds(["https://a.example/3"], post),
    ).resolves.toEqual(["https://a.example/3"]);
  });

  it("does not call the server with nothing to ask", async () => {
    const post = vi.fn();
    expect(await prefetchEmbeds([], post)).toEqual([]);
    expect(post).not.toHaveBeenCalled();
  });
});
