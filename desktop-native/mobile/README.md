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

- **Shared, one copy:** the `yap-ui` crate (`desktop-native/rust/crates/yap-ui`),
  which both apps link. It holds the `Yap.Ui` QML module (`Theme`,
  `ShellState`, `MessageDelegate`, `TreeRow`, `ServerIcon` and the
  components they use: 30 files, 4,720 lines), the domain objects (`Session`, `Nav`,
  `Chat`, `Voice`, `Profile` and the portable singletons such as `Admin`,
  `Emoji`, `Extras`, `Invites`, `Panels`), all 13 list models, the
  GUI-thread registry, the core's runtime and gateway, and the portable C++
  (`AnimatedPicture`, the icon and dither image providers). Together with
  the core, that is everything a row shows and every rule behind it.
- **Per platform:** about 840 lines of QML (shell, pages, composer, and
  the mobile side of four seams), about 130 lines of Rust (the app's
  `Platform`: TLS client, realtime, token store), and about 450 lines of
  C++/Java shim for back, keyboard, haptics, share, text size and fling.
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
  - Qt's own HTTPS needs OpenSSL on Android; pictures go over the core's
    rustls client instead (see "Gaps").

Keep the Rust boundary exactly as it is: the core does not know which view
layer draws it, and the Compose baseline proves a native view stays possible
for any one surface (for example a call screen) if Qt ever falls short there.

## What is shared, what is per platform

| Layer                         | Shared with desktop (unchanged)                                                                                                                                                                                                                                                                                                                       | Per platform                                                                                                                                                                                                                                               |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Core                          | `yap-core`, `yap-cache`, `yap-realtime`, `yap-protocol`, `yap-anim`; `gateway.rs`                                                                                                                                                                                                                                                                     | TLS backend (rustls on mobile), the token store (an app-private file; Keystore in a release), the config and data folder handed in (`yap_mobile_init`)                                                                                                     |
| Models and bridge objects     | yap-ui: every `QAbstractListModel` (servers, tree, messages, members, DMs, …), `Session`/`Nav`/`Chat`/`Voice`/`Profile` and the portable singletons, one definition for both apps; the registry and the order view updates apply in                                                                                                                   | The app's `Platform` (`yap_ui::platform`): HTTP client, realtime, token store, start and quit hooks, the view updates only it shows (toasts, sounds, badge), uploads, share sources, video. Desktop: `rust/src/native.rs`; mobile: `qt/rust/src/client.rs` |
| Theme tokens                  | `Theme.qml` (palettes, type roles, motion, spacing), `ShellState.qml`                                                                                                                                                                                                                                                                                 | Which theme (`Theme.themeId` follows the system's light/dark); a text scale is still missing (see the checklist)                                                                                                                                           |
| Components and list delegates | `Yap.Ui`: `MessageDelegate` (the message renderer: grouping, replies, reactions, mention and pending rows, Markdown via Qt rich text), `TreeRow`, `ChannelRow`, `CategoryLabel`, `MemberRow`, `ServerIcon`, `RailPill`, `CountBadge`, `Avatar`, `DisplayName`, `Icon`, `IconKey`, `KitButton`, `Led`, `Spinner`, `LinkText`, `LazyPopup`, `Tooltip` … | Each app's side of the four seams the shared files import from `Yap.Platform`, with the same API: `EmbedCard`, `MessageMedia`, `VoiceStatusLine`, `VoiceStreamHoverCard` (mobile: `qt/rust/qml/seams`; desktop: its own `com.yap.native` files)            |
| C++                           | yap-ui `cpp/`: `AnimatedPicture` and `ViewportWatch` (GIF/APNG/WebP through yap-anim), the `icon` and `dither` image providers, `yap_ui_register(engine)`; `yap_ui.cmake` compiles them in each app's target                                                                                                                                          | `Native` (the platform shim) and `main.cpp`. The Windows C++ (window chrome, toasts, WebView2, Media Foundation, D3D11 video) stays desktop-only                                                                                                           |
| Page layouts and navigation   | —                                                                                                                                                                                                                                                                                                                                                     | Phone pages (`ServersPage`: rail and tree side by side; `ChatPage`; `SignInPage`), a `StackView` with the platform's transitions and back, `PlatformList` (the platform's scroll feel)                                                                     |
| Shell                         | —                                                                                                                                                                                                                                                                                                                                                     | `Main.qml` (edge to edge, phases, foreground/background), the Android manifest, the Java shim                                                                                                                                                              |

