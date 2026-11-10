# Mobile on the native core: research and prototype

How much of the native desktop client (`desktop-native/`) Android and iOS
can share, and how. This folder holds the written comparison, the crate
audit, and an Android prototype that runs the desktop crates unchanged
behind a UniFFI boundary with Jetpack Compose views.

The Expo app (`apps/mobile`) is untouched and keeps shipping. Nothing here
is built by its workflows, and nothing here changes its package, signing or
store listing. The prototype's application id is `dev.yap.coreproto`.

## Recommendation

Use **(c): the Rust core behind UniFFI, with native views** (Compose on
Android, SwiftUI on iOS). Desktop keeps QML.

The deciding fact is that `yap-core` already has the shape this option
needs. One actor owns the state; `Command`s go in; batches of typed
`ViewUpdate`s come out, with row diffs per list. Effects (HTTP, sign-in,
cache, gateway, voice) already run in Rust. The QML side is only a view
that applies those diffs. A Compose or SwiftUI view does the same job, so
the boundary to a platform UI already exists and needs only a binding
layer.

The prototype proves this on the emulator:

- Kova device sign-in, the server list, the channel tree and the message
  list, with live gateway updates;
- the snapshot cache and session restore on a cold start;
- reducer, effects, REST, gateway and cache logic compiled from the desktop
  crates without changes. The only crate change is a TLS backend feature.

Whisker (b) would mean rewriting every view in a pre-1.0 Rust UI DSL whose
lists do not use the platform's recycling views. Qt on mobile (a) shares
QML, but the QML is desktop-shaped, and Qt makes push, background voice and
the App Store harder.

## The options compared

The ratings are for Yap, given the crates as they are on this branch.

|                         | (a) Qt Quick on Android/iOS                                                                                                                                                                                                | (b) Whisker                                                                                  | (c) UniFFI core + Compose/SwiftUI                                                                                                                                                                                  | (d) Hybrids |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------- |
| **Reuse of yap-core**   | All of it, through the existing cxx-qt bridge and list models (`src/models.rs`)                                                                                                                                            | All of it; the views map `ViewUpdate`s to signals                                            | All of it, unchanged (shown by the prototype)                                                                                                                                                                      | All of it   |
| **Reuse of the UI**     | The QML is shared in principle, but it is built for a desktop window: title bar, server rail, hover cards, right-click menus, a 32 px chrome. Mobile needs new layouts, so the share is components and tokens, not screens | None. Whisker's desktop host is winit/wgpu, not QML, so desktop pixels are not shared either | None; each platform has its own thin view layer                                                                                                                                                                    | See below   |
| **Store and view sync** | Already done: cxx-qt `QAbstractListModel`s apply `ViewDiff`                                                                                                                                                                | Rust signals over a retained scene. The host receives `FramePacket`s with resolved geometry  | One `Listener` callback per batch; the app applies the diffs to `SnapshotStateList`s on the main thread (`CoreStore.kt`, about 100 lines). Same contract as QML                                                    | —           |
| **Voice / libwebrtc**   | LiveKit's `webrtc-sys` builds for Android and iOS. Our patched copy (MF H.264, D3D11 zero copy) is Windows-only and has to be gated per target. Audio devices need JNI and AVAudioSession glue                             | No WebRTC package; same Rust engine work as (c)                                              | Same engine as (a): `yap-voice`'s call machine already builds for Android (see the audit). Device, focus and routing glue goes in Kotlin/Swift, where those APIs live                                              | —           |
| **Binary size per ABI** | Qt Core, Gui, Qml, Quick and Controls: typically 20–30 MB per ABI before app code, plus libwebrtc                                                                                                                          | Small runtime plus the core                                                                  | Measured: core `.so` 8.7 MB on arm64 (4.0 MB gzipped), with TLS, SQLite and the whole reducer. Compose is a few MB after R8. libwebrtc adds roughly 10–15 MB, as react-native-webrtc already does for the Expo app | —           |
| **Build and CI**        | A Qt build per ABI plus host tools, on top of today's custom static Qt 6.11 for Windows. iOS needs macOS for Qt and Xcode                                                                                                  | `whisker` CLI and its generated Gradle/Xcode projects; Rust toolchain only                   | cargo-ndk plus Gradle; the stock SDK and NDK on `ubuntu-latest`. iOS: cargo plus Xcode on `macos-latest` (free for a public repo)                                                                                  | —           |
| **Push notifications**  | Qt has no FCM or APNs module, so Java/ObjC glue is needed anyway                                                                                                                                                           | `whisker-firebase` has a messaging package                                                   | Firebase Messaging and APNs used directly, first-class. An iOS notification service extension can link the core to update unread counts                                                                            | —           |
| **Background voice**    | An Android foreground service and iOS CallKit/PushKit are both platform code outside Qt; Qt's Android service support is awkward                                                                                           | Plugin modules                                                                               | Native: a foreground service of type `microphone` and `ConnectionService`, CallKit with PushKit VoIP pushes, `AVAudioSession` background audio. The Rust call runs in the same process                             | —           |
| **App stores**          | Qt is LGPL/GPL. On iOS, static linking under the LGPL needs relinkable objects or a commercial licence. Qt Quick Controls look non-native in review                                                                        | Fine                                                                                         | Fine; native UI is the low-risk path in review                                                                                                                                                                     | —           |
| **Maturity**            | Mature, but Qt for mobile is a niche                                                                                                                                                                                       | v0.16 (2026-11), three releases in the last month, one organisation                          | UniFFI is Mozilla's, shipped in Firefox for Android and iOS; Compose and SwiftUI are the platform defaults                                                                                                         | —           |
| **Cost**                | One mobile UI (QML), new layouts, a heavy toolchain                                                                                                                                                                        | One Rust UI for both platforms, but everything is rewritten and desktop stays separate       | Two thin mobile UIs (Compose + SwiftUI) over one core                                                                                                                                                              | —           |

