# Performance pass (2026-11-06)

This pass measures the native client against the Tauri desktop app and
against its own video-phase numbers. It then fixes the hot spots in this
order: idle CPU, memory, scroll jank and startup.

- Base: the tree at 6dae4cc8, rebuilt in `<scratch>/yapn/rel` (the exe there
  was older than HEAD).
- After: the same tree with the fixes below (build "fix5").
- Machine: the user's PC with an RTX 3080 and three 1080p60 monitors,
  running Windows 11 26200.

## Method

All runs use the release build. They use `--profile jontest` (the user's
real account) unless noted otherwise. Calls use `--profile spike`, the test
account.

### Tools

The tools live in `<scratch>/yapn/tools`. Every script that stops a process
does so by PID, and only a process it started.

| tool                         | what it measures                                                                                                                                                                                                                                                           |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `measure-native.ps1`         | Three modes. `-Mode start` launches N times and reads the `[startup]` lines. `-Mode sample -TargetPid` reports CPU %, working set, private bytes, threads, handles and GPU engine use (Task Manager's `\GPU Engine(pid_N_*)` counters). `-Mode frames` parses frame times. |
| `threads-cpu.ps1`            | CPU per thread, with thread names.                                                                                                                                                                                                                                         |
| `threads-origin.ps1`         | Threads grouped by start module.                                                                                                                                                                                                                                           |
| `vmmap.ps1`                  | Committed memory by type, and the largest private allocations.                                                                                                                                                                                                             |
| `winpid.ps1`                 | Posts close-to-tray, minimise and restore, and moves our window without activating it.                                                                                                                                                                                     |
| `tour.ps1`, `tour-split.ps1` | The memory tour (posted clicks only, read-only on the account).                                                                                                                                                                                                            |
| `qmlprof-session.ps1`        | Records QML profiler traces for startup, idle and scroll.                                                                                                                                                                                                                  |
| `vblank.ps1`                 | Real vblank rate of a display (`D3DKMTWaitForVerticalBlankEvent`).                                                                                                                                                                                                         |

### New in the app

- `[startup] fonts after` and `[startup] qml loaded after` markers
  (`cpp/main.cpp:273`, `:344`).
- An opt-in profiling build: CMake `-DYAP_QML_PROFILER=ON`
  (`CMakeLists.txt:111`). It links the static QML profiler services.
- `YAP_DEBUG_ANIMATIONS=N` (`cpp/main.cpp:170`). Every N seconds it lists the
  running QML animations, the `AnimatedPicture`s still ticking, the app
  state and the focus window.
- An `[anim]` line for each decode, giving its size and the cache total
  (`cpp/animatedpicture.cpp`).

### How each area was measured

- **Startup.**
  - Five or six launches; run 1 is the first launch after a rebuild ("cold").
  - The `[startup] cached content`, `messages shown` and `gateway ready`
    marks count from the core's start, which is `qml loaded` (about 0.45 s)
    after launch. `first frame` counts from process start.
  - The cached content is already in the first frame (it is ready 5–10 ms
    after the core starts), so "first content" equals "first frame".
- **Idle.**
  - Sampled 60 s after a 50–60 s settle, in three states: window visible,
    minimised (`SC_MINIMIZE`, which really deactivates the window), and
    hidden to the tray (`WM_CLOSE`).
  - A window launched by the harness never becomes foreground, but Qt still
    reports it as active. "Visible" samples are therefore the focused case.
    Real unfocus goes through the same `ApplicationInactive` path that the
    minimise sample exercises.
- **Frame timing.**
  - `QSG_RENDER_TIMING=1` and the render thread's
    `syncAndRender ... elapsed since last call` intervals.
  - Scroll: 200 wheel ticks up and 200 down at 16 ms (about 7,500 px/s),
    with the window on an uncovered monitor and the displays awake.
  - While the displays slept (vblank waits returned at once), DWM paced
    every present at about 4 Hz (`swap≈250 ms`). Samples from that time
    count frames and work, not on-screen fps.
- **QML profiler.**
  - Traces of startup, 20 s idle and scroll on the profiling build, parsed
    with the qt-qml-profiler skill's parser.
  - `QSG_RENDERER_DEBUG=change` showed which scene-graph nodes are dirtied
    each frame.
  - ETW/WPR was not used: the static exe has no PDB, so its stacks would be
    meaningless.
- **Memory tour.**
  - Ten rounds across 3 servers and #general/#test, the Shop, a DM, Settings
    and the emoji picker (75 navigations), then a 30 s settle.
  - `tour-split.ps1` repeats each feature 10 times on its own.
  - Every voice row is avoided: no call is ever made on the user's account.
- **Calls.**
  - Test account in Native Spike Lab › General; headless Edge peer
    (`peer-video.mjs`); every call under 2 minutes.
  - `YAP_FAKE_MIC=<synthetic wav>` and `YAP_NULL_SPEAKER=1` keep the room
    off the mic and the tones off the speakers. The own pipeline (APM,
    RNNoise, mix, Opus) runs; the WASAPI device I/O is the part replaced.
  - RNNoise on vs off comes from both the process sample and the pipeline's
    own per-frame timers (`[voice] stats ... rnnoise=`).

## Findings and fixes

### 1. Idle CPU: a 100 Hz WebRTC thread from sign-in

**Finding.** Signing in applied the saved audio devices
(`audio.rs choose_devices`), which created the `PeerConnectionFactory`.

- Its `SyntheticAudioDevice` (webrtc-sys) runs a 10 ms repeating task from
  `Init()` until the factory dies, whether it plays or not.
- With it came three more WebRTC threads.
- Cost: 0.72 % of a core in the tray, before any call.

**Fix.** The factory is made by the first call and never earlier.

- `choose_devices` only records the choice when there is no factory; the
  call applies it on open (`yap-voice/src/engine/webrtc.rs:364`).
- `check_devices` takes the same guard (`:836`).
- `devices()` lists endpoints through WASAPI. It returns the same ids
  (`IMMDevice::GetId`, which the own path already opens) and names
  (`PKEY_Device_FriendlyName`) as the ADM (`wasapi.rs:437`, `webrtc.rs:340`).
- The factory logs `[voice] webrtc factory created`.

### 2. Idle CPU: ten stuck animations and a 60 Hz animation timer

**Finding.** `YAP_DEBUG_ANIMATIONS` found ten animations running forever:
nine `Behavior on opacity { OpacityAnimator }` in `Led.qml` and one in
`UnsavedBar.qml`.

- They sit inside closed popups (menu rows, settings switches) whose items
  have no window.
- An Animator started there never completes. It keeps the QML animation
  timer ticking, and with it a frame request every vsync.

**Fix.** All 45 `Behavior { OpacityAnimator | ScaleAnimator }` became
`NumberAnimation`, across 30 files (for example `components/Led.qml:23`).
Behaviors run for 100–300 ms, so moving them to the GUI thread costs
nothing measurable. Afterwards: `0 running`.

### 3. Animated content kept playing in the background

**Finding.** Several things played or decoded regardless of app state.

- `EmbedVideo` GIF-mode loops (X/Klipy mp4 GIFs) kept a Media Foundation
  player decoding while the app was unfocused, hidden or minimised.
- `MediaTile` and `VoiceStatusLine` GIFs played whenever the item was
  visible, including rows in the list's cache buffer.

**Fix.** These now pause when the app is not active (unfocused, hidden or
minimised), as decorations, server GIFs and webm already did.

- `EmbedVideo.qml:88`; a loop made while the app is inactive only peeks its
  first frame.
- `MediaTile.qml:78`, `VoiceStatusLine.qml:57`.

**Web rule.** `useAppActive` keeps an unfocused window active unless it is
also occluded. Native has no occlusion signal, so it pauses on unfocus.
That is stricter than the web, and it is the rule the brief asks for.

### 4. Memory: hidden UI decoding, players kept alive, cached GIF frames

Four sources:

- **`AnimatedPicture` decoded hidden items.** It decoded on
  `componentComplete`, even inside closed popups and pages: the profile
  editor's preview and shop cards. Now:
  - it decodes only when visible in a window (`animatedpicture.cpp:294`);
  - it hands its frames back when hidden (`:457`);
  - the unreferenced-frames cache dropped from 96 MB to 32 MB (`:39`).
- **Every video preview kept a player.** Each preview (an attachment or a
  direct video embed) built a Media Foundation player to peek at its first
  frame, then kept it paused for the life of the row.
  - Now the frame is grabbed (at device pixels) and the player goes until
    Play (`EmbedVideo.qml:116`, `:269`).
  - This is what Chromium's media suspend does for a `<video preload>`.
- **GIF tiles cached every frame.** `AnimatedImage { cache: true }` in chat
  GIF tiles and the GIF picker kept every decoded frame (QMovie `CacheAll`)
  for as long as the row lived, pooled rows included.
  - Now frames decode as they show, as a browser does (`MediaTile.qml:72`,
    `GifPicker.qml:314`).
- **The GIF picker laid out emoji at startup.** Its React-mode row (labels
  with emoji) is now made in voice mode only (`GifPicker.qml:199`, see
  startup).

### 5. Scroll jank on rich history

**Finding.** In the profiler, a message row with embeds costs 6–8 ms to make:

- `MessageDelegate.qml:137` and `:513`, about 3 ms each;
- `EmbedCard.qml:40`, about 2 ms per card.

A fast wheel scroll made several rows inside one frame.

**Fix.** `cacheBuffer: 1200` (`ChatView.qml:320`; the default is 320 px).
Rows within 1,200 px are incubated across frames before they show.

### 6. Startup

**Finding.**

- The first emoji the app lays out pays Qt's font-fallback lookup, about
  250 ms. It was the GIF picker's React-mode labels, built eagerly with the
  composer.
- About 19,700 QML objects are made at startup. Most belong to closed
  popups: Settings, ProfileEditor, menus, pickers.

**Fix.** The picker row is made lazily (see 4). The eager popups are left
for later (see Remaining gaps).

## Before and after (native)

"Video phase" is the earlier native measurement. Before = 6dae4cc8; after =
this pass.

### Startup, idle and memory

| metric                                                      | video phase             | before                               | after                                 |
| ----------------------------------------------------------- | ----------------------- | ------------------------------------ | ------------------------------------- |
| first frame, warm (p50 of runs 2–6)                         | 480–730 ms              | 1,031 ms                             | **912 ms**                            |
| first frame, cold (run 1, after a rebuild)                  | n/a                     | 1,351 ms                             | 1,160 ms                              |
| cached first content (in the first frame)                   | ~0.7 s                  | ~1.03 s                              | **~0.91 s**                           |
| `qml loaded` after launch                                   | n/a                     | ~460 ms (from log stamps)            | 447–465 ms                            |
| gateway ready, from core start                              | 1.9–3.7 s (from launch) | 0.89–1.33 s                          | 0.48–1.03 s                           |
| idle, tray, CPU                                             | 0.00 %                  | 0.72 %                               | **0.03 %**                            |
| idle, minimised, CPU                                        | n/a                     | not measured                         | **0.10 %**, GPU 0                     |
| idle, visible + focused, text-only fixture                  | 0.00 %                  | not measured                         | **0.08 %**, GPU 0                     |
| idle, visible + focused, #general with GIFs and decorations | n/a                     | not measured (displays asleep)       | 10.4–10.8 %, GPU 7.5–9 % (~50 fps)    |
| idle, visible, covered window, displays asleep              | n/a                     | 0.92 %, GPU 0.7 %, 4 fps             | n/a (asleep only during the base run) |
| idle working set / private, #general                        | 84 / 100 MB             | 319 / 332 MB                         | **232 / 249 MB**                      |
| idle working set / private, chat-10k fixture                | 84 / 100 MB             | n/a                                  | 219 / 242 MB                          |
| threads / handles, idle                                     | 91 / n/a                | 101 / 1,221                          | 98 / 1,275                            |
| WebRTC threads before any call                              | n/a                     | 4 (`SyntheticAudioDevice` at 100 Hz) | **0**                                 |
| QML animations running at idle                              | 0                       | 10 (60 Hz timer)                     | **0**                                 |

### Memory tour and scroll

| metric                                                | video phase | before                                                                  | after                                              |
| ----------------------------------------------------- | ----------- | ----------------------------------------------------------------------- | -------------------------------------------------- |
| memory tour, private: start → peak → after 30 s       | n/a         | 339 → 604 → 523 MB (fix4)                                               | **303 → 503 → 493 MB**                             |
| tour per feature (10×)                                | n/a         | settings +26, emoji +3, shop −25, 30 channel switches +334 MB (warm-up) | server switches after warm-up: −8 MB               |
| scroll, real history (frame interval p50/p95/p99/max) | n/a         | 16/36/58/164 ms (fix4)                                                  | 16/33/65/**78** ms                                 |
| scroll frames > 33 ms / > 50 ms                       | n/a         | 45 / 24 of 891                                                          | 43 / 19 of 897                                     |
| CPU during that scroll                                | n/a         | 38 %                                                                    | **24 %**                                           |
| scroll, chat-10k fixture                              | 60 fps      | n/a                                                                     | p95 17, max 32 ms, 0 frames > 33 ms (60 fps holds) |
| animated cache after a startup                        | n/a         | 69 MB (offscreen editor previews included)                              | decorations 2 MB + GIF tiles in view               |

### Calls and size

| metric                                  | video phase       | before                        | after                                                             |
| --------------------------------------- | ----------------- | ----------------------------- | ----------------------------------------------------------------- |
| in a call, audio only, RNNoise on       | 7.2 % (Opus only) | n/a                           | 9.5 %, GPU 4.7 %                                                  |
| in a call, audio only, RNNoise off      | n/a               | n/a                           | 11.6 % (process noise ±2 %)                                       |
| RNNoise cost, pipeline timer            | n/a               | n/a                           | 49 µs per 10 ms frame = **0.5 % of a core** (APM 90 µs, mix 5 µs) |
| watching one camera (640×360 at 15 fps) | ~12 %             | n/a                           | 14.7 %                                                            |
| sharing 1080p30 (software)              | ~47 %             | n/a                           | not re-measured (below)                                           |
| exe                                     | 49.6 MB           | 79.2 MiB                      | 79.2 MiB (83,044,352 B)                                           |
| installer `Setup.exe` / `nupkg`         | n/a               | 42.7 / 34.7 MiB (built 08:26) | same (not rebuilt)                                                |

Notes:

- "fix4" means the build just before the last two fixes (the GIF cache and
  `cacheBuffer`). The base scroll was measured on it because the displays
  slept during the base run.
- The base-build tour went up to 604 MB; the fix4 tour is shown because the
  base exe was not kept.
- The share sample was not taken. In test-safety mode
  (`YAP_SHARE_ONLY_OURS`) the picker shows other apps as hidden tiles. The
  share test window could not be reached in the picker's grid with posted
  input before the call's time limit, so the call was left instead.

## Tauri comparison

Yap 3.10.1 (Tauri + CEF) was running. These are read-only samples by process
name; its window was covered by a maximised browser and not focused.

| metric            | Tauri, live now | Tauri, FEASIBILITY.md | native, after                                         |
| ----------------- | --------------- | --------------------- | ----------------------------------------------------- |
| processes         | 8               | 8                     | 1                                                     |
| working set       | 250 MB          | 310 MB                | 232 MB (#general), 219 MB (fixture)                   |
| private bytes     | 769 MB          | 652 MB                | 249 MB / 242 MB                                       |
| threads / handles | 310 / 5,689     | 310 / n/a             | 98 / 1,275                                            |
| idle CPU, 60 s    | 5.49 %          | 6.7 %                 | 0.03 % tray, 0.10 % minimised, 0.08 % focused fixture |
| GPU engine use    | 2.6 %           | n/a                   | 0 % unless something animates on screen               |
| installed size    | 406 MB          | 406 MB                | 79.2 MiB exe                                          |

How they compare:

- **Idle and memory.** Native is ahead on every idle and memory row. Its
  private bytes are a third of Tauri's.
- **Animated content while focused.** Here native is behind (10 % against
  Tauri's 5.5 % covered).
  - Qt Quick redraws the whole window for every GIF or APNG frame, and three
    `layer`s re-render each frame: the gradient display name's two layers
    and the round-avatar mask.
  - Chromium composites only damaged layers.
  - Tauri's covered window may also be paused by Chromium's occlusion
    tracking, so this is not like-for-like.

## Remaining gaps

1. **Hardware encode (batch N6b).** Sharing 1080p30 in software (~47 % of a
   core in the video phase) is the known in-call CPU item. A camera publish
   also encodes in software. Done for shares in N6b (see "In-call video"
   below: 52.5 % to 33.4 %); the camera stays on libvpx simulcast.
2. **Focused idle with animated content (~10 % at ~50 fps).**
   - Every frame of an on-screen GIF or APNG costs a full-window sync and
     render.
   - Each frame also re-renders three `layer`s: `DisplayName.qml`'s gradient
     effect (label and sweep) and `Avatar.qml`'s round mask for decorated
     faces.
   - Next steps:
     - draw the gradient name and the round mask without `layer` (a
       `ShaderEffect` on the image texture, or pre-masked pixmaps);
     - pause GIF tiles that are outside the viewport but inside the cache
       buffer.
3. **Baseline memory is up (text-only fixture 242 MB private, against 100
   MB in the video phase).**
   - About 19,700 QML objects are created at startup, most for closed popups
     (Settings, ProfileEditor, menus, the pickers).
   - `QMediaDevices` initialises Media Foundation at startup (the camera
     list).
   - The NVIDIA driver runs 79 threads.
   - Next steps: Loader-gate popup contents and list cameras lazily. This
     also cuts startup: first frame is still ~0.91 s against 0.48–0.73 s
     earlier.
4. **Big animated GIF tiles decode every frame up front.** One X GIF tile is
   64 frames at 432×432, or 45.6 MB. A decode-on-demand ring (as Chromium
   does) would cap it.
5. **Scroll on rich history.** About 5 % of frames are still over 33 ms in
   a very fast wheel scroll, with a max of 78 ms.
   - The likely cause: rows reused from the pool (`reuseItems`) rebind
     synchronously, so the embed `Repeater`s rebuild in-frame, where
     incubation can't help.
   - Next steps: try `reuseItems: false` with the larger buffer, or move
     embed parsing into the model (pre-parsed roles instead of
     `JSON.parse` in bindings).
6. **The tour keeps creeping by 3–20 MB a round after warm-up.** Settings
   accounts for +2.6 MB per open/close (QML garbage or page caches) and
   isn't returned within 30 s.
7. **After any call, the WebRTC factory lives on.**
   - Its 100 Hz `SyntheticAudioDevice` and threads stay for the rest of the
     process, by design (`keep_voice_engine`).
   - A standby session on an occupied voice landing (web parity) costs about
     3.6 % even when minimised.
   - Releasing both when the app is inactive is the next idle win.
8. **The exe is 79.2 MiB, over the ≤ 60 MB budget** in ARCHITECTURE.md
   (49.6 MB in the video phase). The growth comes from embeds, webm, RNNoise
   and capture. A size map of the link is the next step. It is still a
   fifth of Tauri's 406 MB.

Side observation: after the headless peer's browser closed, the native
sidebar kept showing it in the voice channel. The server's roster
(`quit-roster.mjs`) was already empty.

# Performance pass 2 (2026-11-06)

The first pass's remaining gaps, in the brief's order. Before is the tree at
25fbec7b (the pass 1 "after" build, re-measured where noted); after is the
final build of this pass. Same machine and method as pass 1.

## Method (new)

- `YAP_DEBUG_OBJECTS=N` (`cpp/main.cpp`, `dumpObjectCounts`): every N
  seconds, the objects alive in the window's trees, closed popups' trees
  included, split by the outermost popup that owns them, then the files that
  declare the most of the rest. `YAP_DEBUG_GC=1` runs the JS collector and
  trims the component cache before each count.
- `YAP_DEBUG_ANIMATIONS` also lists each `AnimatedImage` (playing, frame,
  shown) and each `ViewportWatch`, walking the item tree (a list's
  delegates are not QObject children, so `findChildren` misses them).
- `QSG_RENDERER_DEBUG=change` to see what each frame dirtied, and
  `QSG_RENDER_TIMING=1` for the render rate.
- Scroll: the render thread's `syncAndRender: start, elapsed since last
call` intervals, inside the two wheel windows only (the pause between
  them is left out). Scripts: `<scratch>/yapn/tools/p2/` (`idle.ps1`,
  `scroll.ps1`, `popups.ps1`, `spike.ps1`, `linkflags.ps1`, `hover.ps1`).
- The displays were checked awake (59 Hz vblank) for the CPU samples. One
  earlier sample (0.65 %) was taken with them asleep and is not used.

## Findings and fixes

### 1. Idle memory: closed popups built at startup

**Finding.** A new counter, `YAP_DEBUG_OBJECTS=N` (`cpp/main.cpp`), walks the
window's trees every N seconds (closed popups' trees too, which hang off the
popup rather than the window) and splits the objects by the outermost popup
that owns them. At idle on the user's account: 13,030 objects, 9,802 of them
(75 %) in 45 closed popups. The largest were the profile editor (1,587),
Settings (912), the voice status dialog (742), the profile popout (456), the
two emoji pickers (442 each), the message menu (395), the peer menu (331),
the account popout (279), the tree and server menus (271, 267), the camera
dialog (250) and ~25 tooltips (245 together).

**Fix.** `components/LazyPopup.qml`: a Loader that makes its popup when it is
first opened (`load()`, synchronous) or while `when` holds, and lets it go a
moment after `closed` (`keep: true` keeps it). A popup that sets no parent
of its own is given the declaring item as parent, so it positions as before.
Every popup in the list above, the drawers' contents and the hover tooltips
go through it; state-driven popups (Settings, the theme sheet, the incoming
call card) open themselves as they are made (`Component.onCompleted`). The
emoji and GIF pickers are kept once made (their grids are costly to build
again); Settings too (see 3).

Also lazy now: the camera list. `CameraCapture` listed cameras in its
constructor, which starts Media Foundation; it now lists on the first read
(`cpp/camera.cpp`, `ensureListed`), and DmCallRegion reads it only once
joined.

What is left of the 120 MB target: a signed-out window (the shell made,
1,700 objects, no account data) is already 136.5 MB private. D3D11 is the
cheapest backend here (D3D12 292 MB, OpenGL 228 MB, same build). The
process heap holds ~95 MB of small blocks with no dominant size, so the
next cuts need a heap profile with stacks (UMDH needs `gflags +ust` on the
image name; not done, as it would also apply to the user's own install of
the same exe).

### 2. Focused idle with animated content

Three causes:

- **GIF tiles in the list's cache buffer played.** `cacheBuffer: 1200` keeps
  rows made up to 1,200 px outside the viewport. Their `AnimatedImage` tiles
  and GIF-mode video loops were `visible` (they are, to Qt), so every frame
  they decoded cost a full window render with nothing new on screen.
  `ViewportWatch` (`cpp/animatedpicture.h`) reports whether its item shows
  inside the window and every clipping ancestor, re-checked after each
  rendered frame; `MediaTile.qml` and `EmbedVideo.qml` play only while it
  says so. (`AnimatedPicture` already did; it now shares the same test.)
- **Every animated picture ran its own timer.** Decorations, nameplates and
  effects at different delays and phases each asked for a render, so a few
  of them kept the window near 60 renders a second. Frame changes now land
  on a shared 30 Hz grid (`animatedpicture.cpp`, `kSlotMs`), so pictures due
  in the same slot share one render; a picture keeps its speed by skipping
  to the frame due when the slot comes.
- **A GIF server icon kept its own clock.** `AnimatedImage` runs a QMovie
  timer of its own. Small GIFs (rail icons) now play through
  `AnimatedPicture`, which decodes them with Qt's GIF reader
  (`decodeGif`), so they join the grid (`ServerIcon.qml`).
- **Every decoration frame cost two renders.** `QSG_RENDERER_DEBUG=change`
  showed each grid tick followed, 15 ms later, by a frame with nothing
  dirty. Offline member rows dimmed through `layer.enabled`; a decoration
  inside such a layer re-renders the layer, and the layer then asks the
  window for another frame. The dim is now the ground at 50 % over the row
  (`MemberEntry.qml`): over a solid ground that is the same picture as the
  row at 50 %, with no layer.

### 3. Settings left memory behind per open

**Finding.** Made per open (the lazy pattern above), 20 opens grew private
bytes by 159 MB, ~8 MB an open; pass 1, with the dialog made once, measured
+2.6 MB an open. QML objects did not leak: the counter came back to the same
total after each close. Forcing the JS collector (`YAP_DEBUG_GC=1`) still
left ~7 MB an open, so it is native memory. Bisecting:

- switching pages inside one open (20 panel rebuilds, the profile card
  included) grew nothing;
- the account popout (the same profile card) and the profile editor (the
  same card, modal) cycled flat;
- the new private allocations were 2.1 and 3.6 MB blocks.

Replacing the modal shadow (below) did not change it (+10.8 MB an open), so
the cause is still open: something the dialog shell makes once per instance.

**Fix (mitigation).** Settings is made on its first open and kept
(`Shell.qml`, `keep: true`): 20 opens now grow nothing (-1.1 MB after the
settle), better than pass 1's +2.6 MB an open, since the shadow no longer
builds a layer per open. The first open costs ~86 MB (the dialog, the
profile card's decoded effects), which stays.

**Shadows.** `FloatShadow.qml` now draws a nine-slice made once per style by
`image://shadow` (`cpp/shadowprovider.cpp`): the CSS shadow (Gaussian,
sigma = blur / 2, the spread and offset exact), no layer and no blur pass,
so every float with a shadow (menus, popovers, the hover bar, dialogs) opens
and redraws without an off-screen render.

### 4. Scroll on rich history

Rows came back from the pool (`reuseItems`) and were rebound inside the
frame they entered on: their embeds and attachments were rebuilt there,
where the cache buffer's incubation cannot help. `ChatView.qml` turns reuse
off; with `cacheBuffer: 1200` new rows are incubated across frames ahead of
the viewport. No frame went past 33 ms in the test below; the price is CPU
while scrolling (46 % against 24 %), since rows are made rather than
rebound.

### 5. Exe size

A `/MAP` of the link, symbols summed by library (approximate: a symbol's
size is the gap to the next public one):

| part                                                                            | MiB  |
| ------------------------------------------------------------------------------- | ---- |
| libwebrtc (LiveKit's prebuilt, with libaom AV1, FFmpeg H.264 pieces, BoringSSL) | 18.1 |
| Rust (all crates, thin LTO, one codegen unit, `opt-level = 3`)                  | 15.9 |
| QML compiled ahead of time (qmlcachegen C++) and type registration              | 8.8  |
| Qt modules (Gui, Quick, Qml, Core, Templates, Basic, Network, ...)              | ~21  |
| static CRT                                                                      | ~3   |
| C built by `cc` (Opus, RNNoise, libvpx, ...)                                    | 2.7  |
| SQLite                                                                          | 1.4  |
| fonts, icons and SVGs (qrc, in `.rdata`)                                        | ~2   |

Fixed: a static Qt links every default plugin of a type. The ICO, ICNS,
TGA, TIFF and WBMP decoders, the OpenSSL TLS backend (Schannel is the one
used), the SVG icon engine, network information and all of the QML debug
and profiler services (outside the profiling build) are now excluded
(`CMakeLists.txt`). The fonts are already subset (1.5 MB for 22 faces).
`/OPT:REF /OPT:ICF` are on (MSVC's default without `/DEBUG`), and the Rust
side already has thin LTO, one codegen unit and stripped symbols.

Not done, and what is left: the budget needs ~18 MiB more. Options, by
size: a libwebrtc built without the software AV1 and H.264 codecs (the
prebuilt has no switch for it); qmlcachegen's bytecode-only mode for cold
screens (Admin, Shop, Settings), which `cxx-qt-build` 0.10 does not expose;
`opt-level = "s"` for the cold Rust crates (~3 MiB, measure the hot paths
first).

### 6. The WebRTC factory after a call

The keeper is still needed while a factory lives: every call opens the ADM
(its device choice is applied there, and `YAP_ADM_AUDIO=1` records and
plays through it), and without a peer connection WebRTC terminates the ADM
for good. So the factory and the keeper now go together, 30 s after the
last call ends, if nothing has used the factory since
(`yap-voice/src/engine/webrtc.rs`, `release_factory_later`); the next call
makes a new factory and a new ADM. The standby on an occupied voice landing
now follows the web's `useVoiceStandbyActive`: the window off screen
(minimised or in the tray) keeps it for 15 s, then drops it; shown again,
it comes back (`Command::WindowShown`, `reduce.rs set_window_shown`).

Measured on the test account (two calls in Native Spike Lab › General, each
under 2 minutes, left with the leave key): `[voice] webrtc factory released`
30 s after each leave; threads 128 in the call, 124 after it, 108 after the
release; minimised afterwards 0.05 % (pass 1: the 100 Hz device ran for the
rest of the process). The second call made a new factory and sent audio
normally (500 frames in and out per 5 s). The hidden-window standby was
tested in the reducer only (no second account was in the channel).

### 7. A stale voice roster in the sidebar

The gateway sends `VOICE_CHANNEL_STATES` after Identify, Resume and a
channel subscribe, but only when some visible channel has someone in it
(`meeting-room.ts sendVoiceChannelStates` returns early on an empty
payload). A peer who left while our gateway was reconnecting therefore
stayed in the sidebar for good: no update for the gap, and no snapshot to
replace it. After a Ready or Resumed the core now checks, after 5 s, which
channels the server resent (a snapshot, or an update for that channel) and
drops the rest (`reduce.rs expect_voice_rosters`, `check_voice_rosters`).
Tests: `a_roster_emptied_while_the_gateway_was_away_should_clear_after_reconnect`
and three more in `tests/reducer.rs`.

## Before and after (pass 2)

On the user's account (`--profile jontest`), Predator Island › #general,
the window focused, unless noted.

| metric                                               | before                             | after                                                  |
| ---------------------------------------------------- | ---------------------------------- | ------------------------------------------------------ |
| QML objects alive at idle                            | 13,030 (9,802 in 45 closed popups) | **3,944** (49 in 2)                                    |
| `qml loaded` after launch, warm / cold               | 462-470 / 635 ms                   | **160-165 / 220 ms**                                   |
| first frame, warm p50 / cold                         | 893 / 1,071 ms                     | **692 / 743 ms**                                       |
| idle working set / private                           | 238 / 263 MB (pass 1: 232 / 249)   | **174 / 194 MB**                                       |
| idle working set / private, chat-10k fixture         | 219 / 242 MB                       | **183 / 208 MB**                                       |
| signed-out window (floor), private                   | n/a                                | 136.5 MB                                               |
| focused idle CPU / GPU (decorations, GIF rail icon)  | 5.9 % / 7.0 %                      | **4.2-4.9 % / 3.8-4.1 %**                              |
| same, before the rail icon joined the grid           | n/a                                | 2.15 % / 2.87 %                                        |
| renders per second at that idle                      | ~48 (half of them empty)           | at most 30 (shared grid)                               |
| GIFs in the list's cache buffer, off screen          | playing                            | **paused**                                             |
| Settings, 20 opens and closes                        | +2.6 MB an open (pass 1)           | **-1.1 MB in all** (kept once made; ~86 MB first open) |
| scroll, rich history: frame interval p50/p95/p99/max | 16/33/65/78 ms                     | **16/20/30/32 ms**                                     |
| scroll frames > 33 ms / > 50 ms                      | 43 / 19 of 897                     | **0 / 0 of 643**                                       |
| CPU during that scroll                               | 24 %                               | 46 %                                                   |
| exe                                                  | 83,044,352 B (79.2 MiB)            | 81,704,448 B (77.9 MiB)                                |
| WebRTC after a call                                  | lives for the process              | **released 30 s after the last call**                  |
| threads in a call / after leave / after release      | n/a                                | 128 / 124 / 108                                        |
| minimised idle after a call                          | 100 Hz device kept running         | **0.05 %**                                             |
| a voice roster emptied during a gateway gap          | stale for good                     | cleared 5 s after Ready                                |

Popups checked open, animating and drawn after the change (posted input,
screenshots): Settings, profile editor, account popout, emoji picker, GIF
picker, notification inbox, server menu, channel tree menu, member menu,
profile popout, message menu, a tooltip. No QML warnings in the final
build's logs (the scroll log has 18 image loads refused by TikTok's CDN,
403, which are content). Escape over the profile editor opened from
Settings did not close it in the harness (its close key did); posted keys
to a window that is never foreground were not checked against the base
build.

## Remaining gaps (pass 2)

1. **Idle memory is 194 MB private, not under 120.** The signed-out floor
   is 136.5 MB (D3D11, the driver, the static Qt and the QML engine). The
   rest needs a heap profile with stacks.
2. **Focused idle with animated content is 4.2-4.9 %, not under 3 %.**
   About 30 renders a second at ~1 ms of CPU each (the render itself is
   0.12 ms; the rest is presenting and the GUI thread). A coarser grid
   (20-25 Hz) would cut it further at some smoothness.
3. **Settings' per-instance leak is mitigated, not found.** Native memory,
   ~10 MB an instance, made by the dialog shell; panels, the profile card
   and the shadow are cleared.
4. **Scroll is smooth but costs twice the CPU** (rows made, not reused).
5. **The exe is 77.9 MiB against a 60 MB budget.** See 5 above: libwebrtc
   without its software AV1/H.264, qmlcachegen bytecode for cold screens,
   `opt-level = "s"` for cold crates.
6. **The hidden-window standby** is covered by reducer tests, not yet by a
   live call with a second account in the channel.

# In-call video (batch N6b, 2026-11-06)

Hardware H.264 for shares and camera backgrounds. Before is the build of
a9a54bd1 (kept as `<scratch>/yapn/n6b-before`), after is this batch's; same
machine (Ryzen 9 7900X, RTX 3080, driver 32.0.16.1047), `--profile spike` on
the owner test account in General (Native Spike Lab), the peer test account
in headless Edge (`<scratch>/yapn/run/n6b/peer-n6b.mjs`: fake media, a 1920x1080
canvas share, `getStats` every 5 s). CPU is the whole process over 15-20 s
(`tools/measure-native.ps1 -Mode sample -TargetPid`), in % of one core.
Shares are of `share-test-window.ps1` windows only (`YAP_SHARE_ONLY_OURS`).

## Before and after

| scenario (ours)                                     | before (VP8, libvpx)   | after                                                                                                                               |
| --------------------------------------------------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| share 1080p30 (1898x1080 window), web peer watching | **52.5 %**             | **33.4 %** (NVENC H.264, 2.3-4 ms per frame on the GPU, ~2-3.5 Mbit/s)                                                              |
| share 1440p60 (2560x1092 window)                    | **111 %** at 56-58 fps | not comparable: in the later session WGC delivered ~4 fps from both test windows to both builds (A/B on the same window), see below |
| watch one web camera (640x360 `h`, VP8)             | 17.1 %                 | 12.9 % (decode unchanged: libvpx 0.3 ms)                                                                                            |
| watch one 1080p web share (VP8 1920x1080@29)        | 25.4 %                 | 16.4 % (decode unchanged: libvpx 1.7 ms)                                                                                            |
| watch one 1080p native share (H.264, native peer)   | n/a                    | 8.5 % at ~4 fps (FFmpeg 1.3 ms per frame)                                                                                           |
| camera on (720p30 VP8 simulcast)                    | n/a                    | 17.3 % none, 33.6 % blur                                                                                                            |
| camera preview in Settings, no call                 | n/a                    | 5.2 % none, 11.7 % blur (25.5 % before tuning)                                                                                      |

Watching costs the same code both ways (software decode); the differences
in those rows are session-to-session noise on a machine doing other work
(System CPU 26 %, `csrss` at 36 % of the 3D engine during the later
session). The share rows are the change: the libvpx encode (the bulk of the
47-52 %) moves to NVENC; what is left is the WGC readback, the BGRA to I420
conversion and scale, the I420 to NV12 copy, the local preview tile, and
the call's audio (RNNoise, APM, Opus).

1440p60: the before build reached 56-58 fps from the 2560-wide test window
at 21:47 (111 % of a core). Later the same window, then a fresh one on an
unused virtual display, delivered ~4 fps to WGC for both builds (the window
itself still painted at ~63 fps; its frame counter moved 8,489 in 134 s),
so no after-number at 60 fps was taken. At 4 fps NVENC encoded 2560x1092 in
6.6-7.3 ms per frame and the process sat at 12.8 %.

Peers decode it: the web peer's `getStats` showed `video/H264` 1898x1080 at
30 fps with `framesDecoded` rising ~150 every 5 s (and 2560x1092 H.264 at
1440p60), the native peer `H264 1898x1080 FFmpeg`, and the web peer
received our blurred camera as `video/VP8` 640x360 at 30 fps.

## Camera background cost

Per 1280x720 frame on the camera thread (the `[camera] background` line,
every 10 s): model input 0.09 ms, blur 0.93-1.09 ms, blend 0.86-1.07 ms
(after: 2.84 and 3.10 ms before the box blur lost its closure and the mask
its per-frame float scaling). Segmentation on its own thread, 12 times a
second: 3.5-4.2 ms with smoothing and the feather (0.87 ms of it is the
DirectML run; 3.7 ms on the CPU provider). GPU: 4.2-4.4 % of the 3D engine
while it runs. Nothing loads until a frame needs it: a background chosen
in the settings costs nothing while no camera runs.

## Left

1. **Zero-copy share.** The rest of the share's CPU is the readback and
   conversions; a D3D11 path (WGC texture to the video processor to the
   MFT) as the desktop app has would remove them.
2. **Hardware decode** needs a patched webrtc-sys (LIBRARIES.md §12.4).
3. **A 1440p60 after-number** on a source that delivers 60 fps.

# Performance pass 3 (2026-11-07)

This pass works on the brief's five targets:

1. idle memory under 120 MB;
2. the exe under 60 MB;
3. the root cause of the Settings memory;
4. focused animated idle under 3 %;
5. a 1440p60 share number.

Before is e1985388 (the 0.2.1 build); after is the final build of this pass
(`<scratch>/yapn/tmp/p3-final.exe`). Both were built from the `native/perf3`
worktree into `<scratch>/yapn/rel-perf`.

## Conditions

- **The displays slept for the whole session.** `vblank.ps1` showed vblank
  waits returning at once, so DWM paced presents at ~4 Hz. Nothing that
  depends on frame pacing could be measured honestly: scroll timing,
  animated idle CPU or a 60 fps share. See the targets below.
- **Another agent was using the owner test account for most of the
  session.** A `--profile spike` process from `<scratch>/yapn/run/sweep7` was
  running, and at 03:41 the sidebar showed a voice session as the owner, so
  my session was stopped at once. Memory was therefore measured in two
  ways:
  - on the offline fixture (`YAP_FIXTURE=chat-10k`, our own profile
    `perf3`: no token, gateway or voice), A/B against the baseline exe;
  - on the owner account, once it was free.
- **The user's own copy was read only** (`YapNativeMine.exe --profile jon`).
  After 14.5 hours up it held 589 MB private: 548 MB committed private and
  a 45 MB working set while hidden. Long-run growth matters more than the
  fresh number, and the Settings finding below is one mechanism of it.

## Method (new)

The tools:

- **`YAP_DEBUG_HEAP=<prefix>`** (`cpp/heaptrace.cpp`) gives live heap and
  VirtualAlloc commit by allocating stack.
  - The exe is one static module, so every Qt, CRT, Rust and libwebrtc
    allocation goes through its own import slots for `HeapAlloc`,
    `HeapReAlloc`, `HeapFree`, `VirtualAlloc` and `VirtualFree`. The tracer
    points those slots at counting wrappers.
  - A dump is written each time `<prefix>.now` appears. Its text is the
    tag; `gc <tag>` runs the JS collector first.
  - It needs no UMDH or `gflags` (gflags would apply to every exe of the
    same name).
- **`YAP_DEBUG_HEAP_HOOK=0`** installs no hooks and writes only the
  whole-process breakdown, so the numbers carry none of the tracer's cost.
  - Private commit is split by allocation into the heaps (HeapWalk
    segments and large blocks), thread stacks (regions with a guard page),
    write-combined memory (the GPU driver's upload memory) and the rest.
  - The largest allocations in the rest are listed.
- **`tools/p3/heapsym.py`** symbolizes dumps with the link map (`/MAP`).
  Rust v0 names are read by hand and C++ names go through `undname`. It
  groups stacks by their first frame that is not allocator plumbing, and by
  category, and diffs two dumps.
- **`tools/p3/heapgrep.py`** sums the stacks that pass through one symbol.
- **`tools/p3/mapsize.py`** and **`catsize.py`** give exe size by library,
  object and category (Rust crates, QML, libwebrtc, `cc` C/C++, Qt) from the
  map.
- **The `tools/p3` run scripts:**
  - `run.ps1 [-Fixture]` launches and samples (measure-native and vmmap);
  - `ab.ps1` alternates A/B runs;
  - `settings-cycle.ps1` opens and closes Settings with a posted gear click
    and Escape to our window;
  - `settings-ab.ps1` and `tray.ps1` are the Settings and tray A/Bs;
  - `build.ps1` is the pass build, with a map.
- **`YAP_FIXTURE_ICON=<url>`** and **`YAP_FIXTURE_GIF=<url>`**
  (`fixture.rs`) add a server icon (a GIF one animates) and two GIF
  attachments to the fixture, giving an animated scene without an account.
  `YAP_FIXTURE_GIF` takes local `file:///` URLs (`<scratch>/yapn/tmp/anim`).
  `YAP_FIXTURE_ICON` needs an http(s) URL: server icons load through
  `absolute_url`, which loads nothing that is not http(s).

## Where the idle memory goes

On the fixture, with the baseline exe and no tracer, committed private
memory is **164 MB**:

| part                  | MB  | what it is                                                                                          |
| --------------------- | --- | --------------------------------------------------------------------------------------------------- |
| the process heap      | 115 | 96 allocated, 19 free inside it                                                                     |
| write-combined memory | 14  | the GPU driver's upload memory                                                                      |
| thread stacks         | 2.4 |                                                                                                     |
| the rest              | 38  | the JS engine's two stacks (6.5), the GPU driver's own regions (~14), Media Foundation, system DLLs |

`PrivateMemorySize64` reads 204 MB, ~40 MB above that. The writable image
sections of every loaded DLL (the exe's own `.data` is 4.2 MB) and
pagefile-backed sections are charged as well.

The heap by owner (traced, 80 MB live):

| owner                                                                                    | MB       |
| ---------------------------------------------------------------------------------------- | -------- |
| the fixture's own data (10,000 messages at ~1.5 KB each in `MessageLog`, and their rows) | ~35      |
| QML engine (property caches, compilation units, bindings) and its JS heap                | 18 + 8.5 |
| scene graph (batch buffers, glyph-cache update batches, nodes)                           | 10.4     |
| text and fonts (2.25 MB of it is `registerFonts`' copies of the 22 faces)                | 3.7      |
| Qt core                                                                                  | 3.5      |
| images (2 MB of it a `Canvas` painted at startup, see below)                             | 2.7      |

No single owner is large. The floor is the engine, the driver and the
heap's slack.

## Fixes

### 1. Idle memory

- **The segment heap** (`cpp/app.manifest`, `<heapType>SegmentHeap`).
  - The NT heap kept ~20 MB of freed blocks committed; the segment heap
    gives them back.
  - Heap commit fell from 115 to 105 MB on the fixture, for the same
    allocations.
  - No CPU change was measurable at idle. The scroll test needs awake
    displays.
- **The JS engine's stacks** (`cpp/main.cpp`).
  - V4 commits its whole JS stack (4 MB) and GC stack (2 MB) up front.
  - Now `QV4_JS_MAX_STACK_SIZE` is 1 MB, still ~130,000 JS values, and
    `QV4_GC_MAX_STACK_SIZE` is 512 KB; a full GC stack only means an extra
    drain.
  - Commit fell from 6.3 MB to 1.8 MB.
- **The drop overlay's dashed frame** (`ChatView.qml`).
  - A window-sized `Canvas`, inside an invisible overlay, was painted at
    startup: 2 MB of image plus its texture.
  - It is now made only while a drag is over the chat.
- **Media Foundation at startup** (`CallPanel.qml`, `DesktopLink.qml`).
  - Both files read `Camera.hasCamera` in bindings outside a call. The
    first read lists the cameras, which starts Media Foundation. (Pass 2
    made the list lazy; these readers came later.)
  - Both now read it in a call only. Startup logs no `[camera] listed`, and
    there are 87 fewer handles.
- **The Settings memory** (below).

### 2. The Settings memory: root cause

**Measurement.** Ten opens with the dialog made per open, on stock Qt,
under the tracer:

- private bytes grew 44 MB;
- the heap grew 8.9 MB, almost all in the window's batch renderer:
  `QD3D11Buffer::create` from `uploadBatch` +3.6 MB, and queued
  `updateDynamicBuffer` copies +2.8 MB;
- write-combined memory, the driver's copies of the same buffers, grew
  10.4 MB;
- the rest of the growth is in the driver;
- QML objects were freed every time (pass 2's counts).

**The cause** is Qt Quick's batch renderer (`qsgbatchrenderer.cpp`):

- Every batch that is no longer needed goes to `m_batchPool`, with its
  vertex, index and uniform buffers, for reuse.
- A reused batch grows its buffers to fit (`unmap` calls `setSize` when
  larger, never smaller). After four changes a buffer turns Dynamic, which
  adds a CPU shadow copy and driver write-combined memory.
- Nothing frees the pool while the window lives.
  `QQuickWindow::releaseResources()` on a shown window reaches
  `Renderer::releaseCachedResources()`, which frees samplers and upload
  pools but not the pooled batches.

How that plays out:

- A dialog opened once leaves its batches in the pool.
- Each new instance lays out its batches differently, so the pooled
  buffers keep growing to the largest content they ever held.
- The growth is large at first and then slows. Over 30 cycles it was
  +14.5 MB for the first open, ~1.6 MB an open up to the tenth, and ~0.4
  after that.
- Native-text glyph caches behave the same way: a font engine's cache goes
  only on a font database change (`pendingFontUpdate`).
- Pass 2's mitigation kept the dialog once made, so the same nodes came
  back and reused their batches exactly. That is why it worked.

**Fix.** A two-line Qt patch (`patches/qt/0001-...patch`, part of the custom
Qt below):

- `Renderer::releaseCachedResources()` also wipes the pooled batches;
- the threaded render loop's release also drops the glyph caches of font
  engines that no text uses.

The app asks for the release:

- `LazyPopup` calls `DesktopShell.releaseGraphicsSoon()` when it lets a
  popup go. The call is debounced to at most once every 30 s, since a
  release also drops the renderer's pipelines.
- `hideToTray` releases before hiding.
- Settings is made per open again (`Shell.qml`, no `keep`).

| fixture, ws / private MB              | start         | after 1 open + close (+35 s) | after 11 opens (+35 s) |
| ------------------------------------- | ------------- | ---------------------------- | ---------------------- |
| before (stock Qt, Settings kept)      | 181.1 / 204.6 | 196.1 / 216.9 (+12.3)        | 208.8 / 230.3 (+25.7)  |
| after (patched Qt, Settings per open) | 168.7 / 189.7 | 173.9 / 195.3 (+5.6)         | 183.9 / 205.7 (+16.0)  |

About 1 MB an open is left, and it is outside Qt:

- the GPU driver keeps the write-combined memory of freed buffers in its
  own pools (14 MB grew to 23 MB, and it stays);
- the heap fragments.

Pass 2's "kept" mitigation was not flat on this build either: +13.4 MB over
ten reopens.

### 3. Exe size

| step                                                                                                                      | exe                                                     |
| ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| before                                                                                                                    | 82,952,704 B (79.11 MiB)                                |
| Rust `opt-level = "s"`, with the per-frame crates at 3 (audio, RNNoise, resampling, segmentation, decoders, capture)      | 73.0 MiB (Rust 19.2 to 14.4 MiB; yap_core 6.15 to 4.02) |
| cold QML compiled to bytecode only (Settings, Admin, Shop, Panels, Apps, Friends, Join, SignIn, Boot and 23 Shell popups) | 71.1 MiB (QML 8.6 to 6.7 MiB)                           |
| the custom Qt (`-optimize-size`, without OpenGL, FreeType, Widgets, D-Bus, SQL or FFmpeg)                                 | Qt and the rest 29.2 to 21.8 MiB                        |
| no libaom AV1 encoder (webrtc-sys patch)                                                                                  | libwebrtc 16.2 to 12.8 MiB                              |
| **after**                                                                                                                 | **62,895,104 B (59.98 MiB)**                            |

- **Rust at "s".**
  - The top-level `opt-level = "s"` also compiles the QML C++ at `/O1`,
    since `cc` takes its level from the crate it builds for.
  - With thin LTO, the final crate's level re-optimizes every module. The
    hot crates therefore keep 3 before LTO but run the LTO stage at "s".
  - RNNoise is 0.5 % of a core and the background blur ~3 %, so a 10–20 %
    slowdown there would not show at the process level. Neither was
    re-measured: there was no call and no camera this session.
- **Bytecode-only QML.**
  - It runs through the interpreter and the JIT (after 3 calls) instead of
    compiled C++.
  - It was applied only to screens and popups that open now and then. The
    chat, sidebars, rail, composer, calls and shared components stay
    compiled.
  - After the change I opened and screenshotted Settings, the profile
    editor, the emoji and GIF pickers and the menus.
- **Encoders left in.**
  - The VP9 encoder was tried and left in: other code still links it, so
    removing it saved only 55 KB.
  - OpenH264 stays, because hardware shares need some factory to advertise
    H.264.

### 4. The tray

- `hideToTray` now releases the spare scene-graph memory before hiding. On
  the fixture, 60 s after hiding, private went from 190.4 to 188.0 MB.
- I also tried dropping the whole scene graph on hide
  (`setPersistentSceneGraph(false)`). It saved 6.8 MB in the tray, but
  shown again the process sat 17 MB above where it started: the rebuild
  allocates while the freed memory sits in the heap and the driver's pools.
  It was not kept.

## Before and after (pass 3)

| metric                                                          | before                           | after                                                                   |
| --------------------------------------------------------------- | -------------------------------- | ----------------------------------------------------------------------- |
| fixture idle ws / private (4 runs each, alternating A/B)        | 180.6 / 203.4 MB                 | **169.2 / 189.4 MB** (−11.4 / −14.0)                                    |
| owner account idle in #general (one run each, A then B)         | 220.9 / 241.6 MB                 | **205.8 / 223.6 MB** (−15.1 / −18.0)                                    |
| owner account, this morning, before another agent's video posts | 153.5 / 168.6 MB (baseline only) | n/a                                                                     |
| handles at idle (fixture)                                       | 1,079                            | 992 (no Media Foundation)                                               |
| process heap committed / allocated (fixture)                    | 115 / 96 MB                      | 105 / 92 MB                                                             |
| JS engine stacks committed                                      | 6.3 MB                           | 1.8 MB                                                                  |
| Settings, first open, kept after close                          | +12.3 MB                         | +5.6 MB                                                                 |
| Settings, ten more opens                                        | +13.4 MB                         | +10.4 MB                                                                |
| exe                                                             | 82,952,704 B (79.11 MiB)         | **62,895,104 B (59.98 MiB)**                                            |
| first frame, fixture (4 runs)                                   | 746–966 ms                       | 641–948 ms (noise; the Qt build ran on the same machine for part of it) |
| idle CPU (fixture, displays asleep)                             | 0.05–0.10 %                      | 0.00–0.05 %                                                             |

The two owner-account runs were taken minutes apart, in the same channel
state: #general showed two video previews that another agent had posted
that morning. The morning baseline is listed to show how much the content
moves the number.

## Targets

1. **Idle memory under 120 MB: not reached.** It is 205.8 / 223.6 MB on the
   owner account in its current state, and 169.2 / 189.4 MB on the fixture.
   What it would take, largest first:
   - **The QML engine (~25 MB).** Every QML type referenced from an inline
     `sourceComponent` is compiled and linked at startup, even when it is
     never made. Loading the cold screens by URL would keep their
     compilation units, property caches and types out until first use.
   - **The GPU driver (~28 MB of write-combined and private memory on this
     NVIDIA card).** It needs fewer dynamic buffers, which means batch
     renderer changes in Qt.
   - **The heap's own slack:** 12 MB at idle, even with the segment heap.
   - **Smaller per-message state in the core:** `Message` is ~1.5 KB
     inline.
   - **Fonts loaded straight from uncompressed resources.** This saves
     2 MB private but adds 0.78 MB to the exe, which does not fit under
     60 MiB yet.
   - **A newer Qt (6.8+):** a DirectWrite font database and the QML
     engine's memory work. It is a migration in its own right.
2. **Exe under 60 MB: reached, at 59.98 MiB (62.9 decimal MB), with no
   margin.** More would come from, cheapest first:
   - a Qt rebuild with `-no-feature-http2` and the other network features
     the app does not use;
   - bytecode-only for more QML;
   - fewer `symphonia` formats;
   - `panic = "abort"`, which is a behaviour change: a panic in a tokio
     task would end the process.
3. **The Settings memory: found and fixed** (above). The ~1 MB an open that
   remains is the GPU driver's pooling, outside the app and Qt.
4. **Focused animated idle under 3 %: not measured.**
   - The displays slept all session. With presents paced at 4 Hz, neither
     the render rate nor its cost is the real one.
   - The animated fixture scene (a GIF rail icon and two chat GIFs) is
     ready for it.
   - The likely next step, from pass 2's numbers: chat GIF tiles still run
     on `AnimatedImage`'s own QMovie clock, off the shared 30 Hz grid, so a
     visible chat GIF adds renders of its own. A streaming GIF decoder on
     the grid, decoding one frame at a time instead of every frame up
     front, would fold them in.
5. **1440p60 share with NVENC: not measured.** It needs awake displays and
   both test accounts in a call, and the owner account was busy.

## Files

Source (worktree `native/perf3`):

- C++:
  - `cpp/heaptrace.cpp` and `cpp/heaptrace.h` (new);
  - `cpp/main.cpp`, `cpp/desktopshell.cpp`, `cpp/desktopshell.h`;
  - `cpp/app.manifest`, `CMakeLists.txt`.
- QML (5 files):
  - `components/LazyPopup.qml`;
  - `screens/Shell/ChatView.qml`, `screens/Shell/Shell.qml`;
  - `screens/Shell/CallPanel.qml`, `screens/Shell/DesktopLink.qml`.
- Rust and build:
  - `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/build.rs`;
  - `rust/crates/yap-core/src/fixture.rs`;
  - `build.ps1`, `.gitignore`.
- Patches (docs/LIBRARIES.md §13):
  - `patches/vendor.ps1`;
  - `patches/qt-build-utils/`, `patches/webrtc-sys/`, `patches/qt/`.

# Performance pass 4 (2026-11-07)

The brief, in order: long-run memory growth first, then validating the
custom Qt and Rust `opt-level = "s"` in real calls, focused animated idle
under 3 %, idle memory towards 120 MB, hardware video decode, and a 1440p60
share number.

Before is the pass 3 build (59acb1e5, `<scratch>/yapn/tmp/p4/soak0.exe`); after
is this pass's final build (`<scratch>/yapn/tmp/p4/final.exe`). For the Qt and
opt-level A/B, `<scratch>/yapn/rel-base4` is this pass's source built with the
stock static Qt 6.6.3 and Rust at opt-level 3
(`CARGO_PROFILE_RELEASE_OPT_LEVEL=3`).

## Conditions

- **The displays were asleep all session.** All three are virtual (the VDD
  primary and two Parsec displays), and DWM paced presents at ~4 Hz (Qt
  logged `swap=250ms`). `SetThreadExecutionState(ES_DISPLAY_REQUIRED)` from
  one of our processes did not wake them. Frame-paced numbers here are
  taken with presents unpaced instead (`YAP_DEBUG_NO_VSYNC`, below).
- **A Qt 6.11.3 build ran for about an hour of the session**, using every
  core. CPU numbers from that hour are only compared with runs interleaved
  under the same load (the call A/B).
- **The user's copy was read only** (`YapNativeMine.exe --profile jon`,
  0.2.1). Its counters were read by PID every 5 minutes for 2 hours, from
  16.8 h up:
  - it held **589.2 MB private the whole time**;
  - 1,261 to 1,273 handles, 99 threads, and a 32-35 MB working set in the
    tray;
  - 131 s of CPU in 115 min hidden, which is 1.9 % of a core;
  - a peak working set of 514.7 MB.

## Method (new)

- **The soak fixture** (`YAP_FIXTURE=soak`,
  `rust/crates/yap-core/src/fixture/soak.rs`):
  - four text channels of 400 messages and a voice channel, offline;
  - a driver in the core replays gateway traffic at `YAP_FIXTURE_SOAK`
    events a second (default 8):
    - new messages; with a media server, one in ten carries a GIF, one a
      picture and one an image embed, each at a URL of its own;
    - edits, deletes, reactions, typing, presence and voice rosters;
  - a channel switch every `YAP_FIXTURE_SOAK_SWITCH` seconds (default 30);
  - deterministic, so runs compare;
  - `YAP_FIXTURE_MEDIA=http://127.0.0.1:5871` serves avatars and media
    (`tools/p4/media-server.mjs`, with pictures from
    `tools/p4/make-media.ps1`).
- **App switches for test runs** (all off unless set):
  - `YAP_DEBUG_SOAK_TRAY=N`: every N s (at least 30) the window goes to
    the tray and comes back 20 s later. It does not take the focus, and it
    is told it is active (`main.cpp`).
  - `YAP_DEBUG_NO_ACTIVATE=1`: every show leaves the focus where it is
    (Qt's `_q_showWithoutActivating`).
  - `YAP_DEBUG_NO_VSYNC=1`: swap interval 0, so a run on a sleeping
    display renders every frame its content asks for.
  - `YAP_NO_TOASTS=1`: each toast is built but never shown
    (`toast_win.cpp`).
  - `YAP_NULL_SPEAKER=1` now also silences the UI sounds (`sound.rs`). The
    first soak smoke test played the message sound through the real
    speakers and raised real toasts; these two switches cover both.
- **Tools** (`<scratch>/yapn/tools/p4/`):
  - `soak.ps1`: a soak run with counters every minute, a Settings
    open/close and a scroll every 4 min, and optional heap dumps;
  - `ab-soak.ps1`: two exes side by side on the soak;
  - `handles.ps1` and `handles-diff.ps1`: a process's handles by object
    type, read-only from the system handle table;
  - `tray-handles.ps1`: handles per tray cycle;
  - `anim-idle.ps1` and `renders.py`: animated idle, as CPU and as renders
    a second per window from `QSG_RENDER_TIMING`;
  - `call-app.ps1`, `call-ab.ps1`, `call-release.ps1`, `cam-retain.ps1`
    and `cam-cpu.ps1`: the owner account on profile `perf`, the headless
    web peer with a camera, and posted clicks to our window only;
  - `cpu.ps1`, and `build.ps1` (a pass build into any build dir);
  - `<scratch>/yapn/tools/p4-sampler.ps1`: read-only counters by PID.
- **Profile names are lowercase only** (`profile.cpp`); an invalid one
  falls back to the default profile.
  - One A/B run used `perf4ab-A` and wrote three files of the default
    profile: `window-state.json`, `quick-switcher.json` and
    `capture-policy.json`.
  - The fixture entry it added to `quick-switcher.json` was removed by
    hand. The window position it saved stayed.

## Findings and fixes

### 1. The blurred camera: +330 MB while on, +93 MB kept for good

This is the largest memory event in normal use. In a call (the owner on our
profile, the web peer with its camera), our camera turned on with the blur:

| step                                    | DirectML (before)  | CPU provider (after) |
| --------------------------------------- | ------------------ | -------------------- |
| start                                   | 155.7 MB           | 156.0 MB             |
| in the call                             | 168.2 MB           | 168.3 MB             |
| camera with blur                        | **498.5 MB**       | **270.3 MB**         |
| camera off (+20 s)                      | 256.8 MB           | 183.3 MB             |
| left the call (+50 s)                   | **248.4 MB** (+93) | **176.0 MB** (+20)   |
| working set with the blur               | 431.5 MB           | 263.5 MB             |
| process CPU over 25 s with the blur     | 9.70 s, 9.67 s     | 8.97 s               |
| segmentation (wall, smoothing included) | 4.09-4.74 ms       | 4.09 ms              |

- **It is a one-off high-water mark, not a leak per session.** A second
  session in the same process added only 2.6 MB.
- **The kept memory is outside the exe's own allocations.** The heap tracer
  saw 46.5 MB live through the exe's imports, while the process heap held
  124.8 MB allocated. The rest belongs to onnxruntime, DirectML and the
  driver, which the tracer does not hook.
- **DirectML saves no CPU on this model.** Its own thread is cheaper
  (172-188 ms per 25 s, against 891 ms on the CPU). But its D3D12
  submission and driver threads cost more than the inference saves on a
  256x256 model.
- **Fix** (`yap-segment`): the CPU provider by default, without its memory
  arena. `YAP_SEGMENT_GPU=1` runs DirectML for comparisons.
- **This matches the user's copy:**
  - its peak working set was 514.7 MB, and we measured 431-492 MB with the
    blurred camera on;
  - its memory stays high in the tray afterwards.

### 2. Toasts leaked their platform objects

The soak's handle count rose in two ways:

- by ~120 at each tray cycle;
- by ~100 semaphores and ~50 events every 30 s with the window in the
  background.

**Bisect, on the soak:**

- not the tray cycle (flat on the plain fixture);
- not the network images, and not the GIFs;
- with the window active, so no toasts, the count was flat.

**Cause:**

- Each toast registered an `Activated` handler and never removed it.
- The registration keeps the toast, and the platform's objects behind it,
  alive.
- So `g_kept`'s limit of 32 never freed anything: about 2.6 kernel handles
  a toast, plus memory.

**Fix** (`toast_win.cpp`):

- `g_kept` keeps each toast with its registration token;
- a dropped toast unregisters first;
- the same happens on every error path, and right away under
  `YAP_NO_TOASTS`.

| side by side, window in the background | before         | after                |
| -------------------------------------- | -------------- | -------------------- |
| handles over 6 min of soak traffic     | 1,263 to 3,244 | 1,030 to 1,041       |
| private at 6 min                       | 249.1 MB       | 240.0 MB             |
| handles over a 2-hour soak             | 1,160 to 3,991 | flat, 1,061 to 1,112 |

The real toast path (`Show`) could not be exercised without putting toasts
on the user's desktop. It shares the fixed code.

### 3. The soak itself: flat after warm-up

The soak ran with media, a channel switch every 30 s, a Settings
open/close and a scroll every 4 min, and a tray cycle every 5 min.

- **The traced heap was flat.** Live heap went −2.0 MB from minute 0 to
  minute 40: the image owners shrank by 4.4 MB and the QML engine grew by
  2 MB.
- **Before** (the pass 3 build): 215 MB at the start, 245 MB at 50 min and
  280-300 MB at 2 h, with handles growing with every toast (above).
- **After the toast fix** (run c2): 210.6 MB at the start, then 306.9,
  308.9, 305.1, 294.9, 306.3 and 308.3 MB from 20 to 70 min. It is flat
  after a 20-minute warm-up of the caches, and handles are flat too.
- **The final build** (run d3, with the GIF file budget below): 213.4 MB
  at the start, then 237.0, 255.5, 242.0, 256.9 and 249.3 MB at 10, 20,
  30, 40 and 46 min. From 20 to 46 min it averaged 263.7 MB, against
  326.0 MB for run c2 over the same span; handles stayed at 1,079-1,112.
- Early in a run the before build sits a little lower (230-245 MB at
  20-50 min), but it keeps rising with every toast.

### 4. Chat GIFs on the shared clock, one frame held

`MediaTile.qml` played chat GIFs with `AnimatedImage`. Each one ran on its
own QMovie clock, off the 30 Hz grid that the decorations and rail icons
share.

- **`AnimatedPicture.stream`** (`cpp/animatedpicture.*`):
  - a GIF is kept as its file and decoded one frame at a time as it plays,
    on the shared grid;
  - the item holds the file and the frame on screen;
  - a GIF more than twice as large as its tile is scaled down as it
    decodes; otherwise the GPU scales it;
  - other formats decode as before.
- `AnimatedPicture.Status` now follows `Image.Status`'s order, so QML can
  compare either one with `Image.Ready`.
- **The GIF files have their own 4 MB budget**, oldest out first. In the
  soak they had filled the whole 32 MB cache budget meant for decoded
  frames, and the plateau sat ~30 MB above the before build.

Focused animated idle, unpaced (`YAP_DEBUG_NO_VSYNC=1`): the chat-10k
fixture with a GIF rail icon and two chat GIFs (pass 3's scene), with our
window told it is active:

| build                                        | renders a second | CPU (one core) |
| -------------------------------------------- | ---------------- | -------------- |
| before (`AnimatedImage`)                     | 31.7             | 10.49 %        |
| after (streamed, on the grid)                | 25.8             | 5.49 %         |
| after, rail icon only (48 px, 20 fps)        | 20.2             | 4.76 %         |
| stock Qt + opt-level 3, same source (2 runs) | 25.8             | 5.83 %, 5.83 % |
| custom Qt + opt-level "s" (2 runs)           | 25.7-25.8        | 6.46 %, 5.62 % |

- **Under 3 % is not reached.** A render costs ~2.1-2.3 ms of CPU whatever
  changed: a 48 px icon alone at 20 fps is 4.8 %.
- Per thread over 20 s (516 renders): GUI 375 ms, render thread 328 ms,
  two NVIDIA driver threads 437 ms.
- Pass 2 measured ~1.5 ms a render on an awake display. Presenting unpaced
  may cost more than a paced present.
- The custom Qt at `/O1` is not why renders cost that much: the stock Qt
  build is within the noise.

### 5. Custom Qt and opt-level "s" in calls: no regression

An interleaved A/B under the same machine load, with the owner on profile
`perf` (RNNoise on) and the web peer with its camera:

| per 25 s                                      | before (stock Qt, opt 3) | after (custom Qt, opt "s") |
| --------------------------------------------- | ------------------------ | -------------------------- |
| voice, RNNoise, watching the peer: mic thread | 672 ms                   | 781 ms                     |
| the same, whole process                       | 2,141 ms                 | 2,156 ms                   |
| plus our camera with blur, whole process      | 9,641 ms                 | 7,922 ms                   |
| video encoder thread                          | 2,641 ms                 | 1,938 ms                   |
| blur / blend per 720p frame                   | 1.85 / 1.72 ms           | 1.32-1.38 / 1.04-1.06 ms   |
| segmentation                                  | 5.22 ms                  | 5.33-5.38 ms               |
| private with the blurred camera               | 496.1 MB                 | 497.7 MB                   |

The machine load moved between runs: the mic thread is 16 % slower one
way, and the blur 30 % faster the other. Nothing points at the
size-optimized builds.

- **Checked working:** joining and leaving, the peer's camera on our
  screen, and our camera both ways with and without the blur.
- **Not checked:** a screen share, WebView2 embeds and the WMF media
  player.

### 6. Qt 6.11.3: built, measured, not adopted yet

Details are in LIBRARIES.md §13.3. Against the custom 6.6.3 on the fixture:

- idle private memory went from 191.3 to 177.5 MB (−14 MB), and the
  working set fell by 8 MB;
- animated idle costs the same;
- the exe is 8.7 MB larger (71.7 MB).

One regression: `ChatHeader.qml`'s "/" shortcut guard reads the `Window`
attached property from a `Shortcut`. Qt 6.11 no longer allows that, so "/"
typed in the composer would open search.

## Before and after (pass 4)

| metric                                     | before                                     | after                                                          |
| ------------------------------------------ | ------------------------------------------ | -------------------------------------------------------------- |
| blurred camera, private while on           | 498.5 MB                                   | **270.3 MB**                                                   |
| kept after the camera and the call         | +92.7 MB                                   | **+20.0 MB**                                                   |
| process CPU with the blurred camera (25 s) | 9.70 s                                     | **8.97 s**                                                     |
| handles after 2 h of soak                  | 3,991 (growing)                            | **~1,100 (flat)**                                              |
| focused animated idle, unpaced             | 10.49 %, 31.7/s                            | **5.49 %, 25.8/s**                                             |
| exe                                        | 62,895,104 B                               | 62,940,672 B                                                   |
| soak: private at 20 / 40 min, then at 2 h  | 230.5 / 235.4 MB, then 280-300 MB (rising) | 255.5 / 256.9 MB, then flat (240.6-280.7 MB from 20 to 46 min) |
| soak: handles                              | 1,160 to 3,991 in 2 h                      | flat, 1,079-1,112                                              |

## Targets

1. **Long-run memory: the two mechanisms found are fixed.** They are the
   blurred camera's DirectML memory and the toast registrations, and the
   soak is flat after its warm-up.
   - The user's 589 MB could not be replayed exactly. The synthetic
     traffic does not reach it, and its counters stayed flat while it was
     hidden.
   - What remains, cheapest first:
     - a soak of calls (join, camera, share, leave, repeated) for hours;
       each call step here was checked once or twice, not for hours;
     - screen shares and watching shares (NVENC, decode buffers) under the
       same retention test as the camera;
     - extending the heap tracer to DLLs loaded later (onnxruntime, Media
       Foundation, the driver), by hooking their import tables at load.
2. **Custom Qt and opt-level "s": validated in calls** (above). A screen
   share, WebView2 and the media player still need a check.
3. **Animated idle under 3 %: not reached** (10.49 % down to 5.49 %).
   Next:
   - a profiler on the GUI thread's ~0.7 ms a render (needs WPR, which
     needs elevation);
   - a measurement on an awake display, to see the real present cost;
   - fewer renders: GIF frames moved to the nearest slot instead of the
     next one, or a 20 Hz grid for chat GIFs (a smoothness cost).
4. **Idle memory towards 120 MB.** Qt 6.11.3 gives −14 MB once the shortcut
   is fixed and the exe size is audited. Not done this pass:
   - cold screens loaded by URL;
   - smaller per-message state. Each message is held three times: in the
     core's log (with a full `User` per message), in the core's view rows
     and in the bridge's model rows. The change is to share authors
     (`Arc<User>`) and rows (`Arc<MessageRow>`). It matters for long
     scrollback, less for a normal channel.
5. **Hardware video decode: not started.** It needs a webrtc-sys patch with
   a Media Foundation H.264 decoder factory, and a 1080p or 1440p share to
   watch.
6. **1440p60 share: not measured.** WGC delivers ~4 fps from a window on a
   sleeping display (N6b), and the displays slept all session.

## Files

- C++:
  - `cpp/toast_win.cpp`: the registration fix and `YAP_NO_TOASTS`;
  - `cpp/animatedpicture.cpp` and `cpp/animatedpicture.h`: streamed GIFs,
    the file budget and the status order;
  - `cpp/main.cpp`: `YAP_DEBUG_SOAK_TRAY`, `YAP_DEBUG_NO_ACTIVATE` and
    `YAP_DEBUG_NO_VSYNC`.
- QML (1 file): `components/MediaTile.qml`, with chat GIFs on
  `AnimatedPicture { stream: true }`.
- Rust:
  - `crates/yap-segment/src/lib.rs`: the CPU provider by default;
  - the soak fixture: `crates/yap-core/src/fixture.rs`,
    `crates/yap-core/src/fixture/soak.rs` (new) and
    `crates/yap-core/src/runtime.rs`;
  - `src/sound.rs`: `YAP_NULL_SPEAKER` silences the UI sounds.
- Qt: `patches/qt/build-qt.ps1` (`-Version`) and `patches/qt/6.11.3/`
  (new).
- Docs: this section, LIBRARIES.md §12 (segmentation) and §13.2-13.3.

# Performance pass 5 (2026-11-07)

The brief, in order:

1. move to Qt 6.11.3 and make it the default;
2. adopt what Qt 6.7-6.11 offers where it pays;
3. bring the exe back under 60 MB;
4. focused animated idle under 3 %;
5. hardware decode for watched shares;
6. a 1440p60 share number.

Before is 04c8d8e5 built on the custom Qt 6.6.3 (`<scratch>/yapn/tmp/p5/before.exe`,
63,847,424 B). After is this pass's final build on the custom Qt 6.11.3
(`<scratch>/yapn/tmp/p5/c7.exe`, 59,124,224 B). Both were built from the
`native/perf3` worktree.

## Conditions

- **The displays slept all session**, as in passes 3 and 4. DWM composed
  one frame in 2 s and vblank waits returned at once.
  - I ran a helper of my own that held
    `SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED)`.
    `PowerSetRequest(PowerRequestDisplayRequired)` and `SC_MONITORPOWER`
    sent to a hidden window of mine did not wake the displays either. Only
    real input would, and that is off limits.
  - Frame-paced numbers are therefore taken unpaced
    (`YAP_DEBUG_NO_VSYNC`). The share was measured with a synthetic source
    (below), because Windows Graphics Capture delivers ~4 fps from a
    sleeping display.
- **The machine was shared.** Another agent's builds (`rustc`) and a `qemu`
  VM used several cores for most of the afternoon. CPU numbers are
  compared only within interleaved runs, and are repeated.
- **A test-account collision, 15 s.** One owner launch started while
  another agent's `--profile spike` session (the owner account) was
  running. It reached "gateway ready", joined nothing, and was stopped by
  PID at once. `tools/p5/app.ps1` now refuses to start while any other
  agent's YapNative or node peer runs.

## Method (new)

Tools are in `<scratch>/yapn/tools/p5/`:

- `build.ps1`: the pass build with a link map, VS 2022;
- `anim-idle.ps1`: pass 4's, on profile `perf5`; it now waits for the
  process to exit;
- `scroll.ps1`: a fast wheel scroll (200 ticks up and down at 16 ms) over
  the chat-10k fixture, with CPU and the basic loop's frame times;
- `app.ps1`: an owner or peer instance, with the account guard;
- `watch.ps1`: swaps the watcher instance and samples it;
- `who.ps1`: read-only list of running Yap sessions;
- `awake.ps1`: the display helper;
- `test.ps1`: `cargo test` and clippy on the custom Qt.

New test switches in the app (off unless set):

- **`YAP_FAKE_SHARE_SOURCE=<w>x<h>`** (`yap-capture/src/win/fake.rs`): a
  share replaces the picked source with a synthetic picture of that size.
  - A bar sweeps across a fixed pattern, so the encoder sees motion in
    every frame.
  - Frames are pushed at the share's rate into the same
    `push_screen_frame` the capture uses.
  - Nothing of the picked source is read, so the only-ours check is
    skipped. Audio follows only when it is synthetic too.
- **`YAP_H264_DECODER=software`**: Media Foundation's H.264 decoder without
  the GPU, for comparisons.

The share and watch runs used two of our instances: the owner on profile
`perf5` sharing, and the peer on `perf5peer` watching.

## 1. Qt 6.11.3, the default

- **`build.ps1`** now uses `<scratch>/qt-custom-6.11/6.11.3-yap` and refuses to
  run without a Qt 6.11. It also takes Visual Studio 2022 explicitly, the
  MSVC Qt is built with: Qt's libraries are link-time code (`-ltcg`), and
  LTCG cannot mix two compiler versions. The newest Visual Studio here is
  2026 (18.4).
- **The `/` regression** (`ChatHeader.qml`): the `Shortcut`'s guard now
  reads `root.Window.activeFocusItem`. The other `Window.*` readers are in
  Items and were fine.
- **Text fields got a context menu.** Qt 6.9 gives every Basic
  `TextField` and `TextArea` a default Cut/Copy/Paste menu on a right
  click. It is off-brand and would bypass `YAP_NO_CLIPBOARD`, so the base
  fields set `ContextMenu.menu: null`. This keeps 6.6's behaviour; the web
  shows none of its own either.
- **QML warnings:** none at startup, in Settings, in the menus, or in a
  call with a share and a watcher. Each run's log was checked.
- **The Qt patches:**
  - The batch half of pass 3's patch is upstream in 6.11.
  - The glyph-cache half (`6.11.3/0001`) now also covers the basic render
    loop (below).
  - Two new ones: `0002` (HLSL-only shaders) and `0003` (time zones), in
    LIBRARIES.md §13.
- **qmlcachegen's bytecode-only mode** (the qt-build-utils patch) is still
  needed: cxx-qt-build 0.10 has no option for it.
- **cxx-qt 0.10** builds and runs on 6.11 unchanged, with no build warnings.
- **CI** still downloads the 6.6.3 archive. LIBRARIES.md §13.3 lists what
  has to be uploaded. Nothing was uploaded.

## 2. Qt 6.7-6.11: adopted and rejected

The sources: the What's New pages for 6.7 to 6.11, and the 6.11.3 sources.

| feature                                                              | since    | verdict               | why                                                                                                                                                                                                                                                                                                                |
| -------------------------------------------------------------------- | -------- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| DXGI vblank thread driving update requests (`QDxgiVSyncService`)     | 6.9      | **adopted**           | It paces the **basic** render loop, which is now our default. Focused animated idle fell 32-40 % (§4).                                                                                                                                                                                                             |
| `RectangularShadow` (Qt Quick Effects)                               | 6.9      | **adopted**           | `FloatShadow.qml` maps CSS box-shadow onto it. `cpp/shadowprovider.*` (133 lines) is deleted, with its CPU Gaussian blur and nine-slice textures. A light-theme screenshot of a menu matches the old one.                                                                                                          |
| `DISCARD_QML_CONTENTS` (`qt_add_qml_module`)                         | 6.8      | **adopted**           | The same done in the qt-build-utils patch: the `.qml` files stay in the resources, empty. 0.7 MiB.                                                                                                                                                                                                                 |
| qmlsc / qmlcachegen improvements                                     | 6.7-6.8  | **adopted, inverted** | Every QML file is now bytecode only. A fast scroll through 10k messages cost 15 % of a core against 18-21 % compiled, and animated idle was within the noise. That is 2.8 MiB.                                                                                                                                     |
| incremental GC, DirectWrite by default, batch-renderer pooling fixes | 6.8-6.9  | adopted (free)        | They come with the version. Pass 4 measured idle memory on the fixture at −14 MB; this pass's fixture idle is −5 MB private, with more UI since.                                                                                                                                                                   |
| `Qt::ExpandedClientAreaHint` (frameless title bar)                   | 6.9-6.11 | rejected              | On Windows, Qt draws its own caption buttons into a layered child window, which it repaints on every resize and hit test. It closes the window on a raw `HTCLOSE`. We would lose the QML bar's hover and press states and the snap-layout flyout. `windowchrome.cpp` stays.                                        |
| `VectorImage` / svgtoqml for Phosphor                                | 6.8      | rejected              | Each icon would be a Shape subtree with its own geometry nodes, against one raster per size and colour in the atlas today. The slash and the off-pair compositing would need QML per icon.                                                                                                                         |
| Qt Multimedia FFmpeg backend                                         | 6.7+     | rejected              | On Windows it loads FFmpeg DLLs (tens of MB), or a static FFmpeg in the exe. FFmpeg's own VP9 decoder drops the BlockAdditional alpha that nameplates use, so `yap-webm` stays. The WMF backend still plays mp4.                                                                                                   |
| HTTP/2 in `QNetworkAccessManager`                                    | -        | rejected              | 6.11 has no `http2` feature to configure out, so there is no size gain. The only client change since 6.6 is `setMaxConcurrentStreams` (6.9). Images are disk-cached and QML loads at most 8 at once, so I measured no reason to lift the HTTP/1.1 pin, which guards against the stall that froze the emoji picker. |
| `SearchField`, `FlexboxLayout`                                       | 6.10     | rejected              | SearchField is a combo box with a suggestion popup and would need the design restyled onto it. FlexboxLayout is a tech preview, and nothing of ours needs flex.                                                                                                                                                    |
| `Popup.popupType: Popup.Window`, variable fonts                      | 6.7-6.8  | not this pass         | These are UI features, not performance (LIBRARIES.md §6). Variable cuts would also be larger than our 1.2 MB of subset static cuts.                                                                                                                                                                                |
| Text fields' default `ContextMenu`                                   | 6.9      | turned off            | See §1.                                                                                                                                                                                                                                                                                                            |
| FluentWinUI3 / Windows 11 style, StyleKit                            | 6.8-6.11 | rejected              | We have our own design. Both are configured out.                                                                                                                                                                                                                                                                   |
| `Loader`, `AnimatedImage`, image caching                             | -        | nothing to adopt      | No change in the notes except `Image.retainWhileLoading` (6.8), which fixes flicker, not cost.                                                                                                                                                                                                                     |

## 3. Exe size: 63.8 to 59.1 MB

| step                                                                                                                  | exe (B)        |
| --------------------------------------------------------------------------------------------------------------------- | -------------- |
| before (6.6.3)                                                                                                        | 63,847,424     |
| the same source on pass 4's 6.11.3 build                                                                              | 72,761,344     |
| the 6.11 configure trimmed (below), fat Rust LTO, `/GL` for `cpp/`, more cold QML as bytecode, empty `.qml` resources | 66,149,888     |
| all QML bytecode only (−2.8 MiB); H.264 decoded by Media Foundation, so libwebrtc's FFmpeg is not linked (−3.1 MiB)   | 61,015,552     |
| no AV1 decoder (dav1d)                                                                                                | **59,124,224** |

By part (MiB, from the link map): Qt and the CRT 21.79 → 23.81, Rust
14.58 → 14.21, libwebrtc 12.77 → 8.80, QML 7.38 → 5.16.

- **The 6.11 configure** (`patches/qt/build-qt.ps1`):
  - **`timezone_locale` off: 3.75 MiB.** It holds CLDR time-zone display
    names, and the app never shows a zone name. Windows' backend required
    it, so patch `0003` falls back to Windows' own zone names.
  - **FreeType off: 0.5 MiB.** 6.11 turned it back on despite
    `-no-freetype`, so it needs `-no-feature-freetype`.
  - **Other features with no user** (each listed in the script): calendars,
    the PDF writer, ODF and Markdown, shared memory, SOCKS5, DNS lookups,
    the Network List Manager monitor, TableView/TreeView, sprites,
    particles, XMLHttpRequest, WorkerScript, PPM and XBM. The unused
    Controls styles are off too, which shortens the build. XPM has to stay:
    drag and drop depends on it, and `DropArea` (file drops on the chat)
    disappears without it.
  - **Shaders without GLSL or MSL** (`0002`): little, since SPIR-V and DXBC
    make up most of each `.qsb`.
- **Rust LTO "fat"** instead of "thin": −0.4 MiB, and the release build
  takes 5 minutes longer.
- **`/GL` for our own C++** (`CMakeLists.txt`). It cannot be used for the
  C++ that `cc` builds into the Rust static library: rustc's archive has no
  symbol index for `/GL` objects, so the link cannot find them.
- **Rejected:**
  - `panic = "abort"`: `yap-capture`, `yap-webm`, the installer and
    shutdown rely on `catch_unwind`;
  - cross-language LTO: Qt is MSVC LTCG and Rust is LLVM bitcode;
  - the QML compiler or JIT out of Qt: the engine needs them;
  - libwebp's encoder: ~0.15 MiB, not worth a patch;
  - fewer symphonia formats: 0.05-0.3 MiB each, and a behaviour change.

**Decisions to confirm** (both are patches that can simply be removed):

- **No AV1 decoder: 1.9 MB.** No Yap client sends AV1 today. The native
  client sends VP8 and H.264, and the web and mobile ones leave the
  browser's or libwebrtc's default, VP8 first: no `setCodecPreferences` in
  their tracked sources. With Cloudflare's SFU the publisher alone picks
  the codec, so a future AV1 publisher would show nothing here.
- **No FFmpeg H.264 decoder: 3.3 MB.** Windows N without the Media Feature
  Pack has no Media Foundation H.264 decoder. On such a machine H.264 is
  not offered and a hardware share would not play. Video playback and the
  hardware share encoder need Media Foundation there anyway.

## 4. Focused animated idle: 7.25 % to 4.95 %, not under 3 %

The scene is pass 3's: the chat-10k fixture, a GIF rail icon and two chat
GIFs, with the window told it is active. Presents are unpaced and each run
lasts 40 s. The three builds were interleaved, three rounds, under the
shared-machine load:

| build                               | CPU (one core)              | renders a second |
| ----------------------------------- | --------------------------- | ---------------- |
| before (6.6.3, threaded loop)       | 7.03, 7.38, 7.34 (7.25)     | 25.7             |
| after, `QSG_RENDER_LOOP=threaded`   | 8.55, 6.01, 7.15 (7.24)     | 25.7             |
| **after (basic loop, the default)** | **5.08, 5.39, 4.37 (4.95)** | 25.5             |

On a quieter machine earlier in the session, the 6.11 build ran at 6.42 %
threaded and 3.89 % basic.

- **What changed:** the basic render loop (`main.cpp`). Our frames take
  1-2 ms, so the threaded loop's overlap bought nothing, and its per-frame
  handoff between the GUI and render threads cost more than that. Qt 6.9's
  DXGI vblank thread now paces the basic loop on Direct3D, which made it
  viable.
- **Checked:** static idle stays at 0.00 % (0.03 % threaded), so the vblank
  thread costs nothing at rest. A fast wheel scroll has a p99 frame of
  2 ms and none over 16 ms. Settings, menus, a call with a 1440p60 share
  and a watcher all drew correctly. Pass 3's glyph-cache release reaches
  this loop through the extended `0001`.
- **Why not under 3 %:** a frame still costs ~1.5-2 ms of CPU, and about
  half of that is in the NVIDIA driver's threads with presents unpaced.
  Next:
  - measure on an awake display, since a paced present may cost less;
  - profile the GUI thread with WPR (needs elevation, through the
    elevate-broker);
  - cut renders by moving chat GIFs to a 20 Hz grid. That is a smoothness
    trade, and was not taken.

## 5. Hardware decode for watched shares

Patch `webrtc-sys/0002` adds a `VideoDecoder` on Media Foundation's H.264
MFT:

- it decodes on the GPU through a D3D11 device manager, made per stream and
  released with it;
- it reads each NV12 picture back with `IMF2DBuffer::Lock2D` and converts
  it to I420 with libyuv;
- low-latency mode is on;
- without a usable GPU it runs the same MFT in software.

The decoder factory uses it for every H.264 stream on Windows.

The owner shared the synthetic source over NVENC. The peer, on our build or
on the before build, watched it. CPU is per 25 s:

| watching                     | before: FFmpeg                 | after: Media Foundation, GPU         | Media Foundation, software |
| ---------------------------- | ------------------------------ | ------------------------------------ | -------------------------- |
| 1440p60, process             | 14.1 s, 13.4 s (**55 %**)      | 10.5 s, 11.7 s, 11.6 s (**45 %**)    | 32.2 s (129 %)             |
| 1440p60, decoder thread      | 4.1-4.3 s                      | 0.7-0.9 s                            | -                          |
| 1080p60, process             | 7.7 s (**31 %**)               | 7.6 s (**30 %**)                     | -                          |
| decode time (stats line)     | 2.4-2.6 ms 1440p, 1.0 ms 1080p | 7.0-7.4 ms (GPU decode and readback) | 4.1 ms                     |
| private while watching 1440p | 233-237 MB                     | **330-339 MB**                       | 257 MB                     |
| private 20 s after leaving   | 170.7 MB                       | 171.0 MB                             | -                          |

- **CPU:** −15 to −20 % at 1440p60 and no change at 1080p60. The decoder
  thread itself falls by 80 %. Media Foundation's own threads and the
  readback take most of that back, and the GUI thread's upload of the
  picture into Qt (~2.8 s per 25 s at 1440p) stays the same.
- **Memory:** +100 MB while watching 1440p, from the D3D11 device and the
  decoder's surfaces. All of it goes when the stream ends. A first version
  kept one device for the process and held it; the device is now per
  stream.
- **The real gain needs zero copy:** the decoded D3D11 texture handed to Qt
  Quick on a shared device, with no readback and no I420 upload. That
  means a native-texture `QVideoFrame` path in `videohub.cpp` and its own
  pass.

## 6. 1440p60 share: 60 fps at 47 % of a core

The owner shared the synthetic 2560x1440 source at the 1440p60 preset over
NVENC (`NVIDIA H.264 Encoder MFT`), with one watcher:

- **sent:** 2560x1440 at 60-61 fps, 14 Mbps, 6.5 ms an encode, with no
  dropped input (in 16,446, out 16,439, 5 replaced);
- **process CPU:** 11.7 s per 25 s, **47 % of one core**. The parts:
  - the generator thread 3.6 s, which includes `push_screen_frame`'s BGRA
    to NV12;
  - the GUI thread 2.7 s, drawing the share's own preview;
  - `yap-h264` 1.2 s;
  - WebRTC's network and worker threads 1.7 s;
- **private:** 351 MB while sharing, 196 MB 20 s after leaving;
- **at 1080p60:** 61 fps, 8 Mbps, 4.8 ms an encode, 9.8 s per 25 s.

A real WGC share adds the capture's GPU readback and drops the generator's
pattern drawing, so its CPU should be close. It still needs an awake
display.

## Before and after (pass 5)

| metric                                    | before                       | after                                               |
| ----------------------------------------- | ---------------------------- | --------------------------------------------------- |
| exe                                       | 63,847,424 B                 | **59,124,224 B** (−7.4 %)                           |
| focused animated idle, unpaced (3 rounds) | 7.25 %                       | **4.95 %**                                          |
| static idle CPU                           | 0.05-0.10 %                  | 0.00 %                                              |
| fixture idle private / ws (4 runs each)   | 193.8-194.5 / 172.0-172.6 MB | **188.7-190.7** / 168.1-175.7 MB (−5 MB)            |
| first frame, fixture                      | 677-945 ms                   | 540-663 ms (one run at 3.5 s, during a cargo build) |
| watching a 1440p60 share, CPU             | 55 %                         | **45 %**                                            |
| watching a 1440p60 share, private         | 233-237 MB                   | 330-339 MB (released on leaving)                    |
| 1440p60 share                             | not measured                 | 60 fps, 47 % of a core (synthetic source)           |
| Qt                                        | 6.6.3 (out of support)       | 6.11.3                                              |

## Targets

1. **Qt 6.11.3: done** and the default. The CI archive is still to upload
   (LIBRARIES.md §13.3).
2. **Qt 6.7-6.11: done** (§2). Adopted: the vblank-paced basic loop,
   `RectangularShadow`, discarded QML contents and the 6.11 features
   configured out.
3. **Exe under 60 MB: reached, 59.1 MB (56.4 MiB),** after the two
   decisions in §3.
4. **Animated idle under 3 %: not reached.** 4.95 % under load, 3.89 % on a
   quiet machine, from 7.25 % and 5.9 %. See §4 for what is next.
5. **Hardware decode: done,** with a CPU gain only at 1440p and a memory
   cost while watching. Zero copy is the next step.
6. **1440p60 share: measured** with a synthetic source, at 60 fps and 47 %.
   A WGC capture needs an awake display.

## Files

Source (worktree `native/perf3`):

- C++:
  - `cpp/main.cpp`: the basic render loop, and the shadow provider removed;
  - `cpp/shadowprovider.cpp` and `cpp/shadowprovider.h` deleted;
  - `CMakeLists.txt`: `/GL` and `mfplat mfuuid`.
- QML (10 files):
  - `components/FloatShadow.qml`: `RectangularShadow`;
  - `screens/Shell/ChatHeader.qml`: the `/` guard;
  - `ContextMenu.menu: null` in `components/KitField.qml`,
    `components/MessageEditor.qml`, `screens/Panels/ThreadPanel.qml`,
    `screens/Shell/AccountPopout.qml`, `screens/Shell/Composer.qml`,
    `screens/Shell/EmojiPicker.qml`, `screens/Shell/QuickSwitcher.qml` and
    `screens/SignIn/SignInView.qml`.
- Rust and build:
  - `rust/build.rs`: all QML bytecode only, the list deleted, and
    `QT_BUILD_UTILS_DISCARD_QML_CONTENTS`;
  - `rust/Cargo.toml`: fat LTO;
  - `rust/crates/yap-capture/src/win/fake.rs` (new) and `win/mod.rs`:
    `YAP_FAKE_SHARE_SOURCE`;
  - `build.ps1`: Qt 6.11.3 and VS 2022.
- Patches (LIBRARIES.md §13):
  - `patches/qt/build-qt.ps1`;
  - `patches/qt/6.11.3/0001` (extended), `0002` and `0003` (new);
  - `patches/qt-build-utils/0002-discard-qml-contents.patch` (new);
  - `patches/webrtc-sys/0002-mf-h264-decoder.patch` and
    `0003-no-av1-decoder.patch` (new);
  - `patches/vendor.ps1`.
- Docs: this section, and LIBRARIES.md §6, §12 and §13.

# Performance pass 6 (2026-11-07)

The brief, in order:

1. zero-copy decode into Qt for watched shares;
2. per-frame render CPU, with focused animated idle under 3 %;
3. idle memory towards 120 MB;
4. with awake displays, a real window-capture 1440p60 share and the
   animated idle with vsync;
5. what CI needs for the Qt 6.11 archive.

The coordinator added a sixth: the startup cost of making all QML
bytecode-only in pass 5.

Before is pass 5's final build (`<scratch>/yapn/tmp/p6/before`, the same
binary as `tmp/p5/c7.exe`, 59,124,224 B). After is this pass's final build
(`<scratch>/yapn/tmp/p6/final`). Intermediate builds, all from the `native/perf3`
worktree:

| build | what it adds                                                                      |
| ----- | --------------------------------------------------------------------------------- |
| `zc`  | zero-copy decode (§1)                                                             |
| `url` | `zc` + cold screens loaded by URL (§3)                                            |
| `aot` | `url` + the startup path compiled to C++ again, shared message authors (§3, §6)   |
| `c1`  | `aot` + the in-process sampler (§2)                                               |
| final | `c1` without the startup C++ (§6), on the Qt with patch `0004` (§2); 59,164,160 B |

## Conditions

- **The displays were awake** for the whole session: vblank waits ran at
  60.0 Hz on all three (`tools/vblank.ps1`). Pass 5's unpaced switch was
  still used for comparisons with its numbers.
- **The machine was shared.** A `qemu` VM used 1-2.5 cores all session, and
  another agent's builds (`link.exe` at ~1.8 cores) ran for part of it. CPU
  numbers are compared only within interleaved runs.
- **The other agent's session.** At 14:02 a `--profile spike` session from
  `<scratch>/yapn/run/sweep10` was running; nothing of mine started until it had
  ended. The pass 6 tools refuse to start while another agent's YapNative or
  node peer runs (`tools/p6/app.ps1`; pass 5's copy carried a tab character
  in its own path and so matched nothing of its own).
- **WPR was not available.** The elevate-broker request for the xperf trace
  timed out unapproved, so the GUI thread was profiled in-process
  instead (§2, `YAP_DEBUG_SAMPLE`). The driver's own threads are seen only
  as per-thread CPU.

## Method (new)

Tools are in `<scratch>/yapn/tools/p6/`:

- `owner.ps1`: the owner on profile `perf6` sharing the synthetic source
  (`YAP_FAKE_SHARE_SOURCE`) at 1440p60 or 1080p60, by posted clicks;
- `watch.ps1`, `watch-heap.ps1`: the peer on `perf6peer` watching it, with
  per-thread CPU over 25 s and three heap breakdowns
  (`YAP_DEBUG_HEAP_HOOK=0`);
- `leave.ps1`: hang up and stop one of ours by PID;
- `anim-idle.ps1`, `anim-ab.ps1`: focused animated idle, builds interleaved,
  paced and unpaced; `-NoTiming` drops `QSG_RENDER_TIMING` (below);
  `<build>@nozc` runs a build with `YAP_ZERO_COPY=0`;
- `startup.ps1`, `startup-ab.ps1`: startup lines of fixture launches,
  builds interleaved, with `-Fresh` for a fresh profile;
- `heap-idle.ps1`: the traced heap of the static fixture at idle;
- `sample-anim.ps1`, `samplesym.py`: GUI-thread samples of the animated
  idle, named from the link map;
- `xperf-session.ps1`: a one-approval elevated xperf trace (not approved
  this session);
- `test.ps1`: tests and clippy on the custom Qt.

New in the app (off unless set):

- **`YAP_DEBUG_SAMPLE=<prefix>`** (`cpp/sampler.cpp`): a sampling profiler
  of the GUI thread that needs no elevation. Each time `<prefix>.now`
  appears it samples for `YAP_DEBUG_SAMPLE_SECONDS` (default 20): it
  suspends the thread every millisecond, unwinds its stack with the
  modules' unwind tables and resumes it. Frames are written as `exe+rva`,
  or a DLL's nearest export. `samplesym.py` names them, leaves out the
  stacks parked in a wait, and sums by leaf, by function anywhere on the
  stack and by area. A trigger written before launch samples the startup.
- **`YAP_ZERO_COPY=0`**: the copying video path (comparisons).

## 1. Zero-copy decode into Qt

### What changed

- **One D3D11 device for the window and the decoder**
  (`VideoHub::shareDevice`, called before the window is first shown).
  - The app creates it as Qt would (BGRA support, the first hardware
    adapter), plus video support for the decoder, and turns on
    multithread protection, since the decoder's thread and the GUI
    thread both use it.
  - Qt Quick takes it through
    `QQuickWindow::setGraphicsDevice(QQuickGraphicsDevice::fromDeviceAndContext(...))`,
    and the decoder through `yap_mf_set_shared_device` (webrtc-sys patch
    `0004`).
  - Off Direct3D 11, with `YAP_ZERO_COPY=0`, or when the device cannot be
    made, nothing changes.
- **The decoder** (`0004`) decodes on that device. Each picture is copied
  on the GPU (`CopySubresourceRegion`) out of the decoder's texture array
  into an NV12 texture from a small pool per stream, and the decoder gets
  its surface back at once. The frame travels through libwebrtc as a
  native buffer; `ToI420()` reads it back only for a consumer that needs
  pixels.
- **Into Rust and back** (`libwebrtc` patch `0001`, `yap-voice`,
  `src/video.rs`): `pump_video` hands a native frame's C++ buffer to the
  output (`VideoOutput::native_frame`); only an output that cannot take it
  gets it read back.
- **Into Qt** (`cpp/videohub.cpp`): `yap_video_native_frame` wraps the
  texture in a `QVideoFrame` whose buffer is a `QHwVideoBuffer`
  (`D3D11FrameBuffer`). Its `textureHandle()` returns the texture when the
  QRhi is ours, and `VideoOutput` samples it as it is: Qt's NV12 shader
  through an R8 view for luma and an R8G8 view for chroma. No readback, no
  I420 conversion, no upload. `map()` reads back once for any consumer
  without our device.
- The `QVideoFrame` holds the decoder's frame, and with it the texture, so
  the pool reuses a texture only once Qt has let it go. The device's
  immediate context orders the next copy after the draw that sampled it.

### Results

The owner shared the synthetic source over NVENC (pass 5's `before` exe);
the peer watched, alternating builds. CPU is per 25 s of the watcher:

| watching                                        | before (pass 5)                  | after (zero copy)                                    |
| ----------------------------------------------- | -------------------------------- | ---------------------------------------------------- |
| 1440p60, process                                | 10.36, 10.70, 10.83 s (**42 %**) | 8.20, 8.97, 8.41 s; final build 7.89 s (**32-36 %**) |
| 1440p60, GUI thread                             | 2.50-2.80 s                      | 1.39-1.69 s                                          |
| 1440p60, decoder thread                         | 0.73-0.94 s                      | 0.33 s                                               |
| 1440p60, `yap-core` (I420 to Qt)                | 0.67-0.97 s, two threads         | 0.28 s                                               |
| 1080p60, process                                | 8.83, 8.62 s (**35 %**)          | 6.41, 6.48 s (**26 %**)                              |
| frames shown                                    | 60 fps                           | 59-60 fps                                            |
| private while watching 1440p60                  | 337-346 MB                       | **257-306 MB**; final build 230-266 MB               |
| private while watching 1080p60                  | 286-290 MB                       | **238-267 MB**                                       |
| 20 s after leaving                              | 185 MB                           | 172 MB                                               |
| heap / driver-private / write-comb. (1440p, MB) | 134 / 127 / 56                   | 98 / 99 / 57-80                                      |

- **CPU: −20 % at 1440p60 and −26 % at 1080p60.** What is left is the
  call itself: WebRTC's network thread (2.4-2.6 s, receiving 14 Mbps), the
  synthetic audio device and the test mic (~1.2 s), and the GUI thread
  drawing the call.
- **Memory: −60 to −80 MB while watching.** The heap lost the I420 frames
  and their `QVideoFrame` copies (−36 MB); the rest is the second D3D11
  device and the MFT's staging copies that are gone. The write-combined
  memory moves between 57 and 80 MB as the driver pools the texture
  uploads of the rest of the UI; it does not grow.
- **The picture is the same** (screenshots `tmp/p6/share/w-*.png`), at the
  same colour conversion as before (the format's default colour space).
- **The cost when nothing is watched:** +1.5 MB private at idle (the
  device has video support), and no measurable CPU (§2's A/B,
  `c1@nozc`).

## 2. Per-frame render CPU and focused animated idle

### The measurement itself was a third of the cost

Profiling the GUI thread (`YAP_DEBUG_SAMPLE`, 60-90 s of the animated idle)
put **13-19 % of its running samples in `ZwWriteFile`**: the
`QSG_RENDER_TIMING` line Qt writes for every frame, which the measurement
scripts of passes 4 and 5 turned on to count renders, going through our
message handler to the redirected stderr. The "2.2 ms a frame" of passes
4 and 5 included it.

Focused animated idle (pass 3's scene: chat-10k, a GIF rail icon and two
chat GIFs, 25.5 renders a second), 40 s runs interleaved, without the
logging:

| build                                | vsync (awake display)        | unpaced    |
| ------------------------------------ | ---------------------------- | ---------- |
| before (pass 5)                      | 3.16, 3.01, 4.02, 3.40, 3.08 | 2.77, 2.62 |
| `c1`                                 | 3.44, 3.36, 3.12, 3.20, 3.91 | 3.40, 3.09 |
| `c1`, own device (`YAP_ZERO_COPY=0`) | 4.06, 3.83, 3.87             |            |

A last round, the three builds interleaved four times (vsync):

| build             | CPU (one core)             | mean   |
| ----------------- | -------------------------- | ------ |
| before (pass 5)   | 3.24, 4.18, 4.34, 3.71     | 3.87 % |
| `c1`              | 4.84, 3.32, 3.44, 3.24     | 3.71 % |
| final (Qt `0004`) | **2.93**, 4.22, 3.79, 3.40 | 3.59 % |

The machine's load moved single runs by up to a point, more than any
difference between the builds. What the A/Bs show is that none of this
pass's changes costs CPU at idle, and that the animated idle is at 3-4 %
of a core, not the 5-7 % passes 4 and 5 reported.

With the logging, as passes 4 and 5 measured: before 5.43 and 6.33 %
paced, 5.19 and 5.51 % unpaced. The shared device's multithread
protection costs nothing measurable: no lock shows in the samples, and the
own-device runs are not lower.

### Where a frame goes

GUI-thread samples of the animated idle (`c1`, vsync, 483 and 518 running
samples over 60 s):

| area of the leaf frame                                             | share of the GUI thread |
| ------------------------------------------------------------------ | ----------------------- |
| the D3D11 runtime and the NVIDIA driver (draw submission, Present) | 41-43 %                 |
| the logging write (gone without `QSG_RENDER_TIMING`)               | 13-19 %                 |
| the batch renderer and QRhi                                        | 16-19 %                 |
| of which, copying each SRB's batched bindings at every bind        | 9-11 %                  |
| GIF decoding (`QGIFFormat::decode`, the streamed frames)           | 3-3.5 %                 |
| sync, polish, QML, animations                                      | < 1 %                   |

- **Scene-graph sync and our items cost almost nothing**: the animated
  items change a texture, and the sync of the static subtrees is below
  the sampling's resolution. Skipping their sync would gain nothing.
- **`QRhiD3D11::setShaderResources` deep-copied the SRB's batched
  bindings at every bind.** The batch renderer binds with dynamic offsets
  for each draw, so each draw copied six stages of `QRhiBatchedBindings`
  (`QVarLengthArray`s) into the command buffer's retain pool. **Qt patch
  `0004`** makes the batches an immutable snapshot, rebuilt only when the
  bindings change, and the command buffer keeps a reference. In the final build's samples
  the copy constructors are gone (they were 10-11 % of the GUI thread);
  `setShaderResources` itself stays at ~5 %. At the process level the
  gain (~0.3 point, by the samples) is inside the machine's noise (table
  above).
- The rest is the driver submitting the frame's draws and presenting.
  Fewer renders or fewer draws are what is left; see Targets.

### Ideas from the brief, evaluated

- **Only re-render the animated item's layer:** Qt Quick has no partial
  update on D3D11; a frame re-records every batch. A `layer` over the
  static chat would cache it as one texture (8 MB of video memory at this
  size) but the GIFs sit inside the list, so the layer would be redrawn
  with them. Not done.
- **A slower tick for many GIFs on one clock:** pass 4 put chat GIFs on the
  shared 30 Hz grid; the scene renders 25.5 times a second, which is the
  GIFs' own frame rates on that grid. A 20 Hz grid stays a smoothness
  trade the brief has not taken.
- **Skipping the sync of static subtrees:** < 1 % of the GUI thread (above).
- **Swapchain present mode, waitable objects, DirectComposition:** Qt 6.11
  already presents with a flip-model swapchain, and the basic render loop
  is paced by the DXGI vblank thread (pass 5). Present's own cost is in
  the driver share above; a waitable object changes latency, not CPU.
  Not changed.

## 3. Idle memory

On the static chat-10k fixture (traced heap, `heap-idle.ps1`, and the
idle runs of §2):

| build                   | live heap (traced) | private (idle runs) |
| ----------------------- | ------------------ | ------------------- |
| before                  | -                  | 190.8-193.1 MB      |
| `url`                   | 76.4 MB            | 188.4-191.5 MB      |
| `c1` (+ shared authors) | **57.1 MB**        | **168.5-171.1 MB**  |

### Shared message authors (−19 MB on the fixture)

The traced heap's largest owner was `MessageLog::upsert`: 24.7 MB for
10,000 messages. `Message` held its author as a full `User` inline (~0.5 KB:
some thirty `Option<String>` and JSON fields), and its `reply_to` held
another, so each `Message` was ~1.5 KB before any of its strings, and the
log's `Vec` doubled to 16,384 of them.

- `Message::author` is now `Option<Arc<User>>` (serde's `rc` feature) and
  `reply_to` is boxed (`yap-protocol`).
- `MessageLog` interns authors: a message whose author equals the one
  already held shares it (`MessageLog::intern`, on every insert path).
- A profile update to one author's messages (`social.rs`) copies on write
  (`Arc::make_mut`).

`MessageLog` fell from 24.7 to 5.4 MB, and Rust's share of the heap from
41.0 to 21.5 MB. A real channel holds hundreds of messages, not 10,000, so
there the saving is ~1-2 KB a message of scrollback.

### Cold screens by URL (−3 MB, −30 ms of startup)

A type named in a document is compiled and linked when the document is,
even inside a `Component` that is never made. `Shell.qml`,
`ExtrasHost.qml`, `AdminHost.qml` and `JoinHost.qml` now load their cold
screens by URL (`source:` instead of `sourceComponent: X {}`): Settings,
the profile editor and modal, Shop and Friends, the server and channel
settings, the create and invite dialogs, the quick switcher and the
reaction, share, invite, voice status, voice switch and start-call
dialogs. Their compilation units, property caches and types now load on
first open.

- Private −3 MB at idle (188.4-191.5 against 193.0-194.7 MB, same exe
  otherwise).
- `qml loaded` 232 → 202 ms, first frame 518 → 480 ms (6 interleaved runs
  each, below).
- First opens checked on the fixture (`tools/p6/popups.ps1`, posted
  clicks; screenshots in `tmp/p6/pop/`): Settings, the invite dialog,
  Friends, Shop and the quick switcher draw as before. The one QML
  warning, `QuickSwitcher`'s `Window.window`, comes from the before build
  too.

### Not done

- The bridge's message rows (`Vec<MessageRow>`, 5.3 MB on the fixture)
  are the second copy of each message; sharing them with the core's view
  needs `Arc` rows through the model macros.
- The QML engine (16 MB of heap and 4 MB of JS pages) is the largest owner
  left, then Qt core (8 MB) and the scene graph (6.6 MB).

## 4. Awake displays

- **Animated idle with vsync:** measured (§2). Paced and unpaced cost the
  same within the noise once the logging is gone.
- **A real 1440p60 window capture: not measured.** All three displays are
  1920x1080, so no window or screen here is 1440p, and with
  `YAP_SHARE_ONLY_OURS` the only windows a test may capture are our own
  (1268x825, mostly static, so WGC delivers frames only when they change).
  The synthetic 1440p60 source (pass 5: 60 fps, 47 % of a core) remains
  the number.

## 5. CI for the Qt 6.11 archive

Written down in LIBRARIES.md §13.3. In short: pack the installed Qt with 7z
(LZMA2 keeps 10 % of an LTCG library, ~0.9 GB for the 8.2 GB tree; zip's
deflate keeps 26 %, ~2.1 GB, over GitHub's 2 GB asset limit), keep the
private headers (`GuiPrivate` and `MultimediaPrivate` are now needed),
unpack with `7z x` under `RUNNER_TEMP`, and build with Visual Studio 2022
17.14 (MSVC 14.44, the Qt build's). Nothing was uploaded.

## 6. Startup: bytecode-only QML

Warm launches of the chat-10k fixture, six rounds interleaved:

| build                                 | fonts    | qml loaded     | first frame    |
| ------------------------------------- | -------- | -------------- | -------------- |
| before (pass 5: all bytecode)         | 46-52 ms | 227-238 ms     | 503-542 ms     |
| `url` (cold screens by URL)           | 45-52 ms | **194-213 ms** | **471-488 ms** |
| `aot` (`url` + startup path compiled) | 46-52 ms | 194-208 ms     | 472-497 ms     |

- **Compiling the startup path to C++ again does not pay.** Main, Shell,
  the chat view, the rail, the sidebar, the composer and the message
  delegate (with the Theme and ShellState singletons) as C++ gave the
  same `qml loaded` as bytecode, for 0.6 MB of exe (59.78 MB, 0.2 MB under
  the limit). The final build keeps every file bytecode.
- **The 147 ms of 6.6.3 was the Qt version, not the compile mode:** pass 5
  measured the same source on 6.11 at 15 % less scroll CPU with bytecode,
  and here the compiled startup path loads no faster.
- **The cold first run.** A fresh profile (`startup.ps1 -Fresh`) started
  in 235 ms here, and a fresh copy of the exe in 221 ms. One first launch
  right after a link took 1,993 ms, and others at 400-650 ms ran while
  another agent's link used two cores: a cold disk and a loaded machine,
  not the QML. The coordinator's 1,555 ms fresh-profile run is consistent
  with that.

## Before and after (pass 6)

| metric                                        | before (pass 5)      | after (pass 6)                         |
| --------------------------------------------- | -------------------- | -------------------------------------- |
| watching a 1440p60 share, CPU per 25 s        | 10.4-10.8 s (42 %)   | **7.9-9.0 s (32-36 %)**                |
| watching a 1440p60 share, private             | 337-346 MB           | **230-306 MB**                         |
| watching a 1080p60 share, CPU per 25 s        | 8.6-8.8 s (35 %)     | **6.4-6.5 s (26 %)**                   |
| watching a 1080p60 share, private             | 286-290 MB           | **238-267 MB**                         |
| focused animated idle, as passes 4-5 measured | 5.4-6.3 % (vsync)    | a third of that was the logging        |
| focused animated idle, without the logging    | 3.0-4.3 %            | 2.9-4.2 % (the same, within the noise) |
| fixture idle private                          | 190.8-193.1 MB       | **168.5-171.8 MB** (−21 MB)            |
| fixture traced heap                           | 76.4 MB (`url`)      | **57.1 MB**                            |
| `qml loaded` / first frame, warm              | 228-238 / 487-542 ms | **195-204 / 456-482 ms**               |
| exe                                           | 59,124,224 B         | 59,164,160 B                           |

## Targets

1. **Zero-copy decode: done.** −20 % CPU and −60 to −80 MB at 1440p60,
   −26 % CPU at 1080p60, at 60 fps with the same picture. Left:
   - the call's own cost while watching: WebRTC's network thread takes
     2.4-2.8 s per 25 s at 14 Mbps, inside libwebrtc;
   - VP8 and VP9 (the camera and software shares) still decode on the CPU
     and go through I420; a Media Foundation VP9 decoder could use the
     same path;
   - device loss: Qt cannot recreate an imported device, so a driver reset
     would leave the window without one (`YAP_ZERO_COPY=0` is the way
     back). Not seen and not tested.
2. **Animated idle under 3 %: about there, and not provable on this
   machine.** A third of passes 4-5's 2.2 ms a frame was the measurement's
   own logging. Without it the scene costs 3-4 % of a core under this
   machine's load, and 2.9 % in the quietest run. What is left is the
   driver (~42 % of the GUI thread) and Qt's batch renderer. Next, if it
   must go lower:
   - fewer renders: a 20 Hz grid for chat GIFs, a smoothness trade;
   - fewer draws per frame: the scene's batch count
     (`QSG_RENDERER_DEBUG=render`);
   - a quiet machine (no VM, no other agent's builds) to resolve 0.2-point
     differences at all, or WPR once it can be approved.
3. **Idle memory towards 120 MB: 192 to 170 MB on the fixture.** Left,
   largest first:
   - the QML engine (16 MB of heap, 4 MB of JS pages);
   - Qt core (8 MB);
   - the scene graph (6.6 MB);
   - the bridge's second copy of each message row (5.3 MB on this
     fixture);
   - the GPU driver's own memory outside the heap.
4. **Awake displays:** the paced animated idle is measured. A real 1440p60
   window capture cannot be made on 1080p displays with only our own
   windows to capture.
5. **CI: written down** in LIBRARIES.md §13.3. Nothing was uploaded.
6. **Startup:** loading cold screens by URL took 30 ms off `qml loaded`.
   Compiling the startup path to C++ did not help, and was reverted.

## Files

Source (worktree `native/perf3`):

- C++:
  - `cpp/videohub.cpp`, `cpp/videohub.h`: the shared device and the
    zero-copy frames;
  - `cpp/main.cpp`: `VideoHub::shareDevice` before the first show, and the
    sampler;
  - `cpp/sampler.cpp`, `cpp/sampler.h` (new): `YAP_DEBUG_SAMPLE`;
  - `CMakeLists.txt`: `GuiPrivate`, `MultimediaPrivate`, the sampler.
- QML (4 files, one line or block each):
  - `screens/Shell/Shell.qml`: Friends and Shop, the profile modal,
    Settings and the profile editor by URL;
  - `screens/Shell/ExtrasHost.qml`: the seven extras dialogs by URL;
  - `screens/Admin/AdminHost.qml`: the six admin dialogs by URL;
  - `screens/Join/JoinHost.qml`: the two join dialogs by URL.
- Rust:
  - `crates/yap-protocol/src/rest.rs`: `Message::author` shared,
    `reply_to` boxed;
  - `crates/yap-core/src/store/state.rs`: `MessageLog` interns authors;
  - `crates/yap-core/src/store/view.rs`, `store/reduce.rs`,
    `store/reduce/social.rs`, `store/reduce/threads.rs`, `fixture.rs`
    and `fixture/soak.rs`: the author's new type;
  - `crates/yap-voice/src/engine/mod.rs` and `engine/webrtc.rs`:
    `VideoOutput::native_frame`;
  - `src/video.rs`: `QtOutput::native_frame`;
  - `Cargo.toml` (serde's `rc`, the `libwebrtc` patch) and `Cargo.lock`;
  - `build.rs`: the comment on the startup-path experiment.
- Patches (LIBRARIES.md §12-13):
  - `patches/webrtc-sys/0004-mf-h264-zero-copy.patch` (new);
  - `patches/libwebrtc/0001-native-buffer-sys-ptr.patch` (new);
  - `patches/qt/6.11.3/0004-d3d11-shared-resource-batches.patch` (new),
    and `patches/qt/build-qt.ps1` (its comment);
  - `patches/vendor.ps1`: `libwebrtc`, and the note on plain diffs.
- Docs: this section, and LIBRARIES.md §12.1, §12.4, §13.1 and §13.3.

# Performance pass 7 (2026-11-07)

The brief, in order:

1. a real window-capture share at 1440p60;
2. hardware decode for VP8 and VP9;
3. idle memory under 120 MB;
4. animated idle under 3 %, proven with a WPR trace or by fewer draws;
5. CI, already written down (nothing uploaded).

The coordinator added a sixth: a Qt rebuild with UI pass 11's DirectWrite
contrast patch (§6).

Before is c4e8c212 with the one-line `view.rs` fix that spike/qt-desktop
also carries (`<scratch>/yapn/tmp/p7/before/`, 59,525,120 B). The other builds
are from the `native/perf3` worktree, on the pass 6 Qt
(`<scratch>/qt-custom-6.11p6`) unless noted:

| build   | what it adds                                                                                |
| ------- | ------------------------------------------------------------------------------------------- |
| `vp9`   | Media Foundation VP9/VP8 decode (§2) and the VP9 share test switch                          |
| `b3`    | `vp9` + one texture per streamed GIF, the QML trims, capped picture decodes, settle release |
| `final` | `b3` on the Qt with patch `0005` (§6)                                                       |

## Conditions

- **The displays were awake** (60.0 Hz vblanks). DISPLAY15 (a Parsec
  virtual display, 1920x1080 at 100 %) ran at 2560x1440@60 for the share
  test, at 100 % scaling, and was restored afterwards (`display.ps1 restore`,
  verified).
- **The machine was shared:** a `qemu` VM, and UI pass 11's sessions and
  builds. CPU is compared only within interleaved runs.
- **UI pass 11 used the same test accounts.** The pass 7 tools refuse to start
  while any other agent's `YapNative.exe` or node peer runs, and waited for
  free windows. Early in the pass two of my exes were named `before.exe` and
  `vp9.exe`, which other agents' checks for `YapNative` do not see. UI pass 11
  ran the owner account once while my `vp9.exe --profile perf7` was running.
  From then on every exe of mine is `tmp/p7/<build>/YapNative.exe`.
- **WPR was approved** by phone (the second request; the first ran as SYSTEM
  without `pwsh` on its PATH). One kernel trace of the animated idle was taken
  (§4).

## Method (new)

Tools are in `<scratch>/yapn/tools/p7/`, copied from p6 with paths and profiles
moved to `tmp/p7` and `perf7`:

- `idle.ps1`: idle memory on `-Fixture chat-10k` or `soak`. Soak is the
  real-account-like fixture: four channels, members, media from
  `tools/p4/media-server.mjs`, and 1 event/s with no channel switches. It
  takes optional QML object counts (`YAP_DEBUG_OBJECTS`) and a heap dump;
- `etlstacks.py`: our process's samples out of an xperf kernel trace
  (`xperf -i <etl> -quiet` piped in), named from the link map, per thread and
  per stack;
- `mft-probe.ps1`: the video decoder MFTs for H.264, VP8 and VP9;
- `cam.ps1`: watching the headless web peer's camera, with the codec it
  negotiated;
- `share-test-window.ps1`: pass N6b's test window, borderless, so it is
  exactly 2560x1440 (run it with Windows PowerShell 5.1: its WinForms
  types do not compile under pwsh 7);
- `wgc-start.ps1`: waits for the accounts to be free, then starts the owner
  and opens the share picker;
- `build.ps1` and `test.ps1`: on the pass 6 Qt by default.

New in the app (test switches, off unless set):

- `YAP_DEBUG_SHARE_CODEC=vp9`: a software share is published with VP9 first.
  No Yap client sends VP9, so this makes a VP9 stream to watch;
- `YAP_VPX_DECODER=software`: VP8 and VP9 stay on libvpx (comparisons).

## 1. Real window-capture share at 1440p60

The source was `share-test-window.ps1` at 2560x1440 on DISPLAY15, set to
2560x1440@60. It is a borderless window of ours that paints a moving pattern
at about 60 Hz and sits at the bottom of the Z order. The owner (`b3`,
profile `perf7`) shared it through the picker at 1440p 60 fps. WGC captured
it at 2560x1440 and NVENC encoded it (`NVIDIA H.264 Encoder MFT`). The peer
(`b3`, `perf7peer`) watched.

| per 25 s                     | sharer, no watcher | sharer, one watcher | watcher               |
| ---------------------------- | ------------------ | ------------------- | --------------------- |
| process CPU                  | 10.70 s (**43 %**) | 11.11 s (**44 %**)  | 6.94 s (**28 %**)     |
| frames                       | sent at 46-58 fps  | sent at 57-59 fps   | received at 53-58 fps |
| encode (NVENC) / decode (MF) | 6.9-8.7 ms         | 6.7-6.8 ms          | 0.1-0.2 ms, zero copy |
| private                      | 352-361 MB         | -                   | 272 MB                |
| threads                      | 202-210, steady    | 205                 | 123                   |

- **Sharer.** This matches pass 5's synthetic 1440p60 number (47 %) with a
  real capture. Its largest threads are:
  - the WGC frame callback, 2.6-3.1 s. It copies each frame to a staging
    texture, maps it (a GPU sync) and converts 2560x1440 BGRA to I420 on the
    CPU;
  - the GUI thread, 2.0-2.2 s, drawing the share's own preview;
  - two more, 1.2-1.8 s and 0.9-1.0 s;
  - `yap-h264`, 0.7 s, the I420 to NV12 copy into the MFT;
  - WebRTC's network and worker threads, 1.1 s.
- **166 of the sharer's threads are the NVIDIA driver's.** The count is
  steady over minutes, so it is a pool (NVENC's MFT and the capture's
  device), not a leak.
- **Watcher.** Pass 6's zero-copy H.264 path at 2560x1440. What is left is
  the call: WebRTC's network thread, 1.36 s at 14 Mbps.
- The screenshots are `tmp/p7/share/wgc-live.png` (sharer) and
  `w-wgc-b3.png` (watcher).
- **What would lower the sharer:** the zero-copy share in LIBRARIES.md §12.4.
  The WGC texture would go through a D3D11 video processor (BGRA to NV12 on
  the GPU) into the MFT on the same device, and NVENC takes D3D11 input. That
  removes the readback, both conversions and the NV12 upload: about half of
  the 43 %. It needs the encoder's input path in `mf_h264.rs` rewritten for
  textures, and the preview fed the same texture. It was not done this pass.

## 2. Hardware decode for VP8 and VP9

**What the machine has** (`tools/p7/mft-probe.ps1`, and a D3D11 probe in
`tmp/p7/probe/dxva.cpp`):

- The RTX 3080 has D3D11 decoder profiles for H.264 and VP9 profile 0, but
  not for VP9 profile 2 (10-bit) or VP8.
- The VP8 and VP9 decoder MFT is the VP9 Video Extensions' one
  (`VP9VideoExtensionDecoder`). It is D3D11-aware.
- Given a D3D11 device manager, it takes VP9. It refuses VP8 with
  `MF_E_UNSUPPORTED_D3D_TYPE`, since NVIDIA has no VP8 DXVA. Intel GPUs
  decode VP8.

**Patch `webrtc-sys/0005`** (LIBRARIES.md §13.1):

- Pass 5-6's Media Foundation decoder now takes a codec. For VP9 profile 0,
  and for VP8, it runs the VP9 Video Extensions' MFT on the GPU through the
  same zero-copy path (NV12 textures on the UI's device, sampled by Qt).
- It is GPU only. A probe, once per codec, checks that the MFT takes the
  codec with a D3D11 device manager. libwebrtc's software-fallback wrapper
  puts libvpx behind it: a failed Configure or any decode error (returned as
  `FALLBACK_SOFTWARE`) moves the stream to libvpx from the next key frame.
- The patch reproduces the vendored crate: `vendor.ps1 -Force`, then
  compared byte for byte.

**VP9, watching a 1080p60 share** (owner: software share with
`YAP_DEBUG_SHARE_CODEC=vp9`; watcher: `vp9`, alternating):

| per 25 s, watcher          | libvpx (`YAP_VPX_DECODER=software`) | Media Foundation, GPU, zero copy |
| -------------------------- | ----------------------------------- | -------------------------------- |
| process CPU                | 10.66 s (**43 %**)                  | 5.83 s, 5.19 s (**21-23 %**)     |
| `VideoDecoderQueue` thread | 1.25 s                              | (not in the top six)             |
| frames                     | 1920x1080 at 60 fps                 | 1920x1080 at 59-60 fps           |
| private while watching     | 212.5 MB                            | 276-277 MB                       |
| private 20 s after leaving | -                                   | 185.3 MB                         |

- **CPU: −45 to −51 %.** libvpx's frame threads and the I420 upload into Qt
  are gone. The picture is the same (`tmp/p7/share/w-vp9-mf.png`).
- **Memory: +64 MB while watching**, from the DXVA decoder's surfaces. It
  goes when the stream ends. This is the same trade as pass 6's H.264 (which
  is 238-267 MB at 1080p60).

**VP8: what the browsers send.** The web peer (headless Edge, the
production web client) publishes VP8 simulcast:

- `f` 1280x720, `h` 640x360 and `q` 320x180, each at 15 fps;
- the SFU forwarded `h` to our grid tile: `VP8 640x360@15 libvpx 0.1ms`;
- watching it cost 4.05 s per 25 s (16 %), most of it the call itself
  (WebRTC's threads, the mic and the audio device).

The web client sets no codec preference, so Chromium's order puts VP8
first. Mobile uses libwebrtc's default order, also VP8 first. VP8 has no
DXVA profile on this GPU, so it stays on libvpx here. A 640x360 VP8 decode
takes 0.1 ms, so the GPU would save little per camera.

## 3. Idle memory

**Baseline owners** (traced heap at idle):

| owner          | chat-10k (60.4 MB live) | soak (52.1 MB live) |
| -------------- | ----------------------- | ------------------- |
| QML engine     | 17.2 + 4.4 V            | 19.5 + 5.2 V        |
| Qt core        | 8.0                     | 9.6                 |
| scene graph    | 6.7                     | 9.3                 |
| pictures       | 1.3                     | 6.2                 |
| Rust core      | 23.5                    | 2.8                 |
| text and fonts | 2.4                     | 2.7                 |

**What is outside the heap.** `PrivateMemorySize` is 172 MB on chat-10k
with 134 MB of private allocations; the other ~37 MB is image copy-on-write
pages and pagefile sections (mostly the GPU driver's). Of the 134 MB:

- ~86 MB is the heap;
- ~13 MB is write-combined memory;
- the rest is the driver's own regions (`tools/vmmap.ps1`).

### What changed

- **Capped picture decodes** (`MediaTile.qml`):
  - A fitted chat picture was decoded at its own size, to learn its real
    size. Qt keeps the decoded image while it is shown: a 1280x720 picture
    cost 3.5 MB of heap, and a 12 MP phone photo would cost ~48 MB.
  - Once the real size is known (from that first decode, or learnt
    earlier), a picture larger than its box decodes at the box's size. The
    box is taken in 128 px steps, so a resize does not decode again at every
    pixel.
  - It never scales up. A capped decode does not overwrite the learnt size.
    `retainWhileLoading` keeps the first decode up while the capped one
    loads.
- **One texture per streamed GIF** (`cpp/animatedpicture.cpp`): §4.
- **A settle release** (`cpp/main.cpp`):
  - 20 s after the first frame, the debounced `releaseGraphicsSoon()` runs
    once.
  - It frees what startup's frames left behind:
    `QRhiResourceUpdateBatch` op lists grown to 1.35 MB, pooled buffers and
    unused glyph caches.
  - It costs one pipeline rebuild on the next frame.
- **Fewer live QML objects** (6,362 to 5,392 on soak, −15 %). These are
  made only while they are used:
  - `BootView`: after its fade (92 objects in the signed-in shell);
  - `SignInView`: while signed out (43);
  - each tree row's `VoiceStatusLine`: on the row that draws a status (282);
  - each member's `NameplateLayer`: for a member with a nameplate;
  - each `DisplayName`'s neon `MultiEffect`: for a neon name.

| idle, private (2 runs each, interleaved) | before         | after (`b3`)                    |
| ---------------------------------------- | -------------- | ------------------------------- |
| chat-10k                                 | 170.8-173.0 MB | **163.6-165.4 MB** (−7)         |
| soak (real-account-like)                 | 174.8-177.4 MB | **157.0-158.7 MB** (−17 to −20) |
| soak, traced live heap                   | 52.1 MB        | **41.1 MB**                     |

On soak the heap fell by owner as follows:

- pictures, 6.2 to 2.1 MB;
- scene graph, 9.3 to 6.4;
- QML engine, 19.5 to 17.6;
- Qt core, 9.6 to 8.1;
- Quick items, 1.6 to 1.2.

### Not done, and why

- **The bridge's second copy of the message rows.** It is 9 MB on chat-10k
  (the `Vec<MessageRow>`, 557 B a row inline, plus its strings). On soak all
  of the Rust core is 2.8 MB, and a real channel holds hundreds of rows, not
  10,000: under 1 MB.
  - The core keeps no rows of its own. It builds them from `MessageLog` and
    ships diffs.
  - For the model to read from the core instead, the GUI thread would have
    to reach the core's state, behind a lock, on every `data()` call.
  - That cost is out of proportion to the gain on real data.
- **Fonts from uncompressed resources** would save 2.2 MB of heap and add
  0.8 MB to the exe. That would take it over 60 MB.
- **A smaller chat `cacheBuffer`** (1,200 px; MessageDelegate is 1,104
  objects) would trade scroll smoothness, which the user has just reported
  as worse.

## 4. Animated idle

### The WPR trace

One kernel trace was taken: CPU sampling every 0.12 ms with stacks, 60 s of
pass 3's animated scene on the before build, analysed with `etlstacks.py`.
It finds what pass 6's in-process sampler could not: time spent outside the
GUI thread, and frames without unwind data. Our process's samples split as:

- GUI thread, 60 %;
- the NVIDIA driver's worker thread, 28 %;
- DXGI and vblank threads, 7 %.

On the GUI thread:

| area (inclusive)                                                    | share         |
| ------------------------------------------------------------------- | ------------- |
| rendering (`renderWindow`)                                          | 61.5 %        |
| of which the batch renderer (`render`)                              | 30.2 %        |
| of which `endFrame` (command buffer execution, Present)             | 22.4 %        |
| **GIF decoding** (`AnimatedPicture::advance`, `QGIFFormat::decode`) | **14.6-18 %** |
| **a new D3D11 texture per GIF frame** (`QD3D11Texture::create`)     | **4.5 %**     |

Notes on the table:

- Pass 6 put GIF decoding at 3 %. The in-process sampler missed Qt's GIF
  decoder, which has no unwind data.
- The streamed GIF tile deleted its texture and made a new one for every
  frame. Each one was a D3D11 create, plus the driver's allocation and
  release.
- The scene draws **83 batches a frame** for 153 nodes
  (`QSG_RENDERER_DEBUG=render`). The alpha pass merges poorly: every
  differently coloured text run and every rounded rectangle is its own
  draw (`tmp/p7/vis-batches.png`).

### What changed

**One texture per streamed GIF** (`AnimatedPicture::updatePaintNode`):

- A new frame of the same size goes into the existing `QSGPlainTexture`
  (`setImage`), which uploads into its `QRhiTexture` again.
- `Qt::QuickPrivate` is now linked, for `qsgplaintexture_p.h`.
- The GIF still animates and loops (screenshots `tmp/p7/gif-1..3.png`: the
  tile changes between captures 0.7 s and 6 s apart).

Paced animated idle, 40 s runs, the builds interleaved over three rounds:

| build  | CPU (one core)       | mean   | private        |
| ------ | -------------------- | ------ | -------------- |
| before | 4.76, 4.49, 3.24     | 4.16 % | 174.4-175.2 MB |
| `b3`   | 4.49, 4.10, **2.89** | 3.83 % | 167.6-169.3 MB |

`b3` is lower in every round, by 0.27-0.39 points (−8 %), and 6-7 MB lower.

### Why not under 3 %, and what would get there

- **The machine's load moves a run by more than a point**: the same build
  ran at 3.24 and 4.76 %. A best of 2.89 % is not a proof.
- **GIF decoding is 15 % of the GUI thread.** A 360x240 GIF of 24 frames
  is 8 MB decoded, so caching frames would trade too much memory. Decoding
  on a worker thread would move the time, not remove it.
- **Fewer draws.** Caching static panels in layers would turn 83 draws into
  a few, but it renders their text into textures. UI pass 11 is
  investigating the text's look, so that was not tried; it needs a pixel
  diff of the text first.
- The DXGI vblank threads wake 60 times a second while anything animates
  (one per monitor the window has been on). They cost nothing at static
  idle, which measured 0.00 %.

## 5. CI

LIBRARIES.md §13.3 has the steps. Nothing was uploaded. The archive to pack
is now `<scratch>/qt-custom-6.11p7/6.11.3-yap` (§6).

## 6. Qt rebuild with DirectWrite's enhanced contrast

UI pass 11 wrote `patches/qt/6.11.3/0005-directwrite-enhanced-contrast.patch`
(spike/qt-desktop, db5834f7); it is copied here unchanged. It applies the
system's enhanced-contrast curve to DirectWrite's glyph coverage, as
Direct2D and Chromium do (LIBRARIES.md §13.3).

- **Built** with `build-qt.ps1 -Root <scratch>/qt-custom-6.11p7 -Clean`:
  `0001`-`0005` on the 6.11.3 source zip, into
  `<scratch>/qt-custom-6.11p7/6.11.3-yap`. `<scratch>/qt-custom-6.11` and `-6.11p6`
  were not touched.
- **The patches applied in full.** `0005` is a plain diff. `0001`, `0002`
  and `0004` carry `diff --git` headers, but the source is extracted outside
  any repository, where `git apply` reads them as it should. Every added
  line of every patch was then found in the extracted tree (a script, before
  the build), and `.yap-patched` was written.
- **The exe links it:** `QT_DWRITE_ENHANCED_CONTRAST` is in the final exe
  and not in `b3`'s.
- **`build.ps1`'s default Qt is now `qt-custom-6.11p7`** (then `p6`, then
  `6.11`).

**Text against the web** (`tools/p7/web-shot.mjs`, a copy of
`web-shot.mjs` with `--dpr`, dark scheme, at the native window's logical
size). The crop is "final smoke: hidden toast" in #general, zoomed 4x with
nearest neighbour: `tmp/p7/text/cmp-1x.png` and `cmp-15x.png`. The number
is the mean luminance of the brightest 10 % of the crop's pixels, which
measures how heavy light-on-dark glyphs are:

| scale | web   | native, pass 6 Qt | native, `0005` |
| ----- | ----- | ----------------- | -------------- |
| 100 % | 221.8 | 207.9             | **215.9**      |
| 150 % | 226.8 | 225.5             | **229.5**      |

- At 100 % the strokes are visibly fuller, and most of the gap to the web
  closes. At 150 % they are a touch heavier than the web's.
- The sidebar's bold labels are the same in all three (241 against 241).
- One difference remains. Native draws ClearType's colour fringes, and
  headless Edge draws grayscale, so the screenshots cannot settle subpixel
  rendering.
- **No CPU cost:** the final build's animated idle ran at 3.87 and 2.77 %,
  against `b3`'s 4.33 and 3.09 % in the same rounds.

## Before and after (pass 7)

| metric                                          | before                  | after                                              |
| ----------------------------------------------- | ----------------------- | -------------------------------------------------- |
| sharing a real 2560x1440 window at 1440p60, CPU | not measured            | **43-44 %** at 54-59 fps (NVENC)                   |
| watching it (H.264, zero copy)                  | not measured            | **28 %** at 53-58 fps                              |
| watching a VP9 1080p60 share, CPU per 25 s      | 10.66 s (43 %, libvpx)  | **5.19-5.83 s (21-23 %, GPU)**                     |
| watching a VP9 1080p60 share, private           | 212.5 MB                | 276-277 MB (released on leaving)                   |
| idle private, chat-10k                          | 170.8-173.0 MB          | **163.6-165.4 MB**                                 |
| idle private, soak                              | 174.8-177.4 MB          | **157.0-158.7 MB**                                 |
| soak traced live heap                           | 52.1 MB                 | **41.1 MB**                                        |
| QML objects alive, soak                         | 6,362                   | **5,392**                                          |
| animated idle, paced (interleaved)              | 4.16 % mean (3.24-4.76) | **3.83 % mean** (2.89-4.49); final build 2.77-3.87 |
| text ink at 100 % (web 221.8)                   | 207.9                   | **215.9**                                          |
| exe                                             | 59,525,120 B            | 59,544,576 B                                       |

## Targets

1. **Real 1440p60 window share: measured.** The sharer runs at 43-44 % of a
   core and the watcher at 28 %, at 54-59 fps. Next: the zero-copy share
   (§1), about half of the sharer's CPU.
2. **Hardware VP9 decode: done**, at −45 to −51 % CPU while watching, on the
   GPU through Media Foundation. **VP8:** the code path is there, but this
   GPU has no VP8 decoder, so on NVIDIA it stays on libvpx (Intel decodes
   it). The browsers and mobile send VP8 simulcast. A camera tile is 640x360
   at 15 fps and 0.1 ms a frame to decode, so the GPU would save little.
   - Not checked: VP8 on an Intel GPU, and a VP9 stream from a browser (no
     client publishes VP9).
3. **Idle memory under 120 MB: not reached.**
   - It is 157-159 MB on soak (−17 to −20) and 164-165 MB on chat-10k (−7).
   - About 37 MB of `PrivateMemorySize` is DLL and driver pages that the app
     does not allocate, so the target as measured would need the heap at
     ~70 MB.
   - What is left, largest first:
     - the QML engine (17.6 MB heap and 5 MB JS pages): fewer types loaded
       at startup;
     - Qt core (8 MB);
     - the scene graph (6.4 MB);
     - the driver's regions;
     - fonts (2.2 MB, which needs exe room);
     - the bridge's row copy (under 1 MB on real data, §3).
   - Real accounts gain most from the capped picture decode: a phone photo
     on screen no longer costs ~48 MB.
4. **Animated idle under 3 %: not proven.**
   - The WPR trace found two costs and one is fixed (a texture per GIF
     frame). The scene now runs at 2.77-4.49 % depending on the machine's
     load.
   - What is left: the GIF decode (15 % of the GUI thread) and 83 draws a
     frame. Layer-caching the static panels is the lever, once the text in
     a layer can be pixel-checked.
5. **CI: written down** (LIBRARIES.md §13.3). The archive to pack is now the
   p7 Qt. Nothing was uploaded.
6. **Qt with `0005`: built, linked, verified** against the web at 100 % and
   150 %, and the default.

## Build, tests, clippy

- Builds: `tools/p7/build.ps1`, which is `build.ps1` with a link map, into
  `<scratch>/yapn/rel-perf` (logs `<scratch>/yapn/build-perf7-1..5.log`; all exit 0).
  `Qt6_DIR` is `<scratch>/qt-custom-6.11p7/6.11.3-yap/lib/cmake/Qt6` after build 5.
- Qt: `<scratch>/yapn/qt-p7-build.log` (exit 0).
- Tests (`CARGO_TARGET_DIR=<scratch>/yapn/check-perf`, `LK_CUSTOM_WEBRTC` as
  `build.ps1` sets it): the workspace without `yap_native`, plus `yap-audio`
  on its own. 1,464 passed, 0 failed, on the pass 6 Qt and again on the p7 Qt (`<scratch>/yapn/test-perf7.log`).
- `cargo clippy --workspace --all-targets -- -D warnings`, with QMAKE at the
  p7 Qt: exit 0, no warnings (`<scratch>/yapn/clippy-perf7.log`).

## Files

Source (worktree `native/perf3`):

- C++ and build:
  - `cpp/animatedpicture.cpp`: one texture per streamed GIF
    (`QSGPlainTexture::setImage`);
  - `cpp/main.cpp`: the settle release 20 s after the first frame;
  - `CMakeLists.txt`: `Qt::QuickPrivate`;
  - `build.ps1`: the p7 Qt by default.
- QML (5 files, each a Loader around what it already had, or one binding):
  - `rust/qml/Main.qml`: `BootView` and `SignInView` behind Loaders;
  - `rust/qml/delegates/TreeRow.qml`: `VoiceStatusLine` only on the row that
    draws it;
  - `rust/qml/delegates/MemberEntry.qml`: `NameplateLayer` only with a
    nameplate;
  - `rust/qml/components/DisplayName.qml`: the neon `MultiEffect` only for a
    neon name;
  - `rust/qml/components/MediaTile.qml`: the capped decode (`fitCap`,
    `retainWhileLoading`).
- Rust:
  - `crates/yap-voice/src/engine/webrtc.rs`: `YAP_DEBUG_SHARE_CODEC=vp9`;
  - `crates/yap-core/src/store/view.rs`: the `User::label` fix from spike
    (`.map(|u| u.label())`).
- Patches (LIBRARIES.md §12.1, §13.1 and §13.3):
  - `patches/webrtc-sys/0005-mf-vpx-decoder.patch` (new);
  - `patches/qt/6.11.3/0005-directwrite-enhanced-contrast.patch` (UI pass
    11's, copied unchanged);
  - `patches/vendor.ps1`: its comment.
- Docs: this section, and LIBRARIES.md §12.1, §13.1 and §13.3.

Tools (outside the repo, `<scratch>/yapn/tools/p7/`): `idle.ps1`,
`etlstacks.py`, `mft-probe.ps1`, `cam.ps1`, `share-test-window.ps1`,
`wgc-start.ps1`, `text-shot.ps1`, `textcmp.py`, `web-shot.mjs` (with `--dpr`),
and the p6 copies (`app.ps1`, `owner.ps1`, `watch.ps1`, `leave.ps1`,
`anim-idle.ps1`, `anim-ab.ps1`, `heap-idle.ps1`, `build.ps1`, `test.ps1`,
`xperf-session.ps1`).

# Clips tab (2026-11-08)

The GIF picker's Clips tab (UI pass 7) was "incredibly laggy". Measured on
`native/perf3` at c6735a84 (spike/qt-desktop 690b8546 + native/text), the
owner test account on profile `perfclips`, Trending clips in the picker
at its normal size (two columns, 10 tiles in view), unpaced presents
(`YAP_DEBUG_NO_VSYNC`), `QSG_RENDER_TIMING`, the test pointer and posted
wheel ticks only. Media counts come from the `YAP_DEBUG_ANIMATIONS` dump,
which now also counts live `MediaPlayer`/`WebmPlayer` objects, how many
play, and their `AudioOutput`s.

## Cause

Every tile in view had its own `LoopVideo`: a Media Foundation
`MediaPlayer` with an `AudioOutput`, playing KLIPY's `send` mp4 (854x480
or 720x404) looped at rest. A tile scrolled into view made a new one, and
a tile scrolled out destroyed it. The GUI-thread sampler while scrolling
(`YAP_DEBUG_SAMPLE`, 12 s): 75 % of the GUI thread was inside
`MFPlayerSession::handleMediaSourceReady` → `setupPlaybackTopology` →
`EVRCustomPresenter` → `D3DPresentEngine::createD3DDevice`, so each new
player built a D3D9 device on the GUI thread. The top leaves were
`NtGdiDdDDICreateContextVirtual` (20.5 %), `CreateAllocation` (9.0 %),
`QueryAdapterInfo` (6.9 %) and `EnumDisplayDevices` (6.1 %). At rest, 10
players decoded and 10 audio streams stayed open. Each clip's
animated-WebP preview, used as its poster, was also decoded whole: 44-120
frames per tile, never shown.

The web (`GifPickerModal.tsx` `ClipVideoPlayer`) autoplays every clip's
`send` mp4, muted, and unmutes the hovered one when the picker is
unmuted. A browser decodes those in its GPU process; the native app pays
the topology on its GUI thread.

## What changed

- A clip rests on its first frame: `AnimatedPicture.still` decodes only
  frame 0 of the WebP preview (KLIPY's 320x180, 10 fps rendition of the
  whole clip), off the GUI thread. The preview's length comes from its
  ANMF headers (`duration`), so the length chip shows at rest without a
  player.
- Only the clip pointed at (or focused) plays (`GifPicker.clipTile`). Its
  WebP animates (`still` off). A long looping WebP posts its first 10
  frames first, and the whole carries on from the frame showing
  (`yap_anim::decode_frames`, `kStartFrames`).
- With sound on (the picker unmuted, video not muted), the pointed clip's
  mp4 also plays, in the picker's one `MediaPlayer`. Its `VideoOutput`
  moves into that tile's slot, under the chips, and the player starts
  once the pointer has rested 120 ms. That is one Media Foundation session
  per settled hover, never one per tile.
- `LoopVideo` per tile remains only for a non-clip video with no picture
  (rare).

## Before and after

| metric (Trending clips, 10 tiles in view)          | before                      | after                                  |
| -------------------------------------------------- | --------------------------- | -------------------------------------- |
| live players at rest (playing / audio outputs)     | 10 (10 / 10)                | **1 idle, no source (0 / 1)**          |
| process CPU at rest, 10 s                          | 53-68 % of a core           | **0.5-0.6 %**                          |
| threads                                            | 403-434                     | **105-122**                            |
| continuous wheel, 6 x 60 ticks at 16 ms: CPU       | 522 % of a core             | **61-68 %**                            |
| same: frames rendered                              | 62 in 11.1 s (6/s)          | **1,962-1,964 in 10.7-10.8 s (182/s)** |
| same: GUI frame gap p50 / p95 / p99                | 94 / 457 / 632 ms           | **5 / 6 / 7-8 ms**                     |
| same: gaps over 50 ms                              | 34                          | **1** (the first frame after idle)     |
| GUI thread busy while scrolling (sampler)          | 70 % (75 % of that MF/D3D9) | **35 %** (scene-graph render, no MF)   |
| hover, muted: pointing to motion                   | n/a (all looped)            | **0-40 ms** (whole preview 70-366 ms)  |
| hover, sound on: pointing to the mp4's first frame | n/a                         | 263-311 ms (120 ms of it the settle)   |
| hover, sound on: GUI time per session              | n/a                         | ~15-22 ms (26 % of 391 samples, 7)     |

The "after" scroll frames are paced only by the work (unpaced presents);
p99 8 ms leaves 60 Hz with room. At rest the window renders only the caret
(2 frames/s). Process reads while scrolling are dominated by new pages'
posters (each a whole animated WebP, 80-560 KB), which the old code also
fetched for every nearby tile, on top of an mp4 stream per tile entering
view. Its idle reads (6.9-7.7 MB per 10 s, the looping mp4s) are gone.

## Not done

- A smaller still. The server (`src/routes/api/gifs.ts`) gives clips only
  `preview` (the animated WebP) and `send` (`file.mp4 || file.hd.mp4 ||
file.md.mp4`, so HD first). A KLIPY still or an `sm`/`md` mp4 as a
  separate field would cut poster bytes and the sound-on decode.
- The sound-on session still builds a D3D9 device on the GUI thread per
  hover (Qt's WMF `D3DPresentEngine`). Sharing one device across
  presenters would need a Qt patch.
- The length chip comes from the 10 fps preview and can read a second
  under the mp4's (1.9 s against 2.03 s shows 0:01, not 0:02). Once the
  mp4 has played, its own duration replaces it.

## Files

- `rust/qml/screens/Shell/GifPicker.qml`: `clipTile`, `clipPlaying`, the
  shared `clipPlayer`/`clipOut`, the tiles' `still` pictures and slots,
  learnt lengths, the `yap.clips` timing log.
- `cpp/animatedpicture.{h,cpp}`: `still`, `duration`, WebP header scan,
  start-first decode of long looping WebPs, carrying on into the whole.
- `rust/src/anim.rs`, `rust/crates/yap-anim/src/lib.rs`:
  `yap_anim_decode`'s `max_frames`, `decode_frames` (with a test).
- `cpp/main.cpp`: media counts in the `YAP_DEBUG_ANIMATIONS` dump.

Tools (outside the repo, `<scratch>/yapn/tmp/clips/`): `launch.ps1`,
`nav.ps1`, `tap.ps1`, `measure.ps1`, `scroll.ps1`, `hover2.ps1`,
`test.ps1`.

# Resize pass (2026-11-08)

Resizing the window was laggy, jumped, and showed a strip trailing the
moving edge (white on the user's machine). This pass measured why and fixed
it in `cpp/` with no Qt patch and no new Qt build.

## Method

Tools are in `<scratch>/yapn/tools/rs/`:

- `rsprobe.exe` (`rsprobe.cpp`): resizes **our** window by PID with
  `SetWindowPos`, wrapped in `WM_ENTERSIZEMOVE` / `WM_EXITSIZEMOVE` as a
  drag sends them. Each step comes 8 ms after the last one returns, and
  each `SetWindowPos` blocks until our GUI thread has handled it.
  - It captures our window only, with Windows.Graphics.Capture, one frame
    per DWM composition.
  - For each frame it finds the user panel (bottom left) and the
    composer's end keys (bottom right). Where they sit against the
    window's edges gives how many steps the content trails the window.
  - It also logs each step's `SetWindowPos` time: the GUI thread's cost
    per step.
  - Modes: `br` drags the bottom-right corner, 1150x750 to 1550x1000 and
    back, twice. `left` drags the left edge, which moves the origin too.
- `--thumb 1` shows a DWM thumbnail of our window in a window of the
  probe's own, beside it, over magenta, and captures that window instead.
  A thumbnail is drawn the way DWM composes the window, so a strip with no
  surface under it shows as black or magenta there. A direct window capture
  shows it only as alpha 0.
- `sweep.ps1` runs both modes on the chat-10k fixture, profile `resize`,
  on the test display at 100%. It adds the basic render loop's frame times
  (`QSG_RENDER_TIMING`).
- `thumb.ps1` runs a thumbnail sweep in miku-dark (the ground is not pure
  black there).
- `anim-idle.ps1` and `static-idle.ps1` measure idle CPU. `scroll.ps1` is
  pass 5's wheel test.
- The GUI thread was sampled with `YAP_DEBUG_SAMPLE` and
  `tools/p6/samplesym.py`, using a `/MAP` build.
- **New diagnostic switch:** `YAP_DEBUG_RESIZE=1` prints each resize
  step's timings (DefWindowProc, erase, render) to stderr.

## Root causes

1. **The flip-model swapchain trails the window.**
   - Qt's D3D11 swapchain is `FLIP_DISCARD` with `SCALING_NONE`. DWM
     composes the flip buffer on its own, apart from the window's bounds.
   - Captured frames showed the content 2 or more steps behind the edge in
     30 % of frames. The `left` drag's p95 was 14 px.
   - In the thumbnail capture, 35 % of the frames had a strip along the
     right or bottom edge that no surface covered: black or transparent.
     That is the trailing canvas (white on the user's display).
   - Rendering synchronously and erasing the background did not change
     this under the flip model. Black and transparent strips stayed.
2. **Two vblanks per resize step.**
   - Each `SetWindowPos` took 30-33 ms (p50), so a drag ran at about 36
     steps a second.
   - The sampler put most of it in `Present` waiting (`endFrame` under
     `handleWmPaint`). Qt rendered each size once from the expose and
     again from the `WM_PAINT` that followed.
   - Under the flip model, each `Present` after a `ResizeBuffers` waits for
     the previous frame to reach the screen.
   - The basic loop's frame time during a resize was 10 ms (p50), almost
     all of it the frame-latency wait.
3. **1-second stalls in a move-and-resize.** In the `left` drag (the origin
   moves), the flip model's frame-latency waitable object often never
   signalled. `beginFrame` sat out its full 1000 ms timeout:
   `SetWindowPos` p95 was 1038 ms, and the drag ran at 5-7 steps a second.
   That was the "skips around". `QT_D3D_MAX_FRAME_LATENCY=0` removed the
   stalls but not the lag.
4. **An unpainted redirection surface.** Qt answers `WM_ERASEBKGND`
   without painting and registers no class brush. Wherever the frame had
   not reached, the redirection surface showed through: magenta in the
   thumbnail without the erase below.

Ruled out:

- **Threaded render loop:** no stalls, but the same 28-30 ms steps, and
  content 2 or more steps behind in about 90 % of frames.
- **DirectComposition** (`CreateSwapChainForComposition` with
  `QT_QPA_DISABLE_REDIRECTION_SURFACE=1`): 33 ms steps, and transparent
  strips in about a third of the edge samples. With no redirection
  surface, nothing fills the gap.
- **QML layout:** `polish` was about 0 ms per step. The text and the list
  are not the cost.

## Fix

- **`cpp/main.cpp`: the blt-model swapchain.**
  - `QT_D3D_NO_FLIP=1` is the default; `QT_D3D_NO_FLIP=0` brings back the
    flip model.
  - A blt `Present` lands in the redirection surface, which DWM resizes
    along with the window.
  - Qt sets no frame-latency waitable object on a blt swapchain, so the
    stall in cause 3 cannot happen.
- **`cpp/windowchrome.cpp`: each size is drawn before `SetWindowPos`
  returns.**
  - `WM_WINDOWPOSCHANGED` with a size change:
    1. `DefWindowProc` sends `WM_SIZE` to Qt.
    2. The redirection surface is filled with the window colour.
    3. `QWindowSystemInterface::flushWindowSystemEvents()` delivers the
       geometry, and the QML layout follows.
    4. If Qt's own expose did not draw a frame, a synchronous expose
       renders and presents one at the new size.
    5. The window is validated.
  - It is skipped while minimised, while hidden, inside Qt's own
    `WM_DPICHANGED` resize, and when re-entered.
  - `YAP_RESIZE_SYNC=0` turns it off.
- **`cpp/windowchrome.cpp`: no second render per step.**
  - A window subclass (`SetWindowSubclass`, linking `comctl32`) runs ahead
    of Qt's window procedure.
  - The `WM_PAINT` that a resize queues after `WM_WINDOWPOSCHANGED` is
    only validated, and only within 250 ms of the resize's own present.
  - A `WM_PAINT` from the move/size loop never reaches an application
    event filter, which is why this needs a subclass.
- **`cpp/windowchrome.cpp`: `WM_ERASEBKGND` paints the window colour.**
  - The colour is `QQuickWindow::color()`, which is `Theme.ply0`.
  - `YAP_RESIZE_ERASE=0` turns it off.

No Qt patch was needed and no new Qt root was built: `QT_D3D_NO_FLIP` is
Qt's own switch.

## Before and after (100 %, chat-10k, two runs each)

| measure (probe drag)                             | before (spike 0.2.9)    | after              |
| ------------------------------------------------ | ----------------------- | ------------------ |
| `SetWindowPos` per step p50 / p95, corner drag   | 30-31 / 45 ms           | 13-14 / 18-19 ms   |
| steps a second, corner drag                      | 35-36                   | 68-72              |
| `SetWindowPos` p95 / max, left-edge drag         | 1022-1038 / 1040 ms     | 15 / 24 ms         |
| steps a second, left-edge drag                   | 5.2-6.6                 | 81-83              |
| render-loop frame time p50 / p95 during the drag | 10 / 15 ms (left: 1011) | 2-3 / 3-6 ms       |
| frames with content 2+ steps behind (corner)     | 30 %                    | 0 %                |
| thumbnail frames with an uncovered edge strip    | 59-62 of 173 (35 %)     | 0-3 of ~55 (0-6 %) |

- **Effect on content lag:** the content is now at most one step behind
  the edge, and a step is half as long. One step is what DWM itself
  allows: the new bounds are composed a frame before the content drawn for
  them.
- **Captures:** `<scratch>/yapn/tmp/resize/captures/before-thumb.png` shows the
  black strips along the right and bottom edges, about 16 px wide.
  `after-thumb.png` has none.
- **Raw data:** per-run frames, steps and saved frames are in
  `<scratch>/yapn/tmp/resize/run-*` and `thumb-*`.

Idle and scroll, same machine, interleaved:

| measure                                           | before                      | after                                               |
| ------------------------------------------------- | --------------------------- | --------------------------------------------------- |
| static idle, 60 s (fixture, unfocused)            | 0.10 / 0.05 %               | 0.00 / 0.00 %                                       |
| focused animated idle (pass 6 scene), % of a core | 2.6-4.6, mean 3.70 (5 runs) | 3.2-4.4, mean 3.73 (5 runs)                         |
| animated idle private bytes                       | 171-173 MB                  | 177 MB                                              |
| fast wheel scroll, % of a core / frame p99        | 46.6, 42.7 % / 2 ms         | 43.2, 46.1 % / 2 ms (blt build before the subclass) |

- **Idle CPU:** unchanged. The spread within each column is bigger than
  the difference between them.
- **Memory:** about +5 MB private in animated idle, which is likely the
  redirection surface that blt presents into.
- **Static idle:** private memory is 4 MB lower.

At 150 % (the test display set to 150 % for the run, then restored). The
window was 1450x850 to 1850x1040 physical, so the members column stays
docked:

| measure                                 | before         | after          |
| --------------------------------------- | -------------- | -------------- |
| `SetWindowPos` p50 / p95, corner drag   | 31.7 / 44.7 ms | 13.9 / 22.9 ms |
| steps a second, corner / left-edge drag | 34.9 / 6.2     | 69.1 / 84.5    |
| `SetWindowPos` p95, left-edge drag      | 1021 ms        | 15 ms          |

- The synchronous path ran: 484 `[resize]` lines over the two drags.
- The patch-tracking metric does not apply at 150 %: its references are
  100 % layouts.

What is left: a one-step strip along the moving edge, about 7 px at the
probe's step size.

- It is DWM's own frame between the new bounds and the content drawn for
  them.
- In the thumbnail capture it shows the window colour. A direct window
  capture shows it as alpha 0.
- Avoiding it would need DWM to apply the window's bounds and its content
  together, and no swapchain model here does that. DirectComposition was
  worse (above).

Not measured: resizing with the voice grid open. That needs the test
accounts, and the cost per step here is presentation, not layout.

Checks:

- `cargo test --workspace --exclude yap_native --exclude yap-audio` and
  `cargo test -p yap-audio` (`CARGO_TARGET_DIR=<scratch>/yapn/check-resize`,
  `LK_CUSTOM_WEBRTC` as `build.ps1` sets it): all passed
  (`<scratch>/yapn/test-resize.log`).
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0
  (`<scratch>/yapn/clippy-resize.log`).

## Files

Source (worktree `native/resize`):

- `cpp/main.cpp`: `QT_D3D_NO_FLIP=1` by default.
- `cpp/windowchrome.cpp` and `cpp/windowchrome.h`:
  - the synchronous resize render (`renderResize`);
  - the `WM_PAINT` subclass;
  - the `WM_ERASEBKGND` fill (`eraseBackground`);
  - the switches `YAP_RESIZE_SYNC`, `YAP_RESIZE_ERASE` and
    `YAP_DEBUG_RESIZE`.
- `CMakeLists.txt`: links `comctl32`.
- Docs: this section, and LIBRARIES.md §13.3 (the swapchain note).

Tools (outside the repo, `<scratch>/yapn/tools/rs/`): `rsprobe.cpp` and
`rsprobe.exe`, `sweep.ps1`, `thumb.ps1`, `app.ps1`, `build.ps1` (with
`/MAP`), `test.ps1`, `anim-idle.ps1`, `static-idle.ps1` and `scroll.ps1`.

# Switching (2026-11-08)

Channel and server switches from memory (docs/ARCHITECTURE.md, "As built:
switching channels and servers").

## Method

- Build: `<scratch>/yapn/rel-cache` (worktree `native/cache`), profile `spike`
  (the test owner) in "Native Spike Lab", whose #general holds 18 messages
  with a YouTube card and #cache-test (made for the run, 3 messages,
  deleted after) holds 3.
- Posted pointer and key input stopped reaching the window early in this
  session: the build before the change took one test-pointer click in its
  first run and none in its second, and this build none (the window,
  the accessibility tree and the message loop were otherwise fine), so
  the tours ran through `YAP_TEST_SWITCHES`, which
  sends the same commands the sidebar, the rail and the keys send
  (`src/client/mod.rs`). The click handlers themselves were not exercised.
- `YAP_TRACE_HTTP=1` for the `[http]` lines; `[switch]` lines are always
  logged. A switch is timed from the input that started it to the effect
  that put its rows in the view batch: the store's share, not the QML
  delegates' (the batch reaches the Qt thread on its next turn).
- Memory: `Get-Process` private bytes every 3 s.

## Results

| switch                              | before                                                                 | now                                                                                                          |
| ----------------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| a channel visited before            | skeleton, `GET /messages` (185-938 ms here)                            | its rows in the same reduce: 0.1-0.4 ms, avg 0.21 ms (44 switches)                                           |
| a channel left < 30 s ago (server)  | the same                                                               | no request at all                                                                                            |
| a channel not warm, window on disk  | skeleton until the page                                                | the disk window in 1.2 ms (page answered after 938 ms, merged by diff)                                       |
| a channel never seen                | skeleton, page (231 ms)                                                | the same (cold)                                                                                              |
| Home → a server whose tree is known | header first, then `GET /channels`, then the first text channel's page | tree, members and the last channel in one batch (0.2 ms); `GET /channels` (223-394 ms) revalidates behind it |

Requests, one tour of 45 switches between #general and #cache-test with
two Home / server round trips: 8 page requests in all (the launch's
revalidate, the disk window's page, and one revalidate per channel each
time it was left for more than 30 s), and 3 tree revalidates. Before the
change the same tour costs 45 page requests and 2 tree requests on top.

Memory (private bytes, settled):

| point                                                    | MB    |
| -------------------------------------------------------- | ----- |
| idle on #general after launch                            | 145.6 |
| after one visit each to #cache-test, Home and the server | 150.9 |
| after 39 more switches and two more Home / server trips  | 153.8 |

The first visits cost what any build pays to make those views (+5.3 MB);
the warm cache itself is bounded at 6 MB estimated and held 21 messages
here. The tour added +2.9 MB, inside the +6 MB budget.

## Not measured

- Clicks, hovers and keys: the 120 ms hover prefetch, the switcher's
  highlight prefetch, Alt / Ctrl+Alt navigation, Esc to mark read, drafts
  per channel and the composer's focus after a switch (the store side of
  the navigation keys and the prefetch is covered by `tests/switching.rs`).
- Switching between two servers (the test account has one); Home to the
  server takes the same path.
- A full 24-channel warm cache in the app (the caps are covered by tests).

## Files

- Store: `store/state.rs` (`WarmCache`, `Lru`, `MessageLog::trim_for_warm`,
  `estimated_bytes`), `store/reduce/warm.rs` (new), `store/reduce.rs`
  (`open_messages`, `select_server`, `on_page`'s merge, the fresh Ready),
  `store/reduce/{threads,extras,media,admin,social}.rs` (pins and tab
  caches, warm reactions, eviction), `store/{effect,input}.rs`
  (`HttpReason`, `LoadWindow`, `SwitchMark`, `Input::UserAt`,
  `WindowLoaded`, `PrefetchChannel`, `SelectHomeLast`, `NavigateChannel`,
  `NavigateServer`).
- Runtime: `runtime.rs` (the stamp on commands, the `[http]` and
  `[switch]` traces, the prefetch drop when busy, the read-only window
  reader).
- Cache: `yap-cache` `db.rs` (schema 4: `last_channels`, migrated from 3;
  `Cache::window`, `Cache::open_read_only`), `lib.rs`.
- App: `src/bridge.rs` (`prefetchChannel`, `selectHomeLast`,
  `navigateChannel`, `navigateServer`), `src/client/mod.rs`
  (`YAP_TEST_SWITCHES`), QML `ChannelRow`, `TreeRow`, `ChatView`,
  `Composer`, `ExtrasHost`, `QuickSwitcher`, `ServerRail`.
- Tests: `tests/switching.rs` (new), `yap-cache` `tests/cache.rs`, and
  `tests/{threads,social}.rs` where a revisit now opens warm.

# HDR screen capture (2026-11-08)

What the HDR capture path (LIBRARIES.md §16) costs, and that the SDR path
did not get slower. This PC's three displays (Parsec and virtual display
adapters on an RTX 3080, all 1920x1080) are SDR, and the display settings
were left alone, so the FP16 path was forced with
`YAP_TEST_HDR_CAPTURE`. No 1440p display exists here: 1440p numbers come
from the offline measurement only.

## Method

- **Offline** (`cargo test --release -p yap-capture -- shader_cost
--ignored --nocapture`): a 1080p and a 1440p texture, one frame every
  16 ms for 330 frames (the first 30 dropped), through the same
  `HdrConverter` and `Staging` the capture uses; GPU time from timestamp
  queries, CPU time around the map.
- **Live**: the owner (Spike, fake devices) sharing the
  `share-test-window.ps1` window (1920x1093 captured, encoded 1896x1080)
  at 1080p60 into a call in Native Spike Lab, NVENC. Process CPU over
  25 s after 25 s of settling (`share-run.ps1` in `tmp/hdr`), and
  `YAP_CAPTURE_TIMING=1` for per-frame GPU and CPU time. "Before" is the
  spike build at 9cabefd9 (`<scratch>/yapn/rel`), whose capture and voice code
  is this branch's base.
- The machine was shared with other agents' builds and VMs (total CPU at
  96 % during one run), so the push time (the I420 conversion and hand-off
  to the encoder) moved between 3 and 40 ms from run to run, on both
  paths. The GPU numbers and the read time held steady.

## Offline: per frame

| size      | path                   | GPU convert  | GPU copy to staging | CPU map (waits for the GPU) |
| --------- | ---------------------- | ------------ | ------------------- | --------------------------- |
| 1920x1080 | BGRA8 (SDR, as before) | -            | 0.79-1.03 ms        | 1.3-1.5 ms                  |
| 1920x1080 | FP16 + tone map (HDR)  | 0.11-0.21 ms | 1.25-2.1 ms         | 2.5-3.5 ms                  |
| 2560x1440 | BGRA8 (SDR, as before) | -            | 1.7-2.7 ms          | 2.9-3.8 ms                  |
| 2560x1440 | FP16 + tone map (HDR)  | 0.17-0.21 ms | 2.3-2.6 ms          | 3.8-5.7 ms                  |

Two runs each. The copy is BGRA8 on both paths (the shader writes BGRA8),
so the readback is the same size; the run-to-run spread is the GPU's
clocks at one frame per 16 ms.

## Live: 1080p60 window share

| run                                   | process CPU (one core) | WGC callback thread / 25 s | sent           |
| ------------------------------------- | ---------------------- | -------------------------- | -------------- |
| before (9cabefd9), SDR                | 43.4 %                 | 4.88 s                     | H.264 @ 54 fps |
| this branch, SDR (BGRA8)              | 44.2 %                 | 5.11 s                     | H.264 @ 55 fps |
| this branch, FP16 forced (`200,1000`) | 46.9 %                 | 5.36 s                     | H.264 @ 56 fps |

`YAP_CAPTURE_TIMING`, averages of 300 frames:

| path                     | GPU convert    | GPU copy     | CPU read    | push       |
| ------------------------ | -------------- | ------------ | ----------- | ---------- |
| SDR (BGRA8)              | -              | 0.47-1.30 ms | 0.93-3.6 ms | 4.7-29 ms  |
| FP16, identity (`80,80`) | 0.043-0.085 ms | 0.63-1.27 ms | 1.1-1.9 ms  | 3.0-3.4 ms |
| FP16, `200,1000`         | 0.037-0.096 ms | 0.53-1.38 ms | 3.0-3.8 ms  | 26-41 ms   |

- **SDR does not regress.** Its per-frame work is the old one; what it
  adds is one uncontended lock to read the wanted format and the
  `yap-share-color` thread, which looks at the display once a second for
  0.5-0.8 ms (DXGI factory, outputs, display config): under 0.1 % of a
  core. The 0.8-point difference in process CPU is inside the run-to-run
  noise of this machine.
- **HDR costs** about 0.1-0.2 ms of GPU per frame for the shader, a
  slightly longer map (the copy waits behind the draw), and 2-3 points of
  one core in this share (the larger FP16 pool surfaces WGC and the driver
  handle). The FP16 frame never reaches the CPU.
- The FP16 runs with the high push times (and 2-36 fps sent) and the SDR
  timing run at 13-29 ms push both happened while the machine was at
  ~96 % CPU; the same paths measured 3-5 ms push when it was not.

## Correctness checks

- `the_shader_should_match_the_cpu_reference`: the GPU output of an FP16
  strip (eight hues, 0.01 to 10.0 on a log scale) through three tone maps
  is within one 8-bit code of `tonemap.rs` everywhere, for a texture with
  and without shader binding.
- `wgc_fp16_identity_should_match_bgra8_on_sdr` (ignored, needs the test
  window): the test window captured as BGRA8 and as FP16 + identity tone
  map at the same time: 99.70 % of pixels identical, the rest in the
  moving bars and clock between the two frames.
- The gradient strip (`YAP_HDR_GRADIENT_PNG`) and a viewer's screenshot
  of a forced `200,1000` share were looked at: the tone-mapped ramps keep
  their hue up to the peak where a scale-and-clip turns orange and skin
  tones yellow and white.
- `hook_probe --fp16`: an FP16 scRGB back buffer (magenta at 2x white)
  hooked and read as (255, 0, 255) with the identity map, and as
  (231, 0, 231) with `YAP_TEST_HDR_CAPTURE=200,1000` (0.8 linear,
  under the knee), the values the reference gives.

## Not verified here

- A real HDR display: detection (`G2084` color space with advanced color
  on), the SDR white level other than 80 nits, the peak and the switch
  between formats when HDR is toggled or a window moves to another
  monitor were not seen live; the code paths are the same as the forced
  ones except for the detection.
- 1440p live (no 1440p display).

# One speaker (2026-11-08)

Everything the app plays now goes through one WASAPI render stream
(`crates/yap-speaker`, LIBRARIES.md §17), and the call's echo canceller
hears its final mix. Before, the soundboard, its tones and Listen Together
played through `waveOut`, and the UI sounds through one `waveOut` handle
each. Only the call's own stream reached the canceller's far end.

## Conditions

- Native Spike Lab / General, the test owner (`spike`) and Spike Peer
  (`spikepeer`), both native, about 2 minutes per call.
- Fake devices only: `YAP_NULL_SPEAKER=1` on both. Our mic is a silent
  file (`YAP_FAKE_MIC`), and `YAP_FAKE_MIC_ECHO=0.5` adds the speaker's
  final mix to it 60 ms late at half level, as a room would. The peer has
  a silent mic.
- `spike` voice settings for the runs: the Standard profile (echo
  cancellation, WebRTC noise suppression and gain control on). The profile
  was on high fidelity (echo cancellation off) and was put back afterwards.
- The source is a 3 s clip at -14 dBFS RMS (a 196 Hz harmonic tone with
  vibrato plus low-passed noise, 3 Hz tremolo). `YAP_TEST_SOUNDBOARD` plays
  it as a soundboard clip every 8 s, through the soundboard's own engine and
  volume, without the UI.
- "Before" is the tree at `9cabefd9` plus two test-only switches. One
  sends `yap-audio`'s output to a clock-paced null sink under
  `YAP_NULL_SPEAKER`. The other adds that sink's output to the fake room
  but not to the far end, as speakers in a room would. Without them, the
  old build would have played the clip on the real speakers.
- Measured from `YAP_MIC_RAW_TAP` and `YAP_MIC_TAP`. The raw tap is the
  mic before the APM; the mic is silent, so it holds only the room's echo.
  The other tap is what is sent, after the APM. 100 ms windows where the
  raw echo is above -50 dBFS count as sound; windows below -70 dBFS count
  as silence.

## Echo of the soundboard in what we send

| run                               | echo in the room | sent during the clip | sent in silence | removed | per clip (median, range) |
| --------------------------------- | ---------------- | -------------------- | --------------- | ------- | ------------------------ |
| before (12 clips, from 15 s)      | -20.2 dBFS       | -20.5 dBFS           | -70.0 dBFS      | 0.3 dB  | 0.3 dB (-4.7 to 0.3)     |
| after (13 clips, from 15 s)       | -20.2 dBFS       | -89.1 dBFS           | -95.9 dBFS      | 69.0 dB | 69.2 dB (67.3 to 69.7)   |
| after, the first clip of the call | -20.2 dBFS       | -20.3 dBFS           |                 | 0.1 dB  |                          |

- Before, the clip goes out at the level it reaches the mic. Sent-to-raw
  correlation is 0.145 before and -0.001 after.
- The first clip of a call is the canceller's first far end, with nothing
  to have converged on yet, so that clip goes out. Every later clip is
  removed by 67 dB or more. In a call with people talking, the canceller
  converges before any clip.
- Listen Together (the live SWR3 stream) in the second call: -18.2 dBFS of
  echo, -38.6 dBFS sent (20.4 dB removed). That covers the station's first
  seconds, overlapping a soundboard clip, under the older harness below.
- Offline, `engine::pipeline::tests`: a clip on the speaker's bus is
  removed by 52.9 dB; the same clip outside the far end (the old topology)
  by 3.1 dB.
- A harness fix, found on the way. The fake room took its echo frame only
  when 70 ms were queued. Whenever the speaker's clock fell a moment behind
  the mic's, the simulated delay jumped by 10 ms, and each jump made the
  canceller re-align. In the first two after-runs, 2-3 of 11-14 clips
  leaked mid-call (medians still 65.3 and 67.7 dB). The room now takes one
  frame per frame once primed, so its delay stays put, as a room's does.
  The before run used the old harness; with no far end it leaks either
  way.

## Output latency

| step                                        | before (`waveOut`)                                  | after (`yap-speaker`)                      |
| ------------------------------------------- | --------------------------------------------------- | ------------------------------------------ |
| play to first mixed frame (19 and 17 clips) | 21.5-27.5 ms (20 ms blocks)                         | 1.6-11.3 ms (10 ms frames)                 |
| mixed to the device's end                   | up to 80 ms of `waveOut` buffers, then the engine's | 10-20 ms queued + 10 ms latency and period |
| total, play to ear                          | about 100 ms and up                                 | 22-41 ms                                   |

- `yap-audio` logs "play to first mixed frame"
  (`[audio] <key>: first frame mixed N ms after the play`), stamped on the
  render thread when the frame is mixed (after the clip is decoded). The
  before number is the same line in the old engine.
- The device side comes from `cargo run -p yap-speaker --example probe`. It
  initialises a client on the default output and closes it unstarted, so
  nothing plays. On VB-Audio Matrix (period 10 ms), a 20 ms request gives a
  22 ms buffer, and the stream latency plus a period is 10 ms. The speaker
  keeps 20 ms ahead (`YAP_SPEAKER_AHEAD_MS`), so 10-20 ms are queued. The
  call's old stream asked for 40 ms and kept 30 ms ahead.
- `waveOut`'s own latency under the audio engine has no API. Measuring it
  means playing sound and recording a loopback, which the test rules do
  not allow.
- The live runs used the null speaker, so there are no real-device
  underrun counts. The stats line now carries them
  (`speaker[<device> frames underruns reopens delay inputs]`): 0 underruns
  and 0 reopens on the null device.

## Outside a call

- UI sounds (Settings > Notifications, the "Call connected" and "Call
  ended" previews): the speaker opened on demand, both sounds were in
  `YAP_RENDER_TAP` at -37 dBFS, and it closed 2 s after the last.
- The mic test outside a call, on the fake mic clip: the monitor was in the
  speaker's mix at -15 dBFS for 30 s, and the meter was live. Studio and
  Voice isolation were switched during it, in place. Closing Settings
  stopped it (`[voice] mic test: off`, then `[speaker] idle: stopped`).

## Build, tests, clippy

- `build.ps1 -BuildDir <scratch>/yapn/rel-audio`: exit 0 (`build-audio-7.log`).
  The live runs used build 6. Build 7 adds one fix: the player's thread
  stays awake until its last events and the meter's final 0 are sent.
- `cargo test --workspace --exclude yap_native --exclude yap-audio`:
  2131 passed, 0 failed. `cargo test -p yap-audio`: 100 passed, 0 failed
  (`CARGO_TARGET_DIR=<scratch>/yapn/check-audio`, `LK_CUSTOM_WEBRTC` set).
- `cargo clippy --workspace --all-targets -- -D warnings` (`QMAKE` set):
  exit 0.

## Not verified

- On real hardware, with sound: a real change of the default device, an
  unplug mid-call (`AUDCLNT_E_DEVICE_INVALIDATED`), and underruns at 20 ms
  ahead on a loaded machine. The notification decisions and the reopen are
  unit-tested, and the client's sizes and latency come from the probe.
- The mic test beside a call, with both captures reading the same looped
  fixture. The test's monitor (in the far end) is then a delayed copy of
  what the call's mic captures, and the call's canceller removed it (the
  sent level fell from 0.9 to 0.01). With one real mic, the monitor always
  lags the live voice, so only its echo can be cancelled. Not checked live.

## Files

- New crate `crates/yap-speaker`: `bus.rs` (inputs, the final soft clip),
  `engine.rs` (the render thread, holds, taps, health, null mode, the
  render tap), `wasapi.rs` (the client, `IMMNotificationClient`, the
  probe), `endpoint.rs` (endpoint id matching, the name migration),
  `voices.rs` (the UI sounds), `wav.rs`, `examples/probe.rs`.
- `yap-voice` `engine/pipeline.rs`: the call's mix as an input, `CallTap`
  as its far end, `MicTest`, and a fake room that keeps its delay;
  `engine/wasapi.rs` (the old speaker removed); `engine/webrtc.rs` (the
  output choice goes to the speaker).
- `yap-audio` `engine.rs` (the mixer as `LocalInput`; `output.rs`, the
  `waveOut` device, removed), `mixer.rs`, `dsp.rs`, `lib.rs`.
- App: `src/sound.rs` (UI sounds on the speaker), `src/apps_audio.rs`
  (`YAP_TEST_SOUNDBOARD`; no device of its own), `src/audio.rs` (the
  name-to-id migration; `startMicTest`, `stopMicTest`, `micTesting`),
  `qml/screens/Settings/SettingsRail.qml` (Test mic).

# Performance pass 8: GIF decode off the GUI thread (2026-11-08)

Pass 7's WPR trace put Qt's GIF decoder (`QGIFFormat::decode` under
`AnimatedPicture::advance`) at 14.6-18 % of the GUI thread in the animated
idle. Streamed GIFs (chat GIFs, the GIF picker, pins) decoded one frame at a
time on the GUI thread, each time a frame was due.

## What changed

- **`yap-anim::gifstream`** (new, on `gif` 0.14.2, which was already in the
  lock):
  - `scan` reads the canvas, frame count, each delay and the loop count from
    the blocks, without decoding a pixel;
  - `GifStream` composites frame after frame onto one canvas as browsers
    do:
    - the canvas starts transparent;
    - the transparent index keeps the pixel under it;
    - a local palette applies to its own frame only;
    - interlaced rows are put in order;
    - disposal 2 clears to transparent; browsers ignore the background
      colour, and Qt does not;
    - disposal 3 restores the rectangle;
    - frames are clipped to the screen, and a 0x0 screen takes the first
      frame's extent;
  - seeking back rewinds and decodes from the start. A frame that cannot be
    read ends the animation there, and the first one is an error.
  - Output is premultiplied RGBA (`Format_RGBA8888_Premultiplied`, which
    the texture upload takes as is), scaled only when the GIF is more than
    twice its tile.
- **FFI** (`src/anim.rs`): `yap_gif_open`, `yap_gif_delay`,
  `yap_gif_stream_new`, `yap_gif_stream_frame` and the matching frees.
- **`AnimatedPicture`** (`cpp/animatedpicture.*`):
  - the loader thread scans the file (`scanGif`). The cache holds the file
    once, in Rust, under the same 4 MB budget for streamed files;
  - each item has a `GifPlayer` (its stream). One worker thread, `yap-gif`,
    decodes the frame due **one frame ahead**. When the slot on the shared
    30 Hz grid comes, the GUI thread only swaps the decoded `QImage` into the
    one `QSGPlainTexture` (pass 7's path);
  - the worker wakes the item only when the item waits for a frame (the
    first frame, a skip, a late decode). A frame decoded ahead waits to be
    taken at its slot;
  - frame buffers the window has let go of are decoded into again, so no
    allocation is made per frame;
  - the timeline (delays, loops, skipping to the frame due after a stall)
    stays on the GUI thread, from the scanned delays;
  - paused (off screen, the app not in front, finished): the timer stops as
    before, and the frame decoded ahead and the spare buffers are dropped
    (`restGif`). What stays is the file, the frame on screen and the Rust
    canvas;
  - `still` shows the first frame only, and `duration` now comes from the
    scan for a streamed GIF too;
  - `YAP_GIF_DECODER=qt` keeps the old path, Qt's reader on the GUI thread,
    for comparisons. A GIF yap-anim refuses (a canvas over 4096 a side or
    16 Mpx) falls back to that path;
  - `YAP_DEBUG_GIF=1` logs frame pacing every 300 frames shown: on time
    (decoded ahead), late (by how much) and first frames (the wait).
- **Delays**: the stream clamps 0-10 ms to 100 ms, as browsers do. The Qt
  path clamped to 16 ms.
- **Fixture**: `YAP_FIXTURE_GIF` takes a comma-separated list, and
  `YAP_FIXTURE_GIF_COUNT=n` puts one GIF in each of the last n messages
  (`yap-core/src/fixture.rs`).

A first version used a 2-thread `QThreadPool` and woke the item for every
frame. It cut the GUI thread by about 0.2 points, but the process went _up_
by about 1 point: the large set measured 6.05 % with Qt and 7.09 % with the
pool (3 rounds). The fix had three parts:

- a single worker that drains every player due in a slot in one wake-up,
  one frame per player at a time, so a GIF slower to decode than to play
  goes to the back of the queue and cannot hold up the others;
- no wake-up for frames decoded ahead;
- reused frame buffers.

## Correctness

A Python script (`craft.py`, scratch, not in the repo) writes 19 GIFs byte
by byte with its own LZW encoder. Pillow decodes its output back to the
source indices.

| case                                   | what it checks                                    |
| -------------------------------------- | ------------------------------------------------- |
| `disposal2_trans`, `disposal2_notrans` | disposal 2, with and without a transparent index  |
| `disposal3`, `disposal3_first`         | disposal 3, and disposal 3 on the first frame     |
| `transparency`                         | transparent indices                               |
| `interlaced`                           | interlace at 33x37 (odd), and a 20x29 subframe    |
| `local_palettes`, `no_global_palette`  | local palettes of 2, 16 and 256 colours           |
| `zero_delays`                          | delays of 0, 1, 2, 5 and 10 cs                    |
| `loops_none`, `loops_3`                | loop counts                                       |
| `offscreen_frame`, `zero_screen`       | a frame past the screen, a 0x0 screen             |
| `palette_overflow`                     | an index past the palette                         |
| `gif87a`                               | GIF87a                                            |
| `huge_4000`, `too_wide_5000`           | 4000x4000 (16 Mpx), 5000x40                       |
| `truncated`                            | a truncated file                                  |
| `chat_320`                             | 320x240, 60 frames, subframes with every disposal |

Real files: the repo's `docs/readme/yappy-bark.gif`, the pass 3 scene's
`chat.gif` and `icon.gif`, and three 50 % re-encodes made with Pillow.

Each file was decoded three ways and compared frame by frame, pixel by
pixel, premultiplied:

- `yap-anim`'s `examples/gifdump.rs`: the stream, at canvas size;
- **Qt's reader**, PySide6 6.12, whose GIF code is the same as 6.11's;
- **headless Edge**: WebCodecs `ImageDecoder`, the decoder `<img>` uses,
  from local bytes on a routed throwaway origin. Nothing was fetched.

Results:

- **Edge: identical on every frame of every file** (`huge_4000` was not run
  in Edge). This includes the truncated file, where both stop after 2
  frames, and the out-of-palette index, where both stop after 1.
- **Qt: identical on every frame of 15 of the 24 files.** Edge sides with
  the stream on every difference:
  - Qt paints the background colour where browsers leave transparency: on
    disposal 2 (`disposal2_notrans`, `disposal3` frame 4, `huge_4000`
    frame 2, `chat_320` from frame 3 on, `offscreen_frame`), and around the
    first frame on a 0x0 screen (`zero_screen`);
  - on disposal 3 of the first frame, Qt keeps the frame where browsers
    clear it;
  - Qt shows one more frame after a truncated LZW stream, and after an
    out-of-palette index, where Edge and the stream stop.
- **Loops**: no NETSCAPE block plays once, 0 plays forever, n plays n+1
  times. This matches Edge's `repetitionCount` and QMovie's count.
- **Delays**: the same as Qt's `nextImageDelay`, clamped.
- `too_wide_5000` is refused by the stream, so the app plays it through Qt's
  reader.

**A `gif` 0.14.2 bug**: its row-by-row deinterlacing stops with "image
truncated" on some interlaced frames, 33x37 among them. Qt, Pillow and Edge
read those whole. `gifstream` reads each frame in one piece and puts the
rows in order itself (LIBRARIES.md §18.4).

**Decode cost**, best of 3, per frame, in `gifdump`. Qt's reader is
PySide6's `read()`, which does not include its conversion to premultiplied:

| file               | stream   | Qt       |
| ------------------ | -------- | -------- |
| chat.gif 360x240   | 0.137 ms | 0.165 ms |
| icon.gif 128x128   | 0.041 ms | 0.050 ms |
| chat_320 320x240   | 0.112 ms | 0.093 ms |
| yappy-bark 320x320 | 0.101 ms | 0.094 ms |

A frame whose descriptor is over 16 Mpx is refused before anything is
allocated for it, whatever the screen size says. The crate itself checks no
limit on that path. The scan refuses such a file, and Qt's reader takes it.

Tests: 16 new `gifstream` unit tests cover the scan, loops, delays, each
disposal, the first frame's disposal 3, local palettes, interlace (seven
sizes), clipping, the 0x0 screen, the huge canvas and huge frame
refusals, seeking back,
truncation and scaling into a stride.

## Method

- Offline and on our own profile (`gif8`): the chat-10k fixture with local
  GIF files, the window at 1200x1040 on DISPLAY15 (1080p, 100 %).
  `WM_ACTIVATE` was posted to mark it active, with no real focus.
  `YAP_NO_TOASTS`, `YAP_NO_CLIPBOARD` and `YAP_TEST_POINTER` were set.
- CPU is `TotalProcessorTime` over 40 s after 28 s of settling: the
  process, the GUI thread (the process's first thread), and the other
  threads by name (`GetThreadDescription`). Each run is one launch.
- The decoder is A/B'd in one exe (`YAP_GIF_DECODER=qt` against the
  default), interleaved over 3 rounds.
- **The machine was shared.** Other agents' builds were running, along with
  a qemu VM (2 cores). Compare only within a round.

Scenes:

- **large**: `YAP_FIXTURE_GIF_COUNT=6` with chat.gif (360x240), chat_320
  and yappy-bark. 3 GIFs are on screen.
- **small**: 10 messages with the three 50 % re-encodes. 4 GIFs are on
  screen.
- **pass 3**: pass 3's scene, two chat.gif messages and the GIF server icon
  (the icon decodes whole and is not streamed). This is the "animated idle"
  target.

## Results

CPU, % of one core, as qt / stream for each round:

| scene  | process                         | GUI thread                      | `yap-gif` |
| ------ | ------------------------------- | ------------------------------- | --------- |
| large  | 5.27/4.69, 6.52/5.55, 6.02/4.72 | 2.66/2.07, 3.55/2.54, 3.12/1.84 | 0.47-0.66 |
| small  | 5.51/4.96, 5.47/4.84, 4.88/5.00 | 3.12/1.91, 2.89/2.30, 2.77/1.80 | 0.39-0.55 |
| pass 3 | 4.84/5.58, 5.27/5.16, 5.74/4.49 | 2.34/2.11, 2.50/2.03, 2.77/1.60 | 0.51-0.74 |

Means:

| scene  | process, Qt | process, stream    | GUI, Qt | GUI, stream        |
| ------ | ----------- | ------------------ | ------- | ------------------ |
| large  | 5.94 %      | **4.99 %** (−16 %) | 3.11 %  | **2.15 %** (−31 %) |
| small  | 5.29 %      | **4.93 %** (−7 %)  | 2.93 %  | **2.00 %** (−32 %) |
| pass 3 | 5.28 %      | **5.08 %** (−4 %)  | 2.54 %  | **1.91 %** (−25 %) |

- **The GUI thread is 25-32 % lower in every round of every scene.** That is
  more than the 15-18 % the trace gave Qt's decoder. The GUI thread no
  longer converts ARGB32 to premultiplied RGBA either, and no longer
  allocates a frame each time.
- **The process total is lower in 7 of 9 rounds**, by 4-16 % on the means.
  About 0.5 points moved to `yap-gif`. The rounds vary by up to 1.3 points
  with the machine's load, so the totals are the weaker result.
- **Paused, the stream costs nothing.** With the app not in front (no
  activation), 30 s measured 0.21 % with Qt and 0.05 % with the stream.
  Scrolled so the GIFs are off screen, 0.16 % and 0.21 %. In both cases the
  GUI thread measured 0.00 %, and the rest is `YapGames`, the game
  detector.
- **Private memory**: +1 to +3 MB with the stream in the large scene
  (198.3 against 195.3 MB) and the same in the others. That is the frame
  decoded ahead and up to two reused buffers per playing GIF. The final
  build drops them while paused.

**Frame pacing** (`YAP_DEBUG_GIF`, every run with the stream):

- 0 late frames out of ~2,090 per large run, ~2,690 per small run and
  ~1,500 per pass 3 run. The only exceptions were 1 frame late by 1 ms in
  one run, and in the first build (the pool) 1-17 late frames, at most 4 ms.
- The first frame of a stream is on screen 0.4-5.4 ms (mean per run) after
  it is opened.

Renders (`QSG_RENDER_TIMING`, large scene, the last two thirds of 40 s):

| decoder | renders/s | gap p50 | gap p95 | gap p99 | gap max | process | GUI    |
| ------- | --------- | ------- | ------- | ------- | ------- | ------- | ------ |
| Qt      | 27.8      | 33 ms   | 66 ms   | 67 ms   | 68 ms   | 8.44 %  | 4.10 % |
| stream  | 27.1      | 33 ms   | 66 ms   | 67 ms   | 68 ms   | 7.46 %  | 2.93 % |

The renders keep the same pacing on the 30 Hz grid. A 66 ms gap is a slot in
which no GIF changed (60 ms and 40 ms GIFs on a 33 ms grid). It is not a
late frame.

## Targets

- **GIF decode off the GUI thread: done.** The GUI thread is 25-32 % lower,
  and frames meet their slots.
- **Animated idle under 3 %: not reached on this machine today.** Pass 3's
  scene measured 4.49-5.58 % with the stream and 4.84-5.74 % with Qt, under
  other agents' load. Pass 7 had 2.77-4.49 % on a quieter machine. What is
  left is pass 7's other cost: rendering (83 draws a frame), the render
  thread and the driver's threads.
- **A real chat GIF**: the test server "Native Spike Lab" #general had none.
  Spike Peer posted `chat.gif` through the REST upload, the post was
  deleted afterwards, and the final build ran as the test owner (profile
  `spike`). The GIF loaded through the attachment URL and played, with 298
  frames on time, 0 late and the first frame 0.5 ms after the stream
  opened. With that one GIF on screen the process used 3.05 % of a core
  over 20 s.
- Not measured: a WPR trace of the new build (no elevation this pass), and
  the GIF picker, whose GIFs come from KLIPY and were not loaded.

## Build, tests, clippy

The checks used `QMAKE` at `<scratch>/qt-custom-6.11p8`, `LK_CUSTOM_WEBRTC` and
`--target-dir <scratch>/yapn/check-gifdeps`. Logs are in `<scratch>/yapn/`.

- **Tests**: the workspace without `yap_native` and `yap-audio` passed
  2,116 with 0 failed (`test-gifdeps-2.log`). `yap-audio` on its own
  passed 102 with 0 failed. `yap-anim` passed 31 with 0 failed after the
  review fixes (`test-gifdeps-anim-3.log`). `yap-webm` prints three
  warnings when built without its `libvpx` feature; they were there before
  this change.
- **Clippy**: `cargo clippy --workspace --all-targets -- -D warnings`
  exited 0 with no warnings (`clippy-gifdeps-3.log`).
- **Builds**: `build.ps1 -BuildDir <scratch>/yapn/rel-gifdeps` exited 0 five
  times (`build-gifdeps-1..5.log`). Build 2 added `/MAP` for LIBRARIES.md
  §18.3. The exe is 58.6 MB.
- **The final build** (round-robin worker, the frame-size guard, rest when
  finished) was checked once more, on one run each:

  | scene, decoder     | process | GUI    | `yap-gif` |
  | ------------------ | ------- | ------ | --------- |
  | large, Qt          | 4.84 %  | 2.24 % | -         |
  | large, stream      | 4.37 %  | 1.72 % | 0.47 %    |
  | small, stream      | 5.10 %  | 2.14 % | 0.31 %    |
  | off screen, stream | 0.23 %  | 0.00 % | -         |

  The streams were still identical to Edge on all 23 files.

## Files

- `rust/crates/yap-anim/src/gifstream.rs` (new), `src/lib.rs`
  (`AnimError::Gif`), `Cargo.toml` (`gif`), `examples/gifdump.rs` (new);
- `rust/src/anim.rs`: the `yap_gif_*` FFI;
- `cpp/animatedpicture.h`, `cpp/animatedpicture.cpp`: `GifPlayer`,
  `GifWorker`, the streamed path, `YAP_GIF_DECODER`, `YAP_DEBUG_GIF`;
- `rust/crates/yap-core/src/fixture.rs`: `YAP_FIXTURE_GIF_COUNT` and the
  URL list.

# Boost to 400 % (2026-11-08)

The per-user volume and stream volume go to 400 %. A track above unity
gain goes through its own look-ahead limiter in the call's mix
(LIBRARIES.md §11.1); at or under unity nothing changes.

## Cost

- Offline, release build (`cargo test --release -p yap-voice --lib
dsp::mix::tests::boost_limiter_cost -- --nocapture`): one stream's mix
  takes 1.6 µs per 10 ms frame at gain 1.0 and 6.0 µs at 4.0 with the
  limiter on, so the limiter costs about 4.5 µs per boosted stream per
  frame (0.045 % of one core). Ten boosted people: about 45 µs a frame.
- In the call (the `[voice] stats` line, `mix=`, one remote stream): 7-8 µs
  per frame at 100 %, 14-15 µs at 400 %.
- Memory: 73 floats of ring and a few words per stream; the per-frame
  scratch (about 7 KB) is on the render thread's stack, nothing is
  allocated.

## Live measurement

- Native Spike Lab / General, Spike Peer (`spikepeer`) talking through
  `YAP_FAKE_MIC`, the test owner (`spike`) listening; `YAP_NULL_SPEAKER=1`
  on both, `spike`'s own mic a silent file. `spike` recorded its final mix
  with `YAP_RENDER_TAP` (120 s).
- The source is a 30 s loop: 20 s of synthetic voiced syllables (harmonic
  tone with pitch glides, 80-300 ms envelopes, gaps; one syllable in six
  near -4 dBFS), then a 10 s log sine sweep, 100 Hz to 4 kHz at -3 dBFS.
- `spike`'s output volume was 10 % in its saved settings, which made the
  400 % run a total gain of 0.4 (the limiter never engaged). It was set to
  100 % for the run and put back afterwards.
- 100 % for 36 s, then 400 % (End on the call menu's "User volume" slider) for 60 s. Windows
  of whole loops, compared at the same place in the loop:

| window                 | 100 %: peak | 100 %: RMS | 400 %: peak | 400 %: RMS | samples at full scale (100 / 400 %) | gain    |
| ---------------------- | ----------- | ---------- | ----------- | ---------- | ----------------------------------- | ------- |
| a whole loop (30 s)    | -4.89 dBFS  | -18.53     | -3.00 dBFS  | -13.20     | 0 / 0                               | +5.3 dB |
| the syllables (17.5 s) | -4.91 dBFS  | -27.38     | -3.00 dBFS  | -19.66     | 0 / 0                               | +7.7 dB |
| the sweep (8.5 s)      | -7.71 dBFS  | -14.15     | -3.00 dBFS  | -9.11      | 0 / 0                               | +5.0 dB |

- The peak at 400 % is the limiter's ceiling, 23198 of 32768 (-3.00
  dBFS), in every second of the boosted run; nothing at or within 0.01 dB
  of full scale, and no flat-topped runs of samples at the peak.
- The gain is under the nominal +12 dB because the material is loud: the
  sweep arrives at -7.7 dBFS (after the sender's processing and Opus), so
  4x would put it at +4.3 dBFS and the limiter takes 7.3 dB off; the
  syllables peak near -5 dBFS and lose about 4 dB on their loud parts.
  Quiet talkers get more of the 12 dB (offline: a -18 dBFS talker gains
  12.0 dB between its shouts).
- The stats line counted 5,832 boosted frames and 3 samples cut by the
  last clamp (the look-ahead guessing wrong, a skewed frame or the gain
  step), out of 5.6 million.
