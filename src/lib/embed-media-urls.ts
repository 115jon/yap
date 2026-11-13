// Every media URL inside a link embed (images, videos, audio, icons), in
// one place, so the server can proxy them and clients can unwrap them with
// the same field list. Page links (embed.url, author.url, "player" iframe
// videos) are not media and are left alone; so is a video without a `kind`
// (embeds stored before it existed may hold a player page there).

import type { EmbedInfo, EmbedMedia } from "@/lib/types";

/** What a field holds: a video (may be an HLS playlist) or other media. */
export type EmbedMediaField = "video" | "media";

/** Maps one media URL; `sourceUrl` is the post or page it came from. */
export type EmbedMediaUrlMap = (
  url: string,
  sourceUrl?: string,
  field?: EmbedMediaField,
) => string;

function mapOptional(
  value: string | undefined,
  map: EmbedMediaUrlMap,
  sourceUrl?: string,
): string | undefined {
  return typeof value === "string" && value ? map(value, sourceUrl) : value;
}

function mapMedia(
  media: EmbedMedia[] | undefined,
  map: EmbedMediaUrlMap,
  sourceUrl: string,
): EmbedMedia[] | undefined {
  if (!Array.isArray(media)) return media;
  return media.map((item) => ({
    ...item,
    url:
      typeof item.url === "string" && item.url
        ? map(item.url, sourceUrl, item.type === "video" ? "video" : "media")
        : item.url,
    thumbnailUrl: mapOptional(item.thumbnailUrl, map, sourceUrl),
  }));
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
    next.thumbnail = {
      ...embed.thumbnail,
      url: map(embed.thumbnail.url, source),
    };
  }
  if (embed.media) next.media = mapMedia(embed.media, map, source);
  // Only a "direct" video is a file; a "player" one is an iframe page.
  if (embed.video?.url && embed.video.kind === "direct") {
    next.video = {
      ...embed.video,
      url: map(embed.video.url, source, "video"),
    };
  }
  if (embed.audio) {
    next.audio = {
      ...embed.audio,
      url: mapOptional(embed.audio.url, map, source),
      artworkUrl: mapOptional(embed.audio.artworkUrl, map, source),
    };
  }
  if (embed.externalCard?.image?.url) {
    next.externalCard = {
      ...embed.externalCard,
      image: {
        ...embed.externalCard.image,
        url: map(embed.externalCard.image.url, source),
      },
    };
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
        ? {
            ...quoted.externalCard,
            image: {
              ...quoted.externalCard.image,
              url: map(quoted.externalCard.image.url, quotedSource),
            },
          }
        : quoted.externalCard,
    };
  }
  if (embed.footer?.iconURL) {
    next.footer = {
      ...embed.footer,
      iconURL: map(embed.footer.iconURL, source),
    };
  }
  return next;
}
