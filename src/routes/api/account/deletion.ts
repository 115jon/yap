import { createFileRoute } from "@tanstack/react-router";

import { accountDeletionRoutes } from "@/server/routes/account-deletion";

// Only referenced inside `server`, so the client build drops the import.
export const Route = createFileRoute("/api/account/deletion")({
  server: {
    handlers: accountDeletionRoutes(),
  },
});
