# Mobile on the native core: research and prototypes

How much of the native desktop client (`desktop-native/`) Android and iOS
can share, and how. The goal (owner, 2026-11-10) is to share the Qt/QML
views themselves, not only the Rust core, and make them look and behave
native on each platform. This folder holds the research, two Android
prototypes on the same unchanged core, and their side-by-side measurements:

- `qt/`: **Qt Quick on Android**, the main path. The desktop's QML,
  list models and portable C++ run on a phone, under a mobile shell and a
  thin platform shim. Application id `dev.yap.qtproto`.
- `rust/` and `android/`: **the Compose baseline**. The same core behind
  UniFFI with Jetpack Compose views, kept to compare against. Application
  id `dev.yap.coreproto`.

The Expo app (`apps/mobile`) is untouched and keeps shipping. Nothing here
is built by its workflows or changes its package, signing or store listing.

## Recommendation

**Go with Qt Quick (a) as the mobile view layer, sharing QML with the
desktop.** The prototype shows the sharing is real, the platform gaps are
small shims, and scrolling is smooth. The costs are measured, not
estimated: a larger download, higher memory, and a short list of things
that feel off, each with a known fix.

- **Shared, unchanged:** 30 desktop QML files (4,710 lines: `Theme`,
  `MessageDelegate`, `TreeRow`, `ServerIcon` and their components), the
  desktop's `models.rs` (all 13 list models), `anim.rs` and `gateway.rs`, and
  three desktop C++ files (`AnimatedPicture`, the icon and dither image
  providers). Together with the core, that is everything a row shows and
  every rule behind it.
- **Per platform:** about 890 lines of QML (shell, pages, composer, and
  the small mobile side of seven desktop seams), 890 lines of Rust (the
  `Session`/`Nav`/`Chat` bridge and the registry the shared models compile
  against), and about 450 lines of C++/Java shim for back, keyboard, haptics,
  share, text size and fling.
- **Verified on the emulator:**
  - Kova sign-in, the server rail, the channel tree and `#general`, with
    the desktop's grouping, mention rows, role colours and Markdown;
  - send from the composer, and a live message from the peer arriving over
    the gateway;
  - the keyboard pushing the composer up as it animates;
  - the predictive back gesture (the page follows the finger, then pops);
  - edge to edge under the system bars, and light mode from the system;
  - 60 Hz scrolling through 170 messages.
- **The price, against the Compose baseline:**
  - about 9 MB more to download (21 MB vs 12.6 MB gzipped);
  - 2.5x the memory after scrolling (148 MB vs 60 MB PSS);
  - Qt's own HTTPS needs OpenSSL on Android (see "Gaps").

Keep the Rust boundary exactly as it is: the core does not know which view
layer draws it, and the Compose baseline proves a native view stays possible
for any one surface (for example a call screen) if Qt ever falls short there.

## What is shared, what is per platform

