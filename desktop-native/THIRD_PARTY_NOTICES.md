# Third-party notices: native clients

The native Windows client (`Yap.exe`, built from `desktop-native/`) and the
Qt Android app (`desktop-native/mobile/qt`) are Yap-owned source under
GPL-3.0-or-later (see [LICENSE](../LICENSE)). This file records the
third-party components they link, embed or ship beside them, and the licence
each is used under. Versions and licences come from the lock files and
vendored sources named in each section; check those when they change.

Third-party binaries are shipped as their vendors released them and keep
their vendors' signatures. They are not described as Yap-signed because the
installer carries them.

## Windows client

### Qt 6.11.3 (LGPL-3.0)

`Yap.exe` links a static Qt 6.11.3: the modules qtbase, qtshadertools,
qtdeclarative, qtsvg, qtimageformats and qtmultimedia, built with
`patches/qt/build-qt.ps1` from the official `qt-everywhere-src-6.11.3`
source release with the patches in `patches/qt/6.11.3/`
(`tools/toolchain.lock.json`, `tools/package-toolchain.ps1`). Qt is used
under the GNU Lesser General Public License, version 3 (its build tools
under the GPL, version 3). The corresponding source is the Qt source release
plus the patches in this repository; `build-qt.ps1` rebuilds the libraries
from them, and the app's own source is published here, so the app can be
relinked against a modified Qt. Qt's own third-party components are listed
in the source release's `qt_attribution.json` files.

### libwebrtc (BSD-3-Clause, with its third-party components)

Voice and video use LiveKit's prebuilt libwebrtc `webrtc-a545de4`
(`webrtc-win-x64-release`, the build `webrtc-sys` 0.3.52 downloads),
unmodified, linked statically. WebRTC is under the BSD 3-Clause licence
("Copyright (c) 2011, The WebRTC project authors"). The archive's
`LICENSE.md` carries the licence of WebRTC and of every component in the
prebuilt library: abseil-cpp, BoringSSL, compiler-rt, dav1d, FFmpeg, fft,
fiat, g711, g722, libaom, libc++, libjpeg-turbo, libsrtp, libvpx, libyuv,
NASM, ooura, OpenH264, Opus, Perfetto, PFFFT, protobuf, RNNoise,
spl_sqrt_floor and zlib.

