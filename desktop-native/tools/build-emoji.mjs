#!/usr/bin/env node
// Builds desktop-native/assets/emoji/emoji-15.tsv from the @emoji-mart/data
// 1.2.1 set 15 file (`sets/15/native.json`), applying the same shortcode
// rules as the web's src/lib/emoji.ts so the Rust side only has to parse.
//
//   node tools/build-emoji.mjs <path/to/native.json> [out.tsv]
//
// One line per emoji, in category order, tab-separated:
//   category id, emoji id, name,
//   skins (`unified` values, `,`-joined, default skin first),
//   shortcodes (space-joined, preferred first: sanitized aliases sorted by the
//     web's score, then the sanitized id),
//   keywords (space-joined), emoticons (space-joined)
// The native strings are not stored: they are the `unified` code points.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const input = process.argv[2];
if (!input) {
  console.error("usage: node build-emoji.mjs <native.json> [out.tsv]");
  process.exit(2);
}
const out =
  process.argv[3] ??
  resolve(
    dirname(fileURLToPath(import.meta.url)),
    "../assets/emoji/emoji-15.tsv",
  );

const data = JSON.parse(readFileSync(input, "utf8"));

function scoreAlias(alias) {
  let score = 0;
  if (alias.includes("+")) score += 4;
  if (/^\d/.test(alias)) score += 3;
  if (alias.includes("_")) score += 1;
  if (alias.length > 18) score += 1;
  return score;
}

function sanitize(value) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9_+-]+/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");
}

const flat = (value) => String(value).replace(/\s+/g, " ").trim();

const aliasesById = new Map();
for (const [alias, id] of Object.entries(data.aliases ?? {})) {
  const list = aliasesById.get(id) ?? [];
  list.push(alias);
  aliasesById.set(id, list);
}
for (const list of aliasesById.values()) {
  list.sort(
    (a, b) =>
      scoreAlias(a) - scoreAlias(b) ||
      a.length - b.length ||
      a.localeCompare(b),
  );
}

const nativeOf = (unified) =>
  String.fromCodePoint(...unified.split("-").map((hex) => parseInt(hex, 16)));

const lines = [];
for (const category of data.categories) {
  for (const emojiId of category.emojis ?? []) {
    const source = data.emojis[emojiId];
    if (!source) continue;

    const skins = (source.skins ?? []).filter(
      (skin) => skin.native && skin.unified,
    );
    if (skins.length === 0) continue;
    for (const skin of skins) {
      if (nativeOf(skin.unified) !== skin.native) {
        throw new Error(`${source.id}: native != unified ${skin.unified}`);
      }
    }

    const shortcodes = [
      ...new Set(
        [...(aliasesById.get(source.id) ?? []), source.id]
          .map(sanitize)
          .filter(Boolean),
      ),
    ];
    if (shortcodes.length === 0) continue;

    const words = (list) =>
      [...new Set((list ?? []).map(flat).filter(Boolean))].join(" ");
    lines.push(
      [
        category.id,
        source.id,
        flat(source.name ?? source.id),
        skins.map((skin) => skin.unified).join(","),
        shortcodes.join(" "),
        words(source.keywords),
        words(source.emoticons),
      ].join("\t"),
    );
  }
}

writeFileSync(out, lines.join("\n") + "\n");
console.log(`${lines.length} emoji -> ${out}`);
