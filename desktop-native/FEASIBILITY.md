# Native Yap desktop client: feasibility

Status: spike, 2026-11-04. Branch `spike/qt-desktop`.

## Summary

A native Qt client for Yap is feasible. Nothing in the protocol needs a
browser:

- sign-in is three HTTPS calls;
- the gateway is JSON over a WebSocket;
- REST is plain JSON with a Bearer token;
- the SFU is Cloudflare Realtime, which takes plain SDP offers and answers
  over HTTPS.

The hard parts are:

- **Native WebRTC:** pushing and pulling tracks with echo cancellation.
- **Feature volume:** the web UI is very large. `chat-reducer.ts` alone is
  90 KB, and voice has VAD, noise reduction, soundboard, Listen Together,
  camera effects, and screen share.

Parity is months of work. A useful native client for chat plus voice is
weeks.

**Recommendation:**

- Rust core with a QML UI through cxx-qt 0.10, built by CMake with Corrosion
  against resonance's static Qt 6.6.3. The result is one `.exe` with no
  QtWebEngine.
- Embeds (YouTube / Listen Together) use the OS WebView2. That costs nothing
  in binary size.
- Voice uses LiveKit's `libwebrtc` crate: prebuilt static libwebrtc, used
  only as a PeerConnection. LiveKit's own server protocol is not involved.

## Measured: prototype vs current desktop app (2026-11-04)

Both against production `yap.115jon.com`, on the same PC.

| metric                   | YapNative prototype                                        | Yap desktop 3.10.1 (Tauri + CEF)                                                                     |
| ------------------------ | ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| what ships               | one `YapNative.exe`, **30.7 MB**, only Windows system DLLs | **406 MB** per installed version (`libcef.dll` 244 MB, `Yap.exe` 40 MB, `dxcompiler.dll` 24 MB, ...) |
| processes                | 1                                                          | 8                                                                                                    |
| idle working set         | **82 MB**                                                  | 310 MB                                                                                               |
| idle private bytes       | **97 MB**                                                  | 652 MB                                                                                               |
| idle CPU, 60 s sample    | **0.03 %** of one core                                     | 6.7 % of one core (see the caveat below)                                                             |
| threads                  | 91                                                         | 310                                                                                                  |
| first frame after launch | **430–510 ms** (5 warm-cache runs)                         | did not run                                                                                          |
| messages on screen       | 1.5–3.1 s (network-bound, see below)                       | did not run                                                                                          |
| gateway Ready            | 1.6–3.3 s                                                  | did not run                                                                                          |

**How each was measured:**

- **Native:** the script launches the exe 5 times and reads the
  `[startup]` lines it writes to stderr. Then it leaves the last instance
  signed in on `#general` for 70 s and samples the final 60 s.
- **CEF:** a read-only `Get-Process` sample of the user's running instance.
  - That instance had been up since 2026-11-03 and had used native screen
    share, so its 6.7 % CPU may include a visible window or call state.
    Treat it as indicative, not like-for-like.
  - I did not measure cold start for the CEF app. Launching the installed
    app would hit its single-instance lock and touch its data.
  - A fair number needs an isolated smoke build (the
    desktop-smoke-isolation recipe).

**Startup is network-bound.** Each API call takes 0.3–0.8 s server-side
(curl: `/api/users/me` 0.40–0.64 s, `/api/voice/socket-ticket` 0.29–0.82 s,
`/api/servers` 0.29–0.41 s). Each one includes a Kova session check.

- The client already overlaps `me` and `servers`.
- Next steps:
  - render the last session's state from a local cache at once;
  - prefetch the gateway ticket alongside `me`.

**Idle CPU fix:** the first build idled at 1.1 % because a looping QML
pulse animation under a hidden item kept the render loop ticking. Gating it
on visibility brought idle to 0.03 %.

## 1. Stack choice

### What resonance gives us

- Static Qt **6.6.3** at `F:\Files\Downloads\qt\src\6.6.3`:
  - MSVC 19.43, `-static -static-runtime`, so `/MT`;
  - OpenSSL is loaded at runtime, not linked. Schannel and OpenSSL TLS
    backends are both built as static plugins.