| Layer                         | Shared with desktop (unchanged)                                                                                                                                                                                                                                                                                                             | Per platform                                                                                                                                                                                                                                                                           |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Core                          | `yap-core`, `yap-cache`, `yap-realtime`, `yap-protocol`, `yap-anim`; `gateway.rs`                                                                                                                                                                                                                                                           | TLS backend (rustls on mobile), the token store (an app-private file; Keystore in a release), the config and data folder handed in (`yap_mobile_init`)                                                                                                                                 |
| Models and bridge objects     | `models.rs`: every `QAbstractListModel` (servers, tree, messages, members, DMs, …) with the same role names                                                                                                                                                                                                                                 | `bridge.rs`: the desktop's `Session`/`Nav`/`Chat` names and signatures, for the part the app covers. `gui.rs`: the registry `models.rs` compiles against. Splitting the desktop's bridge into a shared part and a desktop rest would let both apps compile one file                    |
| Theme tokens                  | `Theme.qml` (palettes, type roles, motion, spacing), `ShellState.qml`                                                                                                                                                                                                                                                                       | Which theme (`Theme.themeId` follows the system's light/dark); a text scale is still missing (see the checklist)                                                                                                                                                                       |
| Components and list delegates | `MessageDelegate` (the message renderer: grouping, replies, reactions, mention and pending rows, Markdown via Qt rich text), `TreeRow`, `ChannelRow`, `CategoryLabel`, `MemberRow`, `ServerIcon`, `RailPill`, `CountBadge`, `Avatar`, `DisplayName`, `Icon`, `IconKey`, `KitButton`, `Led`, `Spinner`, `LinkText`, `LazyPopup`, `Tooltip` … | The mobile side of each desktop-only seam the shared files reach, with the same API: `EmbedCard`, `MessageMedia`, `VoiceStatusLine`, `VoiceStreamHoverCard`, and stub singletons for `Admin`, `Apps`, `Emoji`, `Extras`, `Invites`, `Panels`, `Profile`, `Voice` (`qt/rust/qml/seams`) |
| C++                           | `AnimatedPicture` and `ViewportWatch` (GIF/APNG/WebP through yap-anim), the `icon` and `dither` image providers                                                                                                                                                                                                                             | `Native` (the platform shim) and `main.cpp`. The Windows C++ (window chrome, toasts, WebView2, Media Foundation, D3D11 video) stays desktop-only                                                                                                                                       |
| Page layouts and navigation   | —                                                                                                                                                                                                                                                                                                                                           | Phone pages (`ServersPage`: rail and tree side by side; `ChatPage`; `SignInPage`), a `StackView` with the platform's transitions and back, `PlatformList` (the platform's scroll feel)                                                                                                 |
| Shell                         | —                                                                                                                                                                                                                                                                                                                                           | `Main.qml` (edge to edge, phases, foreground/background), the Android manifest, the Java shim                                                                                                                                                                                          |

The seam is clear in the code. The desktop's portable QML rests on three
kinds of desktop-only thing:

- C++ types in `Yap.Media` (video through Media Foundation, WebView2 embeds);
- `Yap.Chrome` singletons (window chrome, desktop shell);
- desktop domain singletons the prototype does not cover yet.

Each one gets a mobile version under the same name. A shared file that
reaches one of them does not change; the platform supplies the other side.
build.rs mirrors the shared files from `desktop-native/rust` into the
crate's module, gitignored, at build time. Once the desktop moves to
`desktop/`, the shared files should become their own QML module and crate,
with the desktop and mobile shells as two consumers.

## Native look and feel on Qt Quick 6.11