### (b) How Whisker actually works

From its docs and source (`github.com/whiskerrs/whisker`, read at
`ae60287f`, 2026-11-10):

- Components are written in Rust with `render!`. State lives in signals,
  and a retained element scene is laid out by Taffy _in Rust_.
- Each frame becomes a `FramePacket` of semantic operations with resolved
  geometry. Platform hosts project that onto platform objects:
  - Android: plain `ViewGroup`s that "implement no Android layout policy"
    (`WhiskerContainerView.kt`), positioned where Rust says;
  - iOS: UIKit views;
  - web: positioned DOM nodes;
  - desktop: a shared winit/wgpu renderer.
- Mobile calls cross a custom C "Driver ABI" (`whisker-driver-sys`) with
  its own tagged value type, not UniFFI or JNI bindings.
- `List` is keyed Rust control flow over a `ScrollView`, not a
  `RecyclerView` or `UICollectionView` (`docs/list-design.md`). That matters
  for a chat client whose main screen is a long list.

The model is "Rust owns UI state and semantics; the host owns platform
integration and the final pixels". Every screen is Rust code, and widgets
are Whisker's own (built from platform views), not Compose or SwiftUI
components. For Yap, that means writing a third UI (after QML and the Expo
app) in a young framework, with no desktop reuse because desktop stays on
QML.

### (d) Hybrids worth keeping in mind

- **The Expo app on the Rust core.** `uniffi-bindgen-react-native`
  generates TurboModules from the same UniFFI interface. The Expo app could
  take over the reducer, cache and gateway from `yap-mobile` screen by
  screen, without a rewrite and without freezing it. This is the low-risk
  path if the decision is to keep Expo longer. The view contract
  (`Update` batches) is the same as in the prototype.
- **Compose Multiplatform on iOS** over the core, for one mobile view layer
  instead of two. The UniFFI Kotlin bindings use JNA (JVM only), so
  Kotlin/Native needs third-party bindings (Gobley). It is the riskier
  route, so it waits until the SwiftUI cost is actually felt.
- **Qt Quick for a single surface** (for example the call stage) inside a
  native app: possible through `QtQuickView` on Android, but it brings
  Qt's size and toolchain for one screen. Not recommended.

## The prototype

```
desktop-native/mobile/
├── rust/        yap-mobile: UniFFI cdylib over yap-core (its own workspace)
├── android/     Compose app, dev.yap.coreproto
└── build.sh     cargo-ndk → uniffi-bindgen (Kotlin) → Gradle
```

### What crosses the boundary

- **Into the core** (`Core` methods): `start`, `startSignIn`,
  `cancelSignIn`, `signOut`, `selectServer`, `selectChannel`, `loadOlder`,
  `setVisible`, `retryGateway`. Each one sends the same `Command` that the
  QML singletons send.
- **Out of the core**: `Listener.onUpdates(List<Update>)`. These are the
  `ViewUpdate`s the prototype draws, with the core's `ViewDiff`s
  (Reset/Insert/Update/Remove) per list, mapped field by field from
  `ServerRow`, `ChannelTreeRow` and `MessageRow`. The rest are dropped in
  `map_update`.
- **Into the core from the app**: a `TokenVault` (load/store/clear the
  session, backing yap-core's `TokenStore`; plain app-private preferences in
  the prototype, out of Auto Backup), a `LocalTime` (the UTC offset at an
  instant; the desktop asks `QTimeZone`), and the data folder.

