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
- Instant switching (docs/ARCHITECTURE.md, "As built: switching"): the
  24 channels left last stay warm in memory (6 MB at most) and kept
  current by live events, so going back shows the channel at once, where
  it was left, and refetches it quietly (not at all within 30 s in the
  same gateway session for a server channel). A channel not warm shows
  its window kept on disk while its page loads. A server opens at once on
  the channel opened there last (the last DM for the rail's Home key), its
  tree refetched behind it. A channel row the pointer rests on, and the
  switcher's highlighted row, are fetched ahead. The composer keeps each
  channel's draft and reply, and takes the keys after a switch.
- Keys beyond the web: Alt+Up/Down the previous or next text channel
  (DMs at Home), Alt+Shift+Up/Down the previous or next unread one,
  Ctrl+Alt+Up/Down the previous or next server (Home first), and Esc in
  an empty composer marks the channel read.
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
  Voice settings: microphone, input volume (kept, not applied, as on the
  web), input profile, noise suppression (Off or RNNoise), WebRTC noise
  suppression, echo cancellation, high fidelity, spatial audio, the live
  level meter, speaker and output volume, with the web's coupling (high
  fidelity and spatial audio turn echo cancellation and WebRTC noise
  suppression off, RNNoise turns WebRTC noise suppression off). Kept per
  user in `%LOCALAPPDATA%\YapNative\voice-settings.json`.
- The call's own audio path (`crates/yap-voice/src/engine/pipeline.rs`,
  docs/LIBRARIES.md §11): WASAPI mic, WebRTC's APM, then RNNoise when it
  is on, into a `NativeAudioSource`; every remote audio track into our
  stereo mix with its gain and pan, played through WASAPI and handed to
  the same APM as the echo canceller's far end. Spatial audio as the web
  places people (`src/spatial.rs`: the room's shared state from op 2, 9
  and 15, ours sent in op 15; pan and distance gain on voice tracks only,
  while the room, our switch and high fidelity are on and we are not
  deafened). `YAP_ADM_AUDIO=1` falls back to libwebrtc's device module,
  where remote volume and live APM options go through
  `crates/yap-voice/cpp/track_audio.cpp`.
- The Talk Tree: everyone in every voice channel of the server
  (`VOICE_CHANNEL_STATES` / `_UPDATE`), with guides, the lit path to you,
  speaking LEDs and mute/deafen glyphs.
- The account row, the connection strip, and messages through the web's
  Markdown grammar (`yap-core` `markdown.rs`) as Qt rich text.
- The desktop shell, as the desktop app does it (`cpp/desktopshell.*`,
  `yap-core` `shell.rs`): one instance (a second launch shows the first),
  the tray icon with the unread badge, "Show" and "Quit", close to tray,
  Windows toasts for messages, mentions, replies and DMs (click opens the
  channel; a message toast has an inline Reply box with Send, and Mark as
  Read, which work with the window hidden in the tray: `ToastReply` posts
  as a reply to the toasted message, open channel or not), the taskbar
  flash, the saved window rect, start minimized and open on startup, the
  web's UI sounds (`synth.rs`, played through `waveOut`), and a rotating
  log file.
- The taskbar thumbnail toolbar (`cpp/thumbbar.*`, the desktop app's
  `thumbnail_toolbar.rs` and icons): camera, mic, deafen and disconnect
  under the window's preview while in a call or a DM call, plus
  play/pause and skip while Listen Together has a track; hidden
  otherwise. The icons show the state, the tooltips the next action.
- Crash reports: minidumps through `crash-handler` and `minidump-writer`
  (`rust/src/crash.rs`) into the profile's `crashes` folder
  (`%LOCALAPPDATA%\YapNative[\profiles\<name>]\crashes`, the newest 10
  kept, nothing uploaded). The next launch shows "Yap Native closed
  unexpectedly" with Open folder (`cpp/crashnotice.*`) until dismissed.
  `YAP_TEST_CRASH=1` crashes on purpose 4 s after start.