| Behaviour                         | What Qt 6.11 does by itself                                                                                                                        | What the prototype adds                                                                                                                                                                                                                   | State                                                                                                                                                |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Edge to edge, insets              | **Native.** `Qt.ExpandedClientAreaHint` draws under the bars; the `SafeArea` attached property gives the bar and cutout margins                    | Pages pad by `SafeArea.margins`                                                                                                                                                                                                           | Verified, including a taller status bar after a reboot                                                                                               |
| Predictive back                   | **No.** Qt's manifest template turns `enableOnBackInvokedCallback` off and Back arrives as a key                                                   | Java shim: an `OnBackAnimationCallback` registered only while there is a page to pop, its progress driving the page, so the system plays its own back-to-home preview at the root. Before API 33, Back stays a key the shim handles       | Verified: the page follows the finger, then pops; the app stays open                                                                                 |
| Keyboard resize                   | **Partly.** With edge to edge (targetSdk 35+) the window is not resized and Qt reports no IME inset in `SafeArea`                                  | Java shim: the resting IME inset from the decor view's insets, and every frame of the animation from a `WindowInsetsAnimation` callback on the content view (Qt replaces the decor view's own); the composer sits on it (`adjustNothing`) | Verified at rest: the composer sits on the keyboard and the list stays at its end. The per-frame path is in place but was not captured mid-animation |
| IME action, autocorrect           | **Native**: suggestions, auto-capitals and autocorrect come through Qt's input connection. `EnterKey.type` sets the action for a single-line field | `EnterKey.type: Qt.EnterKeySend` on the composer                                                                                                                                                                                          | Suggestions verified. The Send action does not reach the IME for a multi-line `TextArea` (still ↵)                                                   |
| Scroll physics, overscroll, fling | **Partly.** Qt's own rubber band, close to iOS's bounce. No Android stretch; fling speed and friction are Qt's defaults                            | `PlatformList`: the Android 12+ stretch emulated from `verticalOvershoot` (content stays put, scales from the far edge); fling speed and a friction matching `OverScroller`'s spline from `ViewConfiguration` via the shim                | Fling verified by the scroll runs. The stretch is implemented but not yet compared frame by frame                                                    |
| Haptics                           | **No.**                                                                                                                                            | Shim: `View.performHapticFeedback` (tick, long press, confirm, reject) on taps, sends and reactions                                                                                                                                       | Wired; the emulator has no vibrator to confirm it                                                                                                    |
| System font, dynamic text size    | **Partly.** Qt's default font is the system's; explicit `pixelSize`s ignore the system text size                                                   | Shim reads `Configuration.fontScale` (`Native.fontScale`)                                                                                                                                                                                 | Not applied yet: the shared `Theme` needs a `textScale` multiplier on its type roles (desktop at 1)                                                  |
| Dark mode, Material You           | **Native**: `Application.styleHints.colorScheme`, live                                                                                             | `Theme.themeId` follows it. The shim reads Material You's `system_accent1` (`Native.accent`)                                                                                                                                              | Light/dark verified. The accent is not used: Yap's themes are brand palettes (owner decision)                                                        |
| Share sheet                       | **No** (Qt has `QDesktopServices` for URLs only)                                                                                                   | Shim: `ACTION_SEND` chooser, on a long press on a message row                                                                                                                                                                             | Wired, but a long press on the text itself starts text selection first; a phone message menu should own the gesture                                  |
| Notifications                     | **No** FCM/APNs module                                                                                                                             | —                                                                                                                                                                                                                                         | Platform code (FirebaseMessagingService, APNs) under any option, the same work as for Compose                                                        |
| Text selection handles            | **Native**: Qt Quick's text fields draw Android-style handles and the cut/copy/paste bar                                                           | `persistentSelection` on the composer                                                                                                                                                                                                     | Verified: a long press on a message's text shows the handles and the Copy / Select all bar                                                           |
| 60/120 Hz                         | **Native**: the threaded render loop is paced by vsync at the display's rate                                                                       | `[frames]` pacing log (the render thread's swap intervals)                                                                                                                                                                                | 60 Hz verified (the emulator's display); 120 Hz needs a device                                                                                       |
| Hover and right-click affordances | Qt synthesises hover from touch                                                                                                                    | —                                                                                                                                                                                                                                         | Feels off: the desktop's message hover bar shows on touch (see below)                                                                                |

The shim is one C++ singleton (`qt/cpp/native.*`, 280 lines) over one Java
class (`qt/android/src/dev/yap/qtproto/NativeShim.java`, 170 lines). An
iOS build backs the same `Native` properties with Swift or Objective-C:
`UIScreenEdgePanGestureRecognizer` for back, keyboard notifications,
`UIFeedbackGenerator`, `UIActivityViewController`, Dynamic Type's
`preferredContentSizeCategory`.

## Side by side (same emulator, same account, same channel)

Measured on the shared `Comet_API_35` emulator (x86_64, API 35, 60 Hz,
1080×2400) after a cold boot. The host had under 1 GB of RAM free, so the
emulator was partly paged out. Start times are therefore slow and noisy for
both apps alike; compare them with each other, not with a phone.
`#general` held 170 messages for the scroll test.

|                                    | Compose baseline                                                            | Qt Quick                                                                                |
| ---------------------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Cold start (`am start -W`, 3 runs) | 10.1 s, 5.1 s, 3.4 s                                                        | 11.7 s, 3.3 s, 4.8 s (first frame 5.1 s, 1.8 s, 1.4 s)                                  |
| Snapshot read (cached content)     | 222–280 ms                                                                  | 7–75 ms                                                                                 |
| Scrolling `#general` (24 flings)   | 751 frames; jank 6.4%; frame time p50 18 ms, p90 23 ms, p95 32 ms (gfxinfo) | about 1,460 frames; swap interval p50 16.6 ms, p95 18–19 ms; 8 frames over 34 ms (0.5%) |
| Memory after scrolling (PSS)       | 60 MB                                                                       | 148 MB (87 MB before opening the channel)                                               |
| APK, x86_64                        | 19.5 MB (12.6 MB gzipped), R8 off                                           | 61.4 MB                                                                                 |
| APK, arm64                         | core 8.7 MB (4.0 MB gzipped) plus Compose; about 12 MB gzipped              | **58.4 MB, 21.2 MB gzipped** (native libraries 13.4 MB xz)                              |

The two scroll columns measure different things. gfxinfo times each
frame's work on the UI and render threads; Qt's column is the interval
between swaps on its render thread, which gfxinfo cannot see. Both say the
same thing: neither list drops frames in a steady fling. Qt holds vsync
with a few spikes when new rows are created.

<p>
<img src="docs/qt-signin.png" width="180" alt="Qt: sign-in">
<img src="docs/qt-servers.png" width="180" alt="Qt: rail and channel tree">
<img src="docs/qt-chat.png" width="180" alt="Qt: #general">
<img src="docs/qt-markdown.png" width="180" alt="Qt: Markdown, mentions and links">
</p>
<p>
<img src="docs/qt-keyboard-send.png" width="180" alt="Qt: keyboard and a sent message">
<img src="docs/qt-predictive-back.png" width="180" alt="Qt: predictive back in progress">
<img src="docs/qt-text-selection.png" width="180" alt="Qt: native text selection on a message">
<img src="docs/compose-servers.png" width="180" alt="Compose: channel list">
<img src="docs/compose-chat.png" width="180" alt="Compose: #general">
</p>

### What feels off

Qt Quick:

1. The desktop's hover action bar (react, reply, edit, more) shows on a
   touched message, because Qt synthesises hover from touch. Fix: a
   `Theme.hoverUi` (or `ShellState`) flag the shared delegate reads, off
   on touch platforms, and a long-press menu in its place.
2. The composer's IME shows ↵, not Send (`EnterKey` with a multi-line
   `TextArea`).
