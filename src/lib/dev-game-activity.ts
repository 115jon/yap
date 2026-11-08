/**
 * Development only: set this tab's game activity by hand, to test "Playing X"
 * end to end before the desktop app detects games. From the dev tools
 * console:
 *
 *   __yapPlaying("Minecraft")   // looks the name up in the game catalog
 *   __yapPlaying(null)          // stop playing
 *
 * A name found in /desktop/game-catalog.json (by name or alias, ignoring
 * case) gets the catalog's app id, Steam id and icon (through our own
 * /api/game-icons route); any other name is sent with no icon. Installed by
 * the chat store only when `import.meta.env.DEV`.
 */

import {
  gameIconPath,
  type GameActivity,
  type MutableGameActivity,
} from "@/lib/gateway-protocol";

interface CatalogApp {
  id: string;
  name: string;
  aliases?: string[];
  icon_hash?: string;
  steam_app_ids?: string[];
}

let catalog: Promise<CatalogApp[]> | null = null;

function loadCatalog(): Promise<CatalogApp[]> {
  catalog ??= fetch("/desktop/game-catalog.json")
    .then(async (response) =>
      response.ok ? ((await response.json()) as { apps?: CatalogApp[] }) : {},
    )
    .then((body) => body.apps ?? [])
    .catch(() => []);
  return catalog;
}

export async function devGameActivity(
  name: string,
  now = Date.now(),
): Promise<GameActivity> {
  const wanted = name.trim().toLowerCase();
  const apps = await loadCatalog();
  const app = apps.find(
    (candidate) =>
      candidate.name.toLowerCase() === wanted ||
      candidate.aliases?.some((alias) => alias.toLowerCase() === wanted),
  );
  const icon =
    app?.icon_hash !== undefined ? gameIconPath(app.id, app.icon_hash) : null;
  const activity: MutableGameActivity = {
    kind: "playing",
    name: (app?.name ?? name.trim()).slice(0, 128),
    started_at: now,
    source: "manual",
  };
  if (app) activity.app_id = app.id;
  const steamAppId = app?.steam_app_ids?.[0];
  if (steamAppId) activity.steam_app_id = steamAppId;
  if (icon) activity.icon_url = icon;
  return activity;
}

export function installDevGameActivity(
  setGameActivity: (activity: GameActivity | null) => void,
) {
  if (typeof window === "undefined") return;
  (window as unknown as Record<string, unknown>).__yapPlaying = async (
    name: string | null,
  ) => {
    if (!name?.trim()) {
      setGameActivity(null);
      return null;
    }
    const activity = await devGameActivity(name);
    setGameActivity(activity);
    return activity;
  };
}
