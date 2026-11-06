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
  `waveOut`), and a rotating log file.
- User settings, the web's Studio workbench (`qml/screens/Settings`): the
  numbered nav (01 You, 02 Calls, 03 App), search that lands on rows
  (the map, search and rules are `yap-core` `settings.rs`), the poster
  page, rule cards and the preview rails; the settings key opens it, the
  device menus open Voice & video. Account: display name (saved to the
  account), username, email and sign-in methods from Kova. The profile
  editor (account card or Account): avatar and banner uploads, banner,
  background and accent colours, custom status, pronouns and bio, with the
  live card and the unsaved bar. Sessions (list), Privacy (media filter,
  which the GIF search follows; shared links with copy, open and revoke),
  Voice & video (the old voice popup, plus camera, preview and quality),
  Soundboard volume, Keybinds, Appearance (the seven themes in
  `appearance.json`, synced to the account when "Use this theme on all my
  devices" is on, and the preview sheet), Notifications (desktop alerts,
  sound volume and each sound category, Test) and Desktop app (startup and
  closing, the version). What this build cannot do shows "Coming later".

- Media in messages as the web draws it: pictures (one fitted in 640 x 400,
  or the 2-10 tile grid; GIFs animate; sensitive ones blurred behind
  "Show media"), the image viewer (zoom, pan, previous/next, open the
  original, download), videos played in place with the chat player at the
  web's sizes (640 x 400 at the file's aspect; Download on hover, the GIF
  or clip star, the web's "This video cannot be played here" state), file
  cards that save to Downloads. Private video goes through a loopback relay
  that adds the session (`src/loopback.rs`), never through the URL. Attachments load with the session as a
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
- Link embeds as LinkEmbed.tsx draws them (`store/view/embeds.rs`,
  `qml/embeds`), including `MESSAGE_UPDATE` arrivals and the revision rule:
  - link cards (site, title, description, thumbnail or large picture);
    unavailable, restricted, failed and loading posts (EmbedStatus, with
    Try again);
  - direct images (the viewer) and direct video files, played in place;
  - TikTok and Instagram: the portrait card (author, carousel, likes,
    comments, views, caption, the sound chip), refreshed through
    `/api/tiktok-video` and `/api/instagram-video` once per link and played
    through `/api/proxy-media`; TikTok falls back to its own player;
  - X posts: author, folding text, the 1-4 media grid (pictures to the
    viewer, videos in place, GIFs as the animated WebP from
    gif.fxtwitter.com, else the mp4 looping), "From <domain>", the quoted
    post and the counts;
  - YouTube, Spotify and allowed rich players as web players (WebView2) in
    place, one at a time, after a click.
- Chat video (`EmbedVideo.qml`, Qt Multimedia on Media Foundation): the
  poster and duration badge until clicked, then play/pause/replay, the
  scrub band, time, mute and volume, Full Screen (the video viewer); it
  pauses when scrolled away and is released when its row is hidden.
- WebM (`crates/yap-webm`, `cpp/webmplayer.*`), which Media Foundation
  cannot open here: VP8/VP9 (4:2:0, 4:4:4, RGB) with the alpha side
  stream, demuxed by webm-iterable, decoded by the libvpx inside
  `webrtc.lib`, paced on its own thread into a `QVideoSink`. Animated
  nameplates (member list and account row on hover, the shop's stage and
  cards always), webm attachments and links, and video profile banners.
  It decodes only while shown, the window is up and, for decorations, the
  app is active; it loops, holds its first frame when paused, and caps the
  picture at 1920 x 1080 (960 x 540 for decorations) and the file at 64 MB.
- The GIF and clip favourite star on video attachments and X media
  (`/api/gifs?mode=favorites`, `mode=favorite` POST/DELETE, the web's ids),
  and the x that removes our own message's previews (confirm, Shift skips).
- The shop catalog is kept on disk (`cache\catalog.json`) and shown at
  once; it is asked for again after the web's 30 minutes (10 when the
  server said stale).