3. Text does not follow the system text size (no `textScale` in the shared
   `Theme` yet).
4. Desktop proportions in places: 40 px avatars and the desktop's row
   padding read slightly small on a phone. These are tokens, so they can be
   tuned per platform.
5. The first cold start is slow (qmlcachegen compiles the QML to C++ but
   Qt's own libraries load as 15 shared objects); the second and later are
   comparable to Compose.

Compose baseline:

1. Hard-coded dark mode, and dark status-bar icons on the dark background.
2. Markdown shows as source: the core's `html` is Qt rich text, which
   Compose cannot draw without a new renderer.
3. Every row is re-implemented, so it drifts from the desktop's design
   (generic avatars, Material type, no Yap fonts or tokens).
4. Read-only: no composer.

## Size: what is in the APK, and what can go

arm64, release, shared Qt libraries from download.qt.io (`qt/build.sh`):

| Part                                                                                                   | Size    | xz     |
| ------------------------------------------------------------------------------------------------------ | ------- | ------ |
| `libyapqt` (the core, the bridge, the QML compiled to C++, cxx-qt, AnimatedPicture)                    | 13.8 MB | 4.0 MB |
| Qt Gui, Quick, Core, Qml                                                                               | 26.6 MB | 6.4 MB |
| Qt QuickTemplates2, QuickControls2 (Basic), Network, QmlModels, Svg, OpenGL, QuickShapes, QuickEffects | 9.6 MB  | 2.0 MB |
| Plugins: android platform, gif/jpeg/webp/svg, the OpenSSL TLS backend, offscreen                       | 1.9 MB  | 0.5 MB |
| `libc++_shared`                                                                                        | 1.3 MB  | 0.3 MB |
| `classes.dex` (Qt's Java, the shim)                                                                    | 5.3 MB  | —      |

Modules used: Core, Gui, Network, Qml (QmlModels, QmlMeta, WorkerScript),
Quick, QuickControls2 with the Basic style, QuickTemplates2, QuickEffects
(MultiEffect for round avatars), QuickShapes, Svg, OpenGL.

Dropped already (`qt_import_plugins` in `qt/CMakeLists.txt`): the QML
debugger and profiler services, the TIFF/ICO/ICNS/TGA/WBMP decoders, icon
engines, network information. Not linked at all: Multimedia, WebView,
Widgets, Quick 3D, the other Controls styles, SQL.

Next savings, not measured yet:

- R8 on the Java side (5.3 MB of dex);
- the desktop's bytecode-only QML (`patches/qt-build-utils`; desktop pass 5
  saved 2.8 MiB);
- the offscreen platform plugin and the OpenSSL backend (both unused);
- a static Qt for Android with LTO. That needs Qt built from source for
  Android (hours, about 15 GB). Not attempted here, so static vs shared is
  open.

## Gaps found

- **HTTPS for Qt's own network loads.** The core's HTTP is rustls, but
  `Image` and `AnimatedPicture` load avatars and attachments through Qt
  Network. On Android that needs OpenSSL (`libssl`/`libcrypto`), which Qt
  does not ship. Two ways out: bundle KDAB's `android_openssl` (about 3 MB
  per ABI), or give the engine a `QNetworkAccessManagerFactory` whose
  replies come from the core's rustls client. The second keeps one TLS
  stack and also carries the session for private media, as the desktop's
  `AuthMedia` does.