### How the sharing is built

One crate, one QML module, two consumers. No file is copied:

- **`yap-ui`** (`desktop-native/rust/crates/yap-ui`, `links = "yap_ui"`) is a
  cxx-qt crate that owns the QML module `Yap.Ui` (its QML lives in the
  crate, `qml/`; its bridge files in `src/`, one flat folder, as cxx-qt-build
  requires). Its build script ends with `.build().export()`, so cxx-qt-build
  hands its initializers to the app crate that links it and exports the
  module under `CXX_QT_EXPORT_DIR/qml_modules/Yap/Ui`.
- **Each app crate** (`yap_native` on the desktop, `yap_mobile_qt` here)
  depends on it by path. Its CMake imports both modules from the one crate it
  builds: `cxx_qt_import_qml_module(yap_ui_qml URI "Yap.Ui" SOURCE_CRATE
<app crate>)` beside its own, and includes `yap-ui/cpp/yap_ui.cmake` for the
  shared C++ (an INTERFACE library, so it compiles with the app's flags).
- **The app's own QML** is its own module: `com.yap.native` on the desktop,
  `com.yap.mobile` here. Its files `import Yap.Ui`.
- **Seams.** Shared QML imports `Yap.Platform` for the four types that
  differ per app (`EmbedCard`, `MessageMedia`, `VoiceStatusLine`,
  `VoiceStreamHoverCard`). Each app's `main.cpp` registers its own file for
  each name by URL (`qmlRegisterType(QUrl("qrc:/qt/qml/<module>/..."),
"Yap.Platform", 1, 0, name)`); the desktop's are bytecode-only with the
  sources discarded, which URL registration handles. `Yap.Media` and
  `Yap.Chrome` stay C++ registrations: `yap_ui_register` adds the shared
  `AnimatedPicture`; each app registers its `DesktopShell`.
- **Rust seams** go through `yap_ui::platform::Platform`, installed once
  before QML loads (`yap_desktop_init`, `yap_mobile_init`). The registry's
  ordered `apply_one` handles every shared view update and hands the rest to
  `Platform::on_update`.
- The desktop's slim QML (bytecode only, sources discarded) is the
  `slim-qml` feature of yap-ui, which `yap_native` turns on; it needs the
  desktop's patched qt-build-utils and is off here.

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
| Photo picker                      | **No.** Qt's `FileDialog` opens the documents UI                                                                                                   | Shim: the system photo picker (`ACTION_PICK_IMAGES`, no permission) on 13+, `GET_CONTENT` on images and videos before; each item copied into the cache for the core's path-based upload                                                   | Verified on API 35: three pictures picked, uploaded with progress, one retried, sent                                                                 |
| Pinch, double-tap, swipe away     | **Native** building blocks: `PinchArea`, `TapHandler`, `DragHandler`, a snapping `ListView`                                                        | `ImageViewerPage`: zoom about the fingers or the tap, pan, page between pictures, swipe down to close with the scrim fading; Back closes it first                                                                                         | Double-tap, paging, swipe down and Back verified; pinch needs two fingers, which `adb` cannot send                                                   |
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
| APK, x86_64                        | 19.5 MB (12.6 MB gzipped), R8 off                                           | 57.1 MB                                                                                 |
| APK, arm64                         | core 8.7 MB (4.0 MB gzipped) plus Compose; about 12 MB gzipped              | **55.7 MB, 20.5 MB gzipped** (was 61.5 MB, 22.3 MB; "Size" below)                       |

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

arm64, release, shared Qt libraries from download.qt.io (`qt/build.sh`,
signed by `qt/sign.sh`), measured 2026-11-10 before the shared `yap-ui`
crate landed (with it, CI's arm64 APK is 57.9 MB, 21.2 MB gzipped; the
`qt-android` job prints the per-library list). Libraries are stored
uncompressed in the APK (page-aligned, loaded in place), so the APK is
about the sum of its parts; gzip -9 of the whole APK stands in for the
download size.

| Part                                                                                                   | Before  | After   |
| ------------------------------------------------------------------------------------------------------ | ------- | ------- |
| `libyapqt` (the core, the bridge, the QML compiled to C++, cxx-qt, AnimatedPicture)                    | 14.7 MB | 14.4 MB |
| Qt Gui, Quick, Core, Qml                                                                               | 27.7 MB | 27.7 MB |
| Qt QuickTemplates2, QuickControls2 (Basic), Network, QmlModels, Svg, OpenGL, QuickShapes, QuickEffects | 10.2 MB | 10.2 MB |
| Plugins: android platform, gif/jpeg/webp/svg (before: also the OpenSSL TLS backend and offscreen)      | 1.9 MB  | 1.5 MB  |
| QML plugin stubs (`libqml_*`)                                                                          | 0.1 MB  | 0.1 MB  |
| `libc++_shared`                                                                                        | 1.3 MB  | 1.3 MB  |
| `classes.dex` (Qt's Java, the shim, AndroidX)                                                          | 5.3 MB  | 0.3 MB  |
| **APK**                                                                                                | 61.5 MB | 55.7 MB |
| **APK, gzip -9**                                                                                       | 22.3 MB | 20.5 MB |

What made the difference:

- **R8** (`qt/android/build.gradle`, Qt 6.11's template plus a release
  `minifyEnabled`/`shrinkResources`; `proguard-rules.pro` keeps
  `org.qtproject.qt.android.**` and `dev.yap.**`, which C++ reaches over
  JNI by name, and the AndroidX font and `FileProvider` calls Qt's platform
  plugin makes the same way): the dex was mostly Kotlin, kotlinx coroutines
  and AndroidX that `androidx.core` pulls in. 5.3 MB to 0.30 MB.
- **Two plugins dropped** (`qt_import_plugins(... EXCLUDE ...)`): the
  offscreen platform plugin and the OpenSSL TLS backend (Qt for Android
  ships no libssl, so it never loaded). 0.4 MB.
- **Identical code folding** for `libyapqt` (`-Wl,--icf=all`): 0.33 MB.
  `--icf=safe` folded nothing. `--gc-sections` was already on (the NDK's
  toolchain file), and Gradle strips the libraries it packages.

Modules used: Core, Gui, Network, Qml (QmlModels, QmlMeta, WorkerScript),
Quick, QuickControls2 with the Basic style, QuickTemplates2, QuickEffects
(MultiEffect for round avatars), QuickShapes, Svg, OpenGL.

Dropped already (`qt_import_plugins` in `qt/CMakeLists.txt`): the QML
debugger and profiler services, the TIFF/ICO/ICNS/TGA/WBMP decoders, icon
engines, network information, offscreen, the OpenSSL backend. Not linked
at all: Multimedia, WebView, Widgets, Quick 3D, the other Controls styles,
SQL.

Next savings, not measured yet:

- the desktop's bytecode-only QML (`patches/qt-build-utils`; desktop pass 5
  saved 2.8 MiB);
- a static Qt for Android with LTO. That needs Qt built from source for
  Android (hours, about 15 GB). Not attempted here, so static vs shared is
  open.

## Gaps found

- **HTTPS for Qt's own network loads (done).** `Image` and
  `AnimatedPicture` load avatars, server icons, attachments and GIFs
  through Qt Network, whose HTTPS needs OpenSSL on Android, which Qt does
  not ship (`qt/CMakeLists.txt` drops Qt's OpenSSL TLS plugin). The
  engine's `QQmlNetworkAccessManagerFactory` (`qt/cpp/rustnetwork.cpp`)
  answers every http(s) GET with a `QNetworkReply` fed by the core's
  rustls client (`qt/rust/src/http.rs`, its own two-thread runtime, bodies
  capped at 32 MiB); `qrc:`, `file:` and image providers stay Qt's. The
  session goes only to the API's private media (`media_needs_auth`, the
  desktop's rule, from `ViewUpdate::MediaAuth` in the mobile `Platform`),
  and reqwest drops it on a redirect to another host. The disk cache (the
  app's cache folder, 256 MiB) is done in the manager, since
  `QNetworkDiskCache` only plugs into Qt's own HTTP backend: Qt's load
  rules (fresh copies served, stale ones revalidated with
  `If-None-Match`/`If-Modified-Since`), and with no network the last copy
  is shown. Private media and redirected answers are never cached. Open:
  the cache is not emptied on sign-out (only public pictures are in it),
  and the API's avatars answer `max-age=0` with an ETag it does not honour
  (`200`, not `304`), so each avatar is fetched again per launch.
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
│   ├── rust/            cxx-qt crate yap_mobile_qt: the mobile Platform
│   │                    (client.rs) and the com.yap.mobile module; links yap-ui
│   │   └── qml/         mobile shell and pages; seams/ (Yap.Platform types)
│   ├── cpp/             main.cpp, native.* (the platform shim)
│   ├── android/         manifest, NativeShim.java, icon and splash, build.gradle (R8)
│   ├── build.sh         Qt + cxx-qt-cmake → APK (Android) or .app (iOS simulator)
│   ├── fetch-qt.ps1     Qt 6.11.3 from download.qt.io, pinned by qt-sdk.lock.json
│   ├── sign.sh          zipalign + apksigner (v2/v3), .sha256
│   ├── smoke.sh         install, cold start, wait for the first frame
│   └── ci/              the public CI debug keystore (not for release)
├── rust/ android/ build.sh    the Compose baseline (UniFFI)
└── docs/                screenshots
```

### The chat page: composer, attachments, image viewer

`MobileComposer.qml`, `UploadTray.qml` and `ImageViewerPage.qml`, on the
shared `Chat` API (no core changes):

- **Reply.** `ChatPage.startReply(messageId, author, text)` shows the reply
  bar (the author, the text on one line, Cancel) and opens the keyboard; the
  send carries `reply_to_id`. The long-press sheet and swipe to reply call it.
- **Mentions.** Typing `@sp` asks `Chat.queryMentions`; up to five members
  float above the composer and a tap puts `@username ` in. The keyboard's
  word in progress (preedit) counts, so suggestions follow Gboard's
  composing text, and it is committed before the insert.
- **Emoji.** A row of the recents (`Emoji.quick`) and common emoji as text,
  inserted at the caret. The full picker is the shared `EmojiSheet` once it
  lands; server emoji need the desktop's placeholder handling.
- **Attachments.** `+` opens the photo picker (`Native.pickMedia`). The tray
  shows each file's picture, progress and size; remove or cancel, mark
  sensitive, tap a failed tile to retry (its reason shows in the notice
  line), tap a finished picture to view it. Files over the limit stay as the
  core's too-big cards. Sending works with text, pictures or both.
- **The rest.** Send is enabled by typed text (preedit included) or a
  finished upload; typing pings go at most every 3 s (the core sends every
  one it is asked to); the field grows to five lines; each channel keeps its
  unsent text and reply while the app runs (`Drafts`, cleared on sign-out).
- **Viewer.** A tap on a message's picture opens it full screen over the
  chat: pinch or double-tap to zoom, pan, swipe sideways between the
  message's pictures, swipe down or Back to close, a tap to show or hide the
  bar (close, `2 / 3`, the name, Share for the link).

Not done yet: a camera shot from the composer; video in the viewer; saving
to the gallery; sensitive pictures are not blurred in the viewer; on 13+ the
photo picker names files by their media id (`1000000302.jpg`), not the
original name; the status bar icons stay dark over the viewer in the light
theme until the shim can switch them (`Native.lightBars`).

<p>
<img src="docs/qt-mention.png" width="180" alt="Qt: @ suggestions above the composer">
<img src="docs/qt-reply.png" width="180" alt="Qt: replying to a message">
<img src="docs/qt-upload-tray.png" width="180" alt="Qt: three pictures in the upload tray">
<img src="docs/qt-image-viewer.png" width="180" alt="Qt: the image viewer">
</p>

### Build and run the Qt app

Needs:

- Qt 6.11.3: the target Qt (Android, one prefix per ABI, or iOS) and a
  desktop Qt 6.11.3 for the host tools (`QT_HOST_PATH`). `qt/fetch-qt.ps1`
  downloads exactly the archives `qt/qt-sdk.lock.json` names from
  download.qt.io and checks each one's SHA-256; `-Refresh` re-pins them from
  the repository's `Updates.xml` and the `.sha256` files download.qt.io
  publishes next to each archive. Sets: `android-x86_64`,
  `android-arm64_v8a`, `host-linux`, `host-mac`, `ios` (qtbase,
  qtdeclarative, qtsvg, qtimageformats, and ICU on Linux). On Windows the
  host is the custom static Qt in `tools/toolchain.lock.json`;
- the Android SDK, NDK r27c (27.2.12479018, the one Qt 6.11.3 for Android
  was built with; `build.sh` takes `$ANDROID_NDK_ROOT` or the SDK's
  `ndk/27.2.12479018`, and NDK 27.1 also works), JDK 17+, Ninja, 7-Zip,
  PowerShell 7, and Rust with the Android targets.

```sh
pwsh desktop-native/mobile/qt/fetch-qt.ps1 -Dest $YAPM/qt -Set android-x86_64   # + host-linux off Windows
export QT_HOST_PATH=<desktop Qt 6.11.3> ANDROID_SDK_ROOT=<sdk>
YAPM=<a build folder off the system drive>   bash desktop-native/mobile/qt/build.sh x86_64 Release   # or arm64-v8a; -D... go to CMake
bash desktop-native/mobile/qt/sign.sh <the unsigned APK it prints> yap-qt.apk   # debug keystore by default
adb install -r yap-qt.apk
adb logcat -s yap-core default                             # core log, QML warnings, [frames]
```

`build.sh` points the downloaded target Qt's `qmake` wrapper (`qmake.bat`
on Windows, a shell wrapper elsewhere) and `target_qt.conf` at
`QT_HOST_PATH`, as aqtinstall does. Gradle's cache and the build tree go
under `$YAPM`; the target Qt defaults to `$YAPM/qt/<set dir>`
(`QT_TARGET_ROOT` overrides it). A rebuild in the same tree keeps libraries
androiddeployqt copied before, so a plugin dropped from the build stays in
the APK until `android-build/libs` is cleared.

Packaging, from CMake cache variables: `YAP_ANDROID_APP_ID` (default
`dev.yap.qtproto`; also the Gradle namespace and the iOS bundle id),
`YAP_ANDROID_VERSION_CODE` (1) and `YAP_ANDROID_VERSION_NAME` (0.1.0);
`YAP_ANDROID_COMPILE_SDK` and `YAP_ANDROID_BUILD_TOOLS` pin the SDK
platform and build tools (default: the newest installed). The manifest has
no `package` attribute. The launcher icon is adaptive, with a
monochrome layer for themed icons; the splash is ply-0 (light `#f4f4f5`,
dark `#000000`) with the mark: the system splash on Android 12+, the
window background before, then Qt's sticky splash, which `main.cpp` fades
out at the first frame.

`build.sh ios-simulator Release` configures the same project with Qt for
iOS and builds an unsigned simulator `.app` (x86_64: Qt's iOS libraries
have arm64 device and x86_64 simulator slices only). It is a compile and
link check; nothing runs it.

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
type-checks its Swift bindings. For the Qt app, `qt-android` builds the
x86_64 and arm64-v8a APKs on Ubuntu with the pinned Qt (archives cached by
the lock's hash) and NDK r27c, signs them with the public CI debug key
(`qt/ci/debug.keystore`, so one run's APK installs over another's; never a
release key), and uploads each APK with its `.sha256` (14 days; version
code `run * 10 + 2` for arm64, `+ 4` for x86_64, version name
`0.1.<run>+<commit built>`). `qt-android-smoke` cold-starts the x86_64 APK
on an API 35 emulator and fails unless it logs `[startup] first frame`
(`qt/smoke.sh`, which also works against a local emulator; it checks
start-up only, not sign-in). `qt-ios` builds the simulator `.app` on macOS when
`mobile/qt` or the workflow changes, by hand, and weekly. Each job runs only
when its own inputs change.

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

| Crate                           | Android build                                                                                                                               | Desktop-only parts                                                                                                  | Refactor for a portable core                                                                                                                                                                                                                    |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `yap-protocol`                  | builds                                                                                                                                      | none                                                                                                                | none                                                                                                                                                                                                                                            |
| `yap-cache`                     | builds (rusqlite bundled)                                                                                                                   | none now                                                                                                            | data root from the app: **done** (`profile::set_base`)                                                                                                                                                                                          |
| `yap-core`                      | builds with `rustls` and without `desktop`                                                                                                  | `shell.rs`'s tray and taskbar rules, `fixture.rs`/`faults.rs` (dev switches), `auth::Keyring` (the `keyring` crate) | TLS feature, config from the app, `desktop` feature: **done**. `Keyring` stays the desktop's `TokenStore`; mobile supplies its own                                                                                                              |
| `yap-realtime`                  | builds with `rustls`                                                                                                                        | none                                                                                                                | TLS feature: **done**                                                                                                                                                                                                                           |
| `yap-anim`                      | builds; used by the Qt app's `AnimatedPicture`                                                                                              | none                                                                                                                | none                                                                                                                                                                                                                                            |
| `yap-webm`                      | builds                                                                                                                                      | none                                                                                                                | none                                                                                                                                                                                                                                            |
| `yap-ui`                        | builds and runs in the Qt app (x86_64 emulator)                                                                                             | none: what differs per app goes through its `Platform`                                                              | **done**: the models, registry, bridge (`Session`/`Nav`/`Chat`/`Voice`/`Profile`), portable singletons, runtime, gateway, `Yap.Ui` QML and portable C++ moved out of the app crate                                                              |
| app crate `yap_native`          | not portable                                                                                                                                | the desktop's objects (`Desktop`, `Audio`, `Games`, `Hotkeys`, `Updater`, `CustomSounds`) and its `Platform`        | stays desktop-only                                                                                                                                                                                                                              |
| app crate `src/client/voice.rs` | not tried                                                                                                                                   | no Qt; drives `yap-voice` with the desktop's devices                                                                | Move the Qt-free call driver next to `yap-voice`                                                                                                                                                                                                |
| `yap-voice` (no `libwebrtc`)    | builds once given the same TLS switch as yap-core                                                                                           | Win32 in 5 of 28 files: WASAPI devices, Media Foundation                                                            | 1. `default-features = false` for its `yap-realtime` dependency, forwarding the TLS feature. 2. Upstream LiveKit libwebrtc for Android/iOS (the patches are already Windows-gated). 3. libwebrtc's JNI init for its Android audio device module |
| `yap-speaker`                   | builds (5 dead-code warnings)                                                                                                               | the WASAPI engine                                                                                                   | An AAudio/Oboe and AVAudioEngine sink, or libwebrtc's audio device module on mobile                                                                                                                                                             |
| `yap-audio`                     | builds once given the same TLS switch                                                                                                       | none (decode only)                                                                                                  | the TLS feature pair                                                                                                                                                                                                                            |
| `yap-games`, `yap-hotkeys`      | build (Win32 gated out)                                                                                                                     | the whole purpose                                                                                                   | stay desktop-only                                                                                                                                                                                                                               |
| `yap-capture`                   | not portable                                                                                                                                | Windows Graphics Capture, D3D11, the game hook                                                                      | stays desktop-only; mobile screen share is MediaProjection / ReplayKit                                                                                                                                                                          |
| `yap-segment`                   | not portable                                                                                                                                | ORT with DirectML                                                                                                   | stays desktop-only; mobile would use NNAPI/CoreML or ML Kit/Vision                                                                                                                                                                              |
| desktop `cpp/`                  | `animatedpicture`, `iconprovider`, `ditherprovider` build unchanged; `authmedia`, `composerhighlighter`, `wheelscroll` have no Windows code | window chrome, toasts, thumbbar, WebView2, Media Foundation, D3D11 video, crash dumps                               | none for the portable ones; the rest stay desktop-only                                                                                                                                                                                          |

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

1. Done: the shared crate and QML module (`yap-ui`, "How the sharing is
   built"). Next on it: the slim-QML size patch for the mobile build, and
   more of the desktop's QML moved into `Yap.Ui` as phone pages need it.
2. Fix the "feels off" list: a hover flag, a text scale in `Theme`, the
   IME Send action, per-platform size tokens.
3. Images over the core's rustls (a `QNetworkAccessManagerFactory`), then
   avatars and attachments (done; GIF attachments still show their first
   frame until `MessageMedia` uses `AnimatedPicture`).
4. Size: R8, bytecode-only QML, and a static-Qt measurement.
5. CI: Qt for Android on Ubuntu; Qt for iOS on macOS for a compile check of
   the same QML module.
6. Voice and push: upstream libwebrtc on Android, a foreground service, FCM.