The Rust bindings `webrtc-sys` 0.3.52 and `libwebrtc` 0.3.55
([livekit/rust-sdks](https://github.com/livekit/rust-sdks)) are Apache-2.0.
Yap builds them from crates.io with the source patches in
`patches/webrtc-sys/` and `patches/libwebrtc/` applied by
`patches/vendor.ps1`.

### cxx-qt 0.10 (MIT OR Apache-2.0)

The Rust core reaches QML through [cxx-qt](https://github.com/KDAB/cxx-qt)
0.10.0 (`cxx-qt`, `cxx-qt-lib`, `cxx-qt-build`, `qt-build-utils`, by KDAB)
and [cxx](https://github.com/dtolnay/cxx) 1.0.205, all MIT OR Apache-2.0.
`qt-build-utils` is built with the patches in `patches/qt-build-utils/`.
CMake fetches `cxx-qt-cmake` at the commit pinned in `CMakeLists.txt`.

### OBS Studio game-capture files (GPL-2.0, unmodified, OBS-signed)

Game capture uses OBS Studio 33.0.1's own `graphics-hook{64,32}.dll`,
`inject-helper{64,32}.exe` and `get-graphics-offsets{64,32}.exe`, shipped
under `obs-capture\obs-signed\` exactly as the OBS Project released and
signed them. `graphics-hook` statically links Microsoft Detours (MIT). They
are pinned in
[`packaging/obs-signed-hook.lock.json`](packaging/obs-signed-hook.lock.json)
(release asset, per-file SHA-256, file version, signer and thumbprint) and
fetched and checked by `tools/fetch-obs-signed-hook.ps1`. Yap is not
affiliated with or endorsed by the OBS Project.

- Licence: GNU General Public License, version 2. The text ships beside the
  files as `obs-capture\LICENSE-GPLv2.txt`
  ([`packaging/obs-capture/LICENSE-GPLv2.txt`](packaging/obs-capture/LICENSE-GPLv2.txt)).
- Corresponding source: every desktop release attaches
  `OBS-Studio-33.0.1-Sources.tar.gz`, the Detours source at the commit
  obs-deps pins and the obs-deps `2026-10-23` build scripts, checked against
  the same lock. The written offer ships as `obs-capture\SOURCE-OFFER.md`
  ([`packaging/obs-capture/SOURCE-OFFER.md`](packaging/obs-capture/SOURCE-OFFER.md)).

### ONNX Runtime 1.24.4 (MIT) and the MediaPipe selfie segmenter (Apache-2.0)

Camera backgrounds ship Microsoft's ONNX Runtime 1.24.4 DirectML build
(`onnxruntime.dll`, from the `Microsoft.ML.OnnxRuntime.DirectML` NuGet
package, Microsoft-signed and not re-signed) in `segmentation\`, with its
`LICENSE-onnxruntime.txt` (MIT) and `ThirdPartyNotices-onnxruntime.txt`
beside it (`tools/package.ps1`). DirectML itself is the system's and is not
shipped. The Rust binding `ort` 2.0.0-rc.12 is MIT OR Apache-2.0.

The model, `assets/models/selfie_segmenter.onnx`, is MediaPipe's
`selfie_segmenter.tflite` (Apache-2.0, per the model card) converted to ONNX
by `tools/convert-selfie-segmenter.py`.

### Velopack (MIT)

Installs and updates use [Velopack](https://github.com/velopack/velopack):
the `velopack` crate 1.3.39-g8d2844f in the exe, and `vpk`, which adds its
`Update.exe`, the root stub and `Setup.exe` to the package. MIT.

### Fonts (SIL OFL 1.1, one Apache-2.0)

The fonts in `assets/fonts/` are cut from the Fontsource releases the web app
also uses (`tools/build-fonts.py`) and embedded in the app; each family's
licence text is beside it in `assets/fonts/`. Under the SIL Open Font License
1.1: Bricolage Grotesque, Bungee, Cinzel, Cormorant Garamond, Fredoka,
Geist, Geist Mono, Instrument Serif, Major Mono Display, Martian Mono, Press
Start 2P, Rubik Glitch (Rubik Filtered), Silkscreen, Syne, Unbounded,
UnifrakturMaguntia and VT323. Permanent Marker is under the Apache License
2.0 (its text is in `OFL-permanent-marker.txt`).

### Icons and emoji data (MIT)

- [Phosphor Icons](https://github.com/phosphor-icons/core)
  (`@phosphor-icons/core` 2.1.1), MIT
  (`assets/icons/LICENSE-phosphor.txt`).
- Emoji data from [emoji-mart](https://github.com/missive/emoji-mart)
  (`@emoji-mart/data` 1.2.1), MIT, as `assets/emoji/emoji-15.tsv`. The
  Twemoji images (CC-BY 4.0) are not bundled; they load at run time, as on
  the web (`assets/emoji/README.md`).

### Rust crates

The other Rust crates in `rust/Cargo.lock` are compiled into `Yap.exe`.
`cargo deny check licenses` (`rust/deny.toml`) fails the build on any
licence not on its allow list. For the Windows target the list is: MIT,
Apache-2.0, BSD-3-Clause, ISC, Zlib, 0BSD, BSL-1.0, Unlicense, Unicode-3.0,
BlueOak-1.0.0, CDLA-Permissive-2.0 (`webpki-roots`, Mozilla's root list)
and MPL-2.0 (`symphonia` 0.5.5, file-level copyleft). Run
`cargo deny list -l license` in `rust/` for the per-crate list. There is no
generated per-crate notice file yet.

## Android app

The Qt Android app (`dev.yap.app`) runs the same Rust core and QML:

- Qt 6.11.3 for Android (qtbase, qtdeclarative, qtsvg, qtimageformats and
  ICU) from Qt's official online repository, pinned in
  `mobile/qt/qt-sdk.lock.json`, under the LGPL-3.0. The Qt libraries ship in
  the APK as shared `.so` files.
- libwebrtc, `webrtc-sys`, `libwebrtc` and cxx-qt, as on Windows (LiveKit's
  prebuilt for Android, with the same source patches).
- The fonts, icons and emoji data above.
- From Maven, under the licences in their POMs: `androidx.core:core` 1.17.0
  and `com.google.firebase:firebase-messaging` 25.0.1
  (`mobile/qt/android/build.gradle`).
