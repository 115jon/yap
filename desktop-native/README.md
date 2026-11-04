# Yap native (spike)

A native Yap desktop client prototype: a Rust core behind a QML UI via
[cxx-qt](https://github.com/KDAB/cxx-qt), linked against a static Qt into one
`YapNative.exe`.

[FEASIBILITY.md](FEASIBILITY.md) covers the assessment, the measurements and
the decisions.

## What works

- Kova device-code sign-in, with the session stored in Windows Credential
  Manager.
- A developer token field as an alternative to sign-in.
- The server rail and channel tree (categories, text and voice).
- A text channel's history, with Markdown, grouping and edits.
- Live updates over the gateway: create, update, delete, typing.
- Messaging as on the web: optimistic sends with a nonce (failed rows retry
  or go back to the composer), replies, edit (inline, Up in an empty
  composer) and delete (with the confirm dialog, Shift skips it), typing out
  and in, `@` member suggestions, mention highlights, older pages on scroll.
- Unread state: read marks (`/api/read-states`, `READ_STATE_UPDATE`,
  mark-read at the end of a focused list), bold rows with the edge pill,
  mention counts on channels and the rail (unread mention/reply
  notifications), the "New" divider and its banner.
- Audio-only voice calls: a voice channel opens its landing, Join starts the
  call; real mic and speakers with echo cancellation, a participant grid,
  mute and deafen (kept between calls), leave. Interoperates with the web
  client. Wire details are in [VOICE_PROTOCOL.md](VOICE_PROTOCOL.md).
- The Talk Tree: everyone in every voice channel of the server
  (`VOICE_CHANNEL_STATES` / `_UPDATE`), with guides, the lit path to you,
  speaking LEDs and mute/deafen glyphs.
- The account row, the connection strip, and messages through the web's
  Markdown grammar (`yap-core` `markdown.rs`) as Qt rich text.

## Not yet

Video and screen share, device picker, attachments preview, embeds, reactions, DMs, jumping to
messages that are not loaded (`around=`), OS notifications, tray, single instance and auto-update.

## Build (Windows)

You need:

- Visual Studio 2022+ with the C++ tools;
- Rust (stable, `x86_64-pc-windows-msvc`);
- CMake 3.24+ and Ninja;
- a static Qt 6 built with `-static -static-runtime`;
- LiveKit's prebuilt libwebrtc. `webrtc-sys` downloads it (about 430 MB
  extracted) unless `LK_CUSTOM_WEBRTC` or `YAP_LIBWEBRTC_DIR` points at an
  extracted copy. `build.ps1` uses `F:/ct/libwebrtc/...` when present.

```powershell
# Defaults: Qt at F:/Files/Downloads/qt/src/6.6.3, build dir build/Release
./build.ps1 -QtDir C:/path/to/qt-static
```

`build.ps1` enters the VS developer shell, configures with CMake, and builds
with Ninja. A dynamic Qt also works; you then need `windeployqt` to run it.

## Run

```powershell
$env:QT_FORCE_STDERR_LOGGING = "1"   # see Qt warnings (WIN32 app)
./build/Release/YapNative.exe
```

| env var                    | default                   | meaning                                          |
| -------------------------- | ------------------------- | ------------------------------------------------ |
| `YAP_API_URL`              | `https://yap.115jon.com`  | API and gateway                                  |
| `YAP_KOVA_AUTH_URL`        | `https://auth.115jon.com` | Kova                                             |
| `YAP_KOVA_PUBLISHABLE_KEY` | Yap's live key            | Kova app                                         |
| `YAP_GATEWAY_ORIGIN`       | `YAP_API_URL`             | `Origin` sent on realtime sockets                |
| `YAP_TOKEN`                | none                      | use this session instead of the credential store |

The defaults mirror `src/lib/site-defaults.ts`.

## Assets and theme

`assets/` is generated and committed, so a build needs neither Python nor
Node. It is embedded as `:/fonts`, `:/icons` and `:/svg`.

```powershell
# Static cuts of Bricolage, Geist, Geist Mono and Martian Mono from a pinned
# google/fonts commit (SHA-256 checked, versions matched to the web's
# Fontsource files), renamed to one "Yap *" family per cut, plus OFL texts.
uvx --from fonttools --with brotli python -I tools/build-fonts.py
# Phosphor SVGs for every concept in src/components/ui/icon/mapping.ts,
# manifest.json (fill, slash, rotate, off pairs), and the brand marks.
node tools/build-icons.mjs
```

- `Theme.qml` holds every token for the six themes; `YAP_THEME` picks one
  (`dark`, `light`, `miku-dark`, `miku-light`, `spiderman-dark`,
  `spiderman-light`). Type roles are `Theme.type.*` font values.
- `Icon { name: "mic"; variant: "off"; size: 20; color: Theme.ink2 }` draws
  through `cpp/iconprovider.cpp` (`image://icon/<concept>/<variant>/<rgb>`),
  which composites the slash and rotation as `Icon.tsx` does.
- `cpp/windowchrome.cpp` removes the caption (`WM_NCCALCSIZE`) and hit-tests
  the QML title bar (`WM_NCHITTEST`, `HTMAXBUTTON` for snap layouts).
- `YAP_TEXT=qt` switches text from native glyph rendering to distance fields.

## Layout

```
CMakeLists.txt        exe, static Qt plugin imports, cxx-qt crate import
cpp/main.cpp          QGuiApplication, fonts, icon provider, theme, startup timing
cpp/windowchrome.*    custom title bar: caption removal, hit testing, snap layouts
cpp/iconprovider.*    Phosphor icons, tinted and cached per size and colour
assets/, tools/       generated fonts, icons and brand SVGs, and their scripts
rust/src/bridge.rs    YapClient QObject (properties, invokables, signals)
rust/src/client/      core: config, api (REST), auth (device flow), gateway,
                      socket (room/voice sockets), sfu (partytracks), voice (calls)
rust/qml/             UI: Main, SignInView, ServerRail, ChannelSidebar, ChatView, ...
```