- Compatibility guards (`cpp/compat.*`, `cpp/mediafoundation.*`,
  docs/LIBRARIES.md §14): the adapter, driver and feature level in the log;
  GPU safe mode (WARP, no GPU decode or encode) below feature level 11_0,
  after a scene graph error (the app restarts itself in it), after two
  starts in a row that drew nothing, or with `--gpu-safe-mode`; a lost
  device replaced in place (window and decoders); and Windows N/KN without
  the Media Feature Pack: Media Foundation is delay-loaded, mp4 playback,
  H.264 and cameras turn off, and a notice says why.
- The game capture hook (`crates/yap-capture/src/hook`, the desktop app's
  `game_capture` ported): a shared window is also tried with OBS Studio's
  own signed `graphics-hook`, behind Settings > Desktop app > Game capture
  hook (on by default, `gameCaptureHook`). The blocklist, the web app's
  remote blocklist, the learned skips and OBS's `OpenProcess` preflight
  run before anything is injected; the six files pinned in
  `desktop/packaging/obs-signed-hook.lock.json` are verified (SHA-256,
  Authenticode, signer) and staged under the profile's `capture-hook`
  folder. WGC keeps running underneath and carries the share whenever the
  hook is not delivering. DX11 and DX12 games; Vulkan only when an
  OBS-signed hook is already resident (no Vulkan layer is registered).
  `cargo run -p yap-capture --example hook_probe` checks it live against
  `hook_target`, a D3D11 window the probe starts itself.
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
- The chat, REST and settings extras of batch N3 (`yap-core`
  `store/extras.rs` and `command_menu.rs`, `custom_sounds.rs`; the
  `Extras`, `CommandMenu` and `CustomSounds` objects):
  - the header bell's inbox (NotificationBell.tsx): the 50 newest, live
    `NOTIFICATION_CREATE` / `_READ` / `_CLEAR`, open a row (it reads and
    jumps to the message), Mark all read and Clear all (it asks first);
  - the Ctrl+K quick switcher (CommandMenu.tsx), also from the DM
    sidebar's finder: Recent (`quick-switcher.json`) and Unread while idle,
    DMs, text and voice channels, servers and the actions (mute, deafen,
    the theme), cmdk's ranking and the `@ # ! *` prefixes, and the hand-off
    to message search;
  - who reacted (ReactionDetailsModal.tsx) from "View reactions" or a
    right click on a chip, with a moderator's remove;
  - the drop overlay (DragDropOverlay.tsx) over the existing upload path;
  - the channel details' Media (a gallery into the image viewer), Links
    and Files tabs;
  - a voice channel's status (text and a GIF picture, with the voice
    session header) and its line under the channel in the tree;
  - the avatar picker's recent avatars (pick one again, delete one);
  - the emoji picker's AI creations and composer (`/api/emojis`);
  - platform glyphs beside names in the member list and on the profile
    card;
  - the shop's catalog refresh (the server allows its catalog admins only
    and says so to everyone else);
  - Mark unread, Share message (expiry, the link, Manage shares) and View
    reactions on the message menu;
  - sensitive media blurred under the high media filter only, behind the
    SensitiveMediaFrame float;
  - Account: username edits with the availability check, and legacy
    account claims; Sessions: Sign out on another device (the route
    answers 501 today, which the row shows);
  - Invite to channel (ChannelInviteModal.tsx): a fresh 7-day link to the
    channel, from the row's key or its menu;
  - custom UI sounds (CustomSoundsCard.tsx): import one or several files,
    preview, reset, under `%LOCALAPPDATA%\YapNative\custom-sounds`;
  - Listen Together results' action menu, the voice switch confirm (with
    Don't ask again) and the stream watcher list on share tiles.