- **Exit.** Qt's Android exit path tears down the engine while the core's
  threads run. Call `yap_mobile_flush` on going to the background (done)
  and leave by moving the task back, never by quitting the engine.
- **Memory.** 148 MB after a long scroll. Next step: a heap profile of
  `MessageDelegate` on a phone (the desktop's delegate carries hover cards,
  popups and loaders a phone never opens).

## Licensing: owner decisions

These are open questions for the owner, written up here, not decided:

1. **GPL-3.0 and the App Store.** Yap's own crates are GPL-3.0-only. GPL
   code on Apple's store is widely considered incompatible with Apple's
   terms (VLC was removed in 2011). Every third-party crate in `deny.toml`
   is permissive or MPL-2.0, so the owner, who holds the copyright, can add
   an App Store exception or dual-license the mobile build. This applies to
   every option, including Compose/SwiftUI.
2. **Qt's LGPL-3 on iOS.** The LGPL requires that users can relink the app
   against a modified Qt. Three routes:
   - **dynamic Qt frameworks**: users swap the framework. Qt's official iOS
     packages are static, so this needs Qt built from source as frameworks,
     which is not a configuration Qt supports or tests;
   - **relinkable object files**: ship the static build plus the app's
     object files and a link script on request. This is the common LGPL
     answer for iOS, and it works with the store's signing because the
     user relinks for their own device;
   - **a commercial Qt licence** for the mobile apps, which removes both
     questions (a per-developer subscription).
3. **Qt on Android.** Shared Qt libraries under the LGPL are the plain case
   (the `.so` files are replaceable). Nothing to decide unless a static
   build is chosen for size, which brings back the relinking question.

## The prototypes

```
desktop-native/mobile/
├── common/platform.rs   rustls client and the logcat bridge, used by both
├── qt/                  Qt Quick app (dev.yap.qtproto)
│   ├── rust/            cxx-qt crate: bridge.rs, gui.rs, client.rs; build.rs
│   │                    mirrors the shared desktop QML and models.rs
│   │   └── qml/         mobile shell and pages; seams/ (mobile side of desktop seams)
│   ├── cpp/             main.cpp, native.* (the platform shim)
│   ├── android/         manifest and NativeShim.java
│   └── build.sh         Qt for Android + cxx-qt-cmake → APK
├── rust/ android/ build.sh    the Compose baseline (UniFFI)
└── docs/                screenshots
```

### Build and run the Qt app

Needs:

- Qt 6.11.3 for Android, one prefix per ABI. download.qt.io's
  `all_os/android/qt6_6113/*` archives, about 51 MB each; aqtinstall 3.3
  does not know 6.11's layout yet;
- a desktop Qt 6.11.3 for the host tools (`QT_HOST_PATH`; the custom static
  build in `tools/toolchain.lock.json` works);
- the Android SDK and NDK 27, JDK 17+, Ninja, and Rust with the Android
  targets.

```sh
export QT_HOST_PATH=<desktop Qt 6.11.3> ANDROID_SDK_ROOT=<sdk>
YAPM=<a build folder off the system drive> \
  bash desktop-native/mobile/qt/build.sh x86_64 Release   # or arm64-v8a
# then zipalign and apksigner the unsigned APK it prints, and adb install
adb logcat -s yap-core default                             # core log, QML warnings, [frames]
```

`build.sh` points the downloaded Qt's `qmake` wrapper and `target_qt.conf`
at `QT_HOST_PATH`, as aqtinstall does. Gradle's cache and the build tree go
under `$YAPM`.

### Build and run the Compose baseline

