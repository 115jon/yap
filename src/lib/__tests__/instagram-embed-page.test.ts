import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  buildInstagramEmbedPageUrl,
  fetchInstagramEmbedPage,
  parseInstagramEmbedPage,
} from "../instagram-embed-page";

// Trimmed from real logged-out responses of
// https://www.instagram.com/p/<code>/embed/captioned/ (2026-11-08), official
// accounts only (@nasa, @instagram). Signatures are replaced; see the files.
function fixture(name: string): string {
  return readFileSync(
    new URL(`./fixtures/instagram/${name}`, import.meta.url),
    "utf8",
  );
}

function htmlResponse(body: string, init: ResponseInit = {}): Response {
  return new Response(body, {
    status: 200,
    headers: { "content-type": "text/html; charset=utf-8" },
    ...init,
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("parseInstagramEmbedPage", () => {
  it("reads a carousel post: every slide, caption, owner and counts", () => {
    const outcome = parseInstagramEmbedPage(
      fixture("embed-nasa-carousel.html"),
    );
    expect(outcome.status).toBe("ok");
    if (outcome.status !== "ok") return;
    const { result } = outcome;

    expect(outcome.complete).toBe(true);
    expect(result.authorName).toBe("nasa");
    expect(result.authorUrl).toBe("https://www.instagram.com/nasa");
    expect(result.authorVerified).toBe(true);
    expect(result.authorAvatarUrl).toMatch(
      /^https:\/\/scontent-[^/]+\.cdninstagram\.com\//,
    );
    expect(result.title).toMatch(/^Rovers, landers, aircraft/);
    expect(result.likeCount).toBeGreaterThan(0);
    expect(result.commentCount).toBeGreaterThan(0);
    expect(result.videoUrl).toBeNull();
    expect(result.media).toHaveLength(3);
    expect(result.media?.every((item) => item.type === "image")).toBe(true);
    expect(result.media?.[0]).toMatchObject({
      width: expect.any(Number),
      height: expect.any(Number),
    });
    // The page escapes "/" inside contextJSON; the URLs come out clean.
    expect(result.thumbnailUrl).toBe(result.media?.[0]?.url);
    expect(result.thumbnailUrl).not.toContain("\\");
  });

  it("reads a reel with its MP4, poster, duration and view count", () => {
    const outcome = parseInstagramEmbedPage(
      fixture("embed-instagram-reel.html"),
    );
    expect(outcome.status).toBe("ok");
    if (outcome.status !== "ok") return;
    const { result } = outcome;

    expect(outcome.complete).toBe(true);
    expect(result.authorName).toBe("instagram");
    expect(result.videoUrl).toMatch(/\.mp4\?/);
    expect(result.durationSeconds).toBeCloseTo(19.966);
    expect(result.viewCount).toBeGreaterThan(1_000_000);
    expect(result.media).toEqual([
      expect.objectContaining({
        type: "video",
        url: result.videoUrl,
        width: 1096,
        height: 1950,
        contentType: "video/mp4",
        thumbnailUrl: result.thumbnailUrl,
      }),
    ]);
    // "Original audio" is not worth an audio row.
    expect(result.audio).toBeNull();
  });

  it("marks a reel whose MP4 is withheld (licensed audio) as incomplete", () => {
    const outcome = parseInstagramEmbedPage(
      fixture("embed-reel-copyright-blocked.html"),
    );
    expect(outcome.status).toBe("ok");
    if (outcome.status !== "ok") return;
    expect(outcome.complete).toBe(false);
    expect(outcome.result.videoUrl).toBeNull();
    // Still a card: the poster frame, caption and author.
    expect(outcome.result.media?.[0]?.type).toBe("image");
    expect(outcome.result.authorName).toBe("instagram");
  });

  it("falls back to the server-rendered card when contextJSON is gone", () => {
    const html = fixture("embed-instagram-reel.html").replace(
      /<script>[\s\S]*<\/script>/,
      "",
    );
    const outcome = parseInstagramEmbedPage(html);
    expect(outcome.status).toBe("ok");
    if (outcome.status !== "ok") return;
    expect(outcome.complete).toBe(false);
    expect(outcome.result.authorName).toBe("instagram");
    expect(outcome.result.title).toBe(
      "Legend says you can still hear it echoing.⁣",
    );
    // `&amp;` in the src attribute is decoded.
    expect(outcome.result.thumbnailUrl).toContain("&oh=00_FIXTURE&oe=");
  });

  it("recognises a removed or private post", () => {
    expect(parseInstagramEmbedPage(fixture("embed-removed.html"))).toEqual({
      status: "removed",
    });
  });

  it("reports a page with no post data (bot wall, script shell) as empty", () => {
    expect(
      parseInstagramEmbedPage("<html><body><script></script></body></html>"),
    ).toEqual({ status: "empty", httpStatus: 200 });
  });
});

describe("fetchInstagramEmbedPage", () => {
  it("asks as a link-preview crawler, never follows a login redirect", async () => {
    const fetchMock = vi.fn(async () =>
      htmlResponse(fixture("embed-instagram-reel.html")),
    );
    vi.stubGlobal("fetch", fetchMock);

    const outcome = await fetchInstagramEmbedPage("DfajF1zvsbl");

    expect(outcome.status).toBe("ok");
    const [url, init] = fetchMock.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(url).toBe(buildInstagramEmbedPageUrl("DfajF1zvsbl"));
    expect(url).toBe(
      "https://www.instagram.com/p/DfajF1zvsbl/embed/captioned/",
    );
    const headers = new Headers(init.headers);
    expect(headers.get("user-agent")).toContain("Discordbot/2.0");
    expect(headers.get("cookie")).toBeNull();
    expect(init.redirect).toBe("manual");
    expect(init.signal).toBeInstanceOf(AbortSignal);
  });

  it("treats a redirect to the login page as empty", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(null, {
            status: 302,
            headers: {
              location: "https://www.instagram.com/accounts/login/",
            },
          }),
      ),
    );
    expect(await fetchInstagramEmbedPage("DfajF1zvsbl")).toEqual({
      status: "empty",
      httpStatus: 302,
    });
  });

  it("stops downloading once contextJSON has closed", async () => {
    const page = fixture("embed-instagram-reel.html");
    const encoder = new TextEncoder();
    let pulledAfterContext = 0;
    let cancelled = false;
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode(page.replace("</body></html>", "")));
      },
      pull(controller) {
        pulledAfterContext += 1;
        // An endless tail of scripts.
        controller.enqueue(encoder.encode("<script>/* … */</script>"));
      },
      cancel() {
        cancelled = true;
      },
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(body, {
            headers: { "content-type": "text/html" },
          }),
      ),
    );

    const outcome = await fetchInstagramEmbedPage("DfajF1zvsbl");

    expect(outcome.status).toBe("ok");
    expect(cancelled).toBe(true);
    expect(pulledAfterContext).toBeLessThan(3);
  });
});
