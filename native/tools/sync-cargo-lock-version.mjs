#!/usr/bin/env node
// Sets the workspace crates' versions in native/rust/Cargo.lock to
// the versions their Cargo.toml files give them.
//
// release-please bumps `[workspace.package] version` in rust/Cargo.toml
// (release-please-config.json), but not Cargo.lock, and the release build
// runs cargo with --locked. `cargo update -w` would do this, but it needs
// the patched crates (patches/vendor.ps1) and the registry, so this edits
// the lock file's text instead.
//
// Only workspace members (and the root package) change: the [patch] path
// crates (qt-build-utils, webrtc-sys, libwebrtc) are also lock entries
// without a `source`, but they keep their own upstream versions.
//
// The mobile prototypes are workspaces of their own that use these crates
// by path (native/mobile/rust builds --locked), so their lock
// files carry the same versions: `--also <Cargo.lock>` syncs another lock
// to this workspace's versions (only the crates it has).
//
// Usage: node native/tools/sync-cargo-lock-version.mjs [--check]
//          [--also <Cargo.lock>]... [rust dir]
//   --check  change nothing; exit 1 if a lock is out of date.
// Idempotent: a second run finds nothing to change.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const check = args.includes("--check");
const also = [];
const rest = [];
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--check") continue;
  if (args[i] === "--also" && i + 1 < args.length) {
    also.push(path.resolve(args[++i]));
    continue;
  }
  rest.push(args[i]);
}
if (rest.length > 1 || rest.some((a) => a.startsWith("-"))) {
  console.error(
    "usage: sync-cargo-lock-version.mjs [--check] [--also <Cargo.lock>]... [rust dir]",
  );
  process.exit(2);
}
const here = path.dirname(fileURLToPath(import.meta.url));
const rustDir = path.resolve(rest[0] ?? path.join(here, "..", "rust"));

function fail(message) {
  console.error(`sync-cargo-lock-version: ${message}`);
  process.exit(1);
}

// The lines of one [table] (until the next table header), from a Cargo.toml.
function table(text, name) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((l) => l.trim() === `[${name}]`);
  if (start < 0) return null;
  const body = [];
  for (const line of lines.slice(start + 1)) {
    if (/^\s*\[/.test(line)) break;
    body.push(line);
  }
  return body.join("\n");
}

function stringKey(body, key) {
  const m = body?.match(new RegExp(`^\\s*${key}\\s*=\\s*"([^"]*)"`, "m"));
  return m ? m[1] : null;
}

// The version a member's [package] table gives it: its own, or the workspace's.
function packageVersion(body, workspaceVersion, file) {
  if (/^\s*version\.workspace\s*=\s*true\s*$/m.test(body))
    return workspaceVersion;
  if (/^\s*version\s*=\s*\{\s*workspace\s*=\s*true\s*\}\s*$/m.test(body))
    return workspaceVersion;
  const own = stringKey(body, "version");
  if (own) return own;
  fail(`${file}: [package] has no version`);
}

const rootToml = path.join(rustDir, "Cargo.toml");
const rootText = fs.readFileSync(rootToml, "utf8");
const workspaceVersion = stringKey(
  table(rootText, "workspace.package"),
  "version",
);
if (!workspaceVersion) fail(`${rootToml}: no [workspace.package] version`);
if (
  !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.test(workspaceVersion)
) {
  fail(
    `${rootToml}: [workspace.package] version "${workspaceVersion}" is not semver`,
  );
}

// name -> version, for the root package and every member.
const wanted = new Map();
const rootPackage = table(rootText, "package");
if (rootPackage) {
  const name = stringKey(rootPackage, "name");
  if (!name) fail(`${rootToml}: [package] has no name`);
  wanted.set(name, packageVersion(rootPackage, workspaceVersion, rootToml));
}
const membersMatch = table(rootText, "workspace")?.match(
  /^\s*members\s*=\s*\[([\s\S]*?)\]/m,
);
if (!membersMatch) fail(`${rootToml}: no [workspace] members`);
const members = [...membersMatch[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
for (const member of members) {
  if (member.includes("*"))
    fail(`member glob "${member}" is not supported; list members by path`);
  const file = path.join(rustDir, member, "Cargo.toml");
  const body = table(fs.readFileSync(file, "utf8"), "package");
  const name = stringKey(body, "name");
  if (!name) fail(`${file}: [package] has no name`);
  wanted.set(name, packageVersion(body, workspaceVersion, file));
}

// Syncs one lock file; `all`: every wanted crate must be in it (this
// workspace's own lock). Returns the number of out-of-date crates.
function sync(lockFile, all) {
  const lockText = fs.readFileSync(lockFile, "utf8");
  const eol = lockText.includes("\r\n") ? "\r\n" : "\n";
  // Blocks are separated by blank lines; each [[package]] block is one entry.
  const blocks = lockText.split(`${eol}${eol}`);
  const changed = [];
  const seen = new Set();
  const renamed = new Map(); // "name old" -> "name new", for dependency lists
  const out = blocks.map((block) => {
    if (!block.startsWith("[[package]]")) return block;
    const name = block.match(/^name = "([^"]+)"$/m)?.[1];
    const version = block.match(/^version = "([^"]+)"$/m)?.[1];
    const hasSource = /^source = /m.test(block);
    if (!name || !version || hasSource || !wanted.has(name)) return block;
    if (seen.has(name))
      fail(`${name} appears twice without a source in ${lockFile}`);
    seen.add(name);
    const want = wanted.get(name);
    if (version === want) return block;
    changed.push(`${name} ${version} -> ${want}`);
    renamed.set(`${name} ${version}`, `${name} ${want}`);
    return block.replace(/^version = "[^"]+"$/m, `version = "${want}"`);
  });
  const missing = [...wanted.keys()].filter((n) => !seen.has(n));
  if (all && missing.length)
    fail(`not in ${lockFile} as path packages: ${missing.join(", ")}`);

  // A dependency is written "name version" only when the lock holds two
  // versions of that name; rewrite those references too.
  let result = out.join(`${eol}${eol}`);
  for (const [from, to] of renamed) {
    result = result.split(`"${from}"`).join(`"${to}"`);
    result = result.split(`"${from} (`).join(`"${to} (`);
  }

  if (!changed.length) {
    console.log(
      `${lockFile}: ${seen.size} workspace crates already at their Cargo.toml versions (${workspaceVersion}).`,
    );
    return 0;
  }
  for (const line of changed) console.log(`  ${line}`);
  if (!check) {
    fs.writeFileSync(lockFile, result);
    console.log(
      `${lockFile}: set ${changed.length} workspace crates to their Cargo.toml versions (${workspaceVersion}).`,
    );
  }
  return changed.length;
}

let stale = sync(path.join(rustDir, "Cargo.lock"), true);
for (const lockFile of also) stale += sync(lockFile, false);
if (check && stale) {
  fail(
    `${stale} crates out of date; run node native/tools/sync-cargo-lock-version.mjs`,
  );
}