- The voice core and 1:1 calls of batch N4 (`yap-voice` `machine.rs`,
  `machine/health.rs`, `driver.rs`; `yap-core` `store/calls.rs` and
  `store/reduce/calls.rs`; the `Calls` object; QML `screens/Calls`). The
  wire details are in [VOICE_PROTOCOL.md](VOICE_PROTOCOL.md):
  - DM calls as the web rings them: the DM header's call key (Join call
    while it has someone in it), Start call on a DM row's menu (it asks
    first, with Don't ask again), the voice switch confirm when in a voice
    channel, the incoming call card (top right, while that DM is not open:
    Accept or Decline, Escape declines), the call area over the DM's
    messages (people as call tiles with the ringing ring and LED words, the
    lobby keys: Cancel call, Join with video, Join voice, Decline; in the
    call its tiles and the dock: camera, share, Leave), the ring, outgoing
    ring, connect and end tones (a new Calls sound category), the incoming
    call toast, the busy auto-decline, the server's timeout and every ring
    stop reason; "Call with …" on the call strip;
  - voice moves: "Move to…" on a call tile's or Talk Tree leaf's menu for
    moderators, and following a move (toast, leave, join the target with
    its grant);
  - pull self-healing (op 109 out, op 110 republishes), the SFU session
    rebuild on a failed peer connection, releasing video pulls nobody
    shows (pulled again on a video session of their own), and the hidden
    standby on an occupied voice landing (Join promotes it; off with
    `YAP_VOICE_STANDBY=off`);
  - sticker reactions: the dock's GIF & sticker reactions key opens the
    GIF picker in react mode (Single, Burst, Rain, Bounce; 5 at once, then
    one every 3 s, "Slow down!"), and the overlay plays every sticker the
    room relays;
  - the voice details (the connection word in the voice header and the DM
    call dock): transport states, quality, the small multiples, the SFU
    edge, loss, remote tracks, our outgoing video, Copy stats;
  - the audio sentinels' "We can't hear you" banner, and the disconnect
    beacon on quit;
  - mutual servers on the profile card, a 4th quick reaction on the
    message menu (the hover bar keeps 3), error toasts that stay until
    dismissed, the web's message row spacing, and the voice view's header
    and dock as the web has them;
  - the server menu opens at the pointer, 205 wide, as on the web (the
    keyboard opens it under the header).
- Deep links on our own scheme, `yap-native://` (the table and the strict
  rules are in `crates/yap-core/src/links.rs`): `invite/<code>`,
  `chat/@me`, `chat/@me/<dm>`, `chat/<server>`,
  `chat/<server>/<channel>[?message=<id>]` (a voice channel opens its
  panel, never joins), `settings[/<panel>][?row=<row>]` and
  `signin[?code=XXXX-XXXX]` (brings the window up and polls a waiting
  device sign-in at once; it never carries a token). Anything else is
  refused. A link on the command line, or one a second launch hands over
  through the instance pipe, opens in the running app and brings it to the
  front. The scheme lives under `HKCU\Software\Classes\yap-native` and
  belongs to the released install (`YapNative.App`): its install and update
  hooks write it, its uninstall hook removes it, and it rewrites what
  differs at every default-profile start. No other build or profile writes
  it (docs/INSTALLER.md). The web's /activate page opens
  `yap-native://signin?code=…` after approving a "Yap Native" device code.

The shell has its own identity, apart from the installed Yap app: AUMID
and single-instance name `com.115jon.yap.native` (registered under
`HKCU\Software\Classes\AppUserModelId` only), Run value `YapNative` (off
unless "Open on startup" is on), data and logs under
`%LOCALAPPDATA%\YapNative`, and its own `yap-native://` scheme (never
`yap://`).

## Not yet

