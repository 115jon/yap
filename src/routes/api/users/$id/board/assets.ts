import { createFileRoute } from "@tanstack/react-router";

import { profileBoardRoute } from "@/lib/profile-board-routes";

export const Route = createFileRoute("/api/users/$id/board/assets")({
  server: {
    handlers: {
      POST: profileBoardRoute("uploadAsset"),
    },
  },
});
