<div align="center">

<a href="https://yap.115jon.com">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="branding/yappy-lockup-on-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="branding/yappy-lockup-on-light.svg">
    <img alt="Yap" src="branding/yappy-lockup-on-light.svg" width="264" height="96">
  </picture>
</a>

**Voice, chat, video and screen share for one group of friends. The room stays open all evening.**

[![CI](https://img.shields.io/github/actions/workflow/status/115jon/yap/ci.yml?branch=main&label=CI&style=flat-square&labelColor=000)](https://github.com/115jon/yap/actions/workflows/ci.yml)
[![Deploy web](https://img.shields.io/github/actions/workflow/status/115jon/yap/deploy-web.yml?branch=main&label=deploy%20web&style=flat-square&labelColor=000)](https://github.com/115jon/yap/actions/workflows/deploy-web.yml)
[![Desktop release](https://img.shields.io/github/actions/workflow/status/115jon/yap/native-release.yml?label=desktop%20release&style=flat-square&labelColor=000)](https://github.com/115jon/yap/actions/workflows/native-release.yml)
<br>
[![Latest release](https://img.shields.io/github/v/release/115jon/yap?filter=v*&label=release&style=flat-square&labelColor=000&color=ff3d8b)](https://github.com/115jon/yap/releases/latest)
[![Android](https://img.shields.io/github/v/release/115jon/yap?filter=qt-android-v*&include_prereleases&label=android&style=flat-square&labelColor=000&color=3ddc84)](https://github.com/115jon/yap/releases?q=qt-android-v&expanded=true)
[![License](https://img.shields.io/github/license/115jon/yap?style=flat-square&labelColor=000)](LICENSE)
[![Stars](https://img.shields.io/github/stars/115jon/yap?label=stars&style=flat-square&labelColor=000)](https://github.com/115jon/yap)
[![Web app](https://img.shields.io/badge/web-yap.115jon.com-ff3d8b?style=flat-square&labelColor=000)](https://yap.115jon.com)

[Open the web app](https://yap.115jon.com) ·
[Download for Windows](https://github.com/115jon/yap/releases/latest/download/YapSetup.exe) ·
[Android APK](https://github.com/115jon/yap/releases?q=qt-android-v&expanded=true) ·
[Changelog](CHANGELOG.md)

</div>

<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/hero-dark.webp">
  <source media="(prefers-color-scheme: light)" srcset="docs/readme/hero-light.webp">
  <img alt="The Yap home page: the headline 'The room is always open.' above the desktop app in a voice call, with four people on tiles and the channel tree showing who is talking" src="docs/readme/hero-light.webp">
</picture>

## Why Yap

Yap is an open-source place for a friend group to hang out: servers with text
and voice channels, direct messages, calls, video and screen share. It is
built for the evening where someone opens a voice channel, everyone alt-tabs
into a game, and the call is still running hours later. It costs little while
idle and answers at once when someone reaches for it.

- **Voice that stays open.** Leave a channel running; friends drop in without
  an invite or a host. The channel tree shows who is in the call and lights
  up whoever is talking. Opus audio, RNNoise noise suppression and voice
  activity detection.
- **Share the screen, keep the room.** Share a window or a whole screen, or
  on desktop capture a game directly. Everyone in the channel can watch
  while the call carries on.
- **Chat that keeps the thread.** Text channels and DMs with replies,
  threads, reactions, pins, search, GIFs, uploads and link embeds.
- **Listen together.** Queue YouTube or Spotify links and the room hears the
  same song at the same moment.
- **The rest of the kit.** Video with camera backgrounds, a soundboard, small
  activities (Wordle, WarpRush), roles and invites, a command menu, and
  profile collectibles.
- **Open all the way down.** GPL-licensed: the web app, the realtime backend,
  the desktop shell and the Android app are all in this repository.

<table>
  <tr>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="public/screenshots/home/chat-dark.webp">
        <source media="(prefers-color-scheme: light)" srcset="public/screenshots/home/chat-light.webp">
        <img alt="A text channel with replies, reactions, a pinned message, a file upload and a thread" src="public/screenshots/home/chat-light.webp">
      </picture>
    </td>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="public/screenshots/home/share-dark.webp">
        <source media="(prefers-color-scheme: light)" srcset="public/screenshots/home/share-light.webp">
        <img alt="A screen share on the stage, with the people in the call on tiles below it" src="public/screenshots/home/share-light.webp">
      </picture>
    </td>
  </tr>
  <tr>
    <td align="center"><sub>Text channels, threads and reactions</sub></td>
    <td align="center"><sub>Screen share on the stage, the call underneath</sub></td>
  </tr>
</table>

## Get Yap

| Where       | How                                                                                                                                                                                              |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Browser** | Open [yap.115jon.com](https://yap.115jon.com). Nothing to install.                                                                                                                               |
| **Windows** | Download [`YapSetup.exe`](https://github.com/115jon/yap/releases/latest/download/YapSetup.exe) from the [latest release](https://github.com/115jon/yap/releases/latest). The app updates itself. |
| **Android** | Download `yap-qt-X.Y.Z-arm64-v8a.apk` from the newest [`qt-android-v*` release](https://github.com/115jon/yap/releases?q=qt-android-v&expanded=true) and install it over any earlier one.        |

All three sign in to the same account and the same rooms.

<p align="center">
  <img alt="The Yap installer: unpacking files at 40 percent, then installed and starting Yap" src="docs/readme/installer.webp" width="660">
</p>

## How it's built

```text
  Web app            Desktop app            Android app
  React, TanStack    Rust core, Qt/QML      Rust core, Qt/QML
       |                    |                      |
       +---------- HTTP + WebSocket gateway -------+
                            |
             Cloudflare Worker (custom-server-entry.ts)
               |-- MeetingRoom Durable Object: gateway, presence, voice state
               |-- VoiceRoom Durable Object: per-channel media metadata
               |-- D1 (data), R2 (uploads), KV (cache), Queues (jobs)
                            |
          Cloudflare Realtime SFU via PartyTracks (audio, video, screens)
```

- **Web app:** React 19 on TanStack Start, served by a single Cloudflare
  Worker built with the Cloudflare Vite plugin. Tailwind CSS 4 and Base UI.
- **Realtime:** two Durable Objects in [`realtime/`](realtime). `MeetingRoom`
  runs the gateway (identify, heartbeat, presence, voice state) and issues
  voice tokens; `VoiceRoom` verifies, persists and broadcasts media
  descriptors for each voice channel. Audio and video travel through the
  Cloudflare Realtime SFU using [PartyTracks](https://github.com/cloudflare/partykit/tree/main/packages/partytracks),
  behind an authenticated proxy, with Cloudflare TURN for hard networks.
- **Storage:** D1 for servers, channels and messages, R2 for uploads, KV for
  caching and Queues for background work. Schema migrations live in
  [`migrations/`](migrations).
- **Auth:** [Kova](https://github.com/115jon/kova), through `@kova/react` on
  every client.
- **Desktop:** [`native/`](native) is a native Windows
  client: a Rust core behind a QML UI through
  [cxx-qt](https://github.com/KDAB/cxx-qt), linked against a static Qt into
  one `Yap.exe`, with libwebrtc for voice and video and native screen and
  game capture. It installs and updates through Velopack; `YapSetup.exe` is
  a copy of its `Yap.App-win-Setup.exe`.
- **Android:** [`native/mobile`](native/mobile) runs
  the same Rust core and QML on Android under a mobile shell.
- The earlier Tauri desktop app and Expo Android app are archived on the
  `archive/tauri-desktop` and `archive/expo-mobile` branches.

## Desktop release contents

The Windows client links Qt (LGPL-3.0) and libwebrtc statically and ships
Microsoft's ONNX Runtime for camera backgrounds. Game capture uses OBS
Studio's own signed hook and helper binaries, shipped unmodified under
GPL-2.0 with their license text and a written source offer
([`native/packaging/obs-capture`](native/packaging/obs-capture));
each release attaches the corresponding source. See the
[native third-party notices](native/THIRD_PARTY_NOTICES.md).

The capture component injects a graphics hook into a user-selected game process
to capture that process. It is not a security scanner, exploit tool, or bypass
tool.

## Documentation and policies

- [Privacy Policy](PRIVACY.md)
- [Code Signing Policy](CODE_SIGNING_POLICY.md)
- [Security Policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Native third-party notices](native/THIRD_PARTY_NOTICES.md)
- [Deploying the web app](docs/deploy.md)
- [Design system](DESIGN.md) and [product brief](PRODUCT.md)

## Development

The repository uses pnpm (see `packageManager` in `package.json`). Install,
then run the checks CI runs:

```powershell
pnpm install
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
```

Formatting is `oxfmt`, linting is `oxlint` (config in `oxlint.config.ts`, with the
vendored anti-slop rules under `tools/oxlint/anti-slop`), and `lefthook` runs
format, lint and typecheck on commit. `pnpm format` rewrites files, `pnpm lint:fix`
applies safe lint fixes.

`pnpm perf` builds and runs Lighthouse CI against `vite preview` (config in
`lighthouserc.cjs`, reports in `.lighthouseci/reports`). Set
`LHCI_KOVA_SESSION_TOKEN` to include the signed-in chat shell.

### Local sign-in (Kova app)

`pnpm dev` signs in with the **Yap (dev)** Kova app. `pnpm dev:prod-auth`
signs in with the live **Yap** app instead, on port 5199. The startup log
prints one `[ralph] Kova sign-in: …` line naming the app in use.

`RALPH_AUTH=dev|prod` (default `dev`) is read by `vite.config.ts` in `vite dev`
only. It takes the matching publishable key from `wrangler.toml` (dev:
`[env.mobile-dev.vars]`, prod: top-level `[vars]`) and gives the browser and the
local worker the same value, so they cannot disagree. That value beats
`VITE_KOVA_AUTH_PUBLISHABLE_KEY` in `.env.local` and `.dev.vars`, so you don't
need to set it there. `vite build` and deploys ignore `RALPH_AUTH` and keep the
live key from `[vars]`.

Prod sign-in only works from an origin listed on the live Kova app (its
`allowed_origins` and `redirect_uris`). Localhost is not listed by default.

### Dev server ports

`pnpm dev` listens on port 5173. If another process holds that port, the
server stops and says so. Set `RALPH_DEV_PORT` to use another port:

```powershell
$env:RALPH_DEV_PORT = 5174; pnpm dev
```

```bash
RALPH_DEV_PORT=5174 pnpm dev
```

The server never falls back to the next free port on its own, because the
**Yap (dev)** Kova app only allows `http://localhost:5173`,
`http://127.0.0.1:5173` and `http://localhost:1420`. On any other port the
app loads, but sign-in fails until that origin is added to the Kova app.

The native clients build from [`native/`](native); see
[native/README.md](native/README.md).

### Game catalog

The desktop app recognises running games against `public/desktop/game-catalog.json`,
a trimmed copy of Discord's public detectable-applications list that the site
serves itself. It is committed. To refresh it, run `pnpm games:catalog`
(`node scripts/build-game-catalog.mjs`, with `--input <file>` to build from a
saved copy of the list and `--out <file>` to write elsewhere) and commit the
result. The file is minified and sorted by app id; each app carries `id`,
`name`, optional `aliases`, optional `icon_hash`, its win32 `executables`
(`name`, plus `is_launcher` or `arguments` when set) and optional
`steam_app_ids`. Apps with no win32 executable are left out. Icons are not
hot-linked: clients load `/api/game-icons/{id}/{icon_hash}.png`, which fetches
from Discord's CDN once and keeps the image in our R2 bucket.

### Deploying the web app

Pushes to `main` deploy through the **Deploy web** workflow: build, a Worker
Preview named `main`, a smoke test, then production with an automatic rollback
if the production smoke test fails. Production steps run only while the
repository variable `WEB_DEPLOY_ENABLED` is `true`. `pnpm deploy:main` remains
the manual fallback. See [docs/deploy.md](docs/deploy.md) for the steps,
rollbacks, Preview resources and required settings.

Do not use locally built desktop artifacts as release-signing inputs.
Release provenance must be verified by the repository's release workflow.

## Contributing

<img align="right" alt="Yappy, the Yap dog, barking" src="docs/readme/yappy-bark.gif" width="120" height="120">

Issues and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md)
and the [Code of Conduct](CODE_OF_CONDUCT.md) before opening a pull request.
Changes to installer behavior, release workflows, signing configuration,
authentication, toolchain provenance, policies, or game capture require maintainer
review. Report security issues privately as described in
[SECURITY.md](SECURITY.md).

Yappy, the pixel dog above, is drawn on a 32-cell grid and barks while Yap
loads. The app themes dress it up:

<p>
  <img alt="Yappy in the Miku theme" src="docs/readme/yappy-miku.png" width="56" height="56">
  <img alt="Yappy in the Spider-Man theme" src="docs/readme/yappy-spider-man.png" width="56" height="56">
</p>

## Star history

<a href="https://star-history.com/#115jon/yap&Date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=115jon/yap&type=Date&theme=dark">
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=115jon/yap&type=Date">
    <img alt="Star history chart for 115jon/yap" src="https://api.star-history.com/svg?repos=115jon/yap&type=Date">
  </picture>
</a>

## License

Yap-owned source is distributed under the GNU General Public License,
version 3 or later (`GPL-3.0-or-later`). The OBS-derived capture component
remains separately licensed under GPL-2.0-only. The canonical root license is
in [LICENSE](LICENSE).
