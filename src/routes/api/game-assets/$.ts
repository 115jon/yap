import { createFileRoute } from "@tanstack/react-router";
import { GAME_IMAGE_PATH_PATTERN } from "@ralph/protocol";

import { apiError, getBucket } from "@/lib/api-helpers";
import { DEADLOCK_APP_ID } from "@/lib/deadlock";

// GET /api/game-assets/{appId}/{key}.png
// Game art Yap mirrored into its own R2 (hero portraits, rank badges and
// item icons for the Deadlock widget; src/services/deadlock/assets.ts).
// Public, like /api/game-icons, so <img> tags load it without a token, and
// read-only: this route never fetches upstream. A key that was not
// mirrored is a 404, so nothing here can pull a third party's file in.
const MIRRORED_APPS = new Set([DEADLOCK_APP_ID]);

function assetResponse(body: BodyInit | null, contentType: string): Response {
  const headers = new Headers();
  headers.set("Content-Type", contentType);
  // A patch can redraw a hero; a day is fresh enough and keeps the edge hot.
  headers.set(
    "Cache-Control",
    "public, max-age=86400, stale-while-revalidate=604800",
  );
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set(
    "Content-Security-Policy",
    "default-src 'none'; style-src 'unsafe-inline'",
  );
  headers.set("Content-Disposition", "inline");
  return new Response(body, { status: 200, headers });
}

export const GET = async ({
  params,
}: {
  request: Request;
  params?: { _splat?: string };
}) => {
  const splat = params?._splat ?? "";
  const path = `/api/game-assets/${splat}`;
  if (!GAME_IMAGE_PATH_PATTERN.test(path)) {
    return apiError("Image not found", 404);
  }
  const [appId, file] = splat.split("/");
  if (!appId || !file || !MIRRORED_APPS.has(appId)) {
    return apiError("Image not found", 404);
  }
  const object = await getBucket().get(`game-assets/${appId}/${file}`);
  if (!object) return apiError("Image not found", 404);
  const type = object.httpMetadata?.contentType || "image/png";
  return assetResponse(
    object.body as ReadableStream,
    /^image\/(png|webp|jpeg)$/.test(type) ? type : "application/octet-stream",
  );
};

export const Route = createFileRoute("/api/game-assets/$")({
  server: {
    handlers: {
      GET,
    },
  },
});
