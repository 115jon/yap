import { createFileRoute } from "@tanstack/react-router";

import { profileBoardRoute } from "@/lib/profile-board-routes";

export const Route = createFileRoute("/api/users/$id/profile-prefs")({
  server: {
    handlers: {
      GET: profileBoardRoute("getPrefs"),
      PATCH: profileBoardRoute("patchPrefs"),
    },
  },
});
