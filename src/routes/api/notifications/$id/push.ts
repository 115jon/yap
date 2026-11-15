import { createFileRoute } from "@tanstack/react-router";

import { notificationPushRoutes } from "@/server/routes/notification-push";

// Only referenced inside `server`, so the client build drops the import.
export const Route = createFileRoute("/api/notifications/$id/push")({
  server: {
    handlers: notificationPushRoutes(),
  },
});