```sh
CARGO_TARGET_DIR=<off the system drive> bash desktop-native/mobile/build.sh x86_64 release
adb install -r desktop-native/mobile/android/app/build/outputs/apk/release/app-release.apk
```

Sign-in in both shows a device code. Open the link in any signed-in
browser, or approve it with a test account's session:

```
POST https://<kova>/api/pub/apps/<publishable key>/device/lookup   {"userCode": "..."}
POST https://<kova>/api/pub/apps/<publishable key>/device/approve  {"userCode": "..."}
     Authorization: Bearer <session>, Origin: <yap web origin>
```

CI (`.github/workflows/desktop-native-mobile.yml`) builds the Compose
baseline's APK on Ubuntu, and on macOS cross-compiles the core for iOS and
type-checks its Swift bindings. A CI job for the Qt app needs Qt for Android
on the runner (and Qt for iOS on macOS for the iOS check). That is the next
CI step.

## Core refactors in this branch

Each is its own commit, and the desktop resolves and behaves as before:

1. **The app hands the core its config and data folder.**
   `yap_core::config::set(Config)` (env vars stay the desktop's default
   through `Config::from_env`). The device sign-in's platform and device
   name are now config: Kova accepted `android` from the Qt prototype.
   `yap_cache::profile::set_base(path)` replaces `%LOCALAPPDATA%`. Both
   mobile apps use these instead of setting environment variables at
   runtime.
2. **Installer, updater and the Tauri migration behind a `desktop`
   feature** in yap-core (on by default). Mobile builds without them, which
   also drops `rusty-leveldb` (and its stray cdylib) from the mobile graph,
   and yap-core no longer depends on `semver` there. A `yap-desktop` crate can take them when the desktop
   moves to `desktop/`.
3. **Markdown: Qt rich text is fine for QML on mobile.** The shared
   `MessageDelegate` draws the core's `html` (bold, italic, code, links,
   mentions) unchanged on Android. A span tree is only needed if a
   non-Qt view (the Compose baseline) is kept.
4. **The Windows-only libwebrtc patches are already gated.** By inspection
   of `patches/webrtc-sys`: the Media Foundation and D3D11 sources build
   only for `target_os == "windows"`, the decoder factory's hooks are under
   `#if defined(_WIN32)`, and the native-buffer accessor in
   `patches/libwebrtc` is `cfg(target_os = "windows")`. The others (no AV1,
   offer options) are platform-neutral and wanted on mobile too. Cargo's
   `[patch]` is per workspace, so a mobile crate that builds voice lists
   the same vendored copies.

## Crate audit

Each crate was checked by compiling it for `x86_64-linux-android` (NDK 27).

