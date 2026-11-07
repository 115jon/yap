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
   also encodes in software.
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
