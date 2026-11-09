import { createFileRoute } from "@tanstack/react-router";

import { multipartRoute } from "@/lib/multipart-upload-routes";

// POST /api/channels/:id/uploads — start a chunked attachment upload.
// Body: { filename, size, content_type }. See multipart-upload.service.
export const POST = multipartRoute("create");

export const Route = createFileRoute("/api/channels/$id/uploads")({
  server: {
    handlers: {
      POST,
    },
  },
});
