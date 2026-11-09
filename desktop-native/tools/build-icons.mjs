#!/usr/bin/env node
/**
 * Builds desktop-native/assets/icons/ from the web's icon table: the Phosphor
 * Bold SVG of every glyph src/components/ui/icon/mapping.ts uses, the Fill
 * SVG of concepts marked `fill: true`, and manifest.json (concept -> glyph,
 * fill, slash, rotate, plus OFF_PAIR). cpp/iconprovider.cpp reads the
 * manifest and composites slash and rotation the way Icon.tsx does.
 *
 * Also copies the plain-path brand SVGs (home marks, theme banner glyphs)
 * to desktop-native/assets/svg/.
 *
 *   node desktop-native/tools/build-icons.mjs
 *
 * Phosphor is MIT licensed (c) Phosphor Icons.
 */
import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const native = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const root = path.resolve(native, "..");
const iconDir = path.join(root, "src/components/ui/icon");
const outDir = path.join(native, "assets/icons");
const svgDir = path.join(native, "assets/svg");

// mapping.ts is import-free TypeScript; Node strips the types.
const { ICONS, OFF_PAIR } = await import(
  pathToFileURL(path.join(iconDir, "mapping.ts")).href
);

const require = createRequire(path.join(root, "package.json"));
const coreDir = path.resolve(
  path.dirname(require.resolve("@phosphor-icons/core/assets/bold/x-bold.svg")),
  "../..",
);
const coreVersion = JSON.parse(
  readFileSync(path.join(coreDir, "package.json"), "utf8"),
).version;

const SVG_RE =
  /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg" viewBox="0 0 256 256" fill="currentColor">((?:<path d="[^"]+"\/>)+)<\/svg>\s*$/;

/** One glyph weight as a plain black-path SVG (Qt Svg is SVG Tiny). */
function glyphSvg(glyph, weight) {
  const file = path.join(coreDir, "assets", weight, `${glyph}-${weight}.svg`);
  const match = SVG_RE.exec(readFileSync(file, "utf8"));
  if (!match) throw new Error(`Unexpected SVG structure in ${file}`);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="#000">${match[1]}</svg>\n`;
}

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

const files = new Set();
const concepts = {};
for (const [name, spec] of Object.entries(ICONS)) {
  const weights = spec.fill ? ["bold", "fill"] : ["bold"];
  for (const weight of weights) {
    const file = `${spec.glyph}-${weight}.svg`;
    if (!files.has(file)) {
      writeFileSync(path.join(outDir, file), glyphSvg(spec.glyph, weight));
      files.add(file);
    }
  }
  concepts[name] = {
    glyph: spec.glyph,
    ...(spec.fill ? { fill: true } : {}),
    ...(spec.slash ? { slash: true } : {}),
    ...(spec.rotate ? { rotate: spec.rotate } : {}),
  };
}

const manifest = {
  source: `@phosphor-icons/core ${coreVersion} (MIT, (c) Phosphor Icons) via src/components/ui/icon/mapping.ts`,
  concepts,
  offPair: OFF_PAIR,
};
writeFileSync(
  path.join(outDir, "manifest.json"),
  JSON.stringify(manifest, null, 1) + "\n",
);
writeFileSync(
  path.join(outDir, "LICENSE-phosphor.txt"),
  readFileSync(path.join(coreDir, "LICENSE"), "utf8").replaceAll("\r\n", "\n"),
);

// Qt resource list for the icons (CMake reads it; keeps the set in sync).
writeFileSync(
  path.join(outDir, "files.txt"),
  [...files].sort().concat(["manifest.json"]).join("\n") + "\n",
);

mkdirSync(svgDir, { recursive: true });
const brand = [
  ...readdirSync(path.join(root, "public/icons"))
    .filter((f) => /^home-.*\.svg$/.test(f))
    .map((f) => path.join(root, "public/icons", f)),
  path.join(root, "public/themes/miku/miku-wig.svg"),
  path.join(root, "public/themes/spiderman/spiderman-mask.svg"),
];
for (const file of brand)
  copyFileSync(file, path.join(svgDir, path.basename(file)));

console.log(
  `icons: ${Object.keys(concepts).length} concepts, ${files.size} SVGs (Phosphor ${coreVersion}); svg: ${brand.length} brand files`,
);
