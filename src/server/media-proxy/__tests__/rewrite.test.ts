import { describe, expect, it } from "vitest";
import type { EmbedInfo } from "@/lib/types";
import {
  proxiedClientMediaUrl,
  proxiedMediaUrl,
  proxyArtworkUrlsForDelivery,
  proxyAttachmentUrlForDelivery,
  rewriteEmbedsMedia,
  rewriteGifItemMedia,
  unwrapProxiedMediaUrl,
  type MediaRewriteContext,
} from "../rewrite";
import {
  mediaProxyKey,
  signMediaUrl,
  verifyMediaUrlSignature,
} from "../signing";

const key = mediaProxyKey({ MEDIA_PROXY_SECRET: "rewrite-test" })!;
const ctx: MediaRewriteContext = { key, origin: "https://yap.example" };

function parse(url: string | null) {
  const parsed = new URL(url ?? "about:blank");
  return {
    origin: parsed.origin,
    path: parsed.pathname,
    target: parsed.searchParams.get("url"),
    sig: parsed.searchParams.get("sig"),
    sourceUrl: parsed.searchParams.get("sourceUrl"),
  };
}

describe("proxiedMediaUrl", () => {
  it("signs a third-party image into an absolute proxy URL", () => {
    const raw = "https://cdn.example.com/og.png?v=2";
    const out = parse(proxiedMediaUrl(raw, ctx));
    expect(out).toMatchObject({
      origin: "https://yap.example",
      path: "/api/proxy-media",
      target: raw,
    });
    expect(verifyMediaUrlSignature(raw, out.sig, key)).toBe(true);
    expect(out.sig).toBe(signMediaUrl(raw, key));
  });

  it("keeps social CDN media on the allowlisted form with the post", () => {
    const raw = "https://pbs.twimg.com/media/a.jpg";
    const out = parse(proxiedMediaUrl(raw, ctx, "https://x.com/u/status/1"));
    expect(out).toMatchObject({
      target: raw,
      sig: null,
      sourceUrl: "https://x.com/u/status/1",
    });
  });

  it("leaves our own URLs and paths alone", () => {
    for (const value of [
      "https://yap.example/api/avatars/u1.png",
      "/api/attachments/c/a/b.png",
      "",
    ]) {
      expect(proxiedMediaUrl(value, ctx)).toBe(value);
    }
    // The host's root-dot form and any case are still ours.
    expect(proxiedMediaUrl("https://YAP.example./a.png", ctx)).toBe(
      "https://yap.example./a.png",
    );
  });

  it("loads our own zones directly", () => {
    for (const value of [
      "https://cdn.115jon.com/f/a.png",
      "https://115jon.com/a.png",
      "https://CDN.115JON.COM./a.png",
      "https://meet.115jon.site/a.png",
    ]) {
      expect(proxiedMediaUrl(value, ctx)).toBe(new URL(value).href);
    }
    // A look-alike name is someone else's.
    expect(
      parse(proxiedMediaUrl("https://x115jon.com/a.png", ctx)).target,
    ).toBe("https://x115jon.com/a.png");
  });

  it("drops values no client may load", () => {
    for (const value of [
      "data:image/png;base64,AAAA",
      "javascript:alert(1)",
      "relative/a.png",
      // A browser reads these against the page, other parsers as a host.
      "https:evil.example/a.png",
      "http:/evil.example/a.png",
      "ftp://evil.example/a.png",
    ]) {
      expect(proxiedMediaUrl(value, ctx), JSON.stringify(value)).toBeNull();
    }
  });

  it("proxies another host hidden behind characters browsers strip", () => {
    for (const value of [
      "/\t/evil.example/a.png",
      "/\n/evil.example/a.png",
      "\t//evil.example/a.png",
      " //evil.example/a.png",
      "/\\evil.example/a.png",
      "\\\\evil.example/a.png",
      "\u0000https://evil.example/a.png",
      "ht\ttps://evil.example/a.png",
      "https://evil.example/a.png\r\n",
    ]) {
      const out = parse(proxiedMediaUrl(value, ctx));
      expect(out.origin, JSON.stringify(value)).toBe("https://yap.example");
      expect(out.target, JSON.stringify(value)).toBe(
        "https://evil.example/a.png",
      );
      expect(verifyMediaUrlSignature(out.target!, out.sig, key)).toBe(true);
    }
  });

  it("moves a proxy URL we signed on another host onto ours", () => {
    const raw = "https://cdn.example.com/og.png";
    const query = new URLSearchParams({
      url: raw,
      sig: signMediaUrl(raw, key),
    });
    const ours = `https://alias.example/api/proxy-media?${query}`;
    expect(proxiedMediaUrl(ours, ctx)).toBe(
      `https://yap.example/api/proxy-media?${query}`,
    );
    const evil = `https://evil.example/api/proxy-media?${query}`;
    expect(proxiedMediaUrl(evil, ctx)).toBe(
      `https://yap.example/api/proxy-media?${query}`,
    );
    // Any host can serve a path that looks like ours: that is a third party.
    const lookalike = "https://evil.example/api/proxy-media?id=1";
    expect(parse(proxiedMediaUrl(lookalike, ctx)).target).toBe(lookalike);
    const forged =
      "https://evil.example/api/proxy-media?url=x&sig=AAAAAAAAAAAAAAAAAAAAAA";
    expect(parse(proxiedMediaUrl(forged, ctx)).target).toBe(forged);
  });

  it("proxies a protocol-relative URL as https", () => {
    const out = parse(
      proxiedMediaUrl("//evil.example/api/attachments/x.png", ctx),
    );
    expect(out.target).toBe("https://evil.example/api/attachments/x.png");
    expect(out.origin).toBe("https://yap.example");
  });

  it("leaves an HLS playlist alone only in a video field", () => {
    const hls = "https://stream.example/live.m3u8";
    expect(proxiedMediaUrl(hls, ctx, null, "video")).toBe(hls);
    // As a picture it is just a URL a viewer must not fetch directly.
    expect(parse(proxiedMediaUrl(hls, ctx)).target).toBe(hls);
  });

  it("leaves generic URLs as they are when the proxy has no key", () => {
    const raw = "https://cdn.example.com/og.png";
    expect(proxiedMediaUrl(raw, { ...ctx, key: null })).toBe(raw);
  });

  it("is undone by unwrapProxiedMediaUrl", () => {
    const raw = "https://cdn.example.com/og.png?a=1&b=2";
    expect(unwrapProxiedMediaUrl(proxiedMediaUrl(raw, ctx)!)).toBe(raw);
    const legacy = proxiedMediaUrl(
      "https://pbs.twimg.com/a.jpg",
      ctx,
      "https://x.com/u/status/1",
    )!;
    expect(unwrapProxiedMediaUrl(legacy)).toBe("https://pbs.twimg.com/a.jpg");
    expect(unwrapProxiedMediaUrl(legacy, { keepSocialForm: true })).toBe(
      legacy,
    );
  });
});

