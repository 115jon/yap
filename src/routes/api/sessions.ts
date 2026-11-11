import { createFileRoute } from "@tanstack/react-router";

import { sessionRoutes } from "@/server/routes/sessions";

// Only referenced inside `server`, so the client build drops the import.
export const Route = createFileRoute("/api/sessions")({
  server: {
    handlers: sessionRoutes(),
  },
});
