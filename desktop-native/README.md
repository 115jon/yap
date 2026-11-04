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
- Sending from the composer.

## Not yet

Voice, attachments preview, embeds, reactions, DMs, unread state,
notifications, tray, single instance, auto-update, and the brand fonts. Those
need TTF builds; the prototype uses Segoe UI Variable and Cascadia Mono.

## Build (Windows)

You need:

- Visual Studio 2022+ with the C++ tools;
- Rust (stable, `x86_64-pc-windows-msvc`);
- CMake 3.24+ and Ninja;
- a static Qt 6 built with `-static -static-runtime`.

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
| `YAP_GATEWAY_ORIGIN`       | `https://tauri.localhost` | `Origin` sent on the gateway socket              |
| `YAP_TOKEN`                | none                      | use this session instead of the credential store |

The defaults mirror `src/lib/site-defaults.ts`.

## Layout

```
CMakeLists.txt        exe, static Qt plugin imports, cxx-qt crate import
cpp/main.cpp          QGuiApplication, DWM title bar colours, startup timing
rust/src/bridge.rs    YapClient QObject (properties, invokables, signals)
rust/src/client/      core: config, api (REST), auth (device flow), gateway
rust/qml/             UI: Main, SignInView, ServerRail, ChannelSidebar, ChatView, ...
```
