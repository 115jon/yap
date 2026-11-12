# Contributing to Yap

Contributions are welcome through GitHub pull requests. Keep changes scoped,
include focused tests where behavior changes, and do not add generated binaries
or locally built release payloads to the repository.

## Before opening a pull request

- Read the [Code of Conduct](CODE_OF_CONDUCT.md) and [Security Policy](SECURITY.md).
- Use the repository's pinned `pnpm` toolchain.
- Run the relevant lint, typecheck, and focused test commands.
- Explain changes to installer behavior, native code, release workflows, or
  bundled third-party components in the pull request description.

## Writing the pull request

- Title: a conventional commit in plain language, such as
  `fix(voice): rejoining no longer leaves one-way audio`.
- Body: follow `.github/pull_request_template.md` (Problem, Change,
  Verification). Write it as a record for reviewers, not a message to the
  person who asked for the work: no "you can…", "let me know", questions,
  progress narration, or sign-offs. Anything meant for that person goes in
  the conversation, not the PR.
- Verification lists the checks actually run and what they showed, and names
  what was not checked. UI changes need before/after screenshots; motion or
  timing needs a short recording. Upload evidence to the PR; never commit
  PR-only assets.
- No AI attribution: no `Co-Authored-By` trailers, "Generated with" footers,
  or model and harness credits in commits or PRs.
- One request is one PR unless a split is asked for.

## Commits and releases

Commits are conventional commits; release-please turns `feat`, `fix` and
breaking changes on main into release PRs (`release-please-config.json`):

- The root package is the desktop's semver line (`vX.Y.Z`, `CHANGELOG.md`).
  It counts every commit except those that touch only
  `desktop-native/mobile/qt/`.
- The Qt Android app (`desktop-native/mobile/qt/`, tags `qt-android-vX.Y.Z`,
  its own `CHANGELOG.md`) counts the commits that touch that folder. Use
  scope `desktop-native` there too, e.g.
  `fix(desktop-native): keep the call bar above the keyboard on android`;
  `feat`/`fix` there release the app, other types release nothing. See
  `desktop-native/mobile/README.md`, "Releases".

Keep `feat`/`fix` for user-facing changes: a commit that touches both the
app and other paths shows up in both changelogs.

The root's `exclude-paths` also drops commits that change no files, so an
empty commit with a `Release-As: x.y.z` footer does not pin the root's next
version. Pin it with `"release-as"` in `release-please-config.json` (remove
it after the release), or put the footer on a commit that changes a file
outside `desktop-native/mobile/qt/`.

## Review expectations

One pull request should contain one logical change. Changes to installer
behavior, release workflows, signing configuration, authentication, CEF runtime
provenance, policies, or game capture require explicit maintainer review before
merge. Do not commit directly to `main` or bypass review for release-sensitive
files.

By submitting a contribution, you confirm that you have the right to contribute
it under the license applicable to the files you change. Yap-owned source
is distributed under GPL-3.0-or-later; the OBS-derived capture subtree remains
GPL-2.0-only.
