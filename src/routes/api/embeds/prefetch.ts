import { createFileRoute } from "@tanstack/react-router";
import { Schema } from "effect";

import { apiError, apiSuccess, requireAuth } from "@/lib/api-helpers";
import {
  extractEmbedUrls,
  looksPrivateLink,
  MAX_EMBEDS_PER_MESSAGE,
} from "@/lib/embed-urls";
import { checkRateLimit } from "@/lib/rate-limit";
import { isFetchablePreviewUrl } from "@/lib/url-guard";
import { validateBody } from "@/lib/validate-body";
import { featurePaused } from "@/services/site-flags.service";
import { prefetchEmbedUrls } from "@/services/embed-fetcher";

/**
 * A composer pastes or finishes typing a link: resolve its embed now so it is
 * cached (KV) by the time the message is sent, and goes out with
 * MESSAGE_CREATE. Before, nothing happened until the send, and the embed came
 * a queue hop and a provider round trip later.
 *
 * Only positive results are cached from here (half-typed URLs must not spend
 * KV writes). Nothing from the caller's request is forwarded upstream.
 *
 * Links that look like they carry a credential or open a sign-in step (magic
 * links, password resets, OAuth callbacks, signed URLs; looksPrivateLink) are
 * not fetched: the message is not sent yet, and fetching one could spend it.
 */
export const EMBED_PREFETCH_RATE_LIMIT = { limit: 30, windowMs: 60_000 };
const MAX_URL_LENGTH = 2048;

const prefetchSchema = Schema.Struct({
  urls: Schema.mutable(
    Schema.Array(Schema.String.check(Schema.isMaxLength(MAX_URL_LENGTH))),
  ).check(Schema.isMaxLength(MAX_EMBEDS_PER_MESSAGE)),
});

export const POST = async ({ request }: { request: Request }) => {
  const paused = await featurePaused("linkEmbeds", request);
  if (paused) return paused;
  const authResult = await requireAuth();
  if (authResult instanceof Response) return authResult;
  const { userId } = authResult;

  const limited = checkRateLimit(
    userId,
    "embed-prefetch",
    EMBED_PREFETCH_RATE_LIMIT,
  );
  if (limited) return limited;

  const body = await validateBody(request, prefetchSchema, request);
  if (body instanceof Response) return body;

  // The same extraction the send path runs, so the cache keys match.
  const urls = extractEmbedUrls(body.urls.join(" ")).filter(
    (url) => isFetchablePreviewUrl(url) && !looksPrivateLink(url),
  );
  if (urls.length === 0) {
    return apiError("No embeddable URL", 400, "NO_EMBEDDABLE_URL");
  }

  // Only a count goes back: the composer needs nothing more, and returning the
  // scraped metadata would make this a general-purpose page scraper.
  const embeds = await prefetchEmbedUrls(urls);
  return apiSuccess({ resolved: embeds.length });
};

export const Route = createFileRoute("/api/embeds/prefetch")({
  server: {
    handlers: { POST },
  },
});
