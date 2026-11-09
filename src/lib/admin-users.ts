/**
 * The admin gate: user ids in COLLECTIBLES_ADMIN_USER_IDS ("id1, id2") may
 * use POST /api/admin/badges (manual badges). Unset means nobody. Forcing a
 * catalog refresh is the admin console's (yap-admin), not this gate's.
 */

import { parseCollectiblesAdminIds } from "@/lib/collectibles-catalog";

export function adminUserIds(env: unknown): Set<string> {
  const vars = (env ?? {}) as Record<string, unknown>;
  return parseCollectiblesAdminIds(vars.COLLECTIBLES_ADMIN_USER_IDS);
}
