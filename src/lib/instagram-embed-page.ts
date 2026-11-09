// ── instagram-embed-page ────────────────────────────────────────────────────
// Instagram's public embed page, `https://www.instagram.com/p/<code>/embed/
// captioned/`, is what Instagram serves to sites that embed a post. Asked for
// by a known link-preview crawler (Discordbot, facebookexternalhit) it comes
// back server-rendered and logged-out, with a `contextJSON` blob holding the
// post: caption, owner (username, avatar, verified), like / comment / view
// counts, dimensions, every carousel slide and, for reels and videos, a direct
// MP4 URL. A browser user agent gets an empty script shell instead.
//
// No session cookie, no app token: this replaced the logged-in private API
// (whose session expired) as the first source for Instagram embeds.
//
// When `contextJSON` is missing (Instagram has changed the page before) the
// server-rendered markup still has the image, the username and the caption,
// so that is parsed as a fallback. A removed or private post renders a
// "may have been removed" message (`ebmMessage`).
//
// SERVER-SIDE ONLY.

import type { EmbedAudio, EmbedMedia } from "@/lib/types";
import type { InstagramVideoResult } from "@/lib/instagram-video-resolver";
import { getLinkPreviewUserAgent } from "@/lib/platform";
import { decodeHtmlEntities } from "@/lib/html-meta";
import { readTextUntil, safeFetch } from "@/lib/safe-fetch";
import { safeParse } from "@/lib/schema";
import {
  InstagramEmbedContext,
  type InstagramEmbedNode,
  type InstagramEmbedShortcodeMedia,
} from "@/lib/vendor-payloads/instagram";

/** The embed page answers in ~0.3-0.7 s; past this it is not coming. */
export const INSTAGRAM_EMBED_PAGE_TIMEOUT_MS = 6_000;
/** The page is ~230 KB with `contextJSON` near the end. */
const INSTAGRAM_EMBED_PAGE_MAX_BYTES = 1024 * 1024;

export type InstagramEmbedPageOutcome =
  | {
      status: "ok";
      result: InstagramVideoResult;
      /**
       * Author, media, and a playable URL for every video. False when the
       * page had only the markup card, or a video without its MP4 (reels
       * with licensed music are `copyright_blocked` there).
       */
      complete: boolean;
    }
  /** Instagram says the post is gone or not public. */
  | { status: "removed" }
  /** A page without post data (bot wall, login wall, changed markup). */
  | { status: "empty"; httpStatus: number };

export function buildInstagramEmbedPageUrl(shortcode: string): string {
  return `https://www.instagram.com/p/${encodeURIComponent(shortcode)}/embed/captioned/`;
}

const CONTEXT_JSON_PATTERN = /"contextJSON":"((?:[^"\\]|\\.)*)"/;

function readContextJson(html: string): InstagramEmbedContext | null {
  const match = html.match(CONTEXT_JSON_PATTERN);
  if (!match?.[1]) return null;
  try {
    const inner: unknown = JSON.parse(`"${match[1]}"`);
    if (typeof inner !== "string") return null;
    const decoded = safeParse(InstagramEmbedContext, JSON.parse(inner));
    return decoded.success ? decoded.data : null;
  } catch {
    return null;
  }
}

function nonEmpty(value: string | undefined | null): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function mapEmbedNode(node: InstagramEmbedNode): EmbedMedia | null {
  const imageUrl = nonEmpty(node.display_url) ?? nonEmpty(node.thumbnail_src);
  const width = node.dimensions?.width;
  const height = node.dimensions?.height;
  const altText = nonEmpty(node.accessibility_caption);
  const videoUrl = node.is_video ? nonEmpty(node.video_url) : undefined;

  if (videoUrl) {
    return {
      type: "video",
      url: videoUrl,
      width,
      height,
      thumbnailUrl: imageUrl,
      contentType: "video/mp4",
      altText,
      durationSeconds: node.video_duration,
    };
  }
  if (!imageUrl) return null;
  return { type: "image", url: imageUrl, width, height, altText };
}

function readAudio(
  media: InstagramEmbedShortcodeMedia,
): EmbedAudio | undefined {
  const info = media.clips_music_attribution_info;
  const title = nonEmpty(info?.song_name);
  const artist = nonEmpty(info?.artist_name);
  // "Original audio" by the poster carries nothing worth a row.
  if (!title || info?.uses_original_audio) return undefined;
  return { title, artist };
}