### What runs unchanged

From the desktop crates:

- the whole store, reducer and effect runner (`yap_core::spawn`);
- Kova device sign-in (`auth.rs`) and REST (`api.rs`);
- the SQLite snapshot cache (`yap-cache`);
- the gateway task from `rust/src/client/gateway.rs`, compiled in by
  `#[path]` because it has no Qt or Win32 in it;
- `yap-realtime`.

Voice ends at once with "not in the prototype yet".

### Results (Android emulator, API 35, x86_64)

- **Sign-in:** the device code is shown, then approved for the spike test
  account. The token goes to the vault and the server list loads.
- **Tree and messages:** the channel tree (categories, text and voice) and
  the message history. Author grouping, role colours, the mention highlight
  and "(edited)" all come from the reducer.
- **Live:** a message posted by the peer account arrives over the gateway
  as an `Insert` diff, groups under the previous row, and the list follows
  the bottom.
- **Cold start:**
  - the session is restored from the vault;
  - the snapshot is read in 55 ms;
  - cached content shows at 439 ms;
  - messages at 2.7 s;
  - the gateway is ready at 4.0 s.
    These numbers are from a shared, heavily loaded emulator (load average
    10), so they are upper bounds.
- **Size:** `libyap_mobile.so` is 8.7 MB on arm64 (4.0 MB gzipped) and
  9.4 MB on x86_64. The x86_64 release APK is 19.3 MB, with R8 not enabled.
- **Code:**
  - FFI crate: 520 lines;
  - Kotlin store: 175 lines;
  - Compose screens: 425 lines.
    The generated bindings (3.6k lines) are not committed.

Not done:

- sending messages;
- DMs and Home;
- avatars and attachments (initials and a placeholder only);
- Markdown rendering: rows show the source, because the core's `html` is
  Qt rich text;
- voice;
- push;
- iOS;
- release signing (release builds use the debug key).

### Build and run

Needs the Rust targets `x86_64-linux-android` and `aarch64-linux-android`,
`cargo-ndk`, the Android SDK with an NDK, and JDK 17+.

```sh
bash desktop-native/mobile/build.sh x86_64 release   # or arm64-v8a, all; debug
adb install -r desktop-native/mobile/android/app/build/outputs/apk/release/app-release.apk
adb logcat -s yap-core                            # the core's log (its stderr)
```

Sign-in shows a device code. Open the link on any signed-in browser, or
approve it with a test account's session token:

```
POST https://<kova>/api/pub/apps/<publishable key>/device/lookup   {"userCode": "..."}
POST https://<kova>/api/pub/apps/<publishable key>/device/approve  {"userCode": "..."}
     Authorization: Bearer <session>, Origin: <yap web origin>
```

CI (`.github/workflows/desktop-native-mobile.yml`, on changes to this
folder or the core crates) builds the x86_64 APK on Ubuntu, and on macOS
cross-compiles the core for iOS and type-checks the generated Swift
bindings.

## Crate audit

Each crate was checked by compiling it for `x86_64-linux-android` with
cargo-ndk (NDK 27, API 26).