- It has Quick, QuickControls2, WebSockets, Network, Svg, Multimedia, and
  ShaderTools. It has **no** WebEngine.
- Toolchain: VS 18 BuildTools (MSVC 14.50), Ninja, and vcpkg with the
  `x64-windows-static` triplet, through `CMakePresets.json`.
- QWindowKit (static) gives a frameless window with native snap and shadows.
  `QT_DISCARD_FILE_CONTENTS` / qmlcachegen compile QML into the binary.
- The resulting `Resonance.exe` is a single file of 88 MB. Most of that is
  JUCE plus FFmpeg.

### Option A: C++ Qt (Widgets or QML)

**For:**

- Zero friction: it is the resonance recipe as-is.
- `QWebSocket`, `QNetworkAccessManager`, and `QSslSocket` (Schannel) all work
  statically today.

**Against:**

- All protocol, state, and voice code would be C++, which is the
  memory-safety cost the user wants to avoid.
- Native WebRTC from C++ means building libwebrtc (gclient, about 20 GB,
  hours) or the LiveKit C++ SDK, which is young.

### Option B: cxx-qt 0.10 (Rust core + QML UI), recommended

**For:**

- Logic lives in Rust:
  - auth, REST, and the gateway: `reqwest` + `rustls`, `tokio-tungstenite`;
  - the state reducer;
  - voice: the `libwebrtc` crate (LiveKit), which ships prebuilt **static**
    libwebrtc for `x86_64-pc-windows-msvc` with the WebRTC APM (AEC, NS, AGC)
    and the Windows audio device module.
- The C++ is only what cxx-qt generates, plus QML.
- Rust TLS (rustls) avoids Qt's runtime-loaded OpenSSL entirely.

**Against, and how to handle it:**

| issue                                           | mitigation                                                                                                                                                                                                                                                     |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A static Qt needs plugin and QML-module imports | CMake does this for us. CMake owns the exe; Corrosion builds the Rust staticlib. KDAB's documented path, not `cargo` alone.                                                                                                                                    |
| CRT mismatch: Qt uses `/MT`                     | Build Rust with `-C target-feature=+crt-static`.                                                                                                                                                                                                               |
| Build time                                      | The first `libwebrtc` build downloads about 100–200 MB of prebuilt libraries (ask first). After that, builds are incremental.                                                                                                                                  |
| cxx-qt is less mature than raw Qt               | 0.10 is current and has a `link_qt_object_files` feature for static Qt. Support for 6.6.3 is not yet checked; the step 2 build proves it. The risk is contained: if it fails, the Rust core can sit behind a plain C ABI, and the QML/C++ side stays the same. |

**Verdict:** B. The prototype proves the CMake + Corrosion + cxx-qt + static
Qt build first, because that is the biggest unknown.

### QML or Widgets

Use QML with QuickControls2 (Basic style, customised). Yap's look
(`DESIGN.md`) needs custom surfaces, animation, and theming. Widgets would
fight that.

## 2. What "single binary" can mean

1. **Fully static, no WebEngine (chosen).**
   - QtWebEngine cannot be built statically. That is an upstream limitation,
     and it is about 150 MB of Chromium anyway.
   - Expected size: 30–60 MB.
2. **Static app plus WebEngine DLLs.** Rejected: it brings back the size and
   multi-process RAM we are trying to drop.
3. **Embeds through the OS WebView2 (Windows).**
   - WebView2 ships with Windows 11, and `WebView2LoaderStatic.lib` links
     into the exe.
   - It would host the YouTube / Listen Together player and long-tail
     web-only panels, on demand only.
   - The cost is a lazily spawned msedgewebview2 process group. It is
     Windows-only; macOS and Linux would use WKWebView / WebKitGTK.

**Licensing:** Qt is LGPLv3 and Yap is GPLv3.

- A GPLv3 app with its source published meets LGPL's relinking requirement.
- GPL-only Qt modules (Quick3D, Charts, ...) would also be allowed, but we
  don't need them.
- libwebrtc is BSD and Rust crates are MIT/Apache.
- `THIRD_PARTY_LICENSES.md` gets a section.

## 3. Protocol: what a native client must speak

Everything below comes from reading the current code.

### Auth: Kova device code (`@kova/react` `startDeviceSignIn`)