const embed: EmbedInfo = {
  id: "e1",
  url: "https://news.example/story",
  type: "link",
  author: {
    name: "A",
    url: "https://news.example/a",
    iconURL: "https://news.example/a.png",
  },
  provider: { name: "News", url: "https://news.example" },
  thumbnail: { url: "https://cdn.news.example/og.jpg", width: 1200 },
  media: [
    { type: "image", url: "https://cdn.news.example/1.jpg" },
    {
      type: "video",
      url: "https://cdn.news.example/2.mp4",
      thumbnailUrl: "https://cdn.news.example/2.jpg",
    },
  ],
  video: { url: "https://www.youtube.com/embed/abc", kind: "player" },
  audio: {
    url: "https://cdn.news.example/a.mp3",
    artworkUrl: "https://cdn.news.example/art.jpg",
  },
  externalCard: {
    url: "https://other.example",
    image: { url: "https://other.example/i.png" },
  },
  referencedTweet: {
    type: "quoted",
    url: "https://x.com/q/status/2",
    author: {
      name: "Q",
      iconURL: "https://pbs.twimg.com/profile_images/q.jpg",
    },
    media: [{ type: "image", url: "https://pbs.twimg.com/media/q.jpg" }],
  },
  footer: { text: "News", iconURL: "https://news.example/favicon.ico" },
  fields: [],
};

