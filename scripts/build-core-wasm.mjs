#!/usr/bin/env node
// Builds packages/rust/yap-core-wasm for the browser and the Worker:
// cargo (wasm32-unknown-unknown, release), then wasm-bindgen --target web,
// into src/lib/yap-core-wasm/pkg.
//
// The JS glue and its .d.ts are committed, so typecheck runs without Rust.
// The .wasm is not (CONTRIBUTING: no generated binaries), so a build, a
// test run or `vite dev` needs this first. `--check` builds into a temp
// dir and fails when the committed glue differs (CI's rust-shared job).
//
// Needs rustup (packages/rust/rust-toolchain.toml brings the compiler and
// the wasm32-unknown-unknown target) and wasm-bindgen-cli at exactly the
// crate's wasm-bindgen version (WASM_BINDGEN_VERSION).

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const WASM_BINDGEN_VERSION = "0.2.130";
const GLUE = ["yap_core_wasm.js", "yap_core_wasm.d.ts"];

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const rustDir = path.join(root, "packages/rust");
const pkg = path.join(root, "src/lib/yap-core-wasm/pkg");
const check = process.argv.includes("--check");

function run(cmd, args, cwd = root) {
  return execFileSync(cmd, args, {
    cwd,
    stdio: ["ignore", "pipe", "inherit"],
  }).toString();
}

let version;
try {
  version = run("wasm-bindgen", ["--version"]).trim();
} catch {
  console.error(
    "build-core-wasm: wasm-bindgen not found. Install it with\n" +
      `  cargo install wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION} --locked`,
  );
  process.exit(1);
}
if (version !== `wasm-bindgen ${WASM_BINDGEN_VERSION}`) {
  console.error(
    `build-core-wasm: found ${version}, need ${WASM_BINDGEN_VERSION} (the crate pins it).`,
  );
  process.exit(1);
}

// From packages/rust, so rustup picks its rust-toolchain.toml.
run(
  "cargo",
  [
    "build",
    "--quiet",
    "--locked",
    "--release",
    "-p",
    "yap-core-wasm",
    "--target",
    "wasm32-unknown-unknown",
  ],
  rustDir,
);
const targetDir = process.env.CARGO_TARGET_DIR ?? path.join(rustDir, "target");
const wasm = path.join(
  targetDir,
  "wasm32-unknown-unknown/release/yap_core_wasm.wasm",
);

const out = check
  ? fs.mkdtempSync(path.join(os.tmpdir(), "yap-core-wasm-"))
  : pkg;
fs.mkdirSync(out, { recursive: true });
run("wasm-bindgen", [
  "--target",
  "web",
  // Debug names and the toolchain list are dead weight in the download.
  "--remove-name-section",
  "--remove-producers-section",
  "--out-dir",
  out,
  wasm,
]);
// Formatted like the rest of the tree, so format:check passes and the
// committed glue compares byte for byte.
run(process.execPath, [
  path.join(root, "node_modules/oxfmt/bin/oxfmt"),
  ...GLUE.map((file) => path.join(out, file)),
]);

const bytes = fs.statSync(path.join(out, "yap_core_wasm_bg.wasm")).size;
if (check) {
  const stale = GLUE.filter(
    (file) =>
      !fs.existsSync(path.join(pkg, file)) ||
      fs.readFileSync(path.join(pkg, file), "utf8") !==
        fs.readFileSync(path.join(out, file), "utf8"),
  );
  fs.rmSync(out, { recursive: true, force: true });
  if (stale.length > 0) {
    console.error(
      `build-core-wasm: ${stale.join(", ")} differ from a fresh build: run pnpm core-wasm:build and commit them.`,
    );
    process.exit(1);
  }
}
console.log(`build-core-wasm: ${(bytes / 1024).toFixed(1)} KiB wasm`);
