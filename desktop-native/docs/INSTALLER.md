# Installer (Velopack)

The native client ships as one static `YapNative.exe`. Its installer is
[Velopack](https://docs.velopack.io): `vpk pack` wraps the exe into a native
`Setup.exe` (no .NET at install time), a full `.nupkg` and the release feed
files. It replaces, for this client, the .NET/WPF `YapSetup.exe` that the
Tauri app uses (`desktop/installer`).

## Identity and layout

| what           | value                                                                                           |
| -------------- | ----------------------------------------------------------------------------------------------- |
| packId         | `YapNative.App` (test installs: `YapNative.InstallTest`)                                        |
| install root   | `%LOCALAPPDATA%\YapNative.App`                                                                  |
| the exe        | `%LOCALAPPDATA%\YapNative.App\current\YapNative.exe`                                            |
| launcher stub  | `%LOCALAPPDATA%\YapNative.App\Yap Native.exe` (Velopack's, named after the title)               |
| shortcuts      | `Desktop\Yap Native.lnk`, `Start Menu\Programs\Yap Native.lnk`, both to `current\YapNative.exe` |
| updater        | `%LOCALAPPDATA%\YapNative.App\Update.exe`                                                       |
| data root      | `%LOCALAPPDATA%\YapNative` (profiles under `profiles\<name>`), never touched by Velopack        |
| capture hook   | `current\obs-capture\obs-signed\` (six OBS-signed files, see "Game capture hook payload")       |
| title, authors | "Yap Native", "115jon" (Start menu, Desktop, Apps & features)                                   |
| shortcut AUMID | `com.115jon.yap.native`, the default profile's toast identity (`cpp/desktopshell.cpp`)          |
| version        | `[workspace.package]` in `rust/Cargo.toml`, read by `CMakeLists.txt` and `tools/package.ps1`    |
| Velopack       | crate `velopack = "=1.3.39-g8d2844f"`; `vpk` of the same version (the script checks)            |

**Why not `YapNative` as the packId.** Velopack installs to
`%LOCALAPPDATA%\<packId>` and deletes that whole folder on uninstall. The
app's data root is `%LOCALAPPDATA%\YapNative` (settings, caches, logs,
WebView2, every profile), so a packId of `YapNative` would wipe user data on
uninstall. `YapNative.App` keeps the two apart. The packId is permanent for
updates: changing it later means a new app as far as Velopack is concerned.

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

| hook             | does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| after install    | Registers `yap-native://` to `current\YapNative.exe` (the same values the app writes, `yap_core::links::scheme_registration`). The released install (packId `YapNative.App`) always does, taking it back from a dev build that had it; another packId only when the scheme is free, dangling or already its own (`yap_core::install::hook_may_take_scheme`). Creates `HKCU\Software\Classes\AppUserModelId\com.115jon.yap.native` with `DisplayName` and a `YapInstallRoot` marker if the key is absent. |
| after update     | Registers the scheme again, by the same rule. The exe path under `current\` does not change, so the scheme and Run values stay valid across updates.                                                                                                                                                                                                                                                                                                                                                     |
| before uninstall | Removes the `yap-native` key if its open command runs an exe in this root; removes the AUMID key and the default profile's `toast-icon.png` if the marker names this root; removes every `YapNative` / `YapNative-<profile>` Run value whose command runs an exe in this root.                                                                                                                                                                                                                           |

The app itself still registers on every start: the AUMID key (`DisplayName`,
`IconUri`) and the Run value when autostart is on. The scheme is rewritten
at start only by the released install running as the default profile
(`<root named YapNative.App>\current\YapNative.exe` next to
`Update.exe`, `yap_core::install::may_register_at_launch`); a dev build, a
copy, a test install or any named profile never writes it, so the
installed app keeps it. A key written by another copy (a dev build, the
user's own profile) is never removed: a profile's AUMID key carries no
owner, so uninstall leaves `com.115jon.yap.native.<name>` alone.

Hook decisions are appended to `%LOCALAPPDATA%\YapNative\logs\installer.log`
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
  from the root (`Killing process: ...\current\YapNative.exe`), replaces
  `current\`, updates the shortcuts and the uninstall entry, and runs the
  install hook again (idempotent).
- **Update** (`Update.exe apply --package <full.nupkg>`, what an in-app
  updater will call): the same kill, a backup and swap of `current\`, the
  after-update hook, then a restart with the arguments after `--`.
- **Uninstall** (`Update.exe --uninstall [--silent]`): the before-uninstall
  hook, then Velopack removes the shortcuts, the uninstall entry,
  `%TEMP%\velopack_<packId>` and, about 3 s after it exits, the root.

## Parity with the Tauri installer

| old installer (`desktop/installer`)                                             | native installer                                                                                                                                                                                                                                |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| per-user, no elevation, under `%LOCALAPPDATA%`                                  | same: Velopack per-user install to `%LOCALAPPDATA%\YapNative.App`                                                                                                                                                                               |
| Desktop and Start menu shortcuts with the AUMID                                 | same: `--shortcuts Desktop,StartMenuRoot --aumid com.115jon.yap.native`                                                                                                                                                                         |
| HKCU uninstall entry: DisplayName, icon, version, publisher, NoModify, NoRepair | Velopack's `HKCU\...\Uninstall\<packId>`: DisplayName (the title), DisplayIcon (`current\YapNative.exe`), DisplayVersion, Publisher "115jon", InstallLocation, UninstallString, QuietUninstallString, EstimatedSize, NoModify = 1, NoRepair = 1 |
| owns `yap://`, removed on uninstall only if it points at the install            | same rule for `yap-native://` (before-uninstall hook)                                                                                                                                                                                           |
| Run value removed on uninstall if ours                                          | same, for `YapNative` and every `YapNative-<profile>` that points into the root                                                                                                                                                                 |
| closes running instances on upgrade                                             | Velopack stops every process running from the install root before it swaps `current\` (forced, like the old `Process.Kill`)                                                                                                                     |
| user data kept on uninstall                                                     | same: the data root is a different folder; only the default toast icon goes                                                                                                                                                                     |
| `/S`                                                                            | `Setup.exe --silent` (never starts the app)                                                                                                                                                                                                     |
| `--uninstall /S`                                                                | `Update.exe --uninstall --silent` (the entry's QuietUninstallString)                                                                                                                                                                            |
| `/NoLaunch`                                                                     | `Setup.exe -- --no-launch` for an interactive install; silent installs never launch                                                                                                                                                             |
| install log                                                                     | Velopack's log plus `installer.log` for the hooks                                                                                                                                                                                               |
| Authenticode via Artifact Signing                                               | same account and profile, through `vpk --azureTrustedSignFile` (signs the exe, the stub, `Update.exe` and `Setup.exe`)                                                                                                                          |
| no telemetry                                                                    | none: Setup and Update make no network calls (there is no update feed yet); `vpk --skip-updates` keeps vpk from checking for itself                                                                                                             |

Dropped on purpose:

- **Ralph Meet migration** (`LegacyCleanup`, `LauncherMigration`): the native
  client never shipped as Ralph Meet, so there is nothing to move.
- **The root launcher stub** (`desktop/launcher`, `RootLauncher`): Velopack
  brings its own stub at the root, and the real exe path
  (`current\YapNative.exe`) is stable across updates.
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
script stages only `YapNative.exe` and the game capture hook's files, so
nothing else from the build dir ships.
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
| title        | "Installing Yap", Bricolage Title 800, 28 px      | "Installing Yap Native" (the installer's title), 30 px                                  |
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
  `%LOCALAPPDATA%\YapNative[\profiles\<name>]\capture-hook\obs-33.0.1-c37f4412\`,
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
never start the default profile (README, "Testing notes"):

```powershell
./tools/package.ps1 -SkipBuild -BuildDir F:/ct/yapn/inst-build -OutDir F:/ct/yapn/inst-out `
  -PackId YapNative.InstallTest -Title "Yap Native InstallTest" -Version 0.0.1
$env:YAP_PROFILE = "spike"   # in case anything starts the app
F:/ct/yapn/inst-out/YapNative.InstallTest-win-Setup.exe --silent -- --no-launch
```

Run on 2026-11-06 (unsigned, same exe packed as 0.0.1, 0.0.2, 0.0.3):

| step                                  | result                                                                                                                                                                             |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| silent install 0.0.1                  | exit 0; `Update.exe`, stub, `current\YapNative.exe`, `packages\*.nupkg`; uninstall entry as in the parity table; both shortcuts, AUMID `com.115jon.yap.native`; install hook 34 ms |
| hooks with a live dev build present   | scheme (owned by `F:\ct\yapn\rel\YapNative.exe`) and the existing AUMID key left alone, on install and uninstall                                                                   |
| start `--profile spike` + `YAP_TOKEN` | first frame 865 ms, messages 1.6 s, gateway ready                                                                                                                                  |
| Setup 0.0.2 over it, app running      | Setup killed the running PID, installed 0.0.2 (`sq.version`, DisplayVersion), did not relaunch (silent)                                                                            |
| `Update.exe apply` 0.0.3, app running | killed the PID, swapped `current\`, after-update hook, restarted with `--profile spike`                                                                                            |
| `--no-launch` on a first run          | exit 0 in 0.4 s, no window                                                                                                                                                         |
| silent uninstall                      | exit 0; root, shortcuts, uninstall entry gone; `YapNative-spike` Run value into the root removed; Tauri `Yap` Run value, scheme, AUMID, data root unchanged                        |