- **Start:** `POST {KOVA_AUTH_URL}/api/pub/apps/{pk}/device/start` with
  header `X-Publishable-Key: {pk}` and body `{hostname, platform, label}`.
  - The response is `{deviceCode, userCode, verificationUri(Complete),
interval, expiresIn}`.
- **Poll:** `POST .../device/poll` with `{deviceCode}` until it returns
  `{status:"approved", sessionToken}`.
  - It answers 429 `slow_down`, which means poll more slowly (+5 s).
  - `.../device/cancel` cancels the request.
- **Storage:** the session token goes in Windows Credential Manager.
  - Every API call sends `Authorization: Bearer <token>` and
    `X-Publishable-Key`.
- For development, a pasted token skips sign-in.

### Gateway: `wss://{api}/api/gateway`

1. `POST /api/voice/socket-ticket` with `{audience:"global"}` returns
   `{ticket}`. It needs Bearer auth, is single use, and is valid for 60 s.
2. Open the WebSocket with subprotocols `ralph.realtime.v1` and
   `ralph.ticket.<ticket>`.
3. **The server checks `Origin`** (`isAllowedRealtimeOrigin`), and native
   sockets send none.
   - The spike sends `Origin: https://tauri.localhost`, which is already
     allowed for the current desktop app.
   - A real client should get its own allowlisted origin: a one-line change
     in `custom-server-entry.ts`.
   - The ticket is the real authentication; the origin check only guards
     against cross-site WebSocket hijacking in browsers.
4. Ops:

| op      | direction       | meaning                                                                  |
| ------- | --------------- | ------------------------------------------------------------------------ |
| 8       | server → client | Hello, `{heartbeat_interval}`                                            |
| 0       | client → server | Identify, `{name, clerk_user_id, platform, supports_voice_state_deltas}` |
| 7       | client → server | Resume                                                                   |
| 2 / 9   | server → client | Ready / Resumed                                                          |
| 3       | client → server | Heartbeat, exactly `{"op":3}`                                            |
| 6       | server → client | Heartbeat ACK                                                            |
| 19      | server → client | Dispatch, `{event, data, seq}`                                           |
| 18      | server → client | Error                                                                    |
| 26      | client → server | Presence status                                                          |
| 27 / 28 | client → server | Subscribe / unsubscribe a channel                                        |
| 35      | client → server | Subscribe a server                                                       |
| 15      | client → server | Voice state                                                              |
| 33–39   | client → server | Calls                                                                    |

Dispatch events include `MESSAGE_CREATE`, `MESSAGE_UPDATE`,
`MESSAGE_DELETE`, `TYPING_START`, reactions, presence, guild, channel, and
read-state events.

### REST

- `GET /api/users/me`
- `GET /api/servers`
- `GET /api/servers/{id}/channels` returns `{channels, categories}`.
- `GET /api/channels/{id}/messages?limit=50[&before=|&after=|&around=]`
  returns a message page.
- `POST /api/channels/{id}/messages`

### Voice: Cloudflare Realtime SFU through the partytracks proxy

1. A socket ticket with `audience:"voice"` returns `media_bootstrap
{participantId, voiceToken, iceServers}`.
2. The voice gateway is `wss://.../api/channels/{id}/voice`, the VoiceRoom
   DO. It handles join, presence, and track announcements.
3. Media goes through `/api/channels/{id}/partytracks/...`. The worker
   proxies to the Cloudflare Realtime API:
   - `sessions/new`, `sessions/{id}/tracks/new`, `renegotiate`,
     `tracks/close`;
   - bodies are SDP offers and answers plus track lists;
   - the proxy authenticates with `X-Voice-Session-Id` and the voice token,
     and signs session capabilities with HMAC.
4. Nothing here is browser-specific. Any WebRTC stack that can do unified
   plan, Opus, and BUNDLE can speak it. Track names follow
   `{prefix}-{kind}-{participantId}`.

**Native WebRTC options:**

