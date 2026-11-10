import { createFileRoute } from "@tanstack/react-router";

import { profileBoardRoute } from "@/lib/profile-board-routes";

export const Route = createFileRoute("/api/users/$id/guestbook/blocks/$userId")(
  {
    server: {
      handlers: {
        DELETE: profileBoardRoute("unblockGuestbook"),
      },
    },
  },
);
