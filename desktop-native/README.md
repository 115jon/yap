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
- Voice calls: a voice channel opens its landing, and Join starts the call.
  The mic and speakers are real, with echo cancellation. Mute and deafen
  carry over between calls. Interoperates with the web client and follows its
  specs (`packages/voice/STATE_MACHINE.md`, `realtime/SOCKET_CORE.md`,
  `realtime/VOICE_ROSTER.md`): 3 s reconnect grace, the web's teardown order,
  and nothing sent on the wrong socket. The wire details and the remaining
  differences are in [VOICE_PROTOCOL.md](VOICE_PROTOCOL.md).
- Our camera, as the web publishes it: the camera key in the stage bar
  and the call strip (with a device caret), the Camera dialog (preview,
  camera, capture quality, "Preview before turning on camera", which the
  key honours), our tile mirrored, and the camera glyph on Talk Tree
  leaves (`self_video`).
  - Capture: Qt Multimedia (`QCamera` into a `QVideoSink`, `cpp/camera.cpp`)
    at the closest native format to the quality (uncompressed first) and
    at most its frame rate. Each frame goes to the preview as it is and,
    while the call publishes, through one libyuv conversion into the
    encoder's I420 buffer (`crates/yap-voice/src/camera.rs`,
    `cpp/camera_i420.cpp`). Device choice follows `DEVICES.md`: by id,
    then by name (the refreshed id is saved); an unplugged camera falls
    back to the default without touching the saved one and is opened
    again when it is back. Errors as the web words them (no camera, in
    use by another app).
  - Publish: one send-only VP8 transceiver with the web's simulcast
    (`f`/`h`/`q`, `video-layers.ts`), announced with `simulcast_rids`;
    op 15 `self_video`. Off keeps the SFU track on black keepalive frames
    and turning it on again resumes that track (VOICE_PROTOCOL.md).
  - `YAP_FAKE_CAMERA=1`: a moving test pattern instead of any device (no
    `QCamera` is created), through the same frame path.
- Watching video: other people's cameras and screen shares, as the web's
  stage shows them.
  - Tiles: camera video cropped to the tile, the avatar otherwise, speaking
    rings, and a spinner until the first frame.
  - Shares: a share tile with "is sharing" and Watch stream, then the stream
    letterboxed.
  - Focus: a click puts a tile on stage, with a members tray, the header's
    "Linus's Stream" and Live, and Stop watching. Fullscreen hides the shell
    (Esc exits).
  - Cameras are pulled once a visible tile shows them, at the simulcast
    layer for the tile's size (`preferredRid`, `tracks/update` on resize).
    Shares are pulled on Watch and kept until they end. Share audio is
    audible only while the share is on stage.
  - Frames: libwebrtc's decoder, then `NativeVideoStream` (one-frame queue),
    then `src/video.rs`, then `cpp/videohub.cpp`. That is one copy into a
    `QVideoFrame` (YUV420P), and Qt Multimedia's `VideoOutput` converts it
    on the GPU. Frames for a key nothing shows are dropped before the copy.
- Voice controls as on the web: device menus on the mic and headphone
  carets (system default first; switching applies at once in a call, an
  unplugged choice falls back to the default), the output volume, the
  per-user "User volume" (0-200 %) and "Mute" on a right click of a call
  tile or Talk Tree leaf, our own speaking LED and rings with op 5 from
  the web's VAD hysteresis and sensitivity (auto or a manual marker), and
  Voice settings: microphone, input profile, noise suppression, echo
  cancellation, high fidelity, the live level meter, speaker and output
  volume. Kept per user in `%LOCALAPPDATA%\YapNative\voice-settings.json`.
  Remote volume and live APM options go through
  `crates/yap-voice/cpp/track_audio.cpp` (calls the crate does not expose).
- The Talk Tree: everyone in every voice channel of the server
  (`VOICE_CHANNEL_STATES` / `_UPDATE`), with guides, the lit path to you,
  speaking LEDs and mute/deafen glyphs.
- The account row, the connection strip, and messages through the web's
  Markdown grammar (`yap-core` `markdown.rs`) as Qt rich text.