describe("rewriteEmbedsMedia", () => {
  const [out] = rewriteEmbedsMedia([embed], ctx);

  it("proxies every media URL", () => {
    const media = [
      out.author?.iconURL,
      out.thumbnail?.url,
      out.media?.[0].url,
      out.media?.[1].url,
      out.media?.[1].thumbnailUrl,
      out.audio?.url,
      out.audio?.artworkUrl,
      out.externalCard?.image?.url,
      out.referencedTweet?.author?.iconURL,
      out.referencedTweet?.media?.[0].url,
      out.footer?.iconURL,
    ];
    for (const url of media) {
      expect(url, url).toMatch(/^https:\/\/yap\.example\/api\/proxy-media\?/);
    }
    expect(parse(out.thumbnail!.url).sig).toBe(
      signMediaUrl("https://cdn.news.example/og.jpg", key),
    );
    // A quoted tweet's media keep the quoted post as their source.
    expect(parse(out.referencedTweet!.media![0].url).sourceUrl).toBe(
      "https://x.com/q/status/2",
    );
  });

  it("proxies a direct video, and leaves one without a kind alone", () => {
    const [direct, legacy] = rewriteEmbedsMedia(
      [
        {
          ...embed,
          video: { url: "https://cdn.news.example/v.mp4", kind: "direct" },
        },
        { ...embed, video: { url: "https://www.tiktok.com/player/v1/1" } },
      ],
      ctx,
    );
    expect(parse(direct.video!.url).target).toBe(
      "https://cdn.news.example/v.mp4",
    );
    expect(legacy.video?.url).toBe("https://www.tiktok.com/player/v1/1");
  });

  it("leaves page links and player iframes alone", () => {
    expect(out.url).toBe(embed.url);
    expect(out.author?.url).toBe(embed.author?.url);
    expect(out.provider).toEqual(embed.provider);
    expect(out.video).toEqual(embed.video);
    expect(out.externalCard?.url).toBe(embed.externalCard?.url);
    expect(out.referencedTweet?.url).toBe(embed.referencedTweet?.url);
  });

  it("does not touch the stored embed", () => {
    expect(embed.thumbnail?.url).toBe("https://cdn.news.example/og.jpg");
  });

  it("drops media no client may load", () => {
    const [cleaned] = rewriteEmbedsMedia(
      [
        {
          ...embed,
          author: { name: "A", iconURL: "javascript:alert(1)" },
          thumbnail: { url: "https:evil.example/t.png" },
          media: [
            { type: "image", url: "data:image/png;base64,AA" },
            { type: "image", url: "/\t/evil.example/a.png" },
          ],
          video: { url: "ftp://evil.example/v.mp4", kind: "direct" },
          externalCard: {
            url: "https://other.example",
            image: { url: "relative.png" },
          },
          footer: { text: "x", iconURL: "http:/evil.example/f.ico" },
        },
      ],
      ctx,
    );
    expect(cleaned.author?.iconURL).toBeUndefined();
    expect(cleaned.thumbnail).toBeUndefined();
    expect(cleaned.video).toBeUndefined();
    expect(cleaned.externalCard).toEqual({ url: "https://other.example" });
    expect(cleaned.footer).toEqual({ text: "x", iconURL: undefined });
    expect(cleaned.media).toHaveLength(1);
    expect(parse(cleaned.media![0].url).target).toBe(
      "https://evil.example/a.png",
    );
  });

  it("is idempotent", () => {
    expect(rewriteEmbedsMedia([out], ctx)).toEqual([out]);
  });
});

