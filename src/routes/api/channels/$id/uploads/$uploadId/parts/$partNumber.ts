import { createFileRoute } from "@tanstack/react-router";

import { multipartRoute } from "@/lib/multipart-upload-routes";

// PUT /api/channels/:id/uploads/:uploadId/parts/:partNumber — one part as the
// raw request body, with an exact Content-Length.
export const PUT = multipartRoute("part");

export const Route = createFileRoute(
  "/api/channels/$id/uploads/$uploadId/parts/$partNumber",
)({
  server: {
    handlers: {
      PUT,
    },
  },
});
