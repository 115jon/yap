import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api-client", () => ({ apiPost: vi.fn() }));

import { looksPrivateLink } from "../embed-urls";
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

describe("links not to prefetch", () => {
  it("skips links that carry a credential or open a sign-in step", () => {
    const privateLinks = [
      "https://app.example/login?token=abc123",
      "https://app.example/x?code=4821",
      "https://app.example/x?access_token=abc",
      "https://bucket.example/file.png?X-Amz-Signature=deadbeef",
      "https://app.example/x?api_key=1",
      "https://app.example/x?otp=1234",
      "https://app.example/x#access_token=abc&type=magiclink",
      "https://app.example/x?id=" + "a".repeat(48),
      "https://app.example/auth/magic-link/abc",
      "https://app.example/reset-password/abc",
      "https://app.example/oauth/callback",
      "https://user:pw@app.example/",
      "https://app.example/x?sessionId=abc",
      "https://app.example/x?oobCode=abc",
      "https://app.example/x?PHPSESSID=abc",
      "https://app.example/users/sign_in",
      "https://app.example/unsubscribe/x",
      "https://app.example/e/" + "b".repeat(48),
    ];
    for (const url of privateLinks) {
      expect(looksPrivateLink(url), url).toBe(true);
      expect(findPrefetchableUrls(url, { pasted: true }), url).toEqual([]);
    }
  });

  it("still prefetches ordinary links", () => {
    const ordinary = [
      "https://youtu.be/dQw4w9WgXcQ?si=Zx3p0aQ1bC2dE4fG",
      "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42",
      "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=1a2b3c4d5e6f7g8h",
      "https://x.com/someone/status/1234567890123456789",
      "https://en.wikipedia.org/wiki/Author",
      "https://example.com/keyboards?keyword=split",
      "https://en.wikipedia.org/wiki/Magic",
      "https://en.wikipedia.org/wiki/Callback_(computer_programming)",
      "https://blog.example/reset-your-router",
    ];
    for (const url of ordinary) {
      expect(looksPrivateLink(url), url).toBe(false);
    }
    expect(findPrefetchableUrls(ordinary[0], { pasted: true })).toEqual([
      ordinary[0],
    ]);
  });

  it("leaves a link in angle brackets alone", () => {
    expect(
      findPrefetchableUrls("<https://a.example/x> and https://b.example/y ", {
        pasted: true,
      }),
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