- The desktop shell, as the desktop app does it (`cpp/desktopshell.*`,
  `yap-core` `shell.rs`): one instance (a second launch shows the first),
  the tray icon with the unread badge, "Show" and "Quit", close to tray,
  Windows toasts for messages, mentions, replies and DMs (click opens the
  channel), the taskbar flash, the saved window rect, start minimized and
  open on startup, the web's UI sounds (`synth.rs`, played through
  `waveOut`), and a rotating log file. The switches sit in the account
  row's settings menu until a settings screen exists.

- Media in messages as the web draws it: pictures (one fitted in 640 x 400,
  or the 2-10 tile grid; GIFs animate; sensitive ones blurred behind
  "Show media"), the image viewer (zoom, pan, previous/next, open the
  original, download), video as a poster key that opens the system player,
  file cards that save to Downloads. Attachments load with the session as a
  bearer header added by the image loader (`cpp/main.cpp`,
  `src/media.rs`), never as a URL token.
- Uploads: the + key (system file dialog), Ctrl+V with copied files or a
  picture, drag and drop onto the chat; the tray with real progress,
  cancel, retry, the sensitive toggle and the web's 25 MB limit and error
  copy; captions, and attachments-only sends.
- Reactions (chips, mine by `users`, tooltips, toggle, `REACTION_ADD` /
  `_REMOVE` live, rollback on failure), the hover bar's quick reactions
  (recents, then thumbs up, heart, joy) and the emoji picker (sections,
  search, recents, skin tone, kept in `emoji-prefs.json`).
- Twemoji everywhere: message text (native emoji, `:shortcode:`, custom
  `<:name:id>`), reactions and the picker, from `emoji-datasource-twitter`
  16.0.0 (`assets/emoji`, `yap-core` `emoji.rs`, CC-BY 4.0 graphics).
- Link previews from `embeds` (cards with site, title, description,
  thumbnail or large picture; YouTube and other players as a thumbnail
  with a play disc that opens the link; direct media bare; unavailable
  posts), including `MESSAGE_UPDATE` arrivals and the revision rule.
- The GIF picker (KLIPY via `/api/gifs`: trending, search, GIFs, stickers
  and memes, paging), sending through `/messages/gif` at once.

The shell has its own identity, apart from the installed Yap app: AUMID
and single-instance name `com.115jon.yap.native` (registered under
`HKCU\Software\Classes\AppUserModelId` only), Run value `YapNative` (off
unless "Open on startup" is on), data and logs under
`%LOCALAPPDATA%\YapNative`, and no `yap://` registration.

## Not yet

Publishing a screen share, camera backgrounds, the stream watcher list and stream
volume, input volume, a mic test outside a call, inline video playback, clips in the GIF picker,
GIF favorites and categories, stickers, emoji generation, the reaction details
modal, remove-embeds, embed players (WebView2), jumping to
messages that are not loaded (`around=`), `yap://` deep links, toast buttons (reply, mark read)
and avatars, call rings, the taskbar thumbnail toolbar and auto-update.

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
| `YAP_FAKE_CAMERA`          | unset                     | `1`: a test pattern stands in for every camera   |

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
cpp/desktopshell.*    one instance, tray and badge, toasts, Run key, window rect
cpp/toast_win.*       WinRT toasts (WRL, no Qt headers)
cpp/logfile.*         stderr tee into %LOCALAPPDATA%\YapNative\logs, rotated
cpp/videohub.*        remote call video into QVideoSinks (VideoHub singleton)
cpp/camera.*          our camera: devices, QCamera or the test pattern, frames out
assets/, tools/       generated fonts, icons and brand SVGs, and their scripts
rust/src/bridge.rs    YapClient QObject (properties, invokables, signals)
rust/src/client/      core: config, api (REST), auth (device flow), gateway,
                      socket (room/voice sockets), sfu (partytracks), voice (calls)
rust/src/desktop.rs   Desktop QObject: shell settings, badge, toasts, window rect
rust/src/video.rs     video frames to the hub; render demand to the call
rust/src/sound.rs     UI sounds through waveOut
rust/qml/             UI: Main, SignInView, ServerRail, ChannelSidebar, ChatView, ...
```