- Collectibles (`yap-core` `collectibles.rs`): avatar decorations (APNG,
  decoded natively) on message, member, DM, voice and account avatars, the
  profile card and the shop; nameplates behind member and account rows;
  profile effects (one pass on a profile, the loop in the shop) and frames
  on the profile card. `USER_PROFILE_UPDATE` updates every copy, the open
  channel's messages included.
- The shop (`store/shop.rs`, `qml/screens/Shop`): the DM sidebar's Shop row
  opens it in place of Friends. Featured (hero, collections, a rail per
  collection), the four type tabs with counts, collection pages, search,
  cards (art on your own avatar or name, "Free" and the struck source
  price, colour dots), the detail dialog (live preview on you, variants,
  Equip and Remove through `PATCH /api/collectibles/apply`, a 409 retried
  once). The profile editor's Collectibles rows open a picker per slot.
- Server administration as the web does it (`qml/screens/Admin`, the rules
  in `yap-core` `admin.rs`, the store in `store/reduce/admin.rs`): the
  server header menu, Server settings on the shared workbench (Overview
  with the icon, name, share switches and the owner's Delete server; Roles
  with the editor and the web's permission groups; Invites with pause, the
  create form, countdowns, copy and revoke; Bans with unban; the Audit
  log as sentences, the 50 newest as on the web; the API pages by `page`,
  not `before`, so there is no Load more), with long lists built a page at
  a time as the page scrolls (`PagedRows`), Channel settings (name, topic, public sharing, and the
  Deny / Inherit / Allow overrides per role and member), Create channel and
  Create category, the tree's channel, category and sidebar menus with
  delete confirms, drag to reorder within a group, Kick and Ban (with a
  reason) on a member's menu, and role assignment on the profile card.
  Pages and actions follow the user's permissions (their roles' bits, and
  the channel's own `permissions`), and `GUILD_UPDATE` / `GUILD_DELETE` /
  `CHANNEL_UPDATE` / `CHANNEL_DELETE` keep the tree and the open settings
  current. The tree groups channels as the web does (loose text and voice
  first, then every category, empty ones included).
- The GIF picker (KLIPY via `/api/gifs`: trending, search, GIFs, stickers
  and memes, paging), sending through `/messages/gif` at once.
- Threads, pins and jumps (`store/threads.rs`, `qml/screens/Panels`): the
  thread chip and panel (root, replies, its own reply field, live replies,
  edits and reactions, reply counts; edit, delete and reactions on a reply
  the open list has not loaded, at once and undone on a refusal, with the
  list's endpoints and permissions), the channel details (the header's
  title key: the member panel at 360 with Members and Threads tabs; the
  Threads tab lists `/threads` with each starter, its reply count and last
  reply, and opens its thread), pin and unpin with the confirm and
  the pinned glyph (Manage messages or more; anyone in a DM), the pins panel
  with `MESSAGE_PIN` / `MESSAGE_UNPIN`, `Chat.jumpToMessage` with `around=`,
  the detached window (newer pages, Jump to present, no read marks until
  the end), and Copy message link.
- Message search (`yap-core` `search.rs`, `store/search.rs`,
  `qml/screens/Panels/Search*`): the header field (`/` focuses it) opens
  the search panel; the web's grammar (`from:`, `mentions:`, `in:`,
  `has:`, `before:`, `after:`, `during:`, `pinned:`, quoted values) as
  chips with suggestions and keyboard picks, sort, paging, results grouped
  by channel with highlighted excerpts, recent searches
  (`search-recent.json`), and a result jumps to its message. Server search
  in a server, channel search in a DM. Closing the panel gives focus back
  to the control that opened it.
