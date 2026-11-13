// Every media URL inside a link embed (images, videos, audio, icons), in
// one place, so the server can proxy them and clients can unwrap them with
// the same field list. Page links (embed.url, author.url, "player" iframe
// videos) are not media and are left alone; so is a video without a `kind`
// (embeds stored before it existed may hold a player page there).

import type { EmbedInfo, EmbedMedia } from "@/lib/types";

/** What a field holds: a video (may be an HLS playlist) or other media. */
export type EmbedMediaField = "video" | "media";

/**
 * Maps one media URL; `sourceUrl` is the post or page it came from. Null
 * drops the media: an optional URL is removed, and a picture, video or
 * media item that needs one is removed with it.
 */
export type EmbedMediaUrlMap = (
  url: string,
  sourceUrl?: string,
  field?: EmbedMediaField,
) => string | null;

function mapOptional(
  value: string | undefined,
  map: EmbedMediaUrlMap,
  sourceUrl?: string,
): string | undefined {
  if (typeof value !== "string" || !value) return value;
  return map(value, sourceUrl) ?? undefined;
}

function mapMedia(
  media: EmbedMedia[] | undefined,
  map: EmbedMediaUrlMap,
  sourceUrl: string,
): EmbedMedia[] | undefined {
  if (!Array.isArray(media)) return media;
  const mapped: EmbedMedia[] = [];
  for (const item of media) {
    if (!item || typeof item !== "object") continue;
    const url =
      typeof item.url === "string" && item.url
        ? map(item.url, sourceUrl, item.type === "video" ? "video" : "media")
        : item.url;
    if (url === null) continue;
    mapped.push({
      ...item,
      url,
      thumbnailUrl: mapOptional(item.thumbnailUrl, map, sourceUrl),
    });
  }
  return mapped;
}

export function mapEmbedMediaUrls(
  embed: EmbedInfo,
  map: EmbedMediaUrlMap,
): EmbedInfo {
  if (!embed || typeof embed !== "object") return embed;
  const source = embed.url;
  const next: EmbedInfo = { ...embed };

  if (embed.author) {
    next.author = {
      ...embed.author,
      iconURL: mapOptional(embed.author.iconURL, map, source),
    };
  }
  if (embed.thumbnail?.url) {
    const url = map(embed.thumbnail.url, source);
    if (url === null) delete next.thumbnail;
    else next.thumbnail = { ...embed.thumbnail, url };
  }
  if (embed.media) next.media = mapMedia(embed.media, map, source);
  // Only a "direct" video is a file; a "player" one is an iframe page.
  if (embed.video?.url && embed.video.kind === "direct") {
    const url = map(embed.video.url, source, "video");
    if (url === null) delete next.video;
    else next.video = { ...embed.video, url };
  }
  if (embed.audio) {
    next.audio = {
      ...embed.audio,
      url: mapOptional(embed.audio.url, map, source),
      artworkUrl: mapOptional(embed.audio.artworkUrl, map, source),
    };
  }
  if (embed.externalCard?.image?.url) {
    next.externalCard = withCardImage(embed.externalCard, map, source);
  }
  if (embed.referencedTweet) {
    const quoted = embed.referencedTweet;
    const quotedSource = quoted.url || source;
    next.referencedTweet = {
      ...quoted,
      author: quoted.author
        ? {
            ...quoted.author,
            iconURL: mapOptional(quoted.author.iconURL, map, quotedSource),
          }
        : quoted.author,
      media: mapMedia(quoted.media, map, quotedSource),
      externalCard: quoted.externalCard?.image?.url
        ? withCardImage(quoted.externalCard, map, quotedSource)
        : quoted.externalCard,
    };
  }
  if (embed.footer?.iconURL) {
    next.footer = {
      ...embed.footer,
      iconURL: mapOptional(embed.footer.iconURL, map, source),
    };
  }
  return next;
}

type ExternalCard = NonNullable<EmbedInfo["externalCard"]>;

function withCardImage(
  card: ExternalCard,
  map: EmbedMediaUrlMap,
  source: string,
): ExternalCard {
  const image = card.image;
  if (!image?.url) return card;
  const url = map(image.url, source);
  if (url === null) {
    const { image: _dropped, ...rest } = card;
    return rest as ExternalCard;
  }
  return { ...card, image: { ...image, url } };
}