| Crate                             | Android build                                                                                                            | Desktop-only parts                                                                                                                                           | Refactor for a portable core                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `yap-protocol`                    | builds                                                                                                                   | none                                                                                                                                                         | none                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `yap-cache`                       | builds (rusqlite bundled)                                                                                                | the data root is `%LOCALAPPDATA%\Yap.Data` (`profile.rs`), from the environment                                                                              | An injectable data root (the prototype sets `LOCALAPPDATA` to `filesDir` before starting: a stopgap)                                                                                                                                                                                                                                                                                                                                                                                                             |
| `yap-core`                        | builds with `rustls`                                                                                                     | `install.rs`, `update.rs` (Velopack), `tauri.rs` (Tauri migration), the desktop parts of `shell.rs` (tray, taskbar), `fixture.rs`/`faults.rs` (dev switches) | 1. TLS feature: **done here**. 2. Move install/update/tauri into a `yap-desktop` crate or behind a `desktop` feature. 3. `markdown.rs` renders Qt rich text into every `MessageRow.html`: produce a span tree (or keep the source) and let each platform render it, so mobile does not pay for HTML it never draws. 4. `config.rs` reads env vars: take a `Config` from the host. 5. `auth::Keyring` uses the `keyring` crate (no Android or iOS store): the `TokenStore` trait already lets the host supply one |
| `yap-realtime`                    | builds with `rustls`                                                                                                     | none                                                                                                                                                         | TLS feature: **done here**                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| app crate `src/client/gateway.rs` | builds (via `#[path]`)                                                                                                   | none (no Qt, no Win32)                                                                                                                                       | Move it into `yap-realtime` or `yap-core` so both shells use it as a crate                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| app crate `src/client/voice.rs`   | not tried                                                                                                                | no Qt; drives `yap-voice` with the desktop's devices                                                                                                         | Move the Qt-free call driver next to `yap-voice`                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `yap-voice` (no `libwebrtc`)      | builds once given the same TLS switch as yap-core (probed with a local edit; on this branch it still names `native-tls`) | Win32 in 5 of 28 files: WASAPI devices, Media Foundation                                                                                                     | 1. Its `yap-realtime` dependency needs `default-features = false` in `[workspace.dependencies]` so the TLS feature can be forwarded. 2. Gate the patched `webrtc-sys`/`libwebrtc` (MF decode, D3D11 zero copy) to Windows and use upstream LiveKit builds on mobile. 3. Android needs libwebrtc's JNI init for its audio device module                                                                                                                                                                           |
| `yap-speaker`                     | builds (5 dead-code warnings)                                                                                            | the WASAPI engine                                                                                                                                            | An Android (AAudio/Oboe) and iOS (AVAudioEngine) sink, or let libwebrtc's audio device module own playback on mobile                                                                                                                                                                                                                                                                                                                                                                                             |
| `yap-audio`                       | builds once given the same TLS switch as yap-core (probed; on this branch it still names `native-tls`)                   | none (decode only)                                                                                                                                           | the TLS feature pair, and a mobile sink in `yap-speaker`                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `yap-anim`, `yap-webm`            | build                                                                                                                    | none                                                                                                                                                         | none                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `yap-games`, `yap-hotkeys`        | build (Win32 gated out)                                                                                                  | the whole purpose: process scanning, global hotkeys                                                                                                          | stay desktop-only                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `yap-capture`                     | not portable                                                                                                             | Windows Graphics Capture, D3D11, the game capture hook (19 of 30 files Win32)                                                                                | stays desktop-only. Mobile screen share is MediaProjection / ReplayKit                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `yap-segment`                     | not portable                                                                                                             | ORT with DirectML                                                                                                                                            | stays desktop-only. Mobile backgrounds would use ORT with NNAPI/CoreML, or ML Kit/Vision                                                                                                                                                                                                                                                                                                                                                                                                                         |
| app crate (`yap_native`)          | n/a                                                                                                                      | cxx-qt bridge, list models, QML, installer, updater, WebView2, crash dumps                                                                                   | none. Its `models.rs` (ViewDiff → `QAbstractListModel`) is the desktop counterpart of `CoreStore.kt`                                                                                                                                                                                                                                                                                                                                                                                                             |

### The TLS change in this branch

`native-tls` is OpenSSL on Android and there is no system copy to link.
Vendoring OpenSSL does not build with Git for Windows' perl, and it would
add a second TLS stack to the binary. So the backend is now a crate feature:

- `[workspace.dependencies] reqwest` carries no TLS feature;
- `yap-core` and `yap-realtime` have `native-tls` (the default) and
  `rustls` features;
- `yap-voice` and `yap-audio` and the app crate name `native-tls`
  explicitly, so the desktop app resolves exactly as before (only
  `native-tls`, checked with `cargo tree`).

The mobile crate turns defaults off, takes `rustls`, and hands the core a
reqwest client built from a rustls config with ring and the bundled
webpki roots. A release build should switch to
`rustls-platform-verifier`, which honours user-installed CAs and needs a
one-line JNI init plus its AAR.

### Licensing note

The workspace's own crates are GPL-3.0-only. GPL code on Apple's App Store
is widely considered incompatible with Apple's terms, which is why VLC was
removed in 2011. Every third-party crate in `deny.toml` is permissive or
MPL-2.0, so the owner, who holds the copyright on Yap's code, can add an
App Store exception or dual-license the mobile build. This applies to every
option. Qt (a) adds its own LGPL/commercial question on iOS.

## Next steps, if (c) is chosen

1. Split the portable core:
   - an injectable data root and `Config`;
   - `yap-desktop` for install/update/tauri;
   - gateway.rs into a crate;
   - a Markdown span tree next to the Qt HTML.
2. Use `rustls-platform-verifier` and a Keystore-backed `TokenVault`.
3. iOS: build the core for `aarch64-apple-ios` and the simulator, generate
   the Swift bindings, and run a SwiftUI shell with the same three screens
   in CI on `macos-latest`.
4. Voice: gate the Windows `webrtc-sys` patches, init libwebrtc on Android,
   and run a foreground service for the call.
5. Push: FCM/APNs registration from the shells, with a core command to
   register the token with the server.
