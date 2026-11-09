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
| Velopack       | crate `velopack = "=1.3.39-g8d2844f"`; `vpk` of the same version (the script checks)                  |

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
- **Update** (`Update.exe apply --package <full.nupkg>`, what an in-app
  updater will call): the same kill, a backup and swap of `current\`, the
  after-update hook, then a restart with the arguments after `--`.
- **Uninstall** (`Update.exe --uninstall [--silent]`): the before-uninstall
  hook, then Velopack removes the shortcuts, the uninstall entry,
  `%TEMP%\velopack_<packId>` and, about 3 s after it exits, the root.

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
5. **Uninstall, the supported way**, on a background thread:
   `"<root>\Update.exe" --uninstall /S` without a window, up to 120 s.
6. **Restore and clean up**: put back any saved shortcut that is gone; delete
   the Run value `Yap` and `Software\Classes\yap` only if they still point
   into `%LOCALAPPDATA%\Yap` (`tauri::remove_leftovers`); register `yap://`
   and `yap-native://` to this exe again; log whether `Uninstall\Yap` is gone.

Everything is logged to `Yap.Data\logs\installer.log` with a `[tauri]`
prefix. Only the Tauri uninstaller deletes anything of the Tauri app, and only
inside its root; `%APPDATA%\Yap` is read, never changed. Tests:
`crates/yap-core/tests/tauri_replace.rs` (fixture folders, an in-memory
registry, a LevelDB built in memory).

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

| old installer (`desktop/installer`)                                             | native installer                                                                                                                                                                                                                   |
| ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| per-user, no elevation, under `%LOCALAPPDATA%`                                  | same: Velopack per-user install to `%LOCALAPPDATA%\Yap.App`                                                                                                                                                                        |
| Desktop and Start menu shortcuts with the AUMID                                 | same names and AUMID: `--shortcuts Desktop,StartMenuRoot --aumid com.115jon.yap` (`Yap.lnk`)                                                                                                                                       |
| HKCU uninstall entry: DisplayName, icon, version, publisher, NoModify, NoRepair | Velopack's `HKCU\...\Uninstall\Yap.App`: DisplayName "Yap", DisplayIcon (`current\Yap.exe`), DisplayVersion, Publisher "115jon", InstallLocation, UninstallString, QuietUninstallString, EstimatedSize, NoModify = 1, NoRepair = 1 |
| owns `yap://`, removed on uninstall only if it points at the install            | same rule for `yap://` and `yap-native://` (before-uninstall hook)                                                                                                                                                                 |
| Run value removed on uninstall if ours                                          | same, for `Yap`, `Yap-<profile>` and the old `YapNative[-<profile>]` that point into the root                                                                                                                                      |
| closes running instances on upgrade                                             | Velopack stops every process running from the install root before it swaps `current\` (forced, like the old `Process.Kill`)                                                                                                        |
| user data kept on uninstall                                                     | same: the data root is a different folder; only the default toast icon goes                                                                                                                                                        |
| `/S`                                                                            | `Setup.exe --silent` (never starts the app)                                                                                                                                                                                        |
| `--uninstall /S`                                                                | `Update.exe --uninstall --silent` (the entry's QuietUninstallString)                                                                                                                                                               |
| `/NoLaunch`                                                                     | `Setup.exe -- --no-launch` for an interactive install; silent installs never launch                                                                                                                                                |
| install log                                                                     | Velopack's log plus `installer.log` for the hooks                                                                                                                                                                                  |
| Authenticode via Artifact Signing                                               | same account and profile, through `vpk --azureTrustedSignFile` (signs the exe, the stub, `Update.exe` and `Setup.exe`)                                                                                                             |
| no telemetry                                                                    | none: Setup and Update make no network calls (there is no update feed yet); `vpk --skip-updates` keeps vpk from checking for itself                                                                                                |

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
- **Tauri updater (`latest.json`, minisign)**: no auto-update yet. Velopack's
  feed files are produced and kept as artifacts for when it lands.

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
Output: `<packId>-win-Setup.exe`, `<packId>-win-Portable.zip`,
`<packId>-<version>-full.nupkg`, `releases.win.json`, `assets.win.json`,
`RELEASES`.

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
only. It builds the exe, packs it with `tools/package.ps1 -Sign` after the
same `azure/login` OIDC step as `desktop-release.yml`, verifies the
Setup.exe signature with `desktop/scripts/verify-authenticode.ps1`, and
uploads the installer and feed files as a workflow artifact. It publishes
nothing.

It needs a static Qt 6.6.3 (MSVC, `-static -static-runtime`), which no
action provides:

- `runner: github-hosted` downloads `qt-6.6.3-static-msvc-x64.zip` (with
  `bin/qmake.exe` at its root) from the `native-build-assets` release. That
  asset does not exist yet; until it is uploaded this path fails with a
  clear message.
- `runner: self-hosted` runs on a `[self-hosted, windows, x64, yap-native]`
  runner whose `YAP_QT_DIR` points at the static Qt (and optionally
  `YAP_LIBWEBRTC_DIR` at an extracted libwebrtc).

libwebrtc comes from LiveKit's prebuilt release (webrtc-sys downloads it).
The OIDC credential trusts `main` and `v*` tags only: dispatch from `main`.

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