- Friends as the web has them (`qml/screens/Friends`, `store/friends.rs`):
  Home with no DM open is the Friends view (Online, All, Pending split into
  Incoming and Sent, Blocked; Add friend by username with the web's copy;
  Accept, Ignore, Cancel and Remove with their confirms; a row opens the
  DM), the Friends row and the Home key count incoming requests, and the
  profile card's Add friend, Accept, Decline, Cancel request, Remove friend,
  Block (with its strip) and Unblock. `/api/friends` loads at sign-in and
  after a fresh gateway session; `RELATIONSHIP_ADD` / `_REMOVE` keep it
  current.
- Creating and joining servers (`qml/screens/Join`, the store in
  `store/reduce/invites.rs`, links in `yap-core` `links.rs`): the rail's +
  opens Create a server (icon uploaded first, then `POST /api/servers`;
  the new server joins the rail and opens). Invite links open the invite
  landing as a dialog (preview, Accept invite, joined, already a member,
  invalid or expired, errors with Try again): from a deep link, or a click
  on our site's `/invite/<code>` link in a message or embed. Our site's
  channel and message links open in the app. Copy link in the tree's
  channel menu copies `<site>/chat/<server>/<channel>` as the web does.
  Leave server is on the server menu for members (not the owner), with a
  confirm: `DELETE /api/servers/{id}/members/@me`, then the server goes as
  on `GUILD_DELETE`. Until the route is deployed, a 404 or 405 says leaving
  isn't available yet; the owner's 403 shows the route's own sentence.
