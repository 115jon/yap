import HomePageClient from "@/components/HomePageClient";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
  component: HomePageClient,
  head: () => ({
    meta: [
      { title: "Yap: voice, chat and video rooms" },
      {
        name: "description",
        content:
          "Voice, chat, video and screen share for the people you talk to every day. Start a room, send the link, they're in. Runs on Cloudflare's edge.",
      },
    ],
  }),
});
