import { createFileRoute } from "@tanstack/react-router";

import { profileBoardRoute } from "@/lib/profile-board-routes";

export const Route = createFileRoute("/api/users/$id/guestbook/$noteId")({
  server: {
    handlers: {
      DELETE: profileBoardRoute("deleteGuestbookNote"),
    },
  },
});
