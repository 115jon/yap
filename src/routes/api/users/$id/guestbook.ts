import { createFileRoute } from "@tanstack/react-router";

import { profileBoardRoute } from "@/lib/profile-board-routes";

export const Route = createFileRoute("/api/users/$id/guestbook")({
  server: {
    handlers: {
      GET: profileBoardRoute("getGuestbook"),
      POST: profileBoardRoute("postGuestbook"),
    },
  },
});