/** The post as an {@link InstagramVideoResult}, from `shortcode_media`. */
export function buildResultFromShortcodeMedia(
  media: InstagramEmbedShortcodeMedia,
): InstagramVideoResult {
  const slides = media.edge_sidecar_to_children?.edges
    ?.map((edge) => edge.node)
    .filter((node): node is InstagramEmbedNode => Boolean(node));
  const sourceNodes: InstagramEmbedNode[] = slides?.length ? slides : [media];

  const items: EmbedMedia[] = [];
  const seen = new Set<string>();
  for (const node of sourceNodes) {
    const mapped = mapEmbedNode(node);
    if (!mapped || seen.has(mapped.url)) continue;
    seen.add(mapped.url);
    items.push(mapped);
  }

  const firstVideo = items.find((item) => item.type === "video");
  const firstImage = items.find((item) => item.type === "image");
  const username = nonEmpty(media.owner?.username);
  const caption = nonEmpty(media.edge_media_to_caption?.edges?.[0]?.node?.text);
  const timestamp =
    typeof media.taken_at_timestamp === "number" && media.taken_at_timestamp > 0
      ? new Date(media.taken_at_timestamp * 1000).toISOString()
      : null;

  return {
    videoUrl: firstVideo?.url ?? null,
    thumbnailUrl:
      firstVideo?.thumbnailUrl ??
      firstImage?.url ??
      nonEmpty(media.display_url) ??
      null,
    title: caption ?? null,
    durationSeconds: firstVideo?.durationSeconds ?? null,
    media: items.length > 0 ? items : undefined,
    authorName: username ?? nonEmpty(media.owner?.full_name) ?? null,
    authorUrl: username ? `https://www.instagram.com/${username}` : null,
    authorAvatarUrl: nonEmpty(media.owner?.profile_pic_url) ?? null,
    authorVerified: media.owner?.is_verified ?? null,
    likeCount:
      media.edge_liked_by?.count ??
      media.edge_media_preview_like?.count ??
      null,
    commentCount:
      media.edge_media_to_comment?.count ??
      media.edge_media_preview_comment?.count ??
      null,
    viewCount: media.video_view_count ?? media.video_play_count ?? null,
    timestamp,
    audio: readAudio(media) ?? null,
  };
}

function stripTags(html: string): string {
  return decodeHtmlEntities(
    html
      .replace(/<br\s*\/?>/gi, "\n")
      .replace(/<[^>]+>/g, "")
      .replace(/[ \t]+/g, " "),
  )
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/** The server-rendered card: image, username and caption, no counts. */
function readEmbedMarkup(html: string): InstagramVideoResult | null {
  const imageTag = html.match(/<img\b[^>]*class="EmbeddedMediaImage"[^>]*>/i);
  const imageUrl = imageTag?.[0].match(/\ssrc="([^"]+)"/i)?.[1];
  const username = html.match(
    /<span class="UsernameText">([^<]+)<\/span>/i,
  )?.[1];
  const captionBlock = html.match(
    /<div class="Caption">([\s\S]*?)<div class="CaptionComments">/i,
  )?.[1];
  // The block starts with the username link and two <br>s.
  const caption = captionBlock
    ? stripTags(captionBlock.replace(/^\s*<a\b[^>]*>[\s\S]*?<\/a>/i, ""))
    : undefined;
  const decodedImage = imageUrl ? decodeHtmlEntities(imageUrl) : undefined;
  const author = username ? decodeHtmlEntities(username).trim() : undefined;

  if (!decodedImage && !author && !caption) return null;
  return {
    videoUrl: null,
    thumbnailUrl: decodedImage ?? null,
    title: caption || null,
    durationSeconds: null,
    media: decodedImage ? [{ type: "image", url: decodedImage }] : undefined,
    authorName: author ?? null,
    authorUrl: author ? `https://www.instagram.com/${author}` : null,
  };
}

function isRemovedEmbedPage(html: string): boolean {
  return (
    html.includes('class="ebmMessage"') || html.includes("utm_campaign=invalid")
  );
}

/** Read an embed page's HTML. Exported for tests. */
export function parseInstagramEmbedPage(
  html: string,
  httpStatus = 200,
): InstagramEmbedPageOutcome {
  const context = readContextJson(html);
  const media = context?.gql_data?.shortcode_media;
  if (media) {
    const result = buildResultFromShortcodeMedia(media);
    const nodes = media.edge_sidecar_to_children?.edges?.length
      ? media.edge_sidecar_to_children.edges.map((edge) => edge.node)
      : [media];
    const missingVideo = nodes.some(
      (node) => node?.is_video && !nonEmpty(node.video_url),
    );
    return {
      status: "ok",
      result,
      complete:
        Boolean(result.authorName) &&
        Boolean(result.media?.length) &&
        !missingVideo,
    };
  }
  if (isRemovedEmbedPage(html)) return { status: "removed" };
  const markup = readEmbedMarkup(html);
  return markup
    ? { status: "ok", result: markup, complete: false }
    : { status: "empty", httpStatus };
}

/**
 * Fetch and read the embed page for `shortcode`. Network errors and timeouts
 * throw (the caller decides whether that is worth a retry); a non-HTML or
 * non-2xx answer is `empty` with its status.
 */
export async function fetchInstagramEmbedPage(
  shortcode: string,
): Promise<InstagramEmbedPageOutcome> {
  const response = await safeFetch(
    buildInstagramEmbedPageUrl(shortcode),
    {
      headers: {
        "User-Agent": getLinkPreviewUserAgent(),
        Accept: "text/html,application/xhtml+xml;q=0.9,*/*;q=0.5",
        "Accept-Language": "en-US,en;q=0.8",
      },
      // A login wall answers with a redirect; following it only costs time.
      redirect: "manual",
    },
    { timeoutMs: INSTAGRAM_EMBED_PAGE_TIMEOUT_MS },
  );

  const contentType = response.headers.get("content-type") ?? "";
  if (!response.ok || !contentType.includes("html")) {
    await response.body?.cancel().catch(() => {});
    return { status: "empty", httpStatus: response.status };
  }

  // Stop reading once `contextJSON` has closed; the rest is scripts.
  let contextAt = -1;
  const html = await readTextUntil(
    response,
    (text) => {
      if (contextAt < 0) contextAt = text.indexOf('"contextJSON":"');
      return contextAt >= 0 && CONTEXT_JSON_PATTERN.test(text.slice(contextAt));
    },
    INSTAGRAM_EMBED_PAGE_MAX_BYTES,
  );
  return parseInstagramEmbedPage(html, response.status);
}
