# Performance pass (2026-11-06)

This pass measures the native client against the Tauri desktop app and
against its own video-phase numbers. It then fixes the hot spots in this
order: idle CPU, memory, scroll jank and startup.

- Base: the tree at 6dae4cc8, rebuilt in `F:/ct/yapn/rel` (the exe there
  was older than HEAD).
- After: the same tree with the fixes below (build "fix5").
- Machine: the user's PC with an RTX 3080 and three 1080p60 monitors,
  running Windows 11 26200.

## Method

All runs use the release build. They use `--profile jontest` (the user's
real account) unless noted otherwise. Calls use `--profile spike`, the test
account.

### Tools

The tools live in `F:/ct/yapn/tools`. Every script that stops a process
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
  them is left out). Scripts: `F:/ct/yapn/tools/p2/` (`idle.ps1`,
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
a9a54bd1 (kept as `F:/ct/yapn/n6b-before`), after is this batch's; same
machine (Ryzen 9 7900X, RTX 3080, driver 32.0.16.1047), `--profile spike` on
the owner test account in General (Native Spike Lab), the peer test account
in headless Edge (`F:/ct/yapn/run/n6b/peer-n6b.mjs`: fake media, a 1920x1080
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
(`F:/ct/yapn/tmp/p3-final.exe`). Both were built from the `native/perf3`
worktree into `F:/ct/yapn/rel-perf`.

## Conditions

- **The displays slept for the whole session.** `vblank.ps1` showed vblank
  waits returning at once, so DWM paced presents at ~4 Hz. Nothing that
  depends on frame pacing could be measured honestly: scroll timing,
  animated idle CPU or a 60 fps share. See the targets below.
- **Another agent was using the owner test account for most of the
  session.** A `--profile spike` process from `F:/ct/yapn/run/sweep7` was
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
  attachments to the fixture. They take local `file://` URLs
  (`F:/ct/yapn/tmp/anim`), giving an animated scene without an account.

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

Before is the pass 3 build (59acb1e5, `F:/ct/yapn/tmp/p4/soak0.exe`); after
is this pass's final build (`F:/ct/yapn/tmp/p4/final.exe`). For the Qt and
opt-level A/B, `F:/ct/yapn/rel-base4` is this pass's source built with the
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
- **Tools** (`F:/ct/yapn/tools/p4/`):
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
  - `F:/ct/yapn/tools/p4-sampler.ps1`: read-only counters by PID.
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

Before is 04c8d8e5 built on the custom Qt 6.6.3 (`F:/ct/yapn/tmp/p5/before.exe`,
63,847,424 B). After is this pass's final build on the custom Qt 6.11.3
(`F:/ct/yapn/tmp/p5/c7.exe`, 59,124,224 B). Both were built from the
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

Tools are in `F:/ct/yapn/tools/p5/`:

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

- **`build.ps1`** now uses `F:/ct/qt-custom-6.11/6.11.3-yap` and refuses to
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