describe("GIF results", () => {
  it("proxies preview and send, and the send URL unwraps for posting", () => {
    const item = {
      id: "g1",
      preview: { url: "https://static.klipy.com/ii/p.webp", width: 1 },
      send: { url: "https://static.klipy.com/ii/s.mp4", width: 2 },
      sourceUrl: "https://klipy.com/gifs/g1",
    };
    const out = rewriteGifItemMedia(item, ctx);
    expect(parse(out.preview.url).target).toBe(item.preview.url);
    expect(parse(out.send.url).sig).toBe(signMediaUrl(item.send.url, key));
    expect(out.sourceUrl).toBe(item.sourceUrl);
    expect(out.preview.width).toBe(1);
    expect(unwrapProxiedMediaUrl(out.send.url, { keepSocialForm: true })).toBe(
      item.send.url,
    );
  });

  it("proxies an external (sent GIF) attachment and keeps stored files", () => {
    const external = "https://static.klipy.com/ii/s.mp4";
    expect(parse(proxyAttachmentUrlForDelivery(external, ctx)).target).toBe(
      external,
    );
    expect(proxyAttachmentUrlForDelivery("attachments/c/a/x.png", ctx)).toBe(
      "/api/attachments/c/a/x.png",
    );
  });
});

describe("client-supplied URLs", () => {
  const now = Date.UTC(2026, 10, 13, 12, 30);

  it("sign until a set time, rounded to the hour", () => {
    const raw = "https://i.example/art.png";
    const out = new URL(proxiedClientMediaUrl(raw, ctx, now)!);
    const exp = out.searchParams.get("exp");
    expect(Number(exp)).toBe(Date.UTC(2026, 10, 14, 13) / 1000);
    const sig = out.searchParams.get("sig");
    expect(verifyMediaUrlSignature(raw, sig, key, exp, now)).toBe(true);
    // Not a stable signature: it cannot be replayed without `exp`.
    expect(verifyMediaUrlSignature(raw, sig, key)).toBe(false);
    expect(
      verifyMediaUrlSignature(raw, sig, key, exp, Number(exp) * 1000),
    ).toBe(false);
  });

  it("keep a proxy URL the server signed, and drop unsafe values", () => {
    const signed = proxiedMediaUrl("https://i.ytimg.com/vi/a/0.jpg", ctx)!;
    expect(proxiedClientMediaUrl(signed, ctx, now)).toBe(signed);
    expect(proxiedClientMediaUrl("/\t/evil.example/a.png", ctx, now)).toMatch(
      /^https:\/\/yap\.example\/api\/proxy-media\?url=https%3A%2F%2Fevil\.example/,
    );
    expect(proxiedClientMediaUrl("https:evil.example/a.png", ctx, now)).toBe(
      null,
    );
  });

  it("are not moved onto our host once expired", () => {
    const raw = "https://i.example/art.png";
    const expired = proxiedClientMediaUrl(raw, ctx, now - 3 * 86_400_000)!;
    const elsewhere = expired.replace(
      "https://yap.example",
      "https://evil.example",
    );
    expect(parse(proxiedMediaUrl(elsewhere, ctx)).target).toBe(elsewhere);
  });
});

describe("Listen Together artwork", () => {
  it("proxies every artworkUrl in a response", () => {
    const response = {
      kind: "collection",
      tracks: [
        { title: "a", artworkUrl: "https://i.ytimg.com/vi/abc/hqdefault.jpg" },
        { title: "b", artworkUrl: null },
      ],
      collection: {
        artworkUrl: "https://i.scdn.co/image/xyz",
        url: "https://open.spotify.com/x",
      },
    };
    const out = proxyArtworkUrlsForDelivery(response, ctx);
    expect(parse(out.tracks[0].artworkUrl!).target).toBe(
      "https://i.ytimg.com/vi/abc/hqdefault.jpg",
    );
    expect(out.tracks[1].artworkUrl).toBeNull();
    expect(parse(out.collection.artworkUrl).sig).toBe(
      signMediaUrl("https://i.scdn.co/image/xyz", key),
    );
    expect(out.collection.url).toBe("https://open.spotify.com/x");
  });
});
