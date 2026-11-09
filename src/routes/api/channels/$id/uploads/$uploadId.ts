import { createFileRoute } from "@tanstack/react-router";

import { multipartRoute } from "@/lib/multipart-upload-routes";

// GET    /api/channels/:id/uploads/:uploadId — the parts stored so far (resume).
// DELETE /api/channels/:id/uploads/:uploadId — abort the upload.
export const GET = multipartRoute("status");
export const DELETE = multipartRoute("abort");

export const Route = createFileRoute("/api/channels/$id/uploads/$uploadId")({
  server: {
    handlers: {
      GET,
      DELETE,
    },
  },
});