- Voice apps in a call (op 106, `yap-protocol` `voice_app.rs`; the store's
  `store/apps.rs` and `store/reduce/apps.rs`; QML `screens/Apps`):
  - The soundboard from the stage bar: the four built-in tones and the
    server's sounds, play for everyone, preview, the playing rows (pause and
    volume for our own, stop for any), upload (Attach files or owner; audio
    only, 50 MB, `purpose=soundboard`) and delete (our own, or Manage
    server). Clips play locally on every client when the room relays them,
    ours too, never published into the call, as on the web; our own audible
    clip sets op 5's SOUNDSHARE flag. Volume is the clip's volume times the
    soundboard volume setting. Join and leave sounds per server or for all
    servers (Settings, Soundboard), synced to `sound_settings`, at most 3 s.
  - The rest of the web picker: MyInstants (trending and search through
    `/api/myinstants`, played for everyone by its `myinstants.com` URL),
    radio (radio-browser search from the client, no session; a station
    goes to Listen Together), favourites (`/api/myinstants/favorites`, the
    star on every tile), custom sounds kept on this device
    (`soundboard-sounds.json`, 128 KiB at most, relayed as a `data:` URL),
    editing a sound (name, emoji, volume: the server's `PATCH` for its
    sounds, then `soundboard.catalog-updated`), and "Mute soundboard" on a
    person's menu (kept with their voice settings). Tile keys show on hover
    or keyboard focus.
  - The local player is `crates/yap-audio`: symphonia decoding (mp3, AAC/m4a,
    Vorbis, FLAC, WAV, WebM/MKV; Opus through the libopus inside libwebrtc),
    a 48 kHz mixer and `waveOut` on the voice output device.
  - Listen Together: search or a YouTube/Spotify link, queue (append, play
    next, play now), play, pause, seek, skip, remove, clear; the now-playing
    card; Tracks and Collections search, a pasted playlist's preview (Play
    now, Queue), Queue collection (every resolve page, at most 100, one
    import batch), Recently played with Requeue, and radio entries played
    live from the station (a public https stream, never sought). The stream
    (`/api/listen-together/stream`, mp4, Range, the web's
    retry plan) follows the room's anchor and seeks when it drifts more than
    750 ms. Loudness presets as the web's (compressor and limiter,
    approximated; LIBRARIES.md).
  - Activities: the dialog and Daily Word (Wordle) with the NYT puzzle
    (`/api/wordle/today?date=` in New York's date), hard mode, hints, reveal,
    and everyone's progress (`wordle.progress`). Warp Rush is listed but not
    built.
- Deep links on our own scheme, `yap-native://invite/<code>` and
  `yap-native://chat/<server>/<channel>[?message=<id>]`, registered per
  user at startup under `HKCU\Software\Classes\yap-native` (only the
  values that differ are written). A link on the command line, or one a
  second launch hands over, opens in the running app.

The shell has its own identity, apart from the installed Yap app: AUMID
and single-instance name `com.115jon.yap.native` (registered under
`HKCU\Software\Classes\AppUserModelId` only), Run value `YapNative` (off
unless "Open on startup" is on), data and logs under
`%LOCALAPPDATA%\YapNative`, and its own `yap-native://` scheme (never
`yap://`).

## Not yet

Soundboard: station previews, station favicons, the moderator's
per-server soundboard mute (the web has the menu item but never wires it).
Listen Together: recent searches and the result actions menu. Activities
(Daily Word's per-day board, Warp Rush) wait for their rework as web pages.
Camera backgrounds, the stream watcher list and stream
volume, input volume, a mic test outside a call, WebM audio (webm plays
silent), clips in the GIF picker, the favourites tab and GIF categories,
stickers, emoji generation, the reaction details modal, the channel
details' Media,
Links and Files tabs, toast buttons (reply, mark read)
and avatars, call rings, the taskbar thumbnail toolbar and auto-update. In
settings: avatar crop and the recent-avatar picker, display name styles,
bundles (none in the catalog), username changes, sign-out and session revoke from the page,
server profiles, call keybinds, custom
sounds, spatial audio, hardware acceleration and the game capture switch.

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

| env var                    | default                   | meaning                                                                                                                             |
| -------------------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `YAP_API_URL`              | `https://yap.115jon.com`  | API and gateway                                                                                                                     |
| `YAP_KOVA_AUTH_URL`        | `https://auth.115jon.com` | Kova                                                                                                                                |
| `YAP_KOVA_PUBLISHABLE_KEY` | Yap's live key            | Kova app                                                                                                                            |
| `YAP_GATEWAY_ORIGIN`       | `YAP_API_URL`             | `Origin` sent on realtime sockets                                                                                                   |
| `YAP_TOKEN`                | none                      | use this session instead of the credential store                                                                                    |
| `YAP_FAKE_CAMERA`          | unset                     | `1`: a test pattern stands in for every camera                                                                                      |
| `YAP_SHARE_ONLY_OURS`      | unset                     | `1`: share stills and captures only of our own windows (or one titled "Yap Native Share Test"); other windows listed without titles |
| `YAP_FAKE_SHARE_AUDIO`     | unset                     | `1`: share audio is a 440 Hz tone, never a loopback                                                                                 |
| `YAP_PICK_FILES`           | unset                     | `a.png;b.png`: returned by the file picker instead of showing it (tests)                                                            |
| `YAP_EMBED_MUTED`          | unset                     | `1`: every chat player starts muted (tests)                                                                                         |
| `YAP_FAKE_NO_WEBVIEW2`     | unset                     | `1`: WebView2 creation fails for this process, as on a PC without the runtime (tests)                                               |
| `YAP_LOG_EXTERNAL_OPEN`    | unset                     | `1`: links the players would open in the browser are logged as `[external] open <host><path>` instead (tests)                       |
| `YAP_DECOR_IGNORE_FOCUS`   | unset                     | `1`: decorations (webm nameplates) play in an unfocused window too (tests that cannot take focus)                                   |
| `YAP_TRACE_MEDIA_IO`       | unset                     | `1`: log each loopback relay request (range, status, length)                                                                        |

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

- `Theme.qml` holds every token for the six themes. Settings → Appearance
  picks one (or `system`), kept in `appearance.json`; `YAP_THEME` (`dark`,
  `light`, `miku-dark`, `miku-light`, `spiderman-dark`, `spiderman-light`)
  overrides it for a run. Type roles are `Theme.type.*` font values.
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
cpp/webmplayer.*      WebmPlayer: yap-webm frames into a VideoOutput's sink
cpp/authmedia.*       AuthMedia: private attachments through the loopback relay
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
