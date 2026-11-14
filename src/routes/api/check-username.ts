import { createFileRoute } from "@tanstack/react-router";

import { apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { clog } from "@/lib/console-logger";
import { safeParse } from "@/lib/schema";
import { UsernameSchema } from "@/lib/validations";

const log = clog("check-username");

const GET = async ({ request: req }: any) => {
  const authResult = await requireAuth();
  if (authResult instanceof Response) return authResult;
  const { userId } = authResult;

  const url = new URL(req.url);
  const rawUsername = url.searchParams.get("username")?.trim();
  if (!rawUsername) {
    return apiSuccess({ available: false, reason: "too_short" });
  }
  const parsedUsername = safeParse(UsernameSchema, rawUsername);
  if (!parsedUsername.success) {
    return apiSuccess({ available: false, reason: "invalid" });
  }
  const username = parsedUsername.data;

  try {
    const existing = await getDB()
      .prepare(
        // A username is also held while its account can still be restored.
        `SELECT id FROM users WHERE lower(username) = ?1
         UNION ALL
         SELECT user_id AS id FROM account_deletions WHERE reserved_username = ?1
         LIMIT 1`,
      )
      .bind(username.toLowerCase())
      .first<{ id: string }>();

    return apiSuccess({ available: !existing || existing.id === userId });
  } catch (err) {
    log.error("Error:", err);
    return apiSuccess({ available: false, reason: "error" });
  }
};

export const Route = createFileRoute("/api/check-username")({
  server: {
    handlers: {
      GET,
    },
  },
});
