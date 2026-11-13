// Removing link previews from a message (PATCH /api/channels/:id/messages
// with `embeds`). Every client sends `[]` or the previews it still shows,
// so the server keeps only stored embeds the request names and never stores
// an embed a client wrote: a client-written embed could carry any media URL,
// which delivery would then sign for the media proxy.

import type { EmbedInfo } from "@/lib/types";

function identity(value: unknown): string | null {
  if (!value || typeof value !== "object") return null;
  const { id, url } = value as { id?: unknown; url?: unknown };
  if (typeof id !== "string" || typeof url !== "string") return null;
  return JSON.stringify([id, url]);
}

/**
 * The stored embeds `submitted` keeps, in stored order, matched by `id` and
 * page `url`; null when `submitted` names an embed the message does not
 * have (the request is then refused).
 */
export function keptStoredEmbeds(
  submitted: readonly unknown[],
  stored: readonly EmbedInfo[],
): EmbedInfo[] | null {
  const wanted = new Set<string>();
  for (const embed of submitted) {
    const key = identity(embed);
    if (!key) return null;
    wanted.add(key);
  }
  const kept = stored.filter((embed) => {
    const key = identity(embed);
    return key !== null && wanted.delete(key);
  });
  return wanted.size === 0 ? kept : null;
}
