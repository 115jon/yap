#!/usr/bin/env node
/**
 * Builds Yap's game catalog (`public/desktop/game-catalog.json`) from
 * Discord's public detectable-applications list, so the desktop app can
 * recognise running games ("Playing X") against our own file instead of
 * calling Discord. Refresh it by re-running this script and committing the
 * result (`pnpm games:catalog`).
 *
 *   node scripts/build-game-catalog.mjs                  fetch + write default
 *   node scripts/build-game-catalog.mjs --out <file>     write somewhere else
 *   node scripts/build-game-catalog.mjs --input <file>   build from a saved
 *                                                        copy of the list
 *
 * The fetch is a plain unauthenticated GET of
 * https://discord.com/api/v9/applications/detectable (~13 MB, ~25k apps)
 * with a neutral User-Agent: no cookies, no tokens, nothing about a person.
 *
 * Output (minified, sorted by id so re-runs diff cleanly):
 *
 *   {
 *     "version": 1,
 *     "generated_at": "2026-11-08T12:00:00.000Z",
 *     "source": "discord-detectable",
 *     "etag": "<first 16 hex of sha256 over the apps array as written>",
 *     "count": 10466,
 *     "apps": [{
 *       "id": "356875221078245376",            Discord application id (string)
 *       "name": "Overwatch",
 *       "aliases": ["..."],                    only when non-empty
 *       "icon_hash": "a60bb76b...",            32 hex; omitted when missing
 *       "executables": [{                      win32 only
 *         "name": "overwatch.exe",
 *         "is_launcher": true,                 only when true
 *         "arguments": "..."                   only when present
 *       }],
 *       "steam_app_ids": ["2357570"]           only when the app has Steam SKUs
 *     }]
 *   }
 *
 * Apps with no win32 executable are dropped (the Windows client cannot detect
 * them). Single-letter keys would save only ~2% gzipped, so the file keeps
 * readable keys. Icons are not stored here: a client builds
 * `/api/game-icons/{id}/{icon_hash}.png`, which our server fills from
 * Discord's CDN once and keeps in R2.
 */
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

import { buildCatalog } from "./lib/game-catalog.mjs";

const DETECTABLE_URL = "https://discord.com/api/v9/applications/detectable";
const USER_AGENT = "YapGameCatalogBuilder/1";
const DEFAULT_OUT = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../public/desktop/game-catalog.json",
);

/** @param {string[]} argv */
function parseArgs(argv) {
  /** @type {{ input: string | null, out: string, help?: boolean }} */
  const args = { input: null, out: DEFAULT_OUT };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (flag === "--input" || flag === "--out") {
      const value = argv[i + 1];
      if (!value) throw new Error(`${flag} needs a file path`);
      if (flag === "--input") args.input = resolve(value);
      else args.out = resolve(value);
      i += 1;
    } else if (flag === "--help" || flag === "-h") {
      args.help = true;
    } else {
      throw new Error(`unknown argument: ${flag}`);
    }
  }
  return args;
}

/** @param {string | null} input */
async function loadRaw(input) {
  if (input) {
    const text = await readFile(input, "utf8");
    return { text, bytes: Buffer.byteLength(text) };
  }
  const response = await fetch(DETECTABLE_URL, {
    headers: { "User-Agent": USER_AGENT, Accept: "application/json" },
    redirect: "error",
    signal: AbortSignal.timeout(60_000),
  });
  if (!response.ok) {
    throw new Error(`detectable list: HTTP ${response.status}`);
  }
  const buffer = Buffer.from(await response.arrayBuffer());
  return { text: buffer.toString("utf8"), bytes: buffer.length };
}

/** @param {number} bytes */
const kb = (bytes) => `${(bytes / 1024).toFixed(1)} KB`;

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log(
      "usage: build-game-catalog.mjs [--input <file>] [--out <file>]",
    );
    return;
  }
  const raw = await loadRaw(args.input);
  const { catalog, stats } = buildCatalog(JSON.parse(raw.text));
  const json = JSON.stringify(catalog);
  await mkdir(dirname(args.out), { recursive: true });
  await writeFile(args.out, json);

  console.log(`source:   ${args.input ?? DETECTABLE_URL}`);
  console.log(`raw:      ${kb(raw.bytes)} (${stats.input} apps)`);
  console.log(
    `kept:     ${stats.kept} apps; dropped ${stats.dropped} with no win32 executable` +
      (stats.invalid > 0 ? `; ${stats.invalid} malformed skipped` : ""),
  );
  console.log(
    `output:   ${kb(Buffer.byteLength(json))}, ${kb(gzipSync(json, { level: 9 }).length)} gzipped`,
  );
  console.log(`etag:     ${catalog.etag}`);
  console.log(`wrote:    ${args.out}`);
}

main().catch((error) => {
  console.error(`build-game-catalog: ${error.message}`);
  process.exitCode = 1;
});