| Crate                              | Android build                                                                                                                               | Desktop-only parts                                                                                                  | Refactor for a portable core                                                                                                                                                                                                                    |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `yap-protocol`                     | builds                                                                                                                                      | none                                                                                                                | none                                                                                                                                                                                                                                            |
| `yap-cache`                        | builds (rusqlite bundled)                                                                                                                   | none now                                                                                                            | data root from the app: **done** (`profile::set_base`)                                                                                                                                                                                          |
| `yap-core`                         | builds with `rustls` and without `desktop`                                                                                                  | `shell.rs`'s tray and taskbar rules, `fixture.rs`/`faults.rs` (dev switches), `auth::Keyring` (the `keyring` crate) | TLS feature, config from the app, `desktop` feature: **done**. `Keyring` stays the desktop's `TokenStore`; mobile supplies its own                                                                                                              |
| `yap-realtime`                     | builds with `rustls`                                                                                                                        | none                                                                                                                | TLS feature: **done**                                                                                                                                                                                                                           |
| `yap-anim`                         | builds; used by the Qt app's `AnimatedPicture`                                                                                              | none                                                                                                                | none                                                                                                                                                                                                                                            |
| `yap-webm`                         | builds                                                                                                                                      | none                                                                                                                | none                                                                                                                                                                                                                                            |
| app crate `src/models.rs`          | builds unchanged in the Qt app                                                                                                              | none                                                                                                                | Move with `gui.rs`'s registry into a shared crate                                                                                                                                                                                               |
| app crate `src/bridge.rs`/`gui.rs` | not portable as they are: they reach every desktop singleton                                                                                | voice, settings, admin, desktop shell objects                                                                       | Split into the shared domain objects (`Session`, `Nav`, `Chat`, `Profile`, …) and a desktop rest; the Qt app's `bridge.rs` is that shared part in miniature                                                                                     |
| app crate `src/client/gateway.rs`  | builds (via `#[path]`)                                                                                                                      | none (no Qt, no Win32)                                                                                              | Move into `yap-realtime` or `yap-core` so both shells use it as a crate                                                                                                                                                                         |
| app crate `src/client/voice.rs`    | not tried                                                                                                                                   | no Qt; drives `yap-voice` with the desktop's devices                                                                | Move the Qt-free call driver next to `yap-voice`                                                                                                                                                                                                |
| `yap-voice` (no `libwebrtc`)       | builds once given the same TLS switch as yap-core                                                                                           | Win32 in 5 of 28 files: WASAPI devices, Media Foundation                                                            | 1. `default-features = false` for its `yap-realtime` dependency, forwarding the TLS feature. 2. Upstream LiveKit libwebrtc for Android/iOS (the patches are already Windows-gated). 3. libwebrtc's JNI init for its Android audio device module |
| `yap-speaker`                      | builds (5 dead-code warnings)                                                                                                               | the WASAPI engine                                                                                                   | An AAudio/Oboe and AVAudioEngine sink, or libwebrtc's audio device module on mobile                                                                                                                                                             |
| `yap-audio`                        | builds once given the same TLS switch                                                                                                       | none (decode only)                                                                                                  | the TLS feature pair                                                                                                                                                                                                                            |
| `yap-games`, `yap-hotkeys`         | build (Win32 gated out)                                                                                                                     | the whole purpose                                                                                                   | stay desktop-only                                                                                                                                                                                                                               |
| `yap-capture`                      | not portable                                                                                                                                | Windows Graphics Capture, D3D11, the game hook                                                                      | stays desktop-only; mobile screen share is MediaProjection / ReplayKit                                                                                                                                                                          |
| `yap-segment`                      | not portable                                                                                                                                | ORT with DirectML                                                                                                   | stays desktop-only; mobile would use NNAPI/CoreML or ML Kit/Vision                                                                                                                                                                              |
| desktop `cpp/`                     | `animatedpicture`, `iconprovider`, `ditherprovider` build unchanged; `authmedia`, `composerhighlighter`, `wheelscroll` have no Windows code | window chrome, toasts, thumbbar, WebView2, Media Foundation, D3D11 video, crash dumps                               | none for the portable ones; the rest stay desktop-only                                                                                                                                                                                          |

## The other options, briefly

- **(b) Whisker** (`github.com/whiskerrs/whisker`, read at `ae60287f`):
  - every screen is Rust (`render!`), laid out by Taffy in Rust and
    projected onto plain Android `ViewGroup`s and UIKit views through a
    custom C ABI;
  - `List` is Rust control flow over a `ScrollView`, not a recycling list;
  - v0.16, with three releases in the last month.

  It shares nothing with the desktop's QML, so it misses the goal.

- **(c) UniFFI core + Compose/SwiftUI**: the baseline above. It is the most
  native and the lightest (12 MB download, 60 MB), but every row is written
  twice more and drifts from the desktop's design. It stays the fallback for
  a single surface.
- **(d) Hybrids**:
  - the Expo app on the same core through `uniffi-bindgen-react-native`, a
    migration path that freezes nothing;
  - `QtQuickView` to put one Qt Quick surface inside a native app;
  - Compose Multiplatform on iOS (UniFFI's Kotlin bindings are JVM-only, so
    it needs Gobley).

## Next steps for Qt Quick

1. A shared crate and QML module out of `models.rs`, the registry and the
   portable part of `bridge.rs`, plus the shared QML files, once the desktop
   moves to `desktop/`.
2. Fix the "feels off" list: a hover flag, a text scale in `Theme`, the
   IME Send action, per-platform size tokens.
3. Images over the core's rustls (a `QNetworkAccessManagerFactory`), then
   avatars and attachments.
4. Size: R8, bytecode-only QML, and a static-Qt measurement.
5. CI: Qt for Android on Ubuntu; Qt for iOS on macOS for a compile check of
   the same QML module.
6. Voice and push: upstream libwebrtc on Android, a foreground service, FCM.
