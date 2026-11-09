import { createFileRoute } from "@tanstack/react-router";

import { multipartRoute } from "@/lib/multipart-upload-routes";

// POST /api/channels/:id/uploads/:uploadId/complete — assemble the parts and
// create the attachment (same response as messages/upload).
export const POST = multipartRoute("complete");

export const Route = createFileRoute(
  "/api/channels/$id/uploads/$uploadId/complete",
)({
  server: {
    handlers: {
      POST,
    },
  },
});
