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
