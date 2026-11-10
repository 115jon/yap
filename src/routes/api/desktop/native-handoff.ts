import { createFileRoute } from "@tanstack/react-router";

import { apiSuccess } from "@/lib/api-helpers";
import { nativeHandoffConfig } from "@/services/site-flags.service";

/**
 * GET /api/desktop/native-handoff: whether the Tauri desktop app (3.14, the
 * last Tauri release) installs the native app and hands over to it, and the
 * share of installs that do: `{ enabled, rolloutPercent }`. Set in the admin
 * console's Switches; off at 0 % until the owner turns it on. Public and the
 * same for everyone, so it may be cached briefly. The app treats any answer
 * it cannot read, and no answer, as off.
 */
export const GET = async ({ request }: { request: Request }) => {
  const { enabled, rolloutPercent } = await nativeHandoffConfig();
  const response = apiSuccess({ enabled, rolloutPercent }, 200, request);
  response.headers.set("Cache-Control", "public, max-age=60");
  return response;
};

export const Route = createFileRoute("/api/desktop/native-handoff")({
  server: {
    handlers: {
      GET,
    },
  },
});