Soundboard: station previews, station favicons, the moderator's
per-server soundboard mute (the web has the menu item but never wires it).
Listen Together: recent searches. Activities
(Daily Word's per-day board, Warp Rush) wait for their rework as web pages.
Camera backgrounds, stream
volume, applying input volume (the web does not either), a mic test outside a call, WebM audio (webm plays
silent), clips in the GIF picker, the favourites tab and GIF categories,
stickers, the avatar picker's GIF tile and the voice status picture's
upload (GIFs and recent pictures only), toast avatars, toast actions
from Action Center once the app has quit (no COM activator), the game
capture hook's Vulkan layer, and auto-update. In
settings: avatar crop, display name styles,
bundles (none in the catalog), sign-out from the page,
server profiles, call keybinds,
the spatial audio panel (the room map: placement, room size, distance, arc,
dragging seats; `UserCommand::SetSpatialRoom` is ready for it), a stereo
mic for high fidelity, and hardware acceleration.

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

## Installer

`tools/package.ps1` builds the release exe and packs it with Velopack
(`vpk`) into a per-user `YapNative.App-win-Setup.exe`: no admin, no .NET at
install time, installed to `%LOCALAPPDATA%\YapNative.App` (never the data
root `%LOCALAPPDATA%\YapNative`, which uninstall keeps). The version is
`[workspace.package]` in `rust/Cargo.toml`, which CMake reads too.

```powershell
./tools/package.ps1            # unsigned, into build/installer
./tools/package.ps1 -Sign      # Authenticode through Azure Artifact Signing
```

Design, hooks, parity with the Tauri app's installer, signing and CI
(`.github/workflows/desktop-native-release.yml`, manual only) are in
[docs/INSTALLER.md](docs/INSTALLER.md).

## Run

```powershell
$env:QT_FORCE_STDERR_LOGGING = "1"   # see Qt warnings (WIN32 app)
./build/Release/YapNative.exe
```

| env var                         | default                   | meaning                                                                                                                                                                                                                        |
| ------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `YAP_API_URL`                   | `https://yap.115jon.com`  | API and gateway                                                                                                                                                                                                                |
| `YAP_KOVA_AUTH_URL`             | `https://auth.115jon.com` | Kova                                                                                                                                                                                                                           |
| `YAP_KOVA_PUBLISHABLE_KEY`      | Yap's live key            | Kova app                                                                                                                                                                                                                       |
| `YAP_GATEWAY_ORIGIN`            | `YAP_API_URL`             | `Origin` sent on realtime sockets                                                                                                                                                                                              |
| `YAP_TOKEN`                     | none                      | use this session instead of the credential store                                                                                                                                                                               |
| `YAP_PROFILE`                   | unset                     | a profile name (`[a-z0-9-]{1,32}`; also `--profile <name>`): a second, separate copy of the app, see "Profiles"                                                                                                                |
| `YAP_FAKE_CAMERA`               | unset                     | `1`: a test pattern stands in for every camera                                                                                                                                                                                 |
| `YAP_SHARE_ONLY_OURS`           | unset                     | `1`: share stills and captures only of our own windows (or one titled "Yap Native Share Test"); other windows listed without titles                                                                                            |
| `YAP_FAKE_SHARE_AUDIO`          | unset                     | `1`: share audio is a 440 Hz tone, never a loopback                                                                                                                                                                            |
| `YAP_PICK_FILES`                | unset                     | `a.png;b.png`: returned by the file picker instead of showing it (tests)                                                                                                                                                       |
| `YAP_EMBED_MUTED`               | unset                     | `1`: every chat player starts muted (tests)                                                                                                                                                                                    |
| `YAP_FAKE_NO_WEBVIEW2`          | unset                     | `1`: WebView2 creation fails for this process, as on a PC without the runtime (tests)                                                                                                                                          |
| `YAP_LOG_EXTERNAL_OPEN`         | unset                     | `1`: links the players would open in the browser are logged as `[external] open <host><path>` instead (tests)                                                                                                                  |
| `YAP_DECOR_IGNORE_FOCUS`        | unset                     | `1`: decorations (webm nameplates) play in an unfocused window too (tests that cannot take focus)                                                                                                                              |
| `YAP_TRACE_MEDIA_IO`            | unset                     | `1`: log each loopback relay request (range, status, length)                                                                                                                                                                   |
| `YAP_TRACE_HTTP`                | unset                     | `1`: log each API request as it answers (`[http] GET /api/... reason=open\|revalidate\|prefetch\|window\|other N ms ok`) and each disk window read                                                                             |
| `YAP_TEST_SWITCHES`             | unset                     | `<ms>:<kind>[:<id>],...`: navigation commands sent after each delay (`c` channel, `s` server, `h` Home key, `p` prefetch, `n`/`b`/`u` next/previous/unread channel, `v`/`w` next/previous server), for switching tours (tests) |
| `YAP_DEBUG_SDP`                 | unset                     | `<dir>`: each SDP the call sets is written there (holds ICE credentials: a private dir), and the transceivers are logged (tests)                                                                                               |
| `YAP_TEST_DROP`                 | unset                     | `a.png;b.png`: dropped once onto the first open chat (the overlay shows 4 s first), instead of a real drag (tests)                                                                                                             |
| `YAP_TEST_CRASH`                | unset                     | `1`: crash on purpose 4 s after start (an access violation), to check the minidump and the next launch's notice                                                                                                                |
| `YAP_OBS_SIGNED_DIR`            | next to the exe           | the six OBS-signed hook files (default `obs-capture\obs-signed` beside `YapNative.exe`)                                                                                                                                        |
| `YAP_CAPTURE_HOOK_DIR`          | profile `capture-hook`    | where the verified hook files are staged                                                                                                                                                                                       |
| `YAP_TEST_TOAST_ACTION`         | unset                     | `reply:<text>` or `mark-read`: 3 s after a message toast shows, its button runs through the same handler a click would (tests)                                                                                                 |
| `YAP_TEST_TRAY_QUIT_AFTER_SECS` | unset                     | `<n>`: `n` s after launch, the tray menu's Quit runs through the same dispatch as a click (tests)                                                                                                                              |
| `YAP_HOOK_DEBUG`                | unset                     | `1`: the hook's per-frame IPC detail in the log                                                                                                                                                                                |
| `YAP_NO_CLIPBOARD`              | unset                     | `1`: copies are logged (`[clipboard] would copy N characters`), never put on the clipboard (tests)                                                                                                                             |
| `YAP_VOICE_STANDBY`             | on                        | `off`: no hidden standby on an occupied voice landing (VOICE_PROTOCOL.md "Standby")                                                                                                                                            |
| `YAP_TEST_PC_FAIL_AFTER_SECS`   | unset                     | `<n>`: `n` s after a call's media first connects, it sees its peer connection fail once (the SFU session rebuild, tests)                                                                                                       |
| `YAP_TEST_OFFLINE`              | unset                     | `1`: every request of this process fails as with no network (DNS); the gateway cannot connect (yap-core `faults.rs`, tests)                                                                                                    |
| `YAP_TEST_UNREACHABLE`          | unset                     | `1`: every request to the API host fails as a refused connection; the network is up (tests)                                                                                                                                    |
| `YAP_TEST_HTTP_STATUS`          | unset                     | `[METHOD ]<path substring>[$]=<status>[,...]` (`$`: path ends so): matching API requests answer that status and the server's error text, unsent (tests)                                                                        |
| `YAP_TEST_VOICE_FAIL`           | unset                     | `admission` (the join's admission answers 500) or `ice` (the media connection never comes up): a call join fails there (tests)                                                                                                 |
| `YAP_ADM_AUDIO`                 | unset                     | `1`: libwebrtc's device module records and plays (mixed playout; no RNNoise, no panning) instead of the own audio path                                                                                                         |
| `YAP_FAKE_MIC`                  | unset                     | `<file.wav>` (48 kHz 16-bit): the mic, looped and paced by the clock (tests)                                                                                                                                                   |
| `YAP_FAKE_MIC_ECHO`             | unset                     | `<gain>`: with `YAP_FAKE_MIC`, our own speaker mix is added to the mic 60-90 ms late at this gain, as a room's echo (tests)                                                                                                    |
| `YAP_NULL_SPEAKER`              | unset                     | `1`: the mix plays to no device (paced by the clock; the echo canceller is still fed) (tests)                                                                                                                                  |
| `YAP_MIC_RAW_TAP`               | unset                     | `<file.wav>`: records the mic as it comes in (before the APM), up to 120 s (tests)                                                                                                                                             |
| `YAP_MIC_TAP`                   | unset                     | `<file.wav>`: records the mic as it is sent (after the APM and RNNoise), up to 120 s (tests)                                                                                                                                   |
| `YAP_RENDER_TAP`                | unset                     | `<file.wav>`: records the stereo speaker mix as it is played, up to 120 s (tests)                                                                                                                                              |
| `YAP_TEST_NO_MF`                | unset                     | `1`: Media Foundation counts as missing, as on Windows N without the Media Feature Pack: no mp4 playback, no H.264, no cameras, the notice (tests)                                                                             |
| `YAP_GPU_SAFE_MODE`             | unset                     | `1` (or `--gpu-safe-mode`): this run draws with WARP and keeps every GPU user of ours off (`--no-gpu-safe-mode` clears a saved safe mode)                                                                                      |
| `YAP_TEST_GPU_FAIL`             | unset                     | `1`: 2 s after the first frame the scene graph error handler runs: safe mode is saved and the app restarts itself in it (tests)                                                                                                |
| `YAP_TEST_DEVICE_LOSS`          | unset                     | `<n>`: `n` s after the first frame Qt's QRhi reports its device lost, as after a TDR; the device is replaced in place (tests)                                                                                                  |
| `YAP_TEST_STARTUP_CRASH`        | unset                     | `1`: the process ends after the crash guard counted the start, before the window shows; two in a row give safe mode (tests)                                                                                                    |

The defaults mirror `src/lib/site-defaults.ts`.

## Profiles

`YAP_PROFILE=<name>`, or `--profile <name>` (or `--profile=<name>`) on the
command line, runs a named profile: a second copy of the app, signed in on
its own, that never touches the default one or another profile. This is how
you run it as yourself while agents drive a test copy. The switch wins over
the variable. A name is `[a-z0-9-]{1,32}`; anything else (including a bad
`--profile` value) is ignored and the default profile runs. With no profile
nothing changes.

```powershell
./YapNative.exe --profile jon          # or: $env:YAP_PROFILE = "jon"
```

| what                                                  | default profile                                           | profile `<name>`                                          |
| ----------------------------------------------------- | --------------------------------------------------------- | --------------------------------------------------------- |
| data folder (cache, settings, sounds, logs, WebView2) | `%LOCALAPPDATA%\YapNative`                                | `%LOCALAPPDATA%\YapNative\profiles\<name>`                |
| credential                                            | Credential Manager, service `com.115jon.yap.native-spike` | the same service with `:<name>` appended                  |
| instance lock and pipe                                | `...native.instance`, `...native.<session>`               | `...native.instance.<name>`, `...native.<name>.<session>` |
| AUMID (toasts)                                        | `com.115jon.yap.native`, "Yap Native"                     | `com.115jon.yap.native.<name>`, "Yap Native (`<name>`)"   |
| tray window class                                     | `YapNativeTrayWindow`                                     | `YapNativeTrayWindow-<name>`                              |
| tray tooltip, title                                   | "Yap Native", the location                                | "Yap Native (`<name>`)", the location with `(<name>)`     |
| Run (autostart) value                                 | `YapNative`                                               | `YapNative-<name>`, launching with `--profile <name>`     |
| `yap-native://` scheme                                | registered by the released install only                   | never registered                                          |
| Qt `applicationName`                                  | `YapNative`                                               | `YapNative-<name>` (Qt's own cache folders)               |

Two profiles can run at once; a second launch of the same profile shows the
first one's window. `YAP_TOKEN`, when set, overrides the credential for any
profile, so leave it unset for a signed-in profile. The profile rules live in
`cpp/profile.*` and `rust/crates/yap-cache/src/profile.rs` and must agree.

### Testing notes (agents)

Agents run the test account as profile `spike`, with the session from
`F:/ct/yapn/secrets/owner.json` (`peer.json` for a second account) in
`YAP_TOKEN`, so nothing signs in anew and no credential is stored:

```powershell
$env:YAP_TOKEN = (Get-Content -Raw F:/ct/yapn/secrets/owner.json | ConvertFrom-Json).tok
./YapNative.exe --profile spike
```

The default profile, and with it the `yap-native://` scheme and the
`YapNative` Run value, belongs to the user's installed copy: agents never
launch it without `--profile`. To test deep links, point a scheme of your
own at the build (`HKCU\Software\Classes\yap-native-test`, open command
`"<exe>" --profile <name> "%1"`), run with `YAP_TEST_SCHEME=yap-native-test`
(read as `yap-native://`) and `YAP_DEBUG_NO_ACTIVATE=1` (links reveal the
window without taking the focus), and delete the key afterwards. Never print the token files. An installer
test uses its own pack id and title (`-PackId YapNative.InstallTest -Title
"Yap Native InstallTest"`), launches with `--profile spike`, and is
uninstalled afterwards (docs/INSTALLER.md, "Local test").

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
- Text draws as Chromium draws it on Windows: DirectWrite with no hinting
  (natural symmetric, subpixel glyph positions) at every scale, ClearType
  subpixel coverage blended through Skia's per-colour contrast and gamma
  correction (Qt patch `patches/qt/6.11.3/0006`), grayscale inside layers,
  under a scale or rotation, and when ClearType is off.
  `YAP_TEXT_HINTING=none|vertical|full|default` forces a hinting.
  `cpp/main.cpp` sets the contrast and gamma as Chromium picks them
  (contrast 1.0 and the sRGB curve, or the ClearType Tuner's values for the
  primary monitor when it has saved some); `QSG_TEXT_CONTRAST=<k>`,
  `QSG_TEXT_GAMMA=<g>` (0: sRGB), `QSG_TEXT_GRAYSCALE=1` and
  `QSG_TEXT_PREBLEND=0` (Qt's raw blend) override them.
- `cpp/wheelscroll.cpp` scrolls every Flickable as Chromium does: 100 px a
  notch (Windows' 3 lines), eased over 150 ms, touchpads 1:1.
  `YAP_QT_WHEEL=1` puts back Qt's own wheel handling.
- `YAP_DEBUG_DROP_AFTER_SECS=<n>` drops the gateway `n` s after it is
  ready; with `YAP_DEBUG_OFFLINE_SECS=<m>` the process then stays offline
  for `m` s (its sockets and requests fail), for the offline states.

## Layout

```
CMakeLists.txt        exe, static Qt plugin imports, cxx-qt crate import
cpp/main.cpp          QGuiApplication, fonts, icon provider, theme, startup timing
cpp/windowchrome.*    custom title bar: caption removal, hit testing, snap layouts
cpp/iconprovider.*    Phosphor icons, tinted and cached per size and colour
cpp/desktopshell.*    one instance, tray and badge, toasts, Run key, window rect
cpp/toast_win.*       WinRT toasts with Reply and Mark as read (WRL, no Qt headers)
cpp/thumbbar.*        taskbar thumbnail toolbar (ITaskbarList3) in calls
cpp/crashnotice.*     minidump handler install and the next launch's crash notice
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
rust/src/crash.rs     minidumps on a crash (crash-handler + minidump-writer)
rust/crates/yap-capture/src/hook/  game capture hook (OBS-signed graphics-hook)
rust/src/installer.rs Velopack startup and install/uninstall hooks (docs/INSTALLER.md)
tools/package.ps1     release build + vpk pack into Setup.exe (optionally signed)
rust/src/video.rs     video frames to the hub; render demand to the call
rust/src/sound.rs     UI sounds through waveOut
rust/qml/             UI: Main, SignInView, ServerRail, ChannelSidebar, ChatView, ...
```