| option                            | AEC / NS / AGC                                                | devices              | screen share                  | static on Windows | verdict                         |
| --------------------------------- | ------------------------------------------------------------- | -------------------- | ----------------------------- | ----------------- | ------------------------------- |
| LiveKit `libwebrtc` crate         | yes (WebRTC APM)                                              | ADM enumerate/select | desktop capturer in libwebrtc | yes (prebuilt)    | **use**                         |
| webrtc-rs                         | no APM; bring your own (e.g. `webrtc-audio-processing` crate) | cpal                 | DIY                           | yes               | more work and weaker AEC        |
| build libwebrtc from source (C++) | yes                                                           | yes                  | yes                           | yes               | hours of builds and about 20 GB |

The existing Rust `native_share.rs` (desktop screen share) and the RNNoise
work (`F:\dev\C++\JUCE\noise-suppression-for-voice`) can be reused.

## 4. Scope to parity (one engineer, rough)

| slice                  | contents                                                                                                                                                                   | estimate |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| Prototype (this spike) | build, sign-in, gateway, server and channel sidebar, live messages for one channel                                                                                         | days     |
| Text MVP               | send/edit/delete, Markdown, attachments and images, reactions, typing, presence, DMs, unread state, notifications, tray, auto-update                                       | 4–6 wk   |
| Voice audio            | join/leave, mic and speaker, devices, mute/deafen, VAD and speaking rings, noise suppression, soundboard playback                                                          | 3–5 wk   |
| Video + screen share   | camera, pulling tiles, screen share push and watch, quality controls                                                                                                       | 4–6 wk   |
| Long tail              | settings, server admin, roles, invites, emoji/GIF/sticker pickers, collectibles, profiles, search, threads, pins, Wordle, Listen Together (WebView2), mobile-parity polish | 3–6 mo   |

Full parity takes roughly **6–9 months** solo.

**Recommended path:**

- Build the hot path natively: shell, sidebar, chat, and voice.
- Host the long-tail panels as web views of the existing routes (WebView2)
  until each is worth porting.

## 5. What stays web

- The web app itself, which stays primary.
- Embeds and players: YouTube / Listen Together, oEmbed iframes.
- Rarely used admin and settings screens, at first.
- Wordle and other mini-apps.

## Decisions (2026-11-04)

1. **Stack:** Rust + cxx-qt.
2. **Backend:** production `yap.115jon.com`, with a dedicated test account
   (`yap-native-spike@example.test`, Kova user
   `sNOXObG6XFuVTau0rMdgjlm26cIVHzag`) and its own "Native Spike Lab"
   server.
   - The real device-code flow was exercised end to end. The code was
     approved through the same `lookup` / `approve` endpoints the `/activate`
     page uses, with that account's app session.
   - Remove the account and its server when the spike ends.
3. **Prebuilt libraries:** use them.
   - The static Qt 6.6.3 from resonance stays, because it already exists and
     works.
   - The libraries around it are new and current:
     - cxx-qt 0.10;
     - reqwest 0.13 and tokio-tungstenite 0.30 on native TLS (Schannel);
     - `keyring` for Windows Credential Manager.
   - Nothing else is taken over from resonance (no QWindowKit,
     SingleApplication, or JUCE).
   - The title bar is the native one, recoloured through DWM. That keeps
     snap layouts, shadows and resize with no library.

## Findings from building it

- **Static linking works with cargo + CMake:**
  - `cxx_qt_import_crate` (Corrosion) plus `qt_import_qml_plugins` linked
    against the static Qt on the first try;
  - `+crt-static` in `rust/.cargo/config.toml` matches Qt's `/MT`.
- **QML files must live under the crate directory.**
  - A `../qml/Main.qml` path lands in qrc as `com/yap/native/qml/...`, but
    the generated qmldir resolves it to `com/yap/qml/...`.
  - The type lookup then fails at runtime.
- **WIN32-subsystem Qt apps swallow warnings.** Set
  `QT_FORCE_STDERR_LOGGING=1` when debugging.
- **The device flow needs no `Origin`.** `start` and `poll` work from a
  native client as-is.
- **The gateway does need an `Origin`.** The prototype sends the Tauri
  app's (`YAP_GATEWAY_ORIGIN`). A real client should get its own allowlisted
  origin.
- **`MESSAGE_UPDATE` can be partial.** A link unfurl sends embeds without
  `content`, so the client patches only the fields an update carries.
