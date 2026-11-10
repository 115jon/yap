# Installer (Velopack)

The native client ships as one static `Yap.exe` and replaces the Tauri
desktop app (`desktop/`) under its name: "Yap". Until 0.2.21 it was "Yap
Native" (`YapNative.exe`, pack id `YapNative.App`) and ran beside the Tauri
app; see "Rename from Yap Native" for what moved. Its installer is
[Velopack](https://docs.velopack.io): `vpk pack` wraps the exe into a native
`Setup.exe` (no .NET at install time), a full `.nupkg` and the release feed
files. It replaces the .NET/WPF `YapSetup.exe` that the Tauri app uses
(`desktop/installer`), and on its first start it uninstalls the Tauri app
(see "Replacing the Tauri app").

## Identity and layout

| what           | value                                                                                                 |
| -------------- | ----------------------------------------------------------------------------------------------------- |
| packId         | `Yap.App` (test installs: `Yap.InstallTest`), `yap_core::install::PACK_ID`                            |
| install root   | `%LOCALAPPDATA%\Yap.App`                                                                              |
| the exe        | `%LOCALAPPDATA%\Yap.App\current\Yap.exe`                                                              |
| launcher stub  | `%LOCALAPPDATA%\Yap.App\Yap.exe` (Velopack's, named after the title)                                  |
| shortcuts      | `Desktop\Yap.lnk`, `Start Menu\Programs\Yap.lnk`, both to `current\Yap.exe`                           |
| updater        | `%LOCALAPPDATA%\Yap.App\Update.exe`                                                                   |
| uninstall key  | `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Yap.App`, DisplayName "Yap"                 |
| data root      | `%LOCALAPPDATA%\Yap.Data` (profiles under `profiles\<name>`), never touched by Velopack               |
| capture hook   | `current\obs-capture\obs-signed\` (six OBS-signed files, see "Game capture hook payload")             |
| title, authors | "Yap", "115jon" (Start menu, Desktop, Apps & features); `Yap.exe` carries the same in its VERSIONINFO |
| shortcut AUMID | `com.115jon.yap` (the Tauri app's), the default profile's toast identity (`cpp/desktopshell.cpp`)     |
| schemes        | `yap://`, and `yap-native://` as an alias (the same links)                                            |
| Run value      | `Yap` (the Tauri app's name), `Yap-<profile>` for a named profile                                     |
| version        | `[workspace.package]` in `rust/Cargo.toml`, read by `CMakeLists.txt` and `tools/package.ps1`          |
| Velopack       | crate `velopack` `=1.3.39-g8d2844f` (`public-utils`); `vpk` of the same version (the script checks)   |

**Why `Yap.App` as the packId.** Velopack installs to `%LOCALAPPDATA%\<packId>`,
writes its uninstall entry under `Uninstall\<packId>` and deletes that whole
folder on uninstall. A packId of `Yap` would land in the Tauri app's install
root (`%LOCALAPPDATA%\Yap`, which the Tauri uninstaller deletes once it
exits) and on its uninstall key (`Uninstall\Yap`), so replacing the Tauri app
would uninstall this one. `Yap.App` keeps the three apart and matches the old
`YapNative.App` pattern. The packId is permanent for updates: changing it
later means a new app as far as Velopack is concerned.

**Why `Yap.Data` as the data root.** Not `%LOCALAPPDATA%\Yap` (the Tauri
install root, deleted by its uninstaller) and not inside `Yap.App` (deleted by
ours). The Tauri app's own data is `%APPDATA%\Yap` (roaming); this app's has
always been local.

**What keeps its old name.** Internal names nobody sees: the crates
(`yap_native`), the QML module (`com.yap.native`), the CMake target
(`YapNative`, output `Yap.exe`), the credential service
(`com.115jon.yap.native-spike`, so nobody is signed out), the WebView2 host
(`player.yapnative.local`), the log file (`logs\yap-native.log`), the user
agent (`YapNative/<version>`), the test scheme prefix (`yap-native-<name>`)
and the share test window title ("Yap Native Share Test").

## App integration

`cpp/main.cpp` calls `yap_velopack_startup()` (`rust/src/installer.rs`) as
its first statement, before Qt, the process AUMID and the single-instance
lock. It runs `VelopackApp::build()...run()`:

- On a normal launch it returns at once (outside an install, Velopack finds
  no `Update.exe` and does nothing).
- Velopack runs the installed exe with `--veloapp-install`,
  `--veloapp-updated` or `--veloapp-uninstall <version>`. The matching
  "fast callback" runs and the process exits; Velopack kills it after 15 s
  (update) or 30 s (install, uninstall). These hooks do a few registry calls
  and return in milliseconds. They never create a window, never take the
  instance lock and never forward arguments to a running instance.
- A panic inside Velopack is caught; the app then starts as usual.

The hooks act only when the exe really sits in `<root>\current\` next to
`<root>\Update.exe`, and only on values that belong to that root. The rules
are in `yap_core::install` (tested in `crates/yap-core/tests/install.rs`).

| hook             | does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| after install    | Registers `yap://` and `yap-native://` to `current\Yap.exe` (the same values the app writes, `yap_core::links::scheme_registration`, `install::register_schemes`). The released install (packId `Yap.App`) always does, taking them back from a dev build, and `yap://` from the Tauri app; another packId only where the scheme is free, dangling or already its own (`yap_core::install::hook_may_take_scheme`). Creates `HKCU\Software\Classes\AppUserModelId\com.115jon.yap` with `DisplayName` "Yap" and a `YapInstallRoot` marker if the key is absent (`install::register_aumid`). The Tauri app is not touched here: its uninstaller can take longer than the 30 s Velopack gives a hook. |
| after update     | Registers the schemes again, by the same rule. The exe path under `current\` does not change, so the schemes and Run values stay valid across updates.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| before uninstall | Removes each scheme key whose open command runs an exe in this root; removes the AUMID key and the default profile's `toast-icon.png` if the marker names this root; removes every `Yap` / `Yap-<profile>` Run value (and `YapNative` / `YapNative-<profile>` from before the rename) whose command runs an exe in this root (`install::unregister`). A Tauri app's `Yap` value or `yap://` pointing elsewhere is never touched.                                                                                                                                                                                                                                                                  |

The app itself still registers on every start: the AUMID key (`DisplayName`,
`IconUri`) and the Run value when autostart is on. The schemes are rewritten
at start only by the released install running as the default profile
(`<root named Yap.App>\current\Yap.exe` next to `Update.exe`,
`yap_core::install::may_register_at_launch`); a dev build, a copy, a test
install or any named profile never writes them, so the installed app keeps
them. The Run value `Yap` was the Tauri app's too, so the shell writes it
only where `install::may_write_run_value` allows (none there, its own, a
missing exe, or the released install taking it over) and deletes it only
when it starts this very exe (`may_delete_run_value`); a dev build never
takes or removes another app's autostart. A key written by another copy (a
dev build, the user's own profile) is never removed: a profile's AUMID key
carries no owner, so uninstall leaves `com.115jon.yap.<name>` alone.

Hook decisions are appended to `%LOCALAPPDATA%\Yap.Data\logs\installer.log`
(kept on uninstall). Velopack logs Setup to `%LOCALAPPDATA%\velopack\velopack.log`
(or `Setup.exe --log <file>`) and `Update.exe` (updates, uninstall) to
`%LOCALAPPDATA%\velopack\velopack_<packId>.log`.

## Install, upgrade, uninstall

- **Install**: `Setup.exe` extracts `Update.exe`, the stub and `current\`,
  creates the shortcuts, runs the install hook, stops anything running from
  the root, writes the uninstall entry. Interactive Setup then starts the
  app; `--silent` does not. `Setup.exe -- --no-launch` (arguments after
  `--` go to that first run) makes the first run exit at once, for an
  interactive install that should not start the app.
- **Upgrade with a newer Setup.exe**: Setup kills every process running
  from the root (`Killing process: ...\current\Yap.exe`), replaces
  `current\`, updates the shortcuts and the uninstall entry, and runs the
  install hook again (idempotent).
- **Update** (`Update.exe apply --package <full.nupkg>`, what the in-app
  updater calls, see "Updates"): the same kill, a backup and swap of
  `current\`, the after-update hook, then a restart with the arguments
  after `--`.
- **Uninstall** (`Update.exe --uninstall [--silent]`): the before-uninstall
  hook, then Velopack removes the shortcuts, the uninstall entry,
  `%TEMP%\velopack_<packId>` and, about 3 s after it exits, the root.

## Updates

The app updates itself from a Velopack feed: `releases.<channel>.json` and
the matching `.nupkg` packages. Released builds read them from this
repository's GitHub Releases (one release per version, tag
`desktop-native-v<version>`); a test feed is a static folder. The rules are `yap_core::update` (tested in
`crates/yap-core/tests/update.rs`); `rust/src/updater.rs` runs them with
Velopack's `UpdateManager` on a thread of its own (`yap-updater`), and QML
shows them through the `Updater` singleton.

### The flow

1. **Check.** 20 s after the core starts, then every 6 h while the app
   runs, and on "Check for updates". The feed request gives up after 30 s.
   Nothing on the start path waits for it.
2. **Download.** A newer release downloads in the background into
   `<root>\packages`: a delta when the feed has one from the installed
   version (Velopack rebuilds the full package with `Update.exe patch`),
   else the full package. Velopack checks its size and SHA-256 against the
   feed. Settings shows `Downloading version 0.2.23… 42%`.
3. **Ready.** Settings > Desktop app says the version is ready, with
   "Restart now"; the title bar shows "Restart to update"
   (`qml/chrome/UpdatePill.qml`) before the window keys.
4. **Apply.** "Restart now" starts
   `Update.exe apply --package <full.nupkg> --waitPid <pid>` and quits the
   app the usual way (`DesktopShell.quit()`: the call is left, the cache
   committed). Update.exe waits for the process to end, swaps `current\`
   (the after-update hook runs, see "App integration") and starts the new
   version. With **install on quit** (on by default), a plain quit does the
   same without the restart (`--silent --norestart`, from `main.cpp` after
   the event loop), except while Windows is shutting down. If a quit could
   not install it, Velopack installs a downloaded package on the next start
   (`set_auto_apply_on_startup`, only with install on quit on and no pin).
   With install on quit off, only "Restart now" installs; the next start
   shows it as ready again without downloading.

**During a call** the restart waits: the title bar key stays hidden,
"Restart now" is disabled, and Settings says the update installs once you
leave the call (`Updater::can_restart`, the voice view's channel). A quit
during a call still installs, since quitting ends the call.

**Errors** (no network, a missing feed, a bad checksum) are logged as
`[updater] failed: <Velopack's error>` in `logs\yap-native.log` and shown
under the version in Settings as one sentence ("Couldn't update. Yap
couldn't reach the update server. Yap tries again on its own.",
`update::friendly_error`) with "Try again". Retries back off on their own: 1, 2,
4 ... minutes, at most an hour (never longer than the check interval); a
success resets them. An update that is already downloaded stays ready when
a later check fails. A failed check never shows anything in the shell.

### What updates

Only an installed copy, on the default profile. These do nothing but say
why in Settings (`update::eligibility`, the same layout rule as the hooks):

| copy                                                                               | why                                                        |
| ---------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| a dev build, a copy of the exe (no `<root>\current\Yap.exe` + `<root>\Update.exe`) | nothing to update                                          |
| the portable zip (`<root>\.portable`)                                              | not an install                                             |
| a named profile (`--profile`, `YAP_PROFILE`)                                       | profiles share one install; the default profile updates it |
| anything under `F:\ct\yapn` (agents' builds and runs)                              | would replace a build under test                           |

`YAP_UPDATE_TEST=1` lifts the last two for an end-to-end run in a scratch
root; it never makes a dev build or a portable copy update.

### Settings

`Settings > Desktop app > Updates`: the installed version (Velopack's
`sq.version`), the status, "Check for updates" (or "Try again", or
"Restart now"), "Release notes" (the ready update's notes, else this
version's, from `vpk pack --releaseNotes`), the download bar, and two
switches, kept per profile in `Yap.Data\updates.json`:

```json
{
  "channel": "stable",
  "installOnQuit": true,
  "pinnedVersion": null,
  "feedUrl": null
}
```

- **Beta updates** (`channel: "beta"`): reads `releases.beta.json` and
  `releases.win.json` and takes the newest of both, so a beta user moves on
  to a newer stable release. Switching back to stable never downgrades: a
  beta user stays on their version until stable passes it.
- **Pinned version** (`pinnedVersion`, set by hand, no switch): releases
  past it are ignored, deltas included, and a downloaded package past it is
  not offered. Settings says "Updates stop at version 0.2.22."
- **Feed** (`feedUrl`): replaces the compiled feed, for testing.

### The feed

| setting                  | where                                                                                              |
| ------------------------ | -------------------------------------------------------------------------------------------------- |
| compiled default         | `yap_core::update::DEFAULT_FEED_URL`: `https://github.com/115jon/yap`, its native releases         |
| `updates.json` `feedUrl` | overrides the default                                                                              |
| `YAP_UPDATE_URL`         | overrides both: `https://github.com/<owner>/<repo>`, `https://...`, `file:///F:/feed` or `F:\feed` |

A URL that is exactly `https://github.com/<owner>/<repo>` is read as a
repository's releases (`Feed::Github`, below); any other `https://` URL is
a static folder (`Feed::Http`), and so is one release's download folder,
`https://github.com/115jon/yap/releases/download/desktop-native-v0.2.24`,
which pins a client to that release's feed.

**GitHub Releases** (`src/updater.rs`, `GithubReleases`; the rules are
`yap_core::update`, "GitHub Releases"). Each check makes one
unauthenticated API call, `GET /repos/115jon/yap/releases?per_page=100`
(up to three pages while no native release has turned up), within the API's
60 requests an hour per IP. From the list it keeps the **native releases**:
not drafts, tag `desktop-native-v` plus a semver, and on stable neither a
GitHub pre-release nor a pre-release version; beta keeps them all. The
web app's `v3.x`, `mobile-v*`, `cef-build-assets` and the
`native-toolchain-*` release never match. It then reads
`releases.<channel>.json` from the newest native release that has one, and
downloads each package from the newest native release that has a file of
that name, both from the public download URLs (GitHub's CDN, no API call).
A channel with no native release yet is an empty feed ("up to date"), not
an error. Velopack's own `GithubSource` is not used: it reads the newest
ten releases whatever their tag, and the web releases would hide the
native ones.

Every release's `releases.<channel>.json` lists the earlier versions too
(CI downloads the previous one before `vpk pack`, which merges it), so the
newest file is the whole channel. A static folder looks the same, all in
one place: Velopack asks for
`<base>/releases.<channel>.json?localVersion=...&id=...&stagingId=...` and
then `<base>/<FileName>`.

What a native release holds:

```
desktop-native-v0.2.24/
  releases.win.json            # the channel's feed (uploaded last)
  Yap.App-0.2.24-full.nupkg
  Yap.App-0.2.24-delta.nupkg   # from the previous release's full package
  Yap.App-win-Setup.exe        # the installer
  Yap.App-win-Portable.zip
```

A beta (`desktop-native-v0.2.25-beta.1`, a GitHub pre-release) holds
`releases.beta.json` and `Yap.App-beta-Setup.exe` instead.

A `releases.win.json` entry, as vpk 1.3.39 writes it:

```json
{
  "Assets": [
    {
      "PackageId": "Yap.App",
      "Version": "0.2.23",
      "Type": "Full",
      "FileName": "Yap.App-0.2.23-full.nupkg",
      "SHA1": "...",
      "SHA256": "...",
      "Size": 37401870,
      "NotesMarkdown": "...",
      "NotesHtml": "..."
    }
  ]
}
```

### Hosting the feed

GitHub Releases of `115jon/yap`, chosen over an R2 bucket (2026-11-09):
nothing new to run or pay for, CI needs only `GITHUB_TOKEN`, and every
version gets a release page. What that costs, and how it is handled:

- **The Tauri app's updater** reads `releases/latest/download/latest.json`.
  Until the cutover ("Replacing the Tauri app") a native release must
  never be "latest": the workflow creates it as a draft with
  `--latest=false`, publishes it with `make_latest=false`, then checks that
  `releases/latest` did not move (and restores it and fails if it did).
  If the current latest web release is ever deleted, GitHub picks a new
  latest by date and could pick a native one: mark a web release latest
  again by hand.
- **The API rate limit**: 60 unauthenticated requests an hour per IP. One
  call per check, a check every 6 h, retries backing off to an hour; a
  rate-limited check reads "Couldn't reach the update server" and retries.
- **Mixed tags**: handled by the tag prefix (above).
- **Pulling a bad release**: delete its release, or edit its assets. The
  previous release's feed file still lists the versions before it, so
  clients fall back to that one. Clients that already downloaded the bad
  version still install it; ship a fixed version where that matters.

To move the feed later, ship one release whose `DEFAULT_FEED_URL` points at
the new place and publish that release to both.

### Publishing a release

By CI: `.github/workflows/desktop-native-release.yml` ("CI" below) does all
of it. Stable:

1. Bump `[workspace.package] version` in `rust/Cargo.toml` and merge it to
   `main`.
2. Actions > Desktop Native Release > Run workflow, **from `main`**:
   `version` empty (or the same `0.2.24`), `channel` `win`, `notes` the
   release notes (Markdown, shown in Settings > Desktop app).
3. The run tests, builds, signs, packs (with a delta against the previous
   native release) and publishes `desktop-native-v0.2.24`, never "latest".
4. Check: the release lists `releases.win.json`, and an installed copy
   finds the version ("Check for updates").

A beta is the same with `channel` `beta` and a pre-release version
(`0.2.25-beta.1`, whose `0.2.25` must be Cargo.toml's): a GitHub
pre-release with `releases.beta.json`. Beta users read the stable feed too,
so nothing is copied between channels.

By hand (a release CI cannot make): download the previous release's
`releases.<channel>.json` and full `.nupkg` into one folder, run
`./tools/package.ps1 -Sign -OutDir <folder> -Version <v> -Channel <c> -ReleaseNotes notes.md`
there (`-Sign` needs the Signer role, see "Signing"), then
`gh release create desktop-native-v<v> --draft --latest=false`, upload the
new packages, Setup.exe and the Portable zip, then the feed file, and
publish with `gh api -X PATCH repos/115jon/yap/releases/<id> -F draft=false -f make_latest=false`.

### Test hooks and a local feed

| variable              | does                                                                                   |
| --------------------- | -------------------------------------------------------------------------------------- |
| `YAP_UPDATE_URL`      | the feed: a URL, `file:///F:/ct/yapn/run/updater/feed` or a folder path                |
| `YAP_UPDATE_INTERVAL` | seconds between checks (instead of 6 h); retries and the first check never wait longer |
| `YAP_UPDATE_DELAY`    | seconds before the first check (instead of 20 s)                                       |
| `YAP_UPDATE_TEST`     | `1`: a Velopack layout under `F:\ct\yapn` or on a named profile updates (never a copy) |

A local feed with two versions, without installing anything:

```powershell
$feed = "F:/ct/yapn/run/updater/feed"; $out = "F:/ct/yapn/run/updater/pack"
./tools/package.ps1 -SkipBuild -BuildDir F:/ct/yapn/rel-updater -OutDir $out `
  -PackId Yap.UpdateTest -Title "Yap UpdateTest" -Version 0.2.22
./tools/package.ps1 -SkipBuild -BuildDir F:/ct/yapn/rel-updater -OutDir $out `
  -PackId Yap.UpdateTest -Title "Yap UpdateTest" -Version 0.2.23 -ReleaseNotes notes.md
vpk upload local --outputDir $out --path $feed --channel win
```

An installed layout without Setup: the portable zip is the install layout
plus a `.portable` marker. Extract the old version's
`Yap.UpdateTest-win-Portable.zip` into a scratch root, delete `.portable`,
copy its full package into `<root>\packages\` (Setup keeps one there, the
delta base), and start `<root>\current\Yap.exe --profile spike` with
`YAP_UPDATE_TEST=1`, `YAP_UPDATE_URL=<feed>` and `LOCALAPPDATA` / `APPDATA`
pointed at scratch folders. "Restart now" or a quit with install on quit
would run `Update.exe apply` on that root; its after-update hook and
Velopack's apply write per-user registry values (the uninstall entry,
shortcuts), so only do that step on a test machine.

## Replacing the Tauri app

The Tauri app (`desktop/installer`, `desktop/src-tauri`) installs per user to
`%LOCALAPPDATA%\Yap` (`Yap.exe` stub, `Update.exe` = its installer,
`current.json`, `app-<version>\`), writes `Uninstall\Yap`
(`QuietUninstallString` `"<root>\Update.exe" --uninstall /S`),
`HKCU\Software\115jon\Yap`, `yap://` (to its stub), the Run value `Yap` and
`Desktop\Yap.lnk` / `Programs\Yap.lnk` with the AUMID `com.115jon.yap`. Its data
is `%APPDATA%\Yap`: `desktop-settings.json`, `.window-state.json` and the CEF
profile, whose localStorage (Chromium LevelDB) holds the web UI's theme and
sound settings. Its uninstaller kills its processes (by path in its root, and
by name: `RalphMeet`, `inject-helper*`, `get-graphics-offsets*`), deletes the
two `Yap.lnk` by name, its uninstall and publisher keys, `yap://` and the Run
value only if they point into its root, and schedules its root's deletion. It
keeps `%APPDATA%\Yap`.

On the released install's default-profile launches (`yap_replace_tauri_at_launch`,
`rust/src/installer.rs`, rules in `yap_core::tauri`), right after the log
starts and before anything reads the settings:

1. **Import, once** (`tauri::import`, marker `Yap.Data\tauri-import.json`).
   Never over a file this app already has, so an existing native user keeps
   their settings. `desktop-settings.json` gives open on startup, start
   minimized, close to tray, notifications and the game capture hook; the
   localStorage gives the sound switches and volumes
   (`sound-settings-storage`, the signed-in user's), the two voice confirm
   choices (`rm-*-skip-confirm`) and the theme (next-themes `theme`, one of
   `APP_THEMES`); `.window-state.json`'s main window becomes `window-state.json`,
   from physical to device-independent pixels at the DPI of the monitor it
   is on (`MonitorFromRect`, `GetDpiForMonitor`; skipped when it is on none). The LevelDB
   folder (`%APPDATA%\Yap\cef\Default\Local Storage\leveldb` and the other
   candidates in `tauri::local_storage_dirs`, WebView2's
   `%LOCALAPPDATA%\com.115jon.yap\EBWebView` last) is read into memory
   (`rusty-leveldb`'s MemEnv, the LOCK file skipped), so a running Tauri app
   does not matter and nothing on disk is written. If it cannot be read, the
   plain files still import: the first two launches retry it
   (`tauri-import.json` says `"done": false` with a count), the third imports
   without it. The session is not imported: the app signs in again. Emoji recents and other web-only state are not imported.
2. **Detect** (`tauri::detect`): `%LOCALAPPDATA%\Yap` with `Update.exe` and
   `current.json` or an `app-*` folder; never a Velopack layout (a `current\`
   folder), never when `Uninstall\Yap` names another folder. Nothing found:
   leftovers are cleared (step 6) and that is all.
3. **Bounded retries**: `Yap.Data\tauri-replace.json` counts attempts; after
   3 the app stops trying and logs it.
4. **Keep our shortcuts** (`tauri::save_shortcuts`): the bytes of each
   `Yap.lnk` on the desktop and in Programs that names our install root,
   because the Tauri uninstaller deletes both by name. They are written to
   `Yap.Data\tauri-shortcuts\` before the uninstaller starts, so a launch
   after one that ended mid-uninstall puts back any that are gone; the
   folder goes once the Tauri app is gone.
5. **Uninstall, the supported way**, deferred until this launch has
   stayed up: the launch only arms it; once Main.qml has its window (shown,
   or in the tray) the shell calls `yap_tauri_app_started`, and 20 s later
   (`tauri::UNINSTALL_DEFER`, `tauri::UninstallDeferral`) a background
   thread counts the attempt and runs `"<root>\Update.exe" --uninstall /S`
   without a window, up to 120 s. A build that crashes on start leaves the
   Tauri app working.
6. **Restore and clean up**: put back any saved shortcut that is gone; delete
   the Run value `Yap` and `Software\Classes\yap` only if they still point
   into `%LOCALAPPDATA%\Yap` (`tauri::remove_leftovers`); register `yap://`
   and `yap-native://` to this exe again; log whether `Uninstall\Yap` is gone.

Everything is logged to `Yap.Data\logs\installer.log` with a `[tauri]`
prefix. Only the Tauri uninstaller deletes anything of the Tauri app, and only
inside its root; `%APPDATA%\Yap` is read, never changed. Tests:
`crates/yap-core/tests/tauri_replace.rs` (fixture folders, an in-memory
registry, a LevelDB built in memory).

### How the Tauri app gets here (3.14, the bridge)

The last Tauri release installs this app itself
(`desktop/src-tauri/src/native_handoff`). It stays dormant until all of
these hold:

- **The remote switch** (`GET /api/desktop/native-handoff`, the admin
  console's Switches) is on, and the install's bucket is below its rollout
  percent. The switch is off at 0 % by default, and an unreachable endpoint
  means no handoff.
- **A native release exists**: among the 30 newest releases
  (`GET /repos/115jon/yap/releases?per_page=30`, not `releases/latest`),
  the highest one tagged exactly `vX.Y.Z` with X >= 4, neither a draft nor
  a prerelease, with an asset named `Yap.App-win-Setup.exe` that has a
  `sha256:` digest and a size of 10–500 MB, downloading from this
  repository. The `desktop-native-v*` tags never match, so the cutover
  publishes `v4.x.y` releases. Which release is "latest" does not matter
  to the bridge: the Tauri updater keeps reading
  `releases/latest/download/latest.json`, so a 3.14.x can still ship, and
  the cutover carries 3.14's `latest.json` onto each v4 release (planned
  separately) for when one is latest. GitHub is asked at most once per
  6 h after a "not yet" and once an hour after an error.
- **Windows** is build 17763 or later, the floor of Qt 6.11.

The Setup is downloaded to `%TEMP%\yap-native-handoff`, checked against
the digest, moved alone into a fresh `attempt-<guid>` folder there (no
junctions, nothing beside it), held open with read sharing only, and
checked with `WinVerifyTrust`, revocation checked online for the whole
chain (an unanswered check is a failure, never a pass): subject CN
`Jonathan Martinez`, an issuer CN starting with `Microsoft ID Verified CS`,
a timestamp, and the EKUs code signing (`1.3.6.1.5.5.7.3.3`) and this
account's Artifact Signing identity validation
(`1.3.6.1.4.1.311.97.726408799.228623495.642435689.37395988`, on every
certificate under that validation; a new identity validation needs a new
bridge). It runs as `--silent --log %TEMP%\yap-native-handoff\setup.log`
(silent never starts the app). `current\Yap.exe` gets the same signature
check, and the Tauri app starts that file (held open until it runs), not
the root stub. It counts the handoff done only if `current\Yap.exe` still
runs 8 s later, then exits; the deferred uninstall above starts later
still. Later Tauri launches start the native app instead (after two
failed redirects in a row they start Tauri again). State, attempts and
opt-out are in `%APPDATA%\Yap\native-handoff.json` (an unreadable file
keeps the bridge dormant and is never rewritten); the log is
`%TEMP%\yap-native-handoff\handoff.log`.

## Rename from Yap Native

| identifier       | before (0.2.21)                                 | now                                                   |
| ---------------- | ----------------------------------------------- | ----------------------------------------------------- |
| packId           | `YapNative.App`                                 | `Yap.App`                                             |
| install root     | `%LOCALAPPDATA%\YapNative.App`                  | `%LOCALAPPDATA%\Yap.App`                              |
| exe              | `current\YapNative.exe`                         | `current\Yap.exe`                                     |
| title, shortcuts | "Yap Native", `Yap Native.lnk`                  | "Yap", `Yap.lnk`                                      |
| uninstall key    | `Uninstall\YapNative.App`                       | `Uninstall\Yap.App`                                   |
| AUMID            | `com.115jon.yap.native[.<name>]`                | `com.115jon.yap[.<name>]`                             |
| schemes          | `yap-native://`                                 | `yap://` + `yap-native://` alias                      |
| Run value        | `YapNative[-<name>]`                            | `Yap[-<name>]` (old names still removed on uninstall) |
| data root        | `%LOCALAPPDATA%\YapNative`                      | `%LOCALAPPDATA%\Yap.Data` (moved once, below)         |
| Qt cache folder  | `%LOCALAPPDATA%\YapNative[-<name>]`             | `%LOCALAPPDATA%\Yap.Data[-<name>]`                    |
| device label     | "Yap Native"                                    | "Yap" (the web takes both, `native-app-link.ts`)      |
| crash dumps      | `yap-native-<secs>-<pid>.dmp`                   | `yap-<secs>-<pid>.dmp`                                |
| instance lock    | `Local\com.115jon.yap.native.instance[.<name>]` | `Local\com.115jon.yap.app.instance[.<name>]`          |
| instance pipe    | `com.115jon.yap.native[.<name>].<session>`      | `com.115jon.yap.app[.<name>].<session>`               |
| tray class       | `YapNativeTrayWindow[-<name>]`                  | `YapTrayWindow[-<name>]`                              |

A `YapNative.App` install is a different Velopack app: installing `Yap.App`
does not remove it. Its own lock and pipe names keep the two from taking
each other's launches, and the released app removes the old install's
`YapNative[-<profile>]` Run values (those pointing into
`%LOCALAPPDATA%\YapNative.App`) on every default-profile start, so it no
longer starts at sign-in. Its AUMID key and its own start menu entry stay
until it is uninstalled; while both run, they take turns rewriting
`yap-native://` at their default-profile starts. While the old build holds
files in `%LOCALAPPDATA%\YapNative`, the data folder cannot move and the new
one reads it in place, beside the old build: close the old build first.

**The data folder moves once** (`yap_cache::profile::migrate`, tested in
`crates/yap-cache/tests/profile_migration.rs`). The first start that resolves
the data root (`profile::data_root`, also what the C++ shell asks through
`yap_profile_data_root`) renames `%LOCALAPPDATA%\YapNative` to
`%LOCALAPPDATA%\Yap.Data` in one move, every named profile (`spike`,
`spikepeer`, `jon`, ...) with it. If `Yap.Data` exists already (the install
hook logged into it), the old folder is renamed to `YapNative.migrating` and
its entries move in one by one; a file both have stays as
`<stem>.legacy.<ext>` beside the new one. Every step is a rename on one
volume: nothing is copied or deleted, and an interrupted move resumes from
`YapNative.migrating` (the marker below is written only once nothing is
left there). When the old folder cannot move (an older build has
files open in it), nothing moves and the run reads the old folder in place;
the next start tries again. That fallback is for one version. Once moved,
`Yap.Data\.migrated-from-yapnative` stops it for good: a `YapNative` folder an
older build makes again is that build's. Each move is logged to
`logs\migration.log`.

## Parity with the Tauri installer

| old installer (`desktop/installer`)                                             | native installer                                                                                                                                                                                                                                 |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| per-user, no elevation, under `%LOCALAPPDATA%`                                  | same: Velopack per-user install to `%LOCALAPPDATA%\Yap.App`                                                                                                                                                                                      |
| Desktop and Start menu shortcuts with the AUMID                                 | same names and AUMID: `--shortcuts Desktop,StartMenuRoot --aumid com.115jon.yap` (`Yap.lnk`)                                                                                                                                                     |
| HKCU uninstall entry: DisplayName, icon, version, publisher, NoModify, NoRepair | Velopack's `HKCU\...\Uninstall\Yap.App`: DisplayName "Yap", DisplayIcon (`current\Yap.exe`), DisplayVersion, Publisher "115jon", InstallLocation, UninstallString, QuietUninstallString, EstimatedSize, NoModify = 1, NoRepair = 1               |
| owns `yap://`, removed on uninstall only if it points at the install            | same rule for `yap://` and `yap-native://` (before-uninstall hook)                                                                                                                                                                               |
| Run value removed on uninstall if ours                                          | same, for `Yap`, `Yap-<profile>` and the old `YapNative[-<profile>]` that point into the root                                                                                                                                                    |
| closes running instances on upgrade                                             | Velopack stops every process running from the install root before it swaps `current\` (forced, like the old `Process.Kill`)                                                                                                                      |
| user data kept on uninstall                                                     | same: the data root is a different folder; only the default toast icon goes                                                                                                                                                                      |
| `/S`                                                                            | `Setup.exe --silent` (never starts the app)                                                                                                                                                                                                      |
| `--uninstall /S`                                                                | `Update.exe --uninstall --silent` (the entry's QuietUninstallString)                                                                                                                                                                             |
| `/NoLaunch`                                                                     | `Setup.exe -- --no-launch` for an interactive install; silent installs never launch                                                                                                                                                              |
| install log                                                                     | Velopack's log plus `installer.log` for the hooks                                                                                                                                                                                                |
| Authenticode via Artifact Signing                                               | same account and profile, through `vpk --azureTrustedSignFile` (signs the exe, the stub, `Update.exe` and `Setup.exe`)                                                                                                                           |
| no telemetry                                                                    | none: Setup makes no network calls; the app reads the update feed (two static files and the package), sending only Velopack's query string (`localVersion`, `id`, a random `stagingId`); `vpk --skip-updates` keeps vpk from checking for itself |

Dropped on purpose:

- **Ralph Meet migration** (`LegacyCleanup`, `LauncherMigration`): the native
  client never shipped as Ralph Meet, and the Tauri app's uninstaller, which
  this app runs, still removes Ralph Meet leftovers.
- **The root launcher stub** (`desktop/launcher`, `RootLauncher`): Velopack
  brings its own stub at the root, and the real exe path
  (`current\Yap.exe`) is stable across updates.
- **RMPK payload, LZMA blocks, lock diagnostics, WPF window**: Velopack's
  package format and its own UI: a splash drawn after the WPF window (see
  "Splash").
- **Vulkan layer cleanup**: the native client's game capture hook registers
  no Vulkan layer (see "Game capture hook payload"), so there is nothing to
  remove.
- **Tauri updater (`latest.json`, minisign)**: replaced by Velopack's feed
  and the in-app updater ("Updates"). Packages are checked by SHA-256 from
  the feed; there is no separate signature over the feed.

## Building

Needs what `build.ps1` needs (MSVC, CMake, Ninja, Rust, static Qt 6.6.3,
libwebrtc) plus `vpk`, a .NET global tool used only at build time, and
Python with Pillow for the splash (see "Splash"):

```powershell
dotnet tool install -g vpk --version 1.3.39-g8d2844f   # the script does this if vpk is missing
./tools/package.ps1                       # build/Release -> build/installer
./tools/package.ps1 -SkipBuild -BuildDir F:/ct/yapn/inst-build -OutDir F:/ct/yapn/inst-out
```

`-PackId`, `-Version`, `-Title` and `-OutDir` override the defaults. The
script stages only `Yap.exe`, the game capture hook's files and the
segmentation runtime, so nothing else from the build dir ships.
`-Channel beta` packs into the beta channel, `-ReleaseNotes <file.md>`
adds notes (see "Updates"). Output: `<packId>-<channel>-Setup.exe`,
`<packId>-<channel>-Portable.zip`, `<packId>-<version>-full.nupkg` (and a
`-delta.nupkg` when the folder holds the previous release),
`releases.<channel>.json`, `assets.<channel>.json`, `RELEASES`.

## Splash

Velopack's Setup shows one image while it installs: a borderless, topmost
layered window with no text of its own. It treats the image as 96 DPI and
scales it by the monitor's DPI with Lanczos (at most 70 % of the monitor),
and paints its progress bar as a solid strip along the image's bottom edge,
12 px x DPI tall and the full width, in `--splashProgressColor`.

`tools/package.ps1` renders that image at pack time with
`tools/build-splash.py` (Python with Pillow) and passes
`--splashProgressColor "#ff8ad8"` (the accent; vpk 1.3.39 takes `#RRGGBB`
or `None`). The card is the Tauri installer's window (`desktop/installer`:
`MainWindow.xaml`, `Ui/Styles.xaml`, `Ui/Theme.cs`, `Ui/Dither.cs`) in its
dark palette, on 440 x 300:

| part         | old window                                        | splash                                                                                  |
| ------------ | ------------------------------------------------- | --------------------------------------------------------------------------------------- |
| ground, edge | Ply-0 `#000000`, 1 px line-strong (ink-1 14 %)    | same                                                                                    |
| title bar    | "YAP SETUP", Martian Silk 11 px, ink-3; keys      | "YAP SETUP", Martian Silk 12 px, 0.08 em; the version where the keys were (Martian Num) |
| mark         | Yappy 96 px, accent `#ff8ad8`, 32-cell Bayer ramp | same cells (3 px each), from `assets/svg/splash-logo.svg`, jaw open                     |
| kicker       | talk LED (`#3ddc84`, 22 % halo) and status word   | LED and "INSTALLING"                                                                    |
| title        | "Installing Yap", Bricolage Title 800, 28 px      | "Installing Yap", 30 px                                                                 |
| status       | Geist 15 px, ink-2, live text                     | "Setting it up for this Windows account.", Geist Medium 15 px                           |
| meter        | 4 px tint-2 track, accent fill                    | 12 px tint-2 track at the bottom edge; Velopack's accent bar fills it                   |

Everything is drawn at 4x and box-filtered down, so shapes stay on whole
pixels and the dither cells keep hard edges when Velopack upscales. Type is
a step larger and heavier than the WPF window's for the same reason. Without
Python or Pillow the script warns and packs the old `assets/app/yap-256.png`
(the bar is still the accent). To look at it:

```powershell
python tools/build-splash.py --version 0.2.12 --out F:/ct/yapn/tmp/splash.png
```

## Game capture hook payload

The hook (`rust/crates/yap-capture/src/hook`) injects OBS Studio's own
`graphics-hook{64,32}.dll` through OBS's `inject-helper{64,32}.exe` and
`get-graphics-offsets{64,32}.exe`, unmodified and signed by the OBS Project,
the same six files the Tauri app ships. They are pinned once, in
`desktop/packaging/obs-signed-hook.lock.json` (OBS 33.0.1, hook 1.8.8: the
release zip's SHA-256, each file's SHA-256, signer and thumbprint), and the
native app embeds that same lock.

- **Packaging.** `tools/package.ps1` runs the Tauri build's
  `desktop/scripts/fetch-obs-signed-hook.ps1` into
  `stage\obs-capture\obs-signed\`. The script downloads the pinned OBS
  release zip (cached in `-ObsCacheDir`, `%TEMP%
alph-obs-signed-hook` by
  default, or `-ObsZip`), checks the zip's SHA-256, extracts the six files
  and checks each against the lock: SHA-256, a valid Authenticode
  signature, the signer subject and certificate thumbprint, the file
  version. The package script then checks the staged hashes once more, as
  `desktop/scripts/stage-payload.ps1` does. Any mismatch stops the build.
  `-NoCaptureHook` leaves them out (shares then always use WGC).
- **Signing.** With `-Sign`, `vpk` gets `--signExclude obs-capture[\\/]`:
  OBS's files are never re-signed, since a changed byte voids the OBS
  Project signature that anti-cheat vendors allowlist.
- **At run time.** Before every injection the app opens each file without
  write or delete sharing, checks its SHA-256 against the lock and its
  Authenticode signer with `WinVerifyTrust`, and copies the set once to a
  content-addressed folder under the profile's data,
  `%LOCALAPPDATA%\Yap.Data[\profiles\<name>]\capture-hook\obs-33.0.1-c37f4412\`,
  so a DLL a game keeps loaded never sits in Velopack's `current` folder,
  which an update replaces. Uninstall leaves that folder with the rest of
  the data root.
- **No Vulkan layer.** The Tauri app registers an implicit Vulkan layer
  (`HKCU\SOFTWARE\Khronos\Vulkan\ImplicitLayers`) while the setting is
  on. The native client does not register one, from the app or the
  installer, so Vulkan games are hooked only when an OBS-signed hook is
  already resident in them; otherwise they use WGC.
- **Licence.** The files are GPL-2.0, run as separate processes and a
  separate DLL, never linked; the corresponding source is named in the
  lock's `sources`.

## Signing

`-Sign` writes `signing-metadata.json` (endpoint
`https://eus.codesigning.azure.net/`, account `jon115signing`, profile
`publictrust`; overridable through `ARTIFACT_SIGNING_*`) and passes it to
`vpk pack --azureTrustedSignFile`. vpk signs with `DefaultAzureCredential`,
so a local `az login` with the Artifact Signing Certificate Profile Signer
role works; CI logs in with OIDC first. See `docs/desktop/code-signing.md`
for the account. Signing happens inside `vpk pack`, before the package and
Setup.exe hash the files, so the signatures are part of what ships.

**Local signing is not authorised** (checked 2026-11-06): the maintainer's
`az login` has Owner and Artifact Signing Identity Verifier on
`jon115signing`, but not the Certificate Profile Signer data role, and
`vpk` stops with `Azure Trusted Signing denied the request (HTTP 403)`.
Owner does not include signing. Signed builds come from CI; to sign locally,
someone with Owner must first assign the Signer role to that user.

## CI

`.github/workflows/desktop-native-release.yml`, manual (`workflow_dispatch`)
only, one run at a time (concurrency group `desktop-native-release`).
Inputs: `version` (empty: Cargo.toml's), `channel` (`win` or `beta`),
`notes`, and `dry_run`.

| job       | runner        | token                                | does                                                                                                                                                                                                                                                                                                                                                                |
| --------- | ------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `plan`    | ubuntu-latest | `contents: read`                     | checks the version against Cargo.toml and the channel (beta takes `x.y.z-pre`), refuses a tag or release (drafts too) that exists, and a signed run off `main`                                                                                                                                                                                                      |
| `deny`    | ubuntu-latest | `contents: read`                     | `cargo deny check` with cargo-deny's release binary (`taiki-e/install-action`)                                                                                                                                                                                                                                                                                      |
| `test`    | windows-2022  | `contents: read`                     | `cargo test` (the workspace without `yap_native` and `yap-audio`), through sccache                                                                                                                                                                                                                                                                                  |
| `build`   | windows-2022  | `contents: read`                     | the toolchain (below), ONNX Runtime from nuget.org (hash-checked), `build.ps1` through sccache; uploads `Yap.exe` and the runtime, and the cargo `--timings` report and `.ninja_log`                                                                                                                                                                                |
| `release` | windows-2025  | `contents: write`, `id-token: write` | after the other four: the previous release of the channel (feed and full package, for the delta), `azure/login` (OIDC) and `azure/artifact-signing-action` on `Yap.exe`, `package.ps1 -Sign` (vpk signs Update.exe, the stub and Setup.exe), `verify-authenticode.ps1`, then the release: a draft, packages, then the feed file, published with `make_latest=false` |

The build job runs third-party build scripts, so it gets neither the write
token nor OIDC; signing and publishing happen in the release job only.

**Dry run** (`dry_run`): no Azure login or signing, so it works from any
branch, and the release stays a draft (a draft creates no tag). Delete the
draft afterwards (`gh release delete desktop-native-v<v> --yes`); the plan
job refuses to run while it exists.

**Signing** needs the run to come from `main`: the federated credentials on
`AZURE_CLIENT_ID` trust `refs/heads/main` and `v*` tags only (immutable
subject form, docs/desktop/code-signing.md). The job's OIDC subject is the
ref the workflow runs from, not the release tag, so a dispatch from `main`
that creates `desktop-native-v*` needs no new credential. A
`push: tags: desktop-native-v*` trigger would; it would need a flexible
credential matching
`repo:115jon@<owner id>/yap@<repo id>:ref:refs/tags/desktop-native-v*`.

**Speed.** `deny`, `test` and `build` run side by side; `build` is the long
pole. Measured on the 4-core `windows-2022` runner, 2026-11-10:

| run                                   | `build` job | whole run |
| ------------------------------------- | ----------- | --------- |
| before (one job, no compiler cache)   | 36 min      | 38 min    |
| sccache cold (nothing cached)         | 30 min      | 31 min    |
| sccache warm (the previous release's) | 15 min      | 16 min    |

What it does, warm:

- **sccache** (`mozilla-actions/sccache-action`, the Actions cache backend,
  `SCCACHE_GHA_ENABLED`) caches every rustc and C/C++ compile: the
  dependency crates, the C++ that build scripts compile with `cc`
  (cxx-qt-lib's Qt bindings, webrtc-sys, the bridge and QML code of
  `yap_native`), and CMake's `cpp/` (`CMAKE_CXX_COMPILER_LAUNCHER`). About
  90 % of the compiles hit (98 % of the C/C++). rustc goes through `tools/ci/sccache-wrapper.rs`
  (named `sccache.exe`, so `cc` still uses it): `yap_native`'s rustc
  command line is too long for sccache to start, and a staticlib is never
  cached anyway, so that call goes straight to rustc.
- **Not cacheable**, and now most of the run: `yap_native` itself (rustc and
  the fat LTO, about 3.6 min), its build script (moc, qmlcachegen, about
  1.3 min) and the LTCG link of `Yap.exe` over the static Qt (about 5.5
  min). These keep the shipped binary as it was: no profile was changed.
- **Overlap**: `vendor.ps1` and the wrapper build run while the toolchain
  downloads; qmlcachegen runs on every core, with one tool check for the
  module (`patches/qt-build-utils/0003`).
- **Tried, not kept**: a ReFS Dev Drive for `desktop-native/build`
  (`samypr100/setup-dev-drive`; the same cargo and link times, Defender's
  real-time scan is already off on the runner), and
  `[profile.release.build-override] opt-level = 3` (faster build scripts,
  but a different `Yap.exe`: 5.6 KB larger).

**Cache budget.** The repository has 10 GB of Actions cache, shared with
the web app's workflows (LRU, and anything unread for 7 days goes).
sccache stores one entry per compiled unit: one cold run writes about
1,400 entries, 0.8 GB (the release build and the tests' debug build). Each
change to a dependency, the toolchain or the Rust version adds entries
beside the old ones until they age out. Entries written on `main`
are read by every branch; a branch's entries only by that branch, so only
the release runs from `main` keep a warm cache. A release more than a few
days after the last one may find it evicted and build cold. Nothing else
is cached: the toolchain archives (above), the crates (a few seconds from
crates.io) and ONNX Runtime (2 s from nuget.org) download each run.

**Toolchain.** No action provides a static, patched Qt, so it comes from
the `native-toolchain-qt6.11.3-yap-webrtc-a545de4` release (a pre-release,
never latest): the Qt build (`patches/qt/build-qt.ps1`, MSVC 14.44, LTCG)
pruned by `tools/toolchain-qt.exclude`, and LiveKit's libwebrtc
`webrtc-a545de4` as `webrtc-sys` downloads it. `tools/package-toolchain.ps1`
makes the archives (7-Zip, volumes under 2 GB) and
`tools/toolchain.lock.json` (every asset's size and SHA-256);
`tools/fetch-toolchain.ps1` downloads them, checks them against the lock,
unpacks them and sets `YAP_QT_DIR`, `QMAKE`, `YAP_LIBWEBRTC_DIR` and
`LK_CUSTOM_WEBRTC`. Both archives download in one `gh` call and unpack side
by side (about 30 s). They are not cached: the download takes about 15 s,
no faster from the Actions cache, and the cache budget goes to sccache.
The Qt archive adds `bin/qt.conf` (`Prefix = ..`): a static Qt is not
relocatable, and cxx-qt asks qmake for its paths.

Qt's libraries are link-time code, which links only with the MSVC that
compiled them: the build job runs on `windows-2022` (Visual Studio 2022,
MSVC 14.44; `windows-2025` is now the Visual Studio 2026 image) and warns
when the runner's MSVC differs from the lock's `msvc`. When `windows-2022`
is retired, either install the 14.44 toolset on the newer image and pass
`-vcvars_ver=14.44` to `Enter-VsDevShell`, or rebuild Qt with the newer
MSVC and make a new toolchain release. A new Qt build means a new toolchain release (a new tag),
`package-toolchain.ps1 -Upload`, and the new lock file committed.

## Local test

Use a test pack id and title so nothing collides with a real install, and
never start the default profile (README, "Testing notes"). Never run a
released-identity (`Yap.App`) Setup on a PC whose Tauri app or data matter:
its first start uninstalls the Tauri app.

```powershell
./tools/package.ps1 -SkipBuild -BuildDir F:/ct/yapn/inst-build -OutDir F:/ct/yapn/inst-out `
  -PackId Yap.InstallTest -Title "Yap InstallTest" -Version 0.0.1
$env:YAP_PROFILE = "spike"   # in case anything starts the app
F:/ct/yapn/inst-out/Yap.InstallTest-win-Setup.exe --silent -- --no-launch
```

A test pack id is never the released install, so it never replaces the
Tauri app, never takes a live `yap://` and never writes the `Yap` Run value
over another app's. The replacement and the migration are tested on fixture
folders and an in-memory registry (`cargo test -p yap-core --test
tauri_replace`, `cargo test -p yap-cache --test profile_migration`).

The run below is from 2026-11-06, under the old identity (`YapNative.InstallTest`,
unsigned, the same exe packed as 0.0.1, 0.0.2, 0.0.3):

| step                                  | result                                                                                                                                                                             |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| silent install 0.0.1                  | exit 0; `Update.exe`, stub, `current\YapNative.exe`, `packages\*.nupkg`; uninstall entry as in the parity table; both shortcuts, AUMID `com.115jon.yap.native`; install hook 34 ms |
| hooks with a live dev build present   | scheme (owned by `F:\ct\yapn\rel\YapNative.exe`) and the existing AUMID key left alone, on install and uninstall                                                                   |
| start `--profile spike` + `YAP_TOKEN` | first frame 865 ms, messages 1.6 s, gateway ready                                                                                                                                  |
| Setup 0.0.2 over it, app running      | Setup killed the running PID, installed 0.0.2 (`sq.version`, DisplayVersion), did not relaunch (silent)                                                                            |
| `Update.exe apply` 0.0.3, app running | killed the PID, swapped `current\`, after-update hook, restarted with `--profile spike`                                                                                            |
| `--no-launch` on a first run          | exit 0 in 0.4 s, no window                                                                                                                                                         |
| silent uninstall                      | exit 0; root, shortcuts, uninstall entry gone; `YapNative-spike` Run value into the root removed; Tauri `Yap` Run value, scheme, AUMID, data root unchanged                        |
