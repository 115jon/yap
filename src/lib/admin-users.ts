/**
 * The admin gate: user ids in COLLECTIBLES_ADMIN_USER_IDS ("id1, id2") may
 * use the admin routes (catalog sync, manual badges). Unset means nobody.
 */

import { parseCollectiblesAdminIds } from "@/lib/collectibles-catalog";

export function adminUserIds(env: unknown): Set<string> {
  const vars = (env ?? {}) as Record<string, unknown>;
  return parseCollectiblesAdminIds(vars.COLLECTIBLES_ADMIN_USER_IDS);
}
