// Packs the web app's game catalog (public/desktop/game-catalog.json, built
// by scripts/build-game-catalog.mjs) into the snapshot the native app
// bundles: assets/games/game-catalog.json.gz (gzip -9, the same JSON) and
// game-catalog.meta.json (its etag, date and size). yap-games embeds both;
// the app refreshes the catalog from the web at most daily.
//
//   node tools/pack-game-catalog.mjs [path/to/game-catalog.json]

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync, constants } from "node:zlib";

const here = dirname(fileURLToPath(import.meta.url));
const source = resolve(
  process.argv[2] ?? join(here, "../../public/desktop/game-catalog.json"),
);
const out = join(here, "../assets/games");

const raw = readFileSync(source);
const catalog = JSON.parse(raw.toString("utf8"));
if (catalog.version !== 1 || !Array.isArray(catalog.apps)) {
  throw new Error(`${source}: not a version 1 game catalog`);
}
const gz = gzipSync(raw, { level: constants.Z_BEST_COMPRESSION, memLevel: 9 });
mkdirSync(out, { recursive: true });
writeFileSync(join(out, "game-catalog.json.gz"), gz);
const meta = {
  etag: catalog.etag,
  generated_at: catalog.generated_at,
  count: catalog.apps.length,
  bytes: raw.length,
  gzip_bytes: gz.length,
};
writeFileSync(
  join(out, "game-catalog.meta.json"),
  `${JSON.stringify(meta, null, 2)}\n`,
);
console.log(
  `${meta.count} apps, ${raw.length} bytes, ${gz.length} gzipped -> ${out}`,
);
