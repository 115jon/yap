# Native client: libraries, assets and Qt version

Goal: full visual parity between YapNative (Rust core through cxx-qt 0.10, QML
UI, static Qt /MT, one exe, Windows first) and the web/desktop UI. This
document picks the fonts, icons, components, state, media and Qt version.
`PARITY_UI.md` holds the tokens and component specs. `ARCHITECTURE.md` holds
the store and model design. This document only picks what to build them with.

Research date: 2026-11-04. Versions, dates and star counts come from the
crates.io API, the GitHub API, the npm registry and doc.qt.io on that day.
Claims marked UNVERIFIED were not checked against this build.

## 0. Summary

| area          | pick                                                                                                                                                   | runner-up                                                                           |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| Fonts         | Static TTF cuts from the Google Fonts variable TTFs, the same upstream Fontsource serves the web. One family per opsz/wdth cut, in qrc                 | On Qt ≥ 6.7: ship the 4 variable TTFs and set `font.variableAxes`                   |
| Icons         | Per-concept SVGs generated from `mapping.ts`, drawn by a C++ `QQuickImageProvider` (`image://icon/...`). It tints and composites the slash in QPainter | `Shape` + `PathSvg` for animated glyphs; a subset Phosphor TTF for very dense lists |
| Other SVGs    | `Image { sourceSize }` through the static `qsvg` plugin (already in the 6.6.3 build)                                                                   | `VectorImage` once on Qt ≥ 6.8                                                      |
| Components    | Our own kit on `QtQuick.Templates` (the `T.*` controls), no third-party QML library                                                                    | Controls.Basic overrides for long-tail dialogs                                      |
| Frameless     | QWindowKit 1.5.0, static, Quick module                                                                                                                 | Hand-written `WM_NCCALCSIZE` / `WM_NCHITTEST` in `main.cpp`                         |
| App state     | Hand-rolled reducer, effect runner and `ViewDiff` (ARCHITECTURE §1-2), no store crate                                                                  | `imbl` for cheap state snapshots if needed                                          |
| List models   | cxx-qt `#[base = QAbstractListModel]` + `#[inherit]`, written by hand                                                                                  | `kushi` 0.1.0 (watch it; do not adopt)                                              |
| Cache         | `rusqlite` 0.41.1 with `bundled`                                                                                                                       | -                                                                                   |
| Markdown      | Port `MarkdownRenderer.tsx`'s own grammar to Rust (`fancy-regex`). Emit a Qt rich-text subset                                                          | `pulldown-cmark` 0.13.6 (wrong dialect, see §5.3)                                   |
| Emoji         | Twemoji PNGs from `emoji-datasource-twitter@16.0.0`, the same URLs the web uses, plus a disk cache. Native emoji matched from `@emoji-mart/data`       | Vendor the 64px sheet in qrc                                                        |
| Images        | `QQmlNetworkAccessManagerFactory` + `QNetworkDiskCache` for public media; Rust fetch for authenticated media                                           | `QQuickAsyncImageProvider` backed by Rust                                           |
| Long-tail web | `wry` 0.57 `build_as_child` on a child HWND (it uses `webview2-com` and links `WebView2LoaderStatic.lib`)                                              | `webview2-com` 0.39 directly                                                        |
| Call audio    | Own path (§11): WASAPI mic and speaker, libwebrtc's standalone APM, `nnnoiseless` 0.5.2 for RNNoise, our per-track stereo mix                          | libwebrtc's ADM (mixed playout, no hook after its APM)                              |
| Share encode  | Media Foundation hardware H.264 MFT into libwebrtc's pre-encoded pass-through (§12); VP8 without one                                                   | vendor SDKs, OpenH264                                                               |
| Backgrounds   | MediaPipe selfie segmenter as ONNX on ONNX Runtime 1.24.4 DirectML (`ort` rc.12, loaded at run time), own compositor (§12)                             | tract, MediaPipe C API                                                              |
| Game catalog  | `yap-games`: the web's catalog JSON parsed into a flat index, gzipped in the exe with `flate2` (linked already), Win32 through `windows-sys` (§15)     | `sysinfo` for processes                                                             |
| HDR capture   | FP16 (scRGB) WGC frames tone-mapped to BGRA8 by a pixel shader on the capture device; HDR, SDR white and peak from DXGI and the display config (§16)   | the D3D11 video processor; BGRA8 from WGC (no tone map)                             |
| Qt            | Keep 6.6.3 for the parity sprint. Build a static **Qt 6.12 LTS** (6.12.1 is due 2026-11-12) in parallel, then switch                                   | Stay on 6.6.3 (EOL, no fixes)                                                       |

## 1. Fonts

### 1.1 Sources

The web ships `@fontsource-variable/*@5.3.0` WOFF2 subsets (`src/styles.css:9-12`).
Fontsource repackages the Google Fonts API files, so the upstream for exact
parity is the Google Fonts repo, not each foundry's latest release.

| family              | file to vendor (google/fonts `ofl/`)                        | size       | foundry upstream                                                                                  | license |
| ------------------- | ----------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------- | ------- |
| Bricolage Grotesque | `bricolagegrotesque/BricolageGrotesque[opsz,wdth,wght].ttf` | 408 KB     | github.com/ateliertriay/bricolage (no releases; last push 2023-10-09; `fonts/{otf,ttf,variable}`) | OFL-1.1 |
| Geist               | `geist/Geist[wght].ttf`, `geist/Geist-Italic[wght].ttf`     | 169/175 KB | github.com/vercel/geist-font v1.9.0 (2026-10-30, `geist-font-v1.9.0.zip`)                         | OFL-1.1 |
| Geist Mono          | `geistmono/GeistMono[wght].ttf` (+ Italic)                  | 172 KB     | same repo                                                                                         | OFL-1.1 |
| Martian Mono        | `martianmono/MartianMono[wdth,wght].ttf`                    | 148 KB     | github.com/evilmartians/mono v1.1.0 (2025-02-10; ttf, otf, variable, woff2 zips)                  | OFL-1.1 |

URLs:

- `https://github.com/google/fonts/tree/main/ofl/{bricolagegrotesque,geist,geistmono,martianmono}`
- Raw: `https://raw.githubusercontent.com/google/fonts/<commit>/ofl/<dir>/<file>`. Pin a commit hash, not `main`.

Rules:

- **Do not take Geist 1.9.0 from Vercel's release.** It is newer than what the
  web serves, so metrics and glyphs can drift. Parity means the same version
  string the web loads. The build script checks name ID 5 (version) of each
  vendored TTF against the Fontsource WOFF2 in `node_modules`, and fails on a
  mismatch.
- Why not cut straight from `node_modules` as `build-installer-fonts.py`
  does? Fontsource splits each family into `latin`, `latin-ext`, `cyrillic`
  and similar subsets with `unicode-range`. Native needs one face with full
  coverage. The Google Fonts TTF already is that face. The `latin-*-standard`
  WOFF2 files remain a fallback if the versions differ (merge the subsets
  with `fontTools.merge`).

### 1.2 Can Qt 6.6.3 use the axes?

- No. `QFont::setVariableAxis` and `QFont::Tag` were "introduced in Qt 6.7"
  (doc.qt.io/qt-6/qfont.html).
- Qt 6.7 also added QML `font.variableAxes` and turned on variable
  application fonts on all platforms. On Windows that "requires DirectWrite
  or FreeType backend" (doc.qt.io/qt-6/whatsnew67.html).
- Qt 6.8 "changed the font system to use DirectWrite as the default backend"
  (whatsnew68.html). In 6.6 the default is GDI, and the GDI database cannot
  even load named instances of variable application fonts (QTBUG-117835).
- So on 6.6.3, cut static instances. The opsz cuts matter: Bricolage at
  opsz 15 and at opsz 40 are visibly different drawings.

### 1.3 Recommended approach (works on 6.6.3 and later)

- Fork `desktop/scripts/build-installer-fonts.py` (now on `archive/tauri-desktop`) as
  `native/scripts/build-fonts.py`. PARITY_UI §3.4 lists the changes:
  no `track()`, the full instance list, and full coverage instead of the
  Latin subset.
- **Input:** `native/assets/fonts/src/*.ttf` (the vendored Google
  Fonts variable TTFs, with `OFL.txt` each).
- **Output:** `native/assets/fonts/*.ttf`, committed. Commit the
  generated cuts so a build needs no Python.
- **Instances** (one family per opsz/wdth cut, so Qt's weight matching never
  crosses cuts):

| family name            | cuts                                                                                             |
| ---------------------- | ------------------------------------------------------------------------------------------------ |
| `Yap Bricolage Key`    | 600, 700 at opsz 15                                                                              |
| `Yap Bricolage Head`   | 700 at opsz 20, 800 at opsz 18                                                                   |
| `Yap Bricolage Title`  | 800 at opsz 28                                                                                   |
| `Yap Bricolage Poster` | 800 at opsz 40                                                                                   |
| `Yap Geist`            | 400, 500, 600, plus 400 italic. Real weights: usWeightClass set, fsSelection BOLD/ITALIC correct |
| `Yap Geist Mono`       | 400                                                                                              |
| `Yap Martian Silk`     | 500 at wdth 90                                                                                   |
| `Yap Martian Num`      | 500 and 600 at wdth 100                                                                          |

- **Loading:** add the files to a qrc (`qt_add_resources(... PREFIX /fonts)`)
  and call `QFontDatabase::addApplicationFont(":/fonts/...")` in `main.cpp`
  before the engine loads. One family string per role lives in
  `Theme.role.*`. `FontLoader` also works, but it is per-component and
  asynchronous, so the C++ call is simpler.
- **Size:** about 12 static cuts at 60-150 KB each is roughly 1.5 MB in the
  exe. qrc zlib compression shrinks that further.
- **Rendering check:** compare `Text.NativeRendering` against `QtRendering`
  with the fixture captures (`ralph-ui-fixture-captures`), and record the
  decision. On ≥ 6.8 also try `QFont::PreferTypoLineMetrics`, because Chromium
  line boxes differ from GDI's win metrics. UNVERIFIED which one matches.

**Upgrade path (Qt ≥ 6.7, DirectWrite):** ship the 4 variable TTFs as they
are. Then `font.variableAxes: { "opsz": 28, "wdth": 100 }` together with
`font.weight` replaces the cut script. Keep the static cuts until a pixel
diff proves the variable path matches.

### 1.4 License and attribution

- All four are SIL OFL 1.1. Bundling them in the exe is allowed. The
  copyright notice and the OFL text must ship with the program: put them in
  `THIRD_PARTY_LICENSES.md` and on an About/licenses page.
- Instancing and subsetting are a "Modified Version". Renaming to `Yap *`
  families keeps clear of any Reserved Font Name, as the installer script
  already does. Keep name IDs 0, 13 and 14 (copyright and license).

## 2. Icons (exact Phosphor parity)

### 2.1 What exists

- `@phosphor-icons/core@2.1.1` (MIT, npm latest, 2024-03-29) is in
  `node_modules`, with 1512 SVGs per weight under `assets/<weight>/`.
- The web draws only the concepts in `src/components/ui/icon/mapping.ts`
  (about 180) via `glyphs.generated.ts`: Bold paths, plus Fill paths for
  stateful concepts, on a 256 grid.
- `Icon.tsx` adds a Phosphor-style slash (`M48 40 208 216`, stroke 24 Bold or
  16 Fill) with a 12-unit **clear band** masked out of the glyph, and an
  optional `rotate`.
- `@phosphor-icons/web@2.1.2` (MIT, 2025-03-31; not installed) ships
  **per-weight icon fonts**: `src/bold/Phosphor-Bold.{ttf 495 KB, woff,
woff2 150 KB, svg}` and `src/fill/Phosphor-Fill.ttf` (449 KB), plus
  `style.css` and `selection.json` with the codepoints. It has no slashed or
  rotated variants, so it cannot express our composites.

### 2.2 Options on 6.6.3

| option                                                                                                                                                                                     | exactness                                                              | recolor                                            | cost in long lists                                                                                               | slash and mask                                              | verdict                         |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------- | -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- | ------------------------------- |
| A. `QQuickImageProvider` that renders the generated SVG with `QSvgRenderer` at `size × dpr`, fills with `CompositionMode_SourceIn`, and clears the slash band with `CompositionMode_Clear` | Same outlines, rasterised once per (concept, state, size, colour, dpr) | in the URL: `image://icon/mic/off/18/ff6b6b`       | best: atlased textures, one cached pixmap per key, batched draws                                                 | trivial in QPainter                                         | **pick**                        |
| B. `Shape` + `PathSvg`, `CurveRenderer`                                                                                                                                                    | exact, resolution-free                                                 | bind `fillColor` (like `currentColor`), animatable | one geometry node per icon. CurveRenderer is **experimental in 6.6** (no longer experimental in 6.7, whatsnew67) | needs boolean path ops at generate time (no masks in Shape) | animated or large glyphs only   |
| C. Icon font (subset Phosphor TTF or our own font built from the composites)                                                                                                               | glyph outlines with hinting differences at 12-16 px                    | `color`                                            | cheap (glyph cache)                                                                                              | must bake composites into the font                          | dense-list fallback             |
| D. `Image` of a plain SVG + `MultiEffect` colorize                                                                                                                                         | exact                                                                  | per-item effect pass                               | expensive                                                                                                        | no                                                          | reject (PARITY_UI §3.5 agrees)  |
| E. `VectorImage`                                                                                                                                                                           | exact                                                                  | via `svgtoqml`-compiled shapes                     | Shape-based, same as B                                                                                           | -                                                           | Qt ≥ 6.8 only (`Since: Qt 6.8`) |

Why A:

- It is the only option that does the clear-band mask without path booleans.
  Qt Svg in 6.6 is SVG Tiny (masks arrived in 6.7), so the band cannot live
  in the SVG file either.
- In the scene graph, an icon is a texture quad. Long lists (members,
  reactions, the tree) do not multiply Shape nodes. That keeps the
  on-demand-render idle budget honest.
- A colour change (a hover from ink-3 to ink-1) is a new URL. The pixmap
  cache makes repeat states free.
- For a 100 ms colour transition, crossfade two `Image`s or snap. The web's
  icon colour transitions are short, so verify against the fixtures.

Icon provider details:

- About 60 lines of C++ in `native/cpp/iconprovider.{h,cpp}`,
  registered with `engine.addImageProvider("icon", ...)`.
- It loads `:/icons/<concept>-<bold|fill>.svg`.
- It reads `requestedSize`, so `Image { sourceSize }` drives the
  rasterisation size. Set `smooth: false` at integer DPR to avoid blur, and
  round sizes to the device pixel.
- `QSvgRenderer` comes from `Qt6Svg.lib` (present in the 6.6.3 build). The
  `qsvg` image plugin is present too.

Generator:

- Extend `scripts/generate-phosphor-icons.mjs` with `--qml`.
- For each concept it writes `native/assets/icons/<concept>-bold.svg`
  (and `-fill.svg` when `fill: true`) with `rotate` applied. The provider
  composites the slash, using the same constants as `Icon.tsx`.
- It writes `native/assets/icons/manifest.json` with the concept,
  glyph, fill, slash, rotate and the `OFF_PAIR` map, so `Icon.qml` resolves
  states the way `Icon.tsx` does.
- Keep the concept names. Commit the output.

`Icon.qml`: `name`, `state` (idle/active/off), `size` (xs 12 … 3xl 48) and
`color`. It builds the provider URL and sets `sourceSize`.

**If you upgrade Qt (≥ 6.8):** keep A for lists. Use `VectorImage` (or
`svgtoqml` at build time) for the theme glyphs and illustrations. Qt Svg
6.7+ masks also let the generator emit self-contained slashed SVGs.

### 2.3 Non-icon SVG assets

- These are plain paths with no `mask`, `filter`, `pattern`, `clipPath`,
  gradients or `<style>`. They render correctly in Qt Svg Tiny:
  - `public/icons/home-{theme}.svg` (6 files)
  - `public/themes/miku/miku-wig.svg`
  - `public/themes/spiderman/spiderman-mask.svg`
  - `public/window-controls/{close,maximize,minimize}.svg`
- Copy them to `native/assets/svg/` and draw them with
  `Image { source: "qrc:/svg/home-dark.svg"; sourceSize: ... }`.
- The window controls should use the icon concepts (`minus`,
  `windowMaximize`, `windowRestore`, `close`), as the web does
  (`desktop-entry.tsx:252-261`).

License: Phosphor is MIT, © Phosphor Icons (Tobias Fried). Keep the notice
in `THIRD_PARTY_LICENSES.md`.

## 3. QML components and window chrome

### 3.1 Component libraries

| library                                                  | version, maintenance                                                                               | license              | fit                                                                                                                                                                                                              |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Qt Quick **Templates** (`import QtQuick.Templates as T`) | ships with Qt; Qt's documented way to write a custom style                                         | LGPL (Qt)            | **pick**: behaviour, focus, keyboard and accessibility with no visuals. Every `PARITY_UI` §4 component is a `T.Button`, `T.Menu`, `T.Popup`, `T.ToolTip` or `T.TextArea` with our `background` and `contentItem` |
| Controls **Basic**                                       | ships with Qt                                                                                      | LGPL                 | the base for one-off dialogs; already used in the prototype                                                                                                                                                      |
| FluentWinUI3 style                                       | Qt ≥ 6.8; "under development"; "certain elements … may not be customizable" (it uses image assets) | LGPL                 | no. It looks like WinUI, not like Yap                                                                                                                                                                            |
| Material / Universal / Fusion / Imagine                  | in the 6.6.3 build                                                                                 | LGPL                 | no; each is another design language                                                                                                                                                                              |
| FluentUI (zhuzichu520)                                   | 4.6k stars; last release 1.7.7 on 2024-07-10; last push 2026-04-07                                 | MIT (per GitHub API) | no: a Fluent look with a large C++ plugin to build statically, and stale releases                                                                                                                                |
| Qaterial                                                 | v1.5.3 (2026-01-08); 362 stars                                                                     | MIT                  | no: Material Design. Useful only as a reference for theme singletons                                                                                                                                             |
| Kirigami (KDE)                                           | active (KDE/kirigami pushed 2026-11-04)                                                            | LGPL                 | no: it pulls KF6/ECM and has its own HIG and convergent layouts. A static Windows build is heavy                                                                                                                 |
| qml-material (papyros)                                   | dead                                                                                               | LGPL                 | no                                                                                                                                                                                                               |

This confirms the starting point: the design system is bespoke (Talk Tree
world, radius 2, silkscreen labels, LEDs), so any third-party kit would be
restyled until nothing of it is left.

Build `native/rust/qml/ui/` with these components:

- `YText` (role-driven type)
- `Key` and `IconKey`
- `Field`
- `MenuPanel` and `MenuItem`
- `Tip`
- `Float` (elevation)
- `Led`
- `CountBadge`
- `Avatar`
- `Icon`
- `PressArea`
- `FocusRing`

Base them on `QtQuick.Templates`, not on a Controls style. That avoids the
Basic visuals and keeps the static import set small (`qt_import_qml_plugins`
links only what is imported). Set `Accessible.name` and `Accessible.role` on
every component.

### 3.2 Frameless window with snap layouts

| option                                | facts                                                                                                                                                                                                                                                                                                                                                                                              | verdict                                                    |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| **QWindowKit** (`stdware/qwindowkit`) | **1.5.0** (2026-01-18); pushed 2026-11-04; 1.2k stars; Apache-2.0. Qt 6.6.2+. Quick module (`QWindowKit::Quick`, `-DQWINDOWKIT_BUILD_QUICK=ON`), `QWINDOWKIT_BUILD_STATIC`. "Windows 11 Snap Layout support when system button roles are configured". It keeps moving, resizing, the system menu and the Win10 border workaround. Resonance already ships it statically (FEASIBILITY.md:97)        | **pick**                                                   |
| Manual Win32 in `main.cpp`            | `WM_NCCALCSIZE` returns 0 to drop the caption. `DwmExtendFrameIntoClientArea` keeps the shadow. `WM_NCHITTEST` returns `HTCAPTION` for the 32px bar and `HTMAXBUTTON` over the maximise key (needed for the snap flyout). Then you must forward `WM_NCMOUSEMOVE`, `WM_NCLBUTTONDOWN` and `UP` to QML for hover and press, fix the maximised inset (the frame is off-screen) and handle DPI changes | fallback; about 300 lines that QWindowKit already debugged |
| Keep the DWM caption, recoloured      | no 32px back/forward/location bar                                                                                                                                                                                                                                                                                                                                                                  | does not reach parity (PARITY_UI gap 4)                    |

How to use QWindowKit:

- Add it with `FetchContent` pinned to the 1.5.0 tag, with
  `QWINDOWKIT_BUILD_STATIC=ON`, `QWINDOWKIT_BUILD_QUICK=ON` and
  `QWINDOWKIT_BUILD_WIDGETS=OFF`.
- In QML, `WindowAgent.setup(window)`, `setTitleBar(bar)`,
  `setSystemButton(WindowAgent.Maximize, maxKey)` and
  `setHitTestVisible(...)` for the back/forward keys.
- Add its Apache-2.0 notice to `THIRD_PARTY_LICENSES.md`.
- Check: it builds against static 6.6.3 with `/MT`. UNVERIFIED here, but
  resonance proves the same configuration.

## 4. Rust state and models

### 4.1 Store

| candidate                                                                                   | facts                                                                          | verdict                                                                                                                                     |
| ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| hand-rolled `reduce(&mut State, Event) -> Vec<Effect>`, an actor-owned store and `ViewDiff` | ARCHITECTURE §1-2; the web's `chat-reducer.ts` and its tests port rule by rule | **pick**                                                                                                                                    |
| `redux-rs` 0.3.3                                                                            | last release 2022-03-13; about 1.1k recent downloads                           | stale; adds async middleware we do not need                                                                                                 |
| `yewdux`                                                                                    | built for Yew (a web/wasm component framework)                                 | wrong runtime                                                                                                                               |
| `futures-signals` 0.3.34                                                                    | 2024-07-26; FRP signals and `MutableVec` diffs                                 | elegant, but it pushes signal graphs across the bridge. The `ViewDiff` → `QAbstractListModel` mapping is clearer and testable as plain data |
| `imbl` 7.0.2 (maintained fork of `im`, which stopped in 2022)                               | 2026-09-09                                                                     | optional: cheap `State` clones for snapshots and undo; not needed on day 1                                                                  |
| `indexmap` 2.14                                                                             | standard                                                                       | yes, for ordered id-keyed collections (servers, channels)                                                                                   |

Lunatic (an actor VM for WASM) is irrelevant here. Plain tokio tasks and
`mpsc` cover the actor.

### 4.2 List models through cxx-qt 0.10

- cxx-qt 0.10.0 (2026-08-24; KDAB; MIT/Apache) has no ready
  `QAbstractListModel` type in `cxx-qt-lib`.
- The supported pattern is `examples/qml_features/rust/src/custom_base_class.rs`:
  1. Declare `type QAbstractListModel;` with `include!(<QtCore/QAbstractListModel>)`.
  2. Write `#[qobject] #[base = QAbstractListModel]`.
  3. `#[inherit]` `begin_insert_rows`, `end_insert_rows`, `begin_remove_rows`,
     `end_remove_rows`, `begin_reset_model` and the `data_changed` signal (all
     `unsafe` to pair).
  4. `#[cxx_override]` `data`, `row_count` and `role_names`.
- `cxx-qt-lib` has `QModelIndex`, `QPersistentModelIndex`, `QHash` (for role
  names), `QVariant` and `QByteArray`. It also has optional
  `image-v0-25` conversion for `QImage`.
- The threading example (`threading.rs`, `CxxQtThread::queue`) is how the
  store's diffs reach the GUI thread.
- Write one generic Rust adapter, `apply(diff: ViewDiff)` → begin/end calls,
  and use it in every model.
- `kushi` 0.1.0 (reakjra/kushi, 2026-07-13, MIT/Apache) generates qproperty
  and role boilerplate from `build.rs`. It has 0 stars, 0 reverse
  dependencies and one release. Do not adopt it; copy the idea (a role enum
  plus a macro) if the boilerplate hurts.
- `qmetaobject` 0.2.10 (2023) is an alternative bridge. It is stale and
  conflicts with cxx-qt, so no.

### 4.3 Cache

- `rusqlite` 0.41.1 (2026-10-28) with `features = ["bundled"]`. This compiles
  the SQLite amalgamation through `cc`.
- `bundled` follows the crate's `crt-static`, so `/MT` matches
  (`rust/.cargo/config.toml` already sets `+crt-static`).
- Run it on a blocking thread (`spawn_blocking`) or a dedicated writer thread,
  as ARCHITECTURE §4 says.

## 5. Text, emoji, media and web panels

### 5.1 Markdown

- The web does not use a CommonMark library. `src/components/chat/MarkdownRenderer.tsx`
  (297 lines) is a Discord-style subset with one inline regex (`:71`):
  - `` `code` ``, `||spoiler||`, `**bold**`, `*italic*`, `__underline__`
    and `_italic_` (with lookbehind);
  - `~~strike~~` and `[text](url)`;
  - custom emoji `<:name:id>`, `:shortcode:`, `@mention` and bare URLs;
  - fenced ` ``` ` blocks and `> ` quotes.
- Port it to `yap-core::markdown` with `fancy-regex` 0.20 (for the
  `(?<![\w])` lookbehind; the `regex` crate has no lookaround).
- Port `__tests__/MarkdownRenderer.test.tsx` as table tests.
- `pulldown-cmark` 0.13.6 parses CommonMark. CommonMark treats `__x__` as
  bold, has no spoilers, and parses emphasis differently, so it would break
  parity. Use it only if the web switches dialects.
- Output: styled runs, rendered to a small Qt rich-text HTML subset (`<b>`,
  `<i>`, `<u>`, `<s>`, `<a>`, `<code>`, `<span style="background-color">`,
  `<img width height>`) for `TextEdit { textFormat: TextEdit.RichText;
readOnly: true }`. The `TextEdit` keeps text selectable.
- Mentions and spoilers come out as background spans. The token radius is 2,
  so the missing border-radius in Qt rich text is barely visible. Clickable
  regions use `<a href="yap:mention/<id>">` and `onLinkActivated`.
- Code blocks become separate delegates (a `Rectangle` and a `Text`), not
  rich text.

### 5.2 Emoji

- The web never draws system emoji.
- `src/lib/emoji.ts:15-177` maps every native emoji and `:shortcode:` to
  Twemoji PNGs:
  `https://cdn.jsdelivr.net/npm/emoji-datasource-twitter@16.0.0/img/twitter/64/<unified>.png`.
  `emoji-datasource-twitter` 16.0.0 is the npm latest (2025-09-24).
- `splitTextByNativeEmoji` (`:519`) finds emoji with a regex built from
  `@emoji-mart/data` ^1.2.1.
- Native, for parity:
  1. Embed the same `@emoji-mart/data` JSON (MIT) and build the matcher in
     Rust (`aho-corasick` or a longest-match trie, skin tones as in
     `applyNativeEmojiSkinTone`).
  2. Emit `<img src="https://cdn.jsdelivr.net/...png" width=22 height=22>`,
     or the cached `file:` URL.
  3. Fetch through the shared disk cache (§5.3) so the second view is
     offline. Segoe UI Emoji would differ visibly, so never let Qt fall back
     to it for message text.
- The picker grid (~1900 glyphs) uses a `GridView` of `Image`s with the same
  URLs. The sheet alternative is `sheet_twitter_64.png` in the same package
  (one large PNG plus coordinates in the JSON). Vendoring it in qrc costs
  several MB (UNVERIFIED size).
- License: the Twemoji **graphics are CC-BY 4.0** (jdecked/twemoji; the code
  is MIT). Attribution is required: put it in About/licenses and
  `THIRD_PARTY_LICENSES.md`. The emoji-datasource code is MIT.
- jdecked/twemoji v17.0.3 (2026-06-01) is newer. Stay on 16.0.0 until the web
  moves.
- Unverified: whether `QQuickText` or `TextEdit` rich text loads `https:`
  `<img>` asynchronously in 6.6. If not, Rust resolves the cache first and
  emits `file:` URLs, re-rendering the row when the file lands.

### 5.3 Images and media

- **Default:** a `QQmlNetworkAccessManagerFactory` in `main.cpp` that sets a
  `QNetworkDiskCache` (`%LOCALAPPDATA%\Yap Native\http-cache`, about 512 MB).
  `Image { source: url; asynchronous: true; sourceSize: ...; cache: true }`
  then covers avatars, banners, emoji, GIF previews and attachments with
  no Rust code.
  - TLS uses the static Schannel backend (in the 6.6.3 build).
  - `AnimatedImage` uses the `qgif` and `qwebp` plugins, which are present.
- **Authenticated media** (if attachment URLs need a bearer token or are not
  pre-signed; see the open questions): a `QQuickAsyncImageProvider`
  (`image://media/<id>`). Its response calls Rust over cxx (reqwest plus the
  disk cache, decoded with `image` 0.25.10 or handed to Qt as bytes).
- Avatars: radius 2 means a `Rectangle { clip: true }` or a pre-sized
  `sourceSize`, no shader mask. Decorated avatars use a round crop with
  `MultiEffect` mask, only where it is used.

### 5.4 WebView2 for long-tail panels

- **`wry` 0.57.1** (2026-10-23, Tauri; MIT/Apache) calls
  `WebViewBuilder::build_as_child(&hwnd)` on a child HWND that Qt hosts.
  - The host is a `QWindow::fromWinId` container, or a plain child HWND
    positioned from a QML `Item`'s scene rect.
  - It sits on `webview2-com` 0.39.1 (2026-03-11).
  - By default it links `WebView2LoaderStatic.lib`, so there is no DLL to
    ship (webview2-rs README; MSVC targets only).
- Airspace: QML cannot draw over the WebView, so give it a whole panel or a
  modal. Use a separate user-data folder from the Tauri app. Share cookies
  through the app session token, not through the profile.

## 6. Qt version: stay on 6.6.3 or build newer?

**Settled in performance pass 5 (2026-11-07):** the app builds on a custom
static Qt 6.11.3 (§13.3), with `RectangularShadow` adopted. 6.12 was not
needed. The notes below are the original survey.

Facts:

- Qt 6.6 is out of support. 6.6.3 was its last release (2024).
- **Qt 6.12 LTS was released 2026-09-30.** Patch releases are 6.12.1 on
  2026-11-12, 6.12.2 on 2027-01-14 and 6.12.3 on 2027-03-04 (wiki.qt.io/Qt_6.12_Release).
- 6.11.3 (September 2026) ended 6.11, and 6.10.3 ended 6.10.
- 6.8 LTS patches are commercial-only by now (Boot2Qt is on 6.8.8). That last
  point is UNVERIFIED for open-source availability.
- There is **no official static prebuilt** of any Qt for MSVC. Community
  GitHub Actions repos exist, but none publishes a verified `/MT` 6.12 build.
- vcpkg's `qtbase` port is 6.11.3. Its `x64-windows-static` triplet is `/MT`,
  but it compiles from source anyway, and it lags.

What 6.12 gains for parity, with the version each feature arrived in:

| feature                                                                | since | effect on this project                                                               |
| ---------------------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------ |
| variable font axes (`QFont::setVariableAxis`, QML `font.variableAxes`) | 6.7   | the 4 TTFs replace about 12 cuts; exact opsz and wdth at any size                    |
| DirectWrite as the default Windows font backend                        | 6.8   | closer to Chromium's text; variable fonts load                                       |
| `PreferTypoLineMetrics`, `ContextFontMerging`                          | 6.8   | line-height parity with the browser                                                  |
| CurveRenderer out of experimental; `Text.CurveRendering`               | 6.7   | crisp Shapes and large display type                                                  |
| Qt Svg masks, markers, patterns, filters                               | 6.7   | slashed icons and richer theme art as plain SVG                                      |
| `VectorImage` + `svgtoqml` out of tech preview                         | 6.8   | SVG as scene-graph shapes                                                            |
| `Popup.popupType: Popup.Window`                                        | 6.8   | menus, tooltips and popovers that extend past the window edge, as the Tauri app's do |
| `RectangularShadow`                                                    | 6.9   | elevation shadow-2/3 without `MultiEffect` (PARITY_UI §3.7)                          |
| FluentWinUI3                                                           | 6.8   | not needed                                                                           |
| qmlsc and type-compiler improvements, AOT stats                        | 6.8+  | faster QML start, better warnings                                                    |
| security and bug fixes                                                 | -     | 6.6 gets none                                                                        |

Cost estimate (UNVERIFIED; based on typical static builds):

- Configure 6.12.x with the same flags as resonance: `-static
-static-runtime -release -optimize-size`, `-submodules
qtbase,qtdeclarative,qtsvg,qtshadertools,qtimageformats,qtwebsockets`,
  `-skip qtwebengine`, and `-schannel` / `-openssl-runtime`.
- Skip Multimedia, Quick3D and Location unless needed. Voice uses libwebrtc,
  not Qt Multimedia.
- About 45-90 minutes on a 16-core machine with Ninja; 15-25 GB of build
  tree; a 3-6 GB install prefix.
- The exe grows by a few MB at most, because static linking pulls only what
  is used.
- Risk: cxx-qt 0.10 against 6.12 is UNVERIFIED. cxx-qt tracks recent Qt 6,
  so prove it in the first build.

Recommendation:

- Do not block the parity sprint. The fonts (static cuts), icons (provider),
  components (Templates) and QWindowKit picked here all work on 6.6.3 and
  carry over unchanged.
- In parallel, build a static 6.12.1 when it lands (2026-11-12) with a
  `native/scripts/build-qt.ps1` that records the exact configure
  line. Publish the install prefix as an artifact (an internal CDN zip) so
  later machines get a "prebuilt" without rebuilding.
- Switch `-QtDir` once `build.ps1` passes and a fixture pixel diff is no
  worse.
- After the switch, adopt `Popup.Window`, `RectangularShadow` and variable
  fonts, in that order.

## 7. Next steps (vendoring)

1. `native/assets/fonts/src/`: download at a pinned google/fonts commit:
   - `BricolageGrotesque[opsz,wdth,wght].ttf`
   - `Geist[wght].ttf` and `Geist-Italic[wght].ttf`
   - `GeistMono[wght].ttf`
   - `MartianMono[wdth,wght].ttf`
   - each family's `OFL.txt`

   Record the commit and the name-ID-5 versions in `assets/fonts/SOURCES.md`.

2. `native/scripts/build-fonts.py`: fork the installer script:
   - remove tracking;
   - use the instance table in §1.3 and full coverage;
   - check versions against `node_modules/@fontsource-variable/*`.

   Commit the output to `native/assets/fonts/`.

3. `scripts/generate-phosphor-icons.mjs --qml`: write
   `native/assets/icons/*.svg` and `manifest.json`. Add
   `cpp/iconprovider.{h,cpp}` and `qml/ui/Icon.qml`.
4. Copy the theme SVGs (§2.3) to `native/assets/svg/`.
5. Add a `qt_add_resources(YapNative assets PREFIX / FILES ...)` block to
   `CMakeLists.txt` for fonts, icons and SVGs. Add `Qt::Svg` to
   `find_package` and link it. Call `addApplicationFont` and
   `addImageProvider` in `main.cpp`.
6. QWindowKit: add it with `FetchContent` at tag `1.5.0` (static, Quick on,
   Widgets off). Build the 32px `TitleBar.qml`.
7. Rust: add `rusqlite = { version = "0.41", features = ["bundled"] }`,
   `fancy-regex = "0.20"`, `aho-corasick` and `indexmap`. Write the
   `ViewDiff` adapter and the first model (`ServerListModel`) from
   `custom_base_class.rs`.
8. Copy `@emoji-mart/data` `data.json` (pin 1.2.x) into
   `native/assets/emoji/` or `include_bytes!` it in Rust. Add the
   NAM factory with a disk cache in `main.cpp`.
9. Add sections to `THIRD_PARTY_LICENSES.md`:
   - OFL ×4 (fonts)
   - Phosphor MIT
   - Twemoji CC-BY 4.0
   - emoji-mart and emoji-datasource MIT
   - QWindowKit Apache-2.0
   - Qt LGPLv3 (FEASIBILITY.md:166)
   - wry and webview2-com MIT/Apache when added
10. Start `build-qt.ps1` for 6.12.1 (§6).

## 8. Open questions

- Are attachment and avatar URLs public or pre-signed (so the NAM disk cache
  is enough), or do they need the app session header (so `image://media`)?
- Does `TextEdit` rich text in 6.6.3 load `https:` `<img>` sources, and does
  it re-layout when they arrive? Decide `https:` versus pre-resolved `file:`
  after one test.
- Icon colour transitions: does any key animate the icon colour (not only
  the background)? If so, crossfade two provider images or use `Shape` for
  that key.
- `Text.NativeRendering` or `QtRendering` for 11-15px: settle it with a
  fixture pixel diff.
- Do the Google Fonts TTF versions match Fontsource 5.3.0's? If not, merge
  the Fontsource WOFF2 subsets instead.
- Qt 6.12: does cxx-qt 0.10 (and `cxx-qt-cmake` at `64e63f5`) build against
  it statically? Do we keep resonance's 6.6.3 for the voice spike until
  then?
- Display-name fonts (P2, 7 `@fontsource/*` families): ship them all in qrc
  (about 1 MB), or download them on first use?

## Sources

- Qt docs:
  - QFont, `setVariableAxis` "introduced in Qt 6.7": https://doc.qt.io/qt-6/qfont.html
  - What's new in 6.7: https://doc.qt.io/qt-6/whatsnew67.html
  - What's new in 6.8: https://doc.qt.io/qt-6/whatsnew68.html
  - VectorImage, "Since: Qt 6.8": https://doc.qt.io/qt-6/qml-qtquick-vectorimage-vectorimage.html
  - FluentWinUI3: https://doc.qt.io/qt-6/qtquickcontrols-fluentwinui3.html
- Qt releases:
  - https://wiki.qt.io/Qt_6.12_Release
  - https://wiki.qt.io/Qt_6.11_Release
  - "Qt 6.12 LTS Released!" (2026-09-30): https://www.qt.io/blog
- GDI and variable named instances: https://bugreports.qt.io/browse/QTBUG-117835
- Fonts:
  - https://github.com/google/fonts/tree/main/ofl
  - https://github.com/vercel/geist-font/releases
  - https://github.com/evilmartians/mono/releases
  - https://github.com/ateliertriay/bricolage
- Icons:
  - https://www.npmjs.com/package/@phosphor-icons/web (2.1.2)
  - https://unpkg.com/@phosphor-icons/web@2.1.2/src/bold/
  - https://github.com/phosphor-icons/core
- QWindowKit: https://github.com/stdware/qwindowkit
- Component libraries:
  - https://github.com/zhuzichu520/FluentUI
  - https://github.com/OlivierLDff/Qaterial
- cxx-qt:
  - https://github.com/KDAB/cxx-qt (`examples/qml_features/rust/src/custom_base_class.rs`)
  - https://crates.io/crates/cxx-qt
- Crates:
  - https://crates.io/crates/kushi
  - https://crates.io/crates/rusqlite
  - https://crates.io/crates/pulldown-cmark
  - https://crates.io/crates/fancy-regex
  - https://crates.io/crates/redux-rs
  - https://crates.io/crates/futures-signals
  - https://crates.io/crates/imbl
  - https://crates.io/crates/wry
  - https://crates.io/crates/webview2-com
- webview2-rs README: https://github.com/wravery/webview2-rs
- Emoji:
  - https://www.npmjs.com/package/emoji-datasource-twitter
  - https://github.com/jdecked/twemoji
  - https://github.com/iamcal/emoji-data
- Static Qt:
  - vcpkg qtbase port: https://github.com/microsoft/vcpkg/tree/master/ports/qtbase
  - community static builds (unverified): https://github.com/cncherisher/buildQt

## 9. Local playback: soundboard, tones and Listen Together (`yap-audio`)

The web plays soundboard clips with `new Audio(url)`, the built-in tones with
a Web Audio oscillator and the Listen Together stream with an audio element
behind two `DynamicsCompressorNode`s. All of it is local to each client, not
published into the call. `crates/yap-audio` does the same natively. Versions
and dates come from the crates.io API on 2026-11-05.

| area       | pick                                                                                                                                                              | runner-up / rejected                                                                                                                                                                                                 |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Decoding   | `symphonia` 0.5.5 (2025-10-11; MPL-2.0; pure Rust), features `mp3`, `aac`, `isomp4`, `ogg`, `vorbis`, `wav`, `pcm`, `flac`, `mkv`                                 | 0.6.1 (2026-08-13) has a new API; move when the crate is ready for it. FFmpeg bindings: a large C dependency to build `/MT`                                                                                          |
| Opus       | `extern "C"` to `opus_decoder_create` / `opus_decode_float` / `opus_decoder_destroy`, resolved from the libopus inside LiveKit's `webrtc.lib` (feature `libopus`) | `audiopus` / `opus` crates build or link a second libopus (duplicate symbols next to `webrtc.lib`)                                                                                                                   |
| Resampling | `rubato` 5.0.1 (2026-10-01; MIT/Apache), the synchronous FFT resampler (`Fft`, `FixedSync::Input`, 1024-frame chunks)                                             | linear or cubic interpolation: audible aliasing and treble loss on 44.1 → 48 kHz music                                                                                                                               |
| Output     | An input of the app's one speaker (`yap-speaker`, §17): WASAPI shared mode, event driven at the 10 ms engine period, 20 ms ahead, 48 kHz stereo 16-bit            | `waveOut` (until 2026-11-08): an output beside the call's, so the echo canceller never heard it (§17), 31-character names, no default-device follow, 80 ms of buffers. `cpal`: no endpoint ids, no tap after the mix |

### 9.1 Decoder: symphonia

- Maintained (0.5.5 in 2025-10, 0.6.x through 2026-08) and pure Rust, so
  it compiles with the workspace's `+crt-static` and needs no C toolchain.
- The features are exactly what clips and the stream use:
  - `mp3`: most uploads.
  - `aac` + `isomp4`: m4a uploads and the stream's preferred `format=mp4`.
  - `ogg` + `vorbis`.
  - `mkv`: WebM, which demuxes the stream's `format=webm` Opus.
  - `wav` + `pcm`, and `flac`.
  - `adpcm` and `alac` are off.
- Symphonia has no Opus decoder. Its Ogg and MKV demuxers produce Opus
  packets (with `OpusHead` as extra data); `yap-audio` decodes them with
  libopus.

### 9.2 Opus path

- With `--features libopus` the crate declares the three libopus symbols
  and links nothing itself. The app turns the feature on. The final exe
  links `webrtc.lib` (through `yap-voice`), which carries libopus. The
  symbols were checked in `win-x64-release-webrtc-a545de4`.
- Mono and stereo only (mapping family 0). The pre-skip from `OpusHead` is
  dropped at the start. After a seek there is no 80 ms pre-roll.
- Without the feature, an Opus clip fails with a `Failed` event. An Opus
  stream counts as unsupported, which triggers the web's one format switch
  (`webm` → `mp4`) before `PLAYBACK_SRC_NOT_SUPPORTED`.

### 9.3 Resampler: rubato

- Every source is resampled to the 48 kHz mix rate. YouTube's AAC is 44.1
  kHz, so the stream path resamples most of the time.
- `Fft` is band-limited (no audible aliasing) and fixed-ratio, and rubato is
  the most used and tested resampler crate.
- Clips go through `process_all`, which trims the filter delay. The stream
  uses the chunked API and drops the delay itself, so positions line up.
- Source rates outside 8-192 kHz are refused (a header claiming 1 Hz or
  655 kHz would defeat the length cap). A decoded clip keeps at most 60 s and
  at most 60 s x 48 kHz source frames, and at most three clips decode at once.

### 9.4 Output path

- The mixer is an input of the app's one speaker (§17). The speaker's
  render thread pulls 10 ms from it each period (under the mixer's lock;
  the `yap-audio` thread changes voices between frames), sums it with the
  call's mix and the UI sounds, and plays it. In a call, that final mix is
  the echo canceller's far end.
- The `yap-audio` thread wakes the speaker when something becomes audible,
  and forwards the mixer's events every 20 ms while it plays. The speaker
  closes the device after 2 s with nothing to play and no call.
- The device is the voice settings' output, by endpoint id (§17). The
  player no longer has a device of its own.
- The player's mix is soft clipped as before: identity up to 0.8, then a
  tanh shoulder towards 0.98. The speaker's clip on the sum starts above
  that (0.977, towards 0.999), so a single input passes unchanged.

### 9.5 Loudness and tones (approximations)

- **Tones:** ported exactly.
  - The oscillator is a sine. Pop is a band-limited triangle built from odd
    harmonics up to Nyquist, as Web Audio builds it.
  - The gain starts at the volume, ramps exponentially to 0.001 at the
    duration, and the tone stops there.
  - Changing a tone's volume scales its envelope. The web instead holds the
    new volume flat.
- **Compressor static curve:** Chromium's `DynamicsCompressorKernel`.
  - The soft knee, with `k` found by the same 15-step bisection.
  - The constant ratio above `threshold + knee`.
  - The automatic makeup gain `(1 / curve(1.0))^0.6` that every
    `DynamicsCompressorNode` adds. That makeup is why the web's -1 dB
    "limiter" lets a 0 dBFS input out at about -0.4 dBFS. The port keeps
    that behaviour.
- **Envelope:** a peak detector that holds peaks and decays with the release
  time. The gain follows it down with the attack time. Chromium's 6 ms
  look-ahead, its adaptive release curve and its 32-frame updates are not
  reproduced, so transients can overshoot for a few samples. The soft clip
  catches them.
- **Preset values** are copied from `listen-together-audio.ts`.
  - The limiter is ratio 20, knee 0, attack 1 ms, release 100 ms, threshold
    = ceiling.
  - Off, every stage is identity, as `applySettings` does.
  - Parameter changes apply at once, with the makeup gain ramped over
    20 ms, instead of the web's 30 ms `setTargetAtTime`.

### 9.6 Stream fetching

- HTTP Range in aligned 512 KB chunks, with `reqwest` on a one-worker tokio
  runtime that the engine owns. Each read prefetches the next two chunks,
  and the last eight stay cached.
- Symphonia's mp4 reader scans every top-level atom of a seekable source.
  So a fragmented YouTube m4a costs about one request per 512 KB before the
  first sample plays; the prefetch pipelines those requests. UNVERIFIED
  against production tracks.
- Decoding runs on its own thread and keeps about 3 s of PCM queued ahead of
  the mixer. A seek clears the queue and issues a new Range.
- Recovery is `planListenTogetherPlaybackRecovery`, with its codes and copy:
  - 401/403: fail with `PLAYBACK_SESSION`.
  - 429/503: retry after Retry-After or 20 s × 2^n, capped at 60 s.
  - Network errors: retry after 3 s × 2^n.
  - Both retry 3 times.
  - Other ≥ 400, or media the decoder rejects: switch container once.
    Then fail with `PLAYBACK_UNAVAILABLE` / `PLAYBACK_SRC_NOT_SUPPORTED`.

### 9.7 Live radio (phase 5 N2)

- A radio entry plays the station's own `streamUrl`, as the web's second
  audio element does: one plain GET, no Range, no session, never sought.
- Only a URL that passes the room's `sanitizePublicHttpsUrl` is fetched
  (`yap_protocol::voice_app::sanitize_public_https_url`): https, no
  credentials, no local names, no private IPv4 or IPv6 literal.
- `stream/live.rs` reads the body chunk by chunk on the engine's network
  runtime into a 16-chunk channel; the decode thread reads it as a
  non-seekable `MediaSource`. A full PCM queue (paused) holds the socket
  back. 20 s without data counts as a network failure (the web's retry
  plan applies).
- The decoder hint comes from `Content-Type`; `audio/aac` and `audio/aacp`
  are ADTS. MP3, ADTS AAC and Ogg streams play; HLS (`.m3u8`) does not
  (`PLAYBACK_SRC_NOT_SUPPORTED`).

Sources:

- https://crates.io/crates/symphonia (0.5.5, 0.6.1)
- https://github.com/pdeljanov/Symphonia
- https://crates.io/crates/rubato (5.0.1)
- https://github.com/HEnquist/rubato
- https://webaudio.github.io/web-audio-api/#DynamicsCompressorNode
- Chromium `third_party/blink/renderer/platform/audio/dynamics_compressor_kernel.cc`
- https://www.rfc-editor.org/rfc/rfc7845 (OpusHead, pre-skip)
- https://learn.microsoft.com/windows/win32/api/mmeapi/ns-mmeapi-waveoutcapsw (`szPname[32]`)

## 10. Embeds and collectibles: video, web players, APNG (phase 4B)

Versions from the crates already in the lock or the registry cache on
2026-11-05.

| area                   | pick                                                                                                                                                             | why / rejected                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Chat video             | Qt Multimedia `MediaPlayer` + `VideoOutput` on Windows Media Foundation (the only backend in the static Qt 6.6.3)                                                | Plays mp4 (H.264/AAC) from a URL with seeking, including `/api/proxy-media`'s synthetic 206 ranges (live: TikTok, X, a direct mp4). No seekable `QIODevice` was needed: Media Foundation's own HTTP source does the ranges.                                                                                                                                                                                                     |
| WebM                   | `webm-iterable` 0.7.1 (2026-08-28, MIT, pure Rust) in the new `yap-webm` crate; libvpx's VP8/VP9 decoders and libyuv from LiveKit's `webrtc.lib`                 | Qt's backend opens sources through `IMFSourceResolver`, which has no webm byte-stream handler (the player waits in `LoadingMedia` forever). See §10.1.                                                                                                                                                                                                                                                                          |
| APNG and animated WebP | `image` 0.25.10 (`png`, `webp` features, no defaults) in the new `yap-anim` crate, behind `src/anim.rs` and the `AnimatedPicture` item (`cpp/animatedpicture.*`) | Qt's PNG reader shows only APNG's default image; decorations and effects are APNG. One decoder for both formats: frames composited, scaled to the drawn size (never up), premultiplied, delays clamped as browsers do. Decoded on the thread pool; a shared 96 MB cache keyed by URL and size; frames tick only while playing, on screen and with the app active. Qt's `qwebp` could animate WebP too, but one path is simpler. |
| Web players            | `webview2-com` =0.39.1 (`windows` 0.62, as velopack; was Tauri's =0.38.2 on `windows` 0.61 until the audit in §18)                                               | The web's iframes need a browser. Each player is a WebView2 controller in a child window of ours, placed every rendered frame over its QML item and clipped to the chat list; hidden under popups. The loader is linked statically from `WebView2LoaderStatic.lib` (no DLL; `webview2-com-sys`'s own binding would import `WebView2Loader.dll`).                                                                                |

### 10.1 WebM: demuxer and decoder

- **Demuxer: `webm-iterable`.** Discord's nameplates are VP9 with
  `AlphaMode` 1: the alpha is a second VP9 stream in each block group's
  `BlockAdditional` (id 1). `matroska-demuxer` 0.8.1 (2026-08) is the
  tidier API but its `Frame` drops `BlockAdditions`, so alpha would be
  lost. `webm-iterable` (600k downloads, maintained, MIT) is a full EBML
  tag reader: it hands over buffered `BlockGroup`s with their additions,
  and `SimpleBlock`/`Block` lacing. symphonia's mkv reader (already in
  yap-audio) only exposes audio codecs. The file is read whole from
  memory (at most 64 MB) and kept as compressed packets.
- **Decoder: the libvpx in `webrtc.lib`.** `win-x64-release-webrtc-a545de4`
  exports `vpx_codec_dec_init_ver`, `vpx_codec_decode`,
  `vpx_codec_get_frame`, `vpx_codec_vp8_dx`, `vpx_codec_vp9_dx` (and libyuv's
  `I420Scale`, `ScalePlane`, `ARGBScale`, `I420AlphaToARGB`) with C linkage
  (checked with `llvm-nm`). `yap-webm` declares them (feature `libvpx`, the
  structs mirror that libvpx's headers, decoder ABI 12) and links nothing:
  the exe already links `webrtc.lib` through yap-voice, so no second
  libvpx, no C toolchain, `/MT` as before. The crate's build script links
  `webrtc.lib` into its own tests only. `vpx`/`env-libvpx-sys` were not
  needed.
- **Output.** Opaque 4:2:0 goes out as I420 and is copied once into a
  `QVideoFrame` (`Format_YUV420P`), as `videohub.cpp` does for call video;
  4:4:4/4:2:2 is reduced to 4:2:0 first, VP9 RGB (`VPX_CS_SRGB`) and alpha
  streams come out as premultiplied BGRA (`Format_BGRA8888_Premultiplied`).
  Qt 6.6's `QSGVideoMaterial` only turns blending on below full opacity
  ("respect video formats with Alpha" is a TODO there), so an output that
  shows alpha sets `opacity: 0.99`.
- **Pacing and limits.** One thread per player: decode, wait for the
  frame's wall-clock time, hand it over; frames 250 ms late are decoded but
  not shown. Loops after the last frame's duration. Pictures larger than
  the box (1920 x 1080; 960 x 540 for decorations) are box-scaled by
  libyuv; streams above 4K are refused; 10/12-bit fails as unsupported.
  `WebmPlayer` pauses the thread when the output is off screen (outside
  the window or a clipping ancestor), the window is hidden or minimised,
  or (decorations) the app is not active. Public files are kept in a 32 MB
  byte cache so every row wearing one nameplate fetches it once.
- **Measured** (this PC, release build): the shop page at rest 1.7 % of one
  core, 219 MB private; with the Arctic Winter Frost plate (448x84 VP9 +
  alpha, 12 fps) animating on the detail stage 2.8 % and 230 MB.

### 10.2 Private video attachments

`/api/attachments/*` needs the session and Media Foundation's HTTP source
cannot send a header. `QMediaPlayer::setSourceDevice` with a Range-backed
`QIODevice` was built first and failed live: Qt's `MFStream` serves one
async read at a time and its `Seek` returns `S_FALSE` while a read is
pending, so with network-bound reads the MP4 source re-read offset 0 and
stopped after the first frame. The player now gets
`http://127.0.0.1:<port>/m/<128-bit token>/<file name>` from
`AuthMedia.localUrl`; `src/loopback.rs` relays each request (method,
`Range`) to the API with the bearer added there, for the API's private
media only, never following a redirect, and streams the status, type,
length and range back. Media Foundation does its own seeking. WebM
attachments go through WebmPlayer (which fetches with the session the same
way).

Frozen mp4s (fixed): Media Foundation showed three frames of any mp4,
the relay's and a public URL's alike, while its clock ran on (no
EndOfMedia). Qt 6.6's EVR presenter has three samples
(`PRESENTER_BUFFER_COUNT`), and a sample goes back to the pool only when
every `QVideoFrame` on it is gone. EmbedVideo's preview watched
`videoSink.videoFrameChanged` in QML, and each frame handed to JS stayed
alive until the next garbage collection, so the pool ran dry after three.
A `gc()` every 700 ms let it advance about three frames at a time. The
4B build (d115232d) froze the same way, both behind other windows and on
an uncovered monitor, so window position was not the cause. The preview
now waits for `videoSizeChanged` instead, which carries no frame (the MF
renderer clears the sink when a player goes, so a reloaded player sees
the size again).

### 10.3 Failure paths

| case                         | what happens                                                                                                                                                                                                                                                                                             | evidence                                                                                                                                          |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| No WebView2 runtime          | `YAP_FAKE_NO_WEBVIEW2=1` fails environment creation for this process only. First click: the poster stays with a "Player unavailable. Click to open in your browser." chip; the item is `Failed` for good. Second click: `Files.openExternal` (logged instead of opened under `YAP_LOG_EXTERNAL_OPEN=1`). | live: `[webembed] no WebView2 environment: YAP_FAKE_NO_WEBVIEW2=1`, then `[external] open www.youtube.com/watch`; no msedgewebview2 child started |
| Player frame renderer killed | The YouTube iframe's renderer exit arrives as `FRAME_RENDER_PROCESS_EXITED` (kind 3), now fatal too: back to the poster, nothing opens. A click makes a new player.                                                                                                                                      | live: killed renderer 9400 (child of our browser 21260, child of YapNative); poster; click played again                                           |
| Browser process killed       | `BROWSER_PROCESS_EXITED`: back to the poster, and the dead environment is forgotten so the next click creates a new one (before, it would have failed for good).                                                                                                                                         | live: killed browser 3068; poster; click played again                                                                                             |
| Broken mp4 attachment        | Media Foundation's "Unsupported media type": the web's unplayable state with Download video.                                                                                                                                                                                                             | live                                                                                                                                              |
| Broken direct mp4 link       | The server makes no embed for a URL that does not load (as on the web); a direct video that fails to play turns into DirectMediaFallback (4B).                                                                                                                                                           | live (no card); fallback tests only                                                                                                               |
| TikTok lookup fails          | 404 → the status card ("This post was deleted or made private."), 5xx/network → failed with Try again (the hydration TTLs).                                                                                                                                                                              | live (status card); store tests                                                                                                                   |
| 404 proxied media            | `EmbedVideo.failed` → the card's fallback, as above.                                                                                                                                                                                                                                                     | tests only                                                                                                                                        |
| Bad WebM                     | not WebM, truncated, no VP8/VP9 track, unsupported depth: the player fails and the item shows the unplayable state; a truncated file plays what it has.                                                                                                                                                  | crate tests                                                                                                                                       |

WebView2 details:

- Profile in `%LOCALAPPDATA%\YapNative\WebView2`; `--autoplay-policy=no-user-gesture-required`.
- The player is an iframe with the web's sandbox inside a host page served
  from the virtual host `player.yapnative.local` (written to the profile's
  `player` folder). A top-level YouTube embed fails with error 153 (no
  embedding page); framed, it plays.
- Top-level navigation is limited to that page and the desktop CSP's
  `frame-src` hosts; anything else, and every new window, opens in the
  system browser. Dev tools, host objects, web messages, script dialogs,
  zoom and the context menu are off.
- One player at a time: opening another closes the previous one (it shows
  its poster again). Created on click only; a hidden or pooled row closes
  its player. No WebView2 runtime, a failed creation or a crashed page
  opens the link in the browser instead.
- `YAP_EMBED_MUTED=1` mutes every player for test runs.

## 11. Call audio: RNNoise and the own audio path (`yap-voice`, batch N6a)

The web publishes the mic through the browser's processing (AEC, NS, AGC as
`getUserMedia` constraints), then RNNoise in an AudioWorklet
(`@sapphi-red/web-noise-suppressor` 0.3.5, Xiph's RNNoise built to WASM),
and plays each peer through a `StereoPannerNode` for spatial audio. libwebrtc's
platform audio device module (ADM) has neither hook: it mixes every remote
track into one playout and runs its APM between the mic and the encoder with
no callback after it. So the call runs its audio itself
(`crates/yap-voice/src/engine/pipeline.rs`); the ADM only lists devices.
Versions and dates from crates.io on 2026-11-06.

| area           | pick                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | runner-up / rejected                                                                                                                                                                                                                                                        |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| RNNoise        | `nnnoiseless` 0.5.2 (BSD-3-Clause; pure Rust; `default-features = false`, so only `easyfft` and `once_cell` come in), `DenoiseState` on 480-sample frames                                                                                                                                                                                                                                                                                                                                                                                                       | Xiph's C `rnnoise` through `cc` (`rnnoise-c` is deprecated in favour of nnnoiseless): a C build to keep `/MT` for no gain. RNNoise 0.2's larger model: the web's WASM carries the original model, which is the one nnnoiseless ports, so both clients suppress the same way |
| Echo, NS, AGC  | `libwebrtc::native::apm::AudioProcessingModule` (already in libwebrtc 0.3.55): WebRTC's own AEC3, NS, AGC2 and high-pass, one instance fed both the mic (`ProcessStream`) and what we play (`ProcessReverseStream`)                                                                                                                                                                                                                                                                                                                                             | Keep the ADM's internal APM and add RNNoise in an `AudioTransport` wrapper: the wrapper only sees the raw mic, before the APM, so RNNoise would run before echo cancellation (its non-linear gain breaks AEC's echo path model) and the web runs it after                   |
| Mic in         | WASAPI shared mode, event driven, 48 kHz mono 16-bit with `AUTOCONVERTPCM` (`windows` 0.61, as yap-capture's loopback), by the ADM's endpoint id or the default communications endpoint                                                                                                                                                                                                                                                                                                                                                                         | `cpal`: an extra layer with no endpoint-id API matching the ADM's ids; the ADM's own recording: no hook after its APM                                                                                                                                                       |
| Mic to encoder | `NativeAudioSource` (unbuffered, `capture_frame(..).now_or_never()`, as the share audio)                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | -                                                                                                                                                                                                                                                                           |
| Remote audio   | `NativeAudioStream::with_options` per remote audio track (48 kHz stereo, at most 20 queued frames) into our own mixer (`src/dsp/mix.rs`): per-track gain and the `StereoPannerNode` pan law (mono law for a track whose channels are equal, two-channel law for stereo), ramped over a frame; the queue primed at 40 ms and held at 30-60 ms against clock drift by resampling a frame from 477 or 483 samples (linear, 0.625 %), cut only past 120 ms; a look-ahead-free soft limiter on the sum (-3 dBFS, ratio 20, 1 ms / 100 ms, soft clip under -0.2 dBFS) | Per-track gain inside libwebrtc (`SetVolume`, what the ADM path does): scalar only, no pan. A custom audio decoder factory or mixer source: fixed inside webrtc-sys's factory, so it means forking webrtc-sys                                                               |
| Speaker out    | The app's one speaker (`yap-speaker`, §17): the call's mix is one input beside the soundboard, Listen Together and the UI sounds; WASAPI shared mode, event driven, 48 kHz stereo 16-bit, 20 ms ahead, MMCSS "Pro Audio"; the final frame goes to the APM's far end (a `Tap`), its delay hint the queued audio plus `GetStreamLatency` and a period, plus the mic's                                                                                                                                                                                             | The ADM's playout with our mix injected: its APM takes the far end from its own mixer before our frame exists, so the echo canceller would hear the unpanned mix                                                                                                            |

### 11.1 Boost past 100 % (2026-11-08)

The per-user volume and the per-user stream volume go to 400 %
(`MAX_PEER_VOLUME`, yap-voice and yap-cache), the output volume stays at
200 %, so a track's gain reaches 8x (`MAX_GAIN`). The web caps both at
200 % and plays them through a `GainNode` into its master
`DynamicsCompressorNode`. A plain 4x gain on a loud talker would sit on
the mix limiter all the time, and that limiter has no look-ahead: what its
1 ms attack lets past is bent by its soft clip, which is audible as
crackle on a boosted shout.

So a stream above unity gain gets its own limiter, `dsp::mix::BoostLimiter`,
after its gain and pan and before the sum:

| choice        | pick                                                                                                                                                                                                             | not picked                                                                                                                                                                   |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| where         | per stream, after gain and pan, stereo linked (one gain for both channels, so a pan survives it and a spatial seat keeps its place)                                                                              | on the sum: one boosted person would duck everyone else. Before the pan: the stereo law can add the two channels, so the panned result could still pass the ceiling          |
| detector      | sample peak of the louder channel; the need is the gain that brings it to the ceiling                                                                                                                            | RMS: lets peaks through. True peak (oversampled): the mix is 16-bit PCM straight to WASAPI at 48 kHz, with no resampler after it to create inter-sample overs worth the cost |
| look-ahead    | 1.5 ms (72 samples), read from the stream's own jitter queue (30-60 ms already waiting), so it adds no latency                                                                                                   | a delay line: 1.5 ms more latency, and a jump in delay whenever the gain crosses 100 %                                                                                       |
| attack        | the smallest need over the next 1.5 ms (a sliding minimum, van Herk / Gil-Werman), then the mean of the last 73 of those: the gain glides down over 1.5 ms and is exactly at the need when the peak plays        | a one-pole attack: overshoots on a hard onset (what the mix limiter's soft clip then bends)                                                                                  |
| hold, release | 5 ms hold (half a period at 100 Hz, so it does not ride a low voice's waveform), then a 60 ms one-pole release, run on the reduction's depth so it reaches unity exactly in `f32`                                | a release on the gain itself: in `f32` it stalls about 1e-4 under unity and never goes idle                                                                                  |
| ceiling       | -3 dBFS, the mix limiter's threshold: one boosted person reaches that limiter under its threshold and passes it untouched, so only the look-ahead limiter acts                                                   | -1 dBFS: the mix limiter (threshold -3 dBFS, ratio 20) would take it to -2.9 dBFS anyway, through its look-ahead-free attack and soft clip                                   |
| safety        | a clamp at the ceiling after the gain, which only acts when the look-ahead guessed wrong (the gain or pan moved between frames, a skewed frame); counted, `boosted=<frames>/<clamped samples>` in the stats line | -                                                                                                                                                                            |

- At or under unity, a stream never enters the limiter: its samples are
  the same bits as before the boost existed (a test compares them). A
  stream coming back down from a boost only lets go of what the limiter
  holds (hold, release, no new reduction) and returns to the plain path
  within about a second, however loud it is.
- Echo cancellation: the limiter runs inside the call's mix, before the
  speaker's bus, so the frame the render thread writes, and hands the
  APM's far end, is the limited one. The canceller models what was
  played, not the linear 4x (`engine::pipeline` test: a -0.8 dBFS noise
  talker at 4x plays with a -3.00 dBFS peak, nothing at the rails, and is
  cancelled by 52 dB).
- Several boosted people still sum past -3 dBFS; the mix limiter and the
  speaker's soft clip (§17) are unchanged above them.
- The ADM fallback (`YAP_ADM_AUDIO=1`) passes the gain to libwebrtc's
  `SetVolume` (0..10) and has no per-track limiter.
- Cost: PERFORMANCE.md "Boost to 400 %".

Why the far end matters: AEC removes what it is told was played. Here the
render thread hands the APM exactly the frame it writes to the speaker, so a
panned or re-gained mix is cancelled as played, and so is a soundboard clip
or a Listen Together track that shares the speaker (§17). The libwebrtc-feature test
`engine::pipeline::tests::the_echo_canceller_should_remove_what_the_speaker_played`
runs a far-end talker through the mixer and back into the mic 40 ms late at
half level: about 50 dB of it is removed; with echo cancellation off it
passes unchanged.

Costs (this machine): RNNoise takes 34 µs per 10 ms frame in a release build
(`cargo test --release -p yap-voice --no-default-features --lib -- dsp::denoise
--nocapture`; 1.2 ms in debug), about 0.3 % of one core; the call's stats line
(`[voice] stats ... audio[...]`) reports both per frame every 5 s, and
`PARITY_FEATURES.md` has the in-call process CPU with RNNoise on and off.

Escape hatch: `YAP_ADM_AUDIO=1` puts the call back on the ADM (its recording,
APM and mixed playout; no RNNoise, no panning). The path also falls back by
itself: when WASAPI refuses a present device three times in a row (it will
not open as 48 kHz 16-bit), the next call and the rest of the process use
the ADM, which opens devices in their own mix format. Not mid-call, since
the call's tracks were made for one path; the failing call shows its stall
banner instead. A missing device does not fall back (the ADM would find
none either). Krisp is not offered: the web
only knows its browser SDK and reports it unsupported, so the select lists
Off and RNNoise only, as the web's does.

## 12. Video engine: hardware share encode and camera backgrounds (batch N6b)

### 12.1 Hardware H.264 for the share

| piece         | pick                                                                                                                                                          | instead of                                                                       |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Encoder       | Media Foundation's hardware H.264 MFT (NVENC, AMF, Quick Sync), through `windows` 0.61 (`Win32_Media_MediaFoundation`), as the desktop app's `wmf_encoder.rs` | vendor SDKs (NVENC/AMF/oneVPL headers and DLLs), x264/OpenH264                   |
| Into the call | libwebrtc 0.3.55's pre-encoded pass-through: `NativeVideoSource::new_encoded` + `capture_encoded_frame`, sender on `VideoEncoderBackend::PreEncoded`          | a custom `VideoEncoderFactory` (webrtc-sys builds its own; none can be injected) |
| NV12          | libyuv's `I420ToNV12` from the prebuilt (`cpp/screen_i420.cpp`)                                                                                               | our own converter                                                                |

What the prebuilt has (`webrtc-sys` 0.3.52, `win-x64-release-webrtc-a545de4`,
`args.gn`: `rtc_use_h264 = true`, `ffmpeg_branding = "Chrome"`): software
VP8/VP9 (libvpx), AV1 (libaom/dav1d), OpenH264 and FFmpeg's H.264 decoder.
Its NVENC/NVDEC factories compile only on Linux (`build.rs` needs `cuda.h`
there), VA-API is commented out for Windows, and the encoder and decoder
factories are made inside `peer_connection_factory.cpp` with no hook. The
pass-through (`passthrough_video_encoder.cpp`) is single-layer, which is
what a share is (the camera's three simulcast layers stay on libvpx).

Why MF over the vendor SDKs: built into Windows (nothing to download or
ship), one code path for all three vendors, and the desktop app already
proved it against the same SFU and web peers. `YAP_SHARE_ENCODER=software`
keeps VP8 for comparisons; a failed or missing MFT falls back to VP8.

Hardware decode (performance pass 5): incoming H.264 decodes on Media
Foundation's H.264 MFT, on the GPU through a D3D11 device manager, in a
patched webrtc-sys (§13.1). libwebrtc's FFmpeg decoder is no longer linked
on Windows. VP8 and VP9 stay on libvpx.

Zero copy (performance pass 6): the window renders on a D3D11 device the
app makes (video support, multithread protected) and hands to Qt Quick
(`QQuickGraphicsDevice::fromDeviceAndContext`) and to the decoder
(`yap_mf_set_shared_device`). Each decoded picture is copied on the GPU into
an NV12 texture of a small per-stream pool and travels as a native WebRTC
buffer; `cpp/videohub.cpp` wraps it in a `QVideoFrame` whose
`QHwVideoBuffer` returns the texture, and `VideoOutput` samples it directly
(an R8 view for luma, an R8G8 view for chroma). No readback, I420 copy or
upload. `YAP_ZERO_COPY=0` keeps the copying path.

VP9 on the GPU (performance pass 7): the same decoder runs the VP9 Video
Extensions' MFT for VP9 profile 0 (and VP8 where the GPU has a DXVA profile
for it; NVIDIA has none), with libvpx as the fallback wrapper's software
decoder (patch `0005`, §13.1).

### 12.2 Person segmentation for backgrounds

| piece      | pick                                                                                                                                                                                                                    | instead of                                                                                        |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Model      | MediaPipe `selfie_segmenter.tflite` (the web's `/mediapipe` copy, 256x256, fp16), converted to ONNX by `tools/convert-selfie-segmenter.py` into `assets/models/selfie_segmenter.onnx` (427 KB)                          | PINTO's older selfie-segmentation ONNX; MediaPipe's C API                                         |
| Runtime    | ONNX Runtime 1.24.4, the DirectML build (`Microsoft.ML.OnnxRuntime.DirectML`), loaded at run time by `ort` 2.0.0-rc.12 (`load-dynamic`, `directml`)                                                                     | `tract` (pure Rust, CPU only: several times slower); MediaPipe C API (Bazel, no Windows GPU path) |
| GPU        | DirectML, the system's (`System32\DirectML.dll`, 1.15.6 here); ONNX Runtime's CPU provider where it is missing or too old                                                                                               | shipping `DirectML.dll` 1.15.4 (17.7 MB)                                                          |
| Compositor | Our own (`yap_voice::background`): libyuv `ScalePlane` for the blur's shrink/grow, running-sum box blur, fixed-point mask scaling, run-skipping blend; image backgrounds decoded by `image` 0.25 (png, jpeg, webp, gif) | a D3D11 compute pass                                                                              |

The converter maps the model's 11 op types (fp16 weights folded, NHWC to
NCHW, TF `SAME` padding as explicit pads, half-pixel bilinear resizes, the
custom `Convolution2DTransposeBias` as `ConvTranspose`) and was checked two
ways: against a NumPy interpreter of the TFLite graph (max difference
1.2e-5 on a photo, 1.6e-9 on noise) and on a Wikimedia Commons portrait
(the mask covers the person). Input is RGB in 0..1, as MediaPipe feeds it.

`ort` rc.12 because rc.13+ drive ONNX Runtime 1.28+ and 1.24.4 is the last
release with a DirectML build. Loaded, not linked: the exe does not grow,
starts without the DLL, and the settings row says when backgrounds are
unavailable. Windows ships a Windows ML `onnxruntime.dll` in System32, so
ours is loaded by full path.

Costs (RTX 3080): a segmentation is 0.87 ms on DirectML in isolation
(`cargo test --release -p yap-segment`, `YAP_SEGMENTATION_DIR` set), 3.7 ms
on the CPU provider; in the app 3.5-4.2 ms with smoothing and the feather,
12 times a second on its own thread. Per 720p frame: input 0.09 ms, blur
0.9-1.1 ms, blend 0.9-1.1 ms. See PERFORMANCE.md.

**The CPU provider is the default since performance pass 4**
(`YAP_SEGMENT_GPU=1` runs DirectML; the CPU provider runs without its
memory arena). In the app, with a blurred 720p30 camera in a call:

| provider | process CPU over 25 s | private while it runs | kept after the camera stops and the call ends |
| -------- | --------------------- | --------------------- | --------------------------------------------- |
| DirectML | 9.70 s, 9.67 s        | +330 MB (498 MB)      | +93 MB                                        |
| CPU      | 8.97 s                | +100 MB (268 MB)      | +20 MB                                        |

DirectML's inference thread is cheaper (172-188 ms against 891 ms per
25 s), but its D3D12 submission and driver threads cost more than that,
and its device, DirectML operators and driver allocations live in
onnxruntime's and the driver's own heaps after the session is dropped.

### 12.3 Licences and size

| file                                      | licence                                                                                 | size    |
| ----------------------------------------- | --------------------------------------------------------------------------------------- | ------- |
| `selfie_segmenter.onnx` (from MediaPipe)  | Apache-2.0 (MediaPipe models; the model card's terms)                                   | 427 KB  |
| `onnxruntime.dll` 1.24.4 (DirectML build) | MIT, with its `ThirdPartyNotices.txt` (shipped beside it)                               | 16.5 MB |
| DirectML                                  | the system's; not shipped (its redistributable is under the Microsoft DirectML licence) | 0       |
| `ort` 2.0.0-rc.12                         | MIT OR Apache-2.0                                                                       | in exe  |

`tools/package.ps1` stages `segmentation\` (runtime, model, the runtime's
licence and notices; Microsoft's signature checked and kept, excluded from
re-signing). `build.ps1` copies the runtime and model beside a dev build.
`YAP_ORT_DIR` points both at the unpacked NuGet package
(`<scratch>/onnxruntime/dml-1.24.4` here).

Size: the exe grew 1.1 MiB (the MF encoder, `ort`'s loader, the JPEG and GIF
decoders); Setup.exe for the same build grew 6.7 MB with the segmentation
folder (45.8 to 52.5 MB, `-NoCaptureHook` both ways); installed, the folder
is 17.3 MB.

### 12.4 Left for later

- Zero-copy decode: done in performance pass 6 (§12.1).
- A zero-copy share: WGC texture to a D3D11 video processor (BGRA to NV12)
  to the MFT on the same device, as the desktop app does; today the frame
  is read back once for the preview and converted on the CPU.
- Animated (GIF/WebP) background images play their first frame only; the
  web animates them. AVIF is not uploaded from here (the `image` crate's
  AVIF decoder needs dav1d, a C library); one uploaded on the web fails to
  decode and the row says it could not be applied, as the web does.

## 13. Patched dependencies and the custom Qt (2026-11-07)

From performance pass 3 (docs/PERFORMANCE.md). Each patch lives in
`native/patches/<dependency>/` with the script that applies it.
Nothing outside `native/` changes, and no existing toolchain is
overwritten.

### 13.1 Patched crates

`patches/vendor.ps1` copies each crate from the cargo registry into
`rust/vendor/<crate>` and applies `patches/<crate>/*.patch`.

- `rust/vendor` is ignored by git; `rust/Cargo.toml` builds the copies
  through `[patch.crates-io]`.
- `build.ps1` runs the script before CMake. Its stamp (the version plus the
  patch hashes) makes it a no-op when the copies are current.
- On a fresh checkout, run it once before `cargo test` or `cargo clippy`.
- After changing a patch, run it with `-Force`.

| crate                   | patch                                  | why                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ----------------------- | -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `qt-build-utils` 0.10.0 | `0001-qmlcachegen-bytecode-only.patch` | Reads `QT_BUILD_UTILS_QMLCACHEGEN_BYTECODE_ONLY`, a `;`-separated list of a file path suffix, a folder ending in `/`, or `*`. qmlcachegen then compiles those files with `--only-bytecode`. cxx-qt-build 0.10 has no option for it. Pass 3 set a list of cold screens (QML 8.6 to 6.7 MiB). Since pass 5, `rust/build.rs` sets `*`, every file: 2.8 MiB less, with no measured cost in scroll or animated idle.                                                                                                                                                                                                                                                                                                             |
| `qt-build-utils` 0.10.0 | `0002-discard-qml-contents.patch`      | `QT_BUILD_UTILS_DISCARD_QML_CONTENTS=1` (set by `rust/build.rs`) does what Qt 6.8's `DISCARD_QML_CONTENTS` does for `qt_add_qml_module`. The `.qml` files go into the resources empty: rcc gets a second qrc whose `.qml` entries point at one empty file, under the same aliases and file name. qmlcachegen still reads the real qrc. Every file is compiled, so the engine never reads the text. 0.7 MiB.                                                                                                                                                                                                                                                                                                                 |
| `qt-build-utils` 0.10.0 | `0003-parallel-qmlcachegen.patch`      | One qmlcachegen for the module instead of one per file (each one ran `qmlcachegen --help` first), and the files compiled side by side on every core, results kept in the files' order: the same output. CI time only (docs/INSTALLER.md, "CI").                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `webrtc-sys` 0.3.52     | `0001-no-av1-encoder.patch`            | Takes the `LibaomAv1EncoderTemplateAdapter` out of the encoder factory, so the linker drops libaom's AV1 encoder: libwebrtc 16.2 to 12.8 MiB of the exe. The client never sends AV1: VP8 for the camera and software shares (yap-voice puts VP8 first), H.264 for hardware ones. (AV1 decoding went in pass 5: `0003`.)                                                                                                                                                                                                                                                                                                                                                                                                     |
| `webrtc-sys` 0.3.52     | `0002-mf-h264-decoder.patch`           | `src/mf_h264_decoder.cpp` (new): a `VideoDecoder` on Media Foundation's H.264 MFT (`CLSID_MSH264DecoderMFT`). It decodes on the GPU through a D3D11 device manager (one device per stream, released with it) and reads each NV12 picture back with `IMF2DBuffer::Lock2D`, then libyuv converts it to I420. It runs in low-latency mode. Without a usable GPU, or with `YAP_H264_DECODER=software`, the same MFT decodes in software. On Windows the decoder factory uses it for H.264, and libwebrtc's FFmpeg decoder is no longer referenced, so the linker drops it (3.1 MiB). H.264 is offered only when the MFT can be made, which Windows N without the Media Feature Pack cannot do. Numbers: PERFORMANCE.md, pass 5. |
| `webrtc-sys` 0.3.52     | `0003-no-av1-decoder.patch`            | AV1 is no longer offered or decoded, so dav1d leaves the exe (1.8 MiB). No Yap client sends AV1: the native one sends VP8 and H.264, and the web and mobile ones keep their stack's default, VP8. With Cloudflare's SFU the publisher alone picks the codec, so an AV1 publisher would show nothing here. To restore it, remove the patch.                                                                                                                                                                                                                                                                                                                                                                                  |

| `webrtc-sys` 0.3.52 | `0004-mf-h264-zero-copy.patch` | Performance pass 6. `yap_mf_set_shared_device(ID3D11Device*)` registers the UI's device; a decoder configured afterwards decodes on it, copies each picture out of the decoder's texture array (`CopySubresourceRegion`) into an NV12 texture from a per-stream pool, and delivers it as a `kNative` buffer (`D3D11Nv12Buffer`, `storage_representation()` = `yap-d3d11-nv12`; `ToI420()` reads back through a staging texture for a consumer that needs pixels). `yap_d3d11_frame(const livekit_ffi::VideoFrameBuffer*, YapD3D11Frame*)` gives the UI the texture and a reference. Without a registered device it behaves as `0002`. Numbers: PERFORMANCE.md, pass 6. |
| `webrtc-sys` 0.3.52 | `0005-mf-vpx-decoder.patch` | Performance pass 7. VP9 profile 0, and VP8, on the GPU through Media Foundation: the decoder of `0002`/`0004` takes a codec, and for VP8/VP9 creates the MFT of the VP9 Video Extensions (`MFTEnumEx` for `MFVideoFormat_VP90`/`VP80`; Windows ships the package on most installs, and Microsoft Store updates it). GPU only: `MfVpxHardwareAvailable` probes once per codec whether the MFT takes the codec with a D3D11 device manager (DXVA for it), and the factory wraps the decoder in libwebrtc's `CreateVideoDecoderSoftwareFallbackWrapper` with libvpx, which also takes over on any decode error (`WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE`). Output is the zero-copy `D3D11Nv12Buffer` of `0004`. On this machine (RTX 3080) VP9 decodes on the GPU; VP8 has no DXVA profile on NVIDIA and stays on libvpx (Intel GPUs decode VP8). `YAP_VPX_DECODER=software` keeps libvpx. Numbers: PERFORMANCE.md, pass 7. |
| `libwebrtc` 0.3.55 | `0001-native-buffer-sys-ptr.patch` | Performance pass 6. `NativeBuffer::sys_ptr()` (Windows): the C++ `livekit_ffi::VideoFrameBuffer` behind a native frame, which the crate keeps sealed. `yap-voice` passes it to the video output, which asks webrtc-sys for the texture (`0004`). |
| `libwebrtc` 0.3.55 | `0002-offer-options-undefined.patch` | Voice fix (2026-11-07). `OfferOptions` maps `offer_to_receive_audio`/`_video` `false` to `kUndefined` (-1), as a browser's `createOffer()` without options, not 0. The crate sent `false as i32` = 0, and under Unified Plan libwebrtc then takes the receive direction off every receiving transceiver of that kind (`HandleLegacyOfferOptions`). So every offer we made (camera or share push, a republish, a pull's close) turned each pulled track inactive: the SFU answered inactive and stopped sending, and inbound audio and watched video were gone for the rest of the call. Test: `our_offers_should_keep_pulled_audio_flowing` in `engine/webrtc.rs`. |

A patch is a plain unified diff (`--- a/<path>`, `+++ b/<path>`) without a
`diff --git` header: `git apply` run inside this repository silently skips
the files of a git-style patch. Pass 6's first `0004` was one and applied
nothing.

Two webrtc-sys candidates were tried and dropped:

- the VP9 encoder: other objects still link it, so removing it saved only
  55 KB;
- OpenH264: hardware shares need some factory to advertise H.264.

### 13.2 The custom static Qt

`patches/qt/build-qt.ps1` builds Qt 6.6.3 from the qt-everywhere source
zip into `<scratch>/qt-custom/6.6.3-yap`.

- **Modules:** qtbase, qtshadertools, qtdeclarative, qtsvg, qtimageformats
  and qtmultimedia.
- **Toolchain:** Visual Studio 2022 (the MSVC the stock static Qt was built
  with), with `-static -static-runtime -release -ltcg`.
- **Cost:** about 2 h on this machine. It needs 32 GB of build tree and
  0.5 GB of extracted source while it builds, and installs to 8.4 GB (LTCG
  libraries). `-Clean` removes the intermediates after the install.
- **Who uses it:** nothing since performance pass 5, which moved to 6.11.3
  (§13.3). `build-qt.ps1 -Version 6.6.3` still builds it.

What changes from the stock static Qt (`<downloads>/qt/src/6.6.3`):

| option or patch                                                             | effect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-optimize-size`                                                            | `/O1` for Qt itself. Qt and the other libraries in the exe went from 29.2 to 21.8 MiB.                                                                                                                                                                                                                                                                                                                                                                                                               |
| `-no-opengl -no-feature-vulkan`                                             | Qt Quick runs on D3D11 only. The app never chose another backend (`QSG_RHI_BACKEND` no longer offers OpenGL).                                                                                                                                                                                                                                                                                                                                                                                        |
| `-no-freetype -no-widgets -no-dbus -no-feature-sql -no-openssl -schannel`   | Not used here: no QtWidgets, and Schannel is the TLS backend.                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `-no-feature-ffmpeg`                                                        | Qt Multimedia's Media Foundation backend only (the one the app loads).                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `0001-release-pooled-batches-and-unused-glyph-caches.patch` (qtdeclarative) | `QQuickWindow::releaseResources()` on a shown window also frees the batch renderer's pooled batches with their buffers (`Renderer::releaseCachedResources`), and the glyph caches of font engines no text uses (the threaded render loop's release). Without it, a dialog opened once kept its batches and glyph atlases for the window's life. This was the Settings memory. The app asks for a release after a popup is let go (`DesktopShell.releaseGraphicsSoon`) and before hiding to the tray. |

Checked on this build:

- the shell, Settings, the profile editor, the emoji and GIF pickers, the
  menus and the tooltips draw as before, with no QML warnings;
- GIFs animate;
- a hide to the tray and restore works.

Checked in performance pass 4, A/B against the stock Qt with Rust at
opt-level 3 (`<scratch>/yapn/rel-base4`), in calls between the two test
accounts: voice with RNNoise, the web peer's camera on our screen, our
camera both ways with no background and with the blur. No regression; the
numbers are in PERFORMANCE.md, pass 4. Not checked yet: a screen share,
WebView2 embeds and the media player.

### 13.3 Qt 6.11.3, the default (performance pass 5)

Pass 4 built it. Pass 5 adopted it, trimmed its configure and made it
`build.ps1`'s default. The numbers are in PERFORMANCE.md, pass 5.

`pwsh patches/qt/build-qt.ps1` builds it from
`<scratch>/qt-dl/qt-everywhere-src-6.11.3.zip` into
`<scratch>/qt-custom-6.11/6.11.3-yap`.

- **Cost:** about 1 h with Visual Studio 2022 here. It needs 37 GB of build
  tree and 0.6 GB of source while it builds, and installs to 8.2 GB.
- **A fresh CMake cache each run.** CMake keeps every `FEATURE_*` from an
  earlier configure, so a feature taken off the list would otherwise stay
  off.
- **The app links with the same MSVC.** `build.ps1` picks Visual Studio
  2022, because Qt's libraries are LTCG and so is `cpp/` (`/GL`). The link
  fails with C1047 if two compiler versions meet.

What differs from 6.6.3-yap (§13.2), besides the version:

| option or patch                                                    | effect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `-no-feature-freetype`                                             | 6.11 builds FreeType despite `-no-freetype`. The app draws with DirectWrite. 0.5 MiB.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `-no-feature-timezone_locale`                                      | CLDR time-zone display names, 3.75 MiB. The app formats dates in Rust and shows no zone names.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| other `-no-feature-*` (listed in the script)                       | Unused: calendars, PDF, ODF, Markdown, shared memory, system semaphores, SOCKS5, DNS lookups, the Network List Manager monitor, PPM and XBM, TableView/TreeView, sprites, particles, XMLHttpRequest, WorkerScript, and the Controls styles other than Basic. XPM stays: drag and drop needs it, and so does `DropArea`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `6.11.3/0001-release-unused-glyph-caches.patch`                    | The glyph-cache half of the 6.6.3 patch, in the threaded **and** basic render loops (the app runs the basic loop since pass 5). The batch half is upstream: recycled batches give their buffers to pools capped at 2 MB, which `releaseCachedResources()` empties.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `6.11.3/0002-hlsl-only-shaders.patch` with `-DQT_YAP_HLSL_ONLY=ON` | Qt's own `.qsb` shaders carry SPIR-V and DXBC only, with no GLSL or MSL. The app runs on Direct3D only.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `6.11.3/0003-timezone-without-locale-names.patch`                  | Windows' time-zone backend would not build without `timezone_locale`. It now falls back to Windows' own zone names, as before 6.8.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `6.11.3/0004-d3d11-shared-resource-batches.patch`                  | Performance pass 6. `QRhiD3D11::setShaderResources` deep-copied the bound SRB's batched bindings (six stages of `QRhiBatchedBindings`) into the command buffer at every bind, and Qt Quick binds per draw (dynamic offsets): 9-11 % of the GUI thread's time in the animated idle. The batches are now an immutable `std::shared_ptr` snapshot, rebuilt only when the bindings change, and the command buffer keeps a reference. Built into `<scratch>/qt-custom-6.11p6/6.11.3-yap`, so the shared install other worktrees build with was not touched.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `6.11.3/0005-directwrite-enhanced-contrast.patch` (retired)        | UI pass 11. Applied DirectWrite's enhanced-contrast curve `c (k + 1) / (c k + 1)` to the glyph coverage in the font engine, one curve for every text colour. Built into `<scratch>/qt-custom-6.11p7` (the default until the text pass). The text pass removed it: Chromium's correction depends on the text colour (light text on dark is boosted, dark text on light is not), so one colour-blind curve left light-on-dark 9 % thin and dark-on-light 9 % heavy at once. `0006` replaces it, and `QT_DWRITE_ENHANCED_CONTRAST` is gone.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `6.11.3/0006-quick-text-preblend.patch`                            | Text pass. Qt Quick's subpixel text (`Format_A32`, `24bittextmask.frag`) blended DirectWrite's raw ClearType coverage as is. Skia first runs it through a table per text colour (`SkTMaskGamma_build_correcting_lut`: the coverage that, blended plainly, looks like a blend in linear light with a contrast boost; the colour canonicalised to 3 bits a channel, the ground guessed as its inverse). The shader now computes that table per fragment from the colour: `QSG_TEXT_CONTRAST` (default 1.0) and `QSG_TEXT_GAMMA` (default 0, the sRGB curve) are Skia's two inputs, `QSG_TEXT_PREBLEND=0` restores the raw blend. Fitted against Edge 157 glyph by glyph (14 blocks, both themes), contrast 1.0 with sRGB beat every exponent gamma and DirectWrite's own 0.5 / 1.8. Where subpixel coverage would fringe, it falls back to grayscale (the texture's mean coverage, with the colour's luminance), as Chromium does: into a translucent target (the renderer's clear colour has alpha: `layer.enabled`, `ShaderEffectSource`), under a scale or rotation (the A32 material sets `RequiresFullMatrixExceptTranslate`, so such text is unbatched and its matrix is seen; translate-only text still merges), and everywhere with `QSG_TEXT_GRAYSCALE=1` (`main.cpp` sets it when ClearType is off). Built with `0001`-`0004` into `<scratch>/qt-custom-6.11p8/6.11.3-yap`, `build.ps1`'s default since the text pass. |

Not a patch: the swapchain model (resize pass, 2026-11-08).

- `cpp/main.cpp` sets Qt's own `QT_D3D_NO_FLIP=1`, unless it is already set.
- With it, Qt Quick presents through a blt-model D3D11 swapchain (`DXGI_SWAP_EFFECT_DISCARD`).
- Qt's default is a flip-model one (`FLIP_DISCARD`).
- Under the flip model, a resize trailed the window's edge by two or more steps, over uncovered strips.
- It also stalled 1 s in the frame-latency wait during a move-and-resize.
- The numbers are in PERFORMANCE.md, resize pass.
- `QT_D3D_NO_FLIP=0` brings back the flip model.

Checked on this build:

- the shell, Settings, menus and the profile card draw as on 6.6.3, with no
  QML warnings;
- the shadows match (`RectangularShadow`);
- GIFs animate;
- a voice call between two native clients, one sharing at 1440p60 (and
  1080p60) through NVENC, the other watching;
- clippy and the workspace tests (PERFORMANCE.md, pass 5).

**CI (not done; written down in performance pass 6).** Nothing was
uploaded and no secret changed. `.github/workflows/native-release.yml`
still downloads `qt-6.6.3-static-msvc-x64.zip` from the
`native-build-assets` release, and that Qt no longer builds the app. What
the move needs:

1. **The archive.** Pack the installed Qt, `<scratch>/qt-custom-6.11/6.11.3-yap`
   (8.2 GB: `lib/` 6.6 GB of LTCG libraries, `plugins/` 1.2 GB, `qml/`,
   `bin/`, `include/`, `mkspecs/`, `metatypes/`, `modules/`). With pass 6's
   `0004` adopted, pack the tree built with it instead (built here into
   `<scratch>/qt-custom-6.11p6/6.11.3-yap`).
   - **Use 7z (LZMA2), not zip.** Measured on `Qt6Gui.lib` (406 MB):
     deflate keeps 26 %, which puts the whole tree at ~2.1 GB, over
     GitHub's 2 GB limit per release asset; LZMA2 (`7z a -mx=7`) keeps
     10 %, ~0.9 GB for the tree.
   - **Keep the private headers** (`include/*/6.11.3/*/private`) and the
     `lib/cmake/Qt6*Private` packages: since pass 6, `CMakeLists.txt` needs
     `GuiPrivate` and `MultimediaPrivate` (`cpp/videohub.cpp`).
   - `doc/` and `sbom/` (2.4 MB) can go.
   - Upload as `qt-6.11.3-yap-static-msvc-x64.7z` to `native-build-assets`.
2. **The workflow's Qt step:**
   - its name, and the self-hosted message ("a static Qt 6.11.3");
   - `gh release download ... --pattern "qt-6.11.3-yap-static-msvc-x64.7z"`;
   - `7z x "$env:RUNNER_TEMP\qt-6.11.3-yap-static-msvc-x64.7z" -o"$dir"`
     instead of `Expand-Archive` (7-Zip is on the hosted Windows image);
   - the check for `bin/qmake.exe`, and `YAP_QT_DIR` pointing at the
     folder.
   - Unpack under `RUNNER_TEMP`: unpacked Qt is 8.2 GB, and the Rust and
     CMake build dirs take several GB more.
3. **The toolset.** Qt's libraries are LTCG objects from MSVC 14.44
   (Visual Studio 2022 17.14), and so is `cpp/` (`/GL`): a link with
   another compiler version fails with C1047. `build.ps1` already picks
   Visual Studio 2022 and refuses to run without it. `windows-latest`
   (Windows Server 2025) has 17.14 today; pinning the image
   (`windows-2025`) keeps a later image change from breaking the link, and
   a self-hosted runner needs 2022 with the x64 C++ tools installed.
4. **The patched crates** need nothing in the workflow: `build.ps1` runs
   `patches/vendor.ps1`, which since pass 6 vendors three crates
   (`qt-build-utils`, `webrtc-sys`, `libwebrtc`) with `git apply` and fetches
   any that are not in the cargo registry yet (network on a cold cache).
5. **The Rust cache** (`swatinem/rust-cache`) keys on `Cargo.lock`, which
   pass 6 changes (`libwebrtc` now builds from `rust/vendor`), so the first
   run after the move rebuilds everything (~40 min with fat LTO here).

## 14. Compatibility: GPU safe mode, device loss, Windows N (2026-11-08)

`cpp/compat.*` and `cpp/mediafoundation.*`, the patch
`patches/webrtc-sys/0006-mf-optional-device-loss.patch`, and the notices in
`rust/qml/screens/Shell/CompatNoticeCard.qml`.

### 14.1 Startup probes

Before `QGuiApplication`, in the first instance only (`Compat::prepare`):

- **The adapter, once in the log**: `[gpu] adapter <name> (<vendor>:<device>:<subsys>:<rev> driver a.b.c.d), feature level 12_1`.
  The driver version is DXGI's UMD version (`CheckInterfaceSupport(IDXGIDevice)`).
- **The feature level**: `D3D11CreateDevice` on the default adapter with
  levels 12_1 down to 10_0 and no device kept. Qt's shaders are SM 5, so
  below 11_0 (or with no hardware D3D11 device) Qt would get a device and
  then fail every pipeline: the run goes to safe mode instead. The probe
  loads the driver (103-112 ms here), so its result is kept in
  `gpu-compat.json` (`probedAdapter`, `featureLevel`) and probed again only
  when the adapter identity changes; a start then spends 10-16 ms on the
  DXGI identity read. First frame over three starts: 605, 654, 672 ms
  before, 583, 594, 626 ms with the cached level.
- **Media Foundation** (§14.4).

### 14.2 GPU safe mode

Safe mode sets, unless already set: `QSG_RHI_PREFER_SOFTWARE_RENDERER=1`
(Qt Quick on WARP), `YAP_ZERO_COPY=0` (no shared device),
`YAP_H264_DECODER=software`, `YAP_VPX_DECODER=software` and
`YAP_SHARE_ENCODER=software`. Camera backgrounds (DirectML) are not
touched. `VideoHub::shareDevice` also stays off whenever the software
renderer is preferred: our own hardware device would override Qt's choice.

When it is on, and why:

| trigger                                                          | kept                  | ends                                                            |
| ---------------------------------------------------------------- | --------------------- | --------------------------------------------------------------- |
| feature level below 11_0, or no hardware D3D11 device            | decided per run       | a better GPU or driver                                          |
| `sceneGraphError` (the QRhi could not be made)                   | saved, restart        | adapter or driver change, `--no-gpu-safe-mode`, notice          |
| a lost device that could not be replaced, or 3 losses in a run   | saved, restart        | the same                                                        |
| two starts in a row ended before their first frame (crash guard) | saved                 | the same                                                        |
| a scene graph error while already in safe mode                   | saved, quits (code 3) | the same; the next start shows the reason                       |
| Settings > Desktop app > Hardware acceleration off               | saved                 | only that switch (or `--no-gpu-safe-mode`); not a driver change |
| `--gpu-safe-mode`, `YAP_GPU_SAFE_MODE=1`                         | this run              | -                                                               |
| `QSG_RHI_PREFER_SOFTWARE_RENDERER` set by the user               | this run              | -                                                               |

The saved state is `gpu-compat.json` in the profile's data folder
(`safeMode`, `user`, `reason`, `adapter`, `since`, `noticeSeen`,
`startsWithoutFrame`, `mediaNoticeSeen`, `probedAdapter`, `featureLevel`).
A safe mode the app saved remembers the adapter identity (ids and driver
version) it was saved on; a start on a different identity clears it and
tries the GPU again (the crash guard still covers that start). One saved
from Settings (`user`) stays until the switch is turned back on. The
notice card says why (once per saved safe mode; Settings saved ones show
none) and offers "Use the graphics card next time"; after it is dismissed,
**Settings > Desktop app > Hardware acceleration** is the way back. That
switch (the web's row, label and help text) shows the next start's choice:
off saves safe mode, on clears any saved one. The web restarts the app on
the spot; here the change applies at the next start, and the row says
"Restart Yap Native to apply this." while it differs from how the run
draws.

**Restart.** Without a `sceneGraphError` handler Qt shows a message box and
`qFatal`s. Ours logs, saves safe mode, starts the exe again and quits
through the event loop (so `aboutToQuit` leaves a call and commits the
cache), once per process. The new command line keeps the switches and
`--profile <name>`, drops `--autostart` (so the window shows, never only
the tray) and every link or other plain argument (a `yap-native://` link
was handled once), and adds `--yap-relaunched-from=<pid>`. The new process
waits for that pid (at most 60 s) before taking the instance lock. A
failure while already in safe mode saves that reason and quits (exit code
3), so the next start says why; the test switches are removed from the new
process's environment.

**Crash guard.** `prepare` writes `startsWithoutFrame = n + 1`; the first
`frameSwapped`, a clean quit and an early return from `main` (QML failed to
load: not a GPU death) write 0. A start into the tray (start minimized)
puts the count back at once and arms it again when the window is first
shown, so autostart runs that never show the window do not count.

### 14.3 Device loss

On `DXGI_ERROR_DEVICE_REMOVED` (a failed Present or `beginFrame`), Qt's
basic render loop releases the swapchain and the QRhi and makes a new QRhi
at the next frame, from the window's `QQuickGraphicsDevice`. With zero copy
that is our device, now dead. `sceneGraphInvalidated` (direct connection,
inside that frame, before the QRhi is gone) checks
`GetDeviceRemovedReason` on our shared device and `QRhi::isDeviceLost()`
(Qt's own device, without zero copy), and counts the loss on the GUI
thread (the threaded loop invalidates on its render thread, so it posts
there). If the shared device is removed,
`VideoHub::replaceDevice` makes a new one (same flags), gives it to the
window with `setGraphicsDevice`, registers it with the decoders
(`yap_mf_set_shared_device`), drops every feed's current frame and asks
for a frame. The old device and context are kept (frames and Qt's teardown
may still hold them). Without zero copy Qt makes its own new device and
nothing else is needed. A frame wrapper only hands Qt its texture when the
QRhi's device is the one the texture was made on, so a frame from before
the loss is never sampled on the new device.

The decoders (patch 0006) keep the shared device's generation; at the next
`Decode` a decoder whose device was removed, or whose shared device was
replaced, configures itself again (new MFT, device manager and texture
pool) and returns an error for that frame, so libwebrtc asks for a key
frame. The threaded render loop (`QSG_RENDER_LOOP=threaded`) is not
recovered in place: it restarts in safe mode.

### 14.4 Media Foundation optional (Windows N/KN)

The exe imported `mfplat`, `mf`, `mfreadwrite`, `evr`, `dxva2` and `msdmo`
(Qt Multimedia's static windows backend, the H.264 MFT decoder and
encoder, libwebrtc's DMO audio), so on N/KN without the Media Feature Pack
it would not start. All six are `/DELAYLOAD` (`CMakeLists.txt`, with
`delayimp`). `mediafoundation::probe()` maps `mfplat`, `mf`, `mfreadwrite`
and `evr` from System32 (`LoadLibraryExW`, as data); if one is missing:

- the delay-load notify hook resolves those DLLs to the exe and every
  import to a stub returning `E_NOTIMPL` (all 31 imports return an
  HRESULT; one stub fits them on x64), and the failure hook does the same
  for anything unforeseen, so no call raises 0xC06D007E;
- `yap_mf_set_available(false)`: H.264 is not offered (calls use VP8/VP9
  on libvpx), no MF decoder is configured;
- `YAP_SHARE_ENCODER=software`: shares encode VP8;
- the camera list is empty (Qt captures through Media Foundation); the
  test pattern still works;
- QML (`Compat.mediaPlayback`): no mp4 `MediaPlayer` is made (chat videos
  show "Playing this video needs the Media Feature Pack" once started; GIF
  picker loops, clips, nameplates and profile videos rest on their stills;
  audio previews are not playable; video stickers are skipped). WebM plays
  (libvpx), call video draws (VideoOutput needs no MF: Qt's windows
  integration still loads, and its `MFStartup` gets the stub);
- a one-time notice card, "Video playback needs the Media Feature Pack".

Qt Multimedia's core library loads Media Foundation itself with
`QSystemLibrary` (camera and audio-format helpers) and copes with it being
absent; only the static windows plugin imports it, and that is covered by
the delay-load. `YAP_TEST_NO_MF=1` takes the same path on a PC that has
Media Foundation (the delay-loaded imports become the stubs), except that
Qt's `QSystemLibrary` loads still succeed there.

### 14.5 Checked (2026-11-08, RTX 3080, driver 32.0.16.1047)

Profile `spike`, copies of the build, each switch on its own:

| run                                                   | result                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| normal start                                          | adapter line logged, no safe mode, first frame 926 ms, no notice; `startsWithoutFrame` back to 0                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `YAP_TEST_DEVICE_LOSS=5`                              | Qt: "Graphics device lost, cleaning up scenegraph and releasing RHIs"; a new shared device; the window kept drawing (a closed side sheet, a tooltip)                                                                                                                                                                                                                                                                                                                                                             |
| `YAP_TEST_GPU_FAIL=1`                                 | safe mode saved, the app restarted itself (the new pid waited for the old one, ~340 ms), the old one quit with code 3, the new one drew on WARP with the notice                                                                                                                                                                                                                                                                                                                                                  |
| "Use the graphics card next time"                     | `safeMode` false in the file, the card gone                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `--gpu-safe-mode` + `QSG_INFO=1`                      | Qt chose adapter 3, "Microsoft Basic Render Driver"; the window draws; no notice (asked for)                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `YAP_TEST_STARTUP_CRASH=1` twice, then a normal start | the count went 1, 2; the third start: "crash guard: 2 starts in a row", safe mode, first frame 537 ms                                                                                                                                                                                                                                                                                                                                                                                                            |
| saved safe mode with another driver version           | "safe mode cleared: the adapter or its driver changed", GPU again                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `YAP_TEST_NO_MF=1`                                    | the notice (shown once, dismissed); GIF picker GIFs and clip tiles show stills, hovering a clip makes no player; Settings > Voice & video: Qt's `MFStartup` got the stub ("mfplat.dll is not used"), 0 cameras listed; no crash                                                                                                                                                                                                                                                                                  |
| review fixes, same day                                | restart from `--autostart yap-native://invite/compattest`: new command line `--profile spike --yap-relaunched-from=<pid>`, link handled once, window shown; a failure under `--gpu-safe-mode` saved its reason, the next start showed it; the Hardware acceleration switch on cleared the file, off saved `user: true`, kept across a changed driver identity, no notice; `YAP_ZERO_COPY=0` + `YAP_TEST_DEVICE_LOSS`: "device lost (0x00000000)" from `isDeviceLost()`, Qt's new device, the window kept drawing |
| `dumpbin /dependents`                                 | the six DLLs only under "delay load dependencies"                                                                                                                                                                                                                                                                                                                                                                                                                                                                |

Not checked: a real device removal (forcing a TDR resets the whole GPU,
which was in use), a real Windows N install, the decoders' restart on a new
shared device during a call with H.264 video (no call was made), a chat
mp4 under `YAP_TEST_NO_MF` (the test channel has none), and the threaded
render loop's restart.

## 15. Game detection (`yap-games`, 2026-11-08)

What the detector needs is small: process ids and names, one image path
per candidate, the foreground window, a registry value and a JSON file.
No new third-party crate came in for it.

| need              | pick                                                                                                                             | not picked                                                                                                                                                                                                                                                       |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| process list      | `windows-sys` 0.61 (in the tree): `CreateToolhelp32Snapshot`, `Process32FirstW`/`NextW`: 13-15 ms a call with 626 processes here | `sysinfo`: opens every process on each refresh. `NtQuerySystemInformation(SystemProcessInformation)` into a reused buffer: 8.1 ms, our session only (class 53) 6.4 ms, both with a 2 MB buffer; the kernel's walk of every thread is most of the cost either way |
| image path, exit  | `OpenProcess(QUERY_LIMITED_INFORMATION \| SYNCHRONIZE)`, `QueryFullProcessImageNameW`, the handle in `MsgWaitForMultipleObjects` | `RegisterWaitForSingleObject` (a thread-pool callback; the detector already waits on its own handles)                                                                                                                                                            |
| foreground, Steam | `SetWinEventHook` out of context, `RegNotifyChangeKeyValue` (both `windows-sys`)                                                 | polling `GetForegroundWindow`                                                                                                                                                                                                                                    |
| matching rules    | arRPC's `src/process/index.js`, ported (`matcher.rs`)                                                                            | running arRPC (Node, and it reads command lines)                                                                                                                                                                                                                 |
| catalog parse     | `serde_json` streaming into flat arrays (one pass, no `Value` tree)                                                              | `serde_json::Value` (a tree several times the file), `simd-json`                                                                                                                                                                                                 |
| bundled snapshot  | gzip -9 through `flate2` 1.1 (already linked by `png` and velopack): 628 KiB in the exe                                          | xz (lzma-rs): 516 KiB, but a decoder the exe does not have                                                                                                                                                                                                       |

The probe (`cargo run --release -p yap-games --example probe [s]`) prints
the catalog's load time and heap and the detector's CPU over the real
process list, counts only.

## 16. Screen capture: HDR displays (`yap-capture`, 2026-11-08)

A share of an HDR display used to ask Windows Graphics Capture for
`B8G8R8A8`. Windows then converts the scRGB desktop to SDR itself, with no
tone map: HDR games and video come out washed out or clipped. The capture
now follows OBS: on an HDR display it takes `R16G16B16A16Float` (scRGB,
linear, 1.0 = 80 nits) and tone-maps it to SDR on the GPU. SDR displays
are unchanged (BGRA8, as before). No new crate came in: the detection, the
shader and its compiler are all in `windows` 0.61 (one more feature,
`Win32_Graphics_Direct3D_Fxc`); `png` is a dev-dependency for the
gradient check only.

| need            | pick                                                                                                                                                                                         | not picked                                                                                                                                                          |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| HDR on?         | `IDXGIOutput6::GetDesc1` (a new `IDXGIFactory1` each time) `ColorSpace == RGB_FULL_G2084_NONE_P2020` and the display config's advanced color on without `wideColorEnforced`; both must agree | `ColorSpace` alone: `RGB_FULL_G10_NONE_P709` is the composition space, not what an output reports, and only the display config tells auto color management from HDR |
| SDR white       | `DisplayConfigGetDeviceInfo(DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL)`: thousandths of 80 nits, by the monitor's GDI name (`MONITORINFOEXW`) to the path's source name                  | a fixed 80 or 203 nits                                                                                                                                              |
| peak            | `DXGI_OUTPUT_DESC1::MaxLuminance`; under SDR white counts as no headroom                                                                                                                     | `MaxFullFrameLuminance` (too low for highlights)                                                                                                                    |
| which display   | a monitor share: its `HMONITOR`; a window: `MonitorFromWindow(MONITOR_DEFAULTTONEAREST)`, the monitor with the largest part of the window                                                    | -                                                                                                                                                                   |
| following it    | `yap-share-color`, a thread per capture that looks again every second; a change recreates the frame pool in the other format (`Direct3D11CaptureFramePool::Recreate`)                        | display change notifications (`WM_DISPLAYCHANGE` needs a window; a window moving between monitors sends none to us)                                                 |
| the conversion  | a pixel shader (`win/hdr.hlsl`): one full-target triangle, `Load` of its own texel, into a BGRA8 render target that the existing staging copy reads; on the capture's own D3D11 device       | a compute shader: a typed UAV store to `B8G8R8A8_UNORM` is optional hardware support; the D3D11 video processor: no control over the curve                          |
| shader compiler | `D3DCompile` (`d3dcompiler_47.dll`, in every Windows 10 and 11) once per process, `vs_4_0`/`ps_4_0`                                                                                          | prebuilt bytecode in the repo (another build step for a 40-line shader)                                                                                             |

### 16.1 The tone map

`src/tonemap.rs` is the CPU reference; `win/hdr.hlsl` must match it to one
8-bit code (a test checks it on the GPU).

1. Scale by `80 / sdrWhiteNits`: SDR white lands on 1.0, so SDR content
   (the desktop, browser pages, the UI of an HDR game) keeps its
   brightness.
2. Clip negatives (colours outside BT.709) to 0.
3. On the brightest channel (maxRGB) `m`: the identity up to a knee `k`,
   then an extended Reinhard shoulder that meets it with slope 1 and
   reaches 1.0 at the peak `W = maxNits / sdrWhiteNits`:
   `f(m) = k + (1 - k) * t * (1 + t / w²) / (1 + t)`, with
   `t = (m - k) / (1 - k)` and `w = (W - k) / (1 - k)`; 1.0 above `W`.
   All three channels are scaled by `f(m) / m`, so a bright colour keeps
   its hue and saturation instead of going white channel by channel.
4. The sRGB OETF, into `B8G8R8A8_UNORM`.

The knee trades SDR brightness for highlight room. SDR output has no room
above white, so a curve cannot be both the identity on `[0, 1]` and keep
highlights apart. The knee moves with the headroom:
`k = clamp(1 - 0.05 (W - 1), 0.8, 1)`. No headroom (`W <= 1`) gives
`k = 1`, the identity: an SDR display captured in FP16 looks as the BGRA8
path does. At `W = 5` (200-nit white on a 1000-nit display) `k = 0.8`:
everything up to 80 % of SDR white is untouched, SDR white itself lands
at 0.90 linear (243 of 255 after the OETF), and highlights up to the peak
use the rest. BT.2390's EETF was the other candidate. Mapped to an SDR
target (target peak = SDR white) its knee falls near half of SDR white,
which dims the SDR content of the share far more.

### 16.2 The game capture hook

A hooked game with an `R16G16B16A16_FLOAT` back buffer (scRGB, as HDR
games render on Windows) used to stop the hook (`unsupported back buffer
format 10`) and fall back to WGC. The reader now runs the same shader on
the hook's shared texture before its readback, with the tone map of the
display the game's window is mostly on (looked at again every 2 s), or
the identity when that display is SDR. `hook_probe --fp16` checks it
against `hook_target --fp16`.

Left for later: 10-bit back buffers (`R10G10B10A2`, format 24) stay
refused. They are either HDR10 (PQ, BT.2020) or plain 10-bit SDR, and the
OBS hook's `hook_info` does not carry the swap chain's color space, so the
reader cannot tell which; WGC (now tone-mapped) carries those games. The
picker's thumbnails stay BGRA8.

### 16.3 Test switches

- `YAP_TEST_HDR_CAPTURE=<sdrWhiteNits>,<maxNits>`: the FP16 capture and
  that tone map on any display. On an SDR display WGC's FP16 frames are
  at most 1.0, so `80,80` must look as the BGRA8 path does; other values
  show the scale and the curve.
- `YAP_CAPTURE_TIMING=1`: per-frame GPU time of the conversion and of the
  copy to staging (timestamp queries), CPU time of the read and of the
  push, logged every 300 frames (PERFORMANCE.md, "HDR screen capture").
- `cargo test -p yap-capture -- hdr --include-ignored --nocapture`: the
  shader against the CPU reference on this GPU, the displays this PC
  reports (`display_probe`), the shader's cost at 1080p and 1440p
  (`shader_cost`), and with `YAP_HDR_GRADIENT_PNG=<path>` a strip of
  eight hue ramps from 0.01 to 10.0 (0.8 to 800 nits) through three tone
  maps, each above a naive scale-and-clip of the same ramps.

## 17. One speaker for everything (`yap-speaker`, 2026-11-08)

Until this change the app had three outputs. The call played through its
own WASAPI stream (§11). The soundboard, its tones and Listen Together
played through `waveOut` (§9.4). The UI sounds played through one
`waveOut` handle per sound (`src/sound.rs`). Only the call's stream fed the
echo canceller's far end. During a call on laptop speakers, the mic
therefore picked up the soundboard and the stream, and sent them to
everyone (PERFORMANCE.md "One speaker": 0.3 dB of a soundboard clip
removed). `waveOut` also matched devices by names cut to 31 characters,
never followed a change of the Windows default device, and kept 80 ms of
buffers.

Now everything the app plays is an `Input` of one render stream. The
call's echo canceller hears the final mix, so it cancels a soundboard clip
or a track like a voice.

| need           | pick                                                                                                                                                                                                                                                                 | not picked                                                                                                                                                                                                  |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| topology       | one process-wide render thread; every source is an `Input` (`mix_into` adds 10 ms of 48 kHz stereo `f32`); one `Tap` list after the final mix                                                                                                                        | yap-audio feeding the call's render path only while a call runs: two output paths to keep in step (the device, its follow, its loss), and a soundboard clip that started before the call would jump devices |
| device API     | WASAPI directly through `windows` 0.61 (in the tree): shared mode, `EVENTCALLBACK`, `AUTOCONVERTPCM`, 48 kHz stereo 16-bit, a 20 ms buffer request, 20 ms ahead, MMCSS "Pro Audio"                                                                                   | `cpal` 0.16: no endpoint ids, no `IMMNotificationClient`, and its own thread and callback between us and the tap. `IAudioClient3` low-latency periods: the engine's 10 ms period is already the target      |
| device choice  | the endpoint id (`IMMDevice::GetId`), stable across reboots and hot-plug; `None` = the default communications endpoint (what the call always opened and the menus list as "Default")                                                                                 | names: `waveOut` cut them to 31 characters, so two headsets of one model matched the same entry                                                                                                             |
| default follow | `IMMNotificationClient` on the render thread's enumerator: `OnDefaultDeviceChanged(eRender, eCommunications)` reopens when following the default; `OnDeviceStateChanged`/`Removed` for the open endpoint reopen; the chosen one coming back reopens on it            | polling `GetDefaultAudioEndpoint`                                                                                                                                                                           |
| device loss    | any error from `GetCurrentPadding`, `GetBuffer` or `ReleaseBuffer` (`AUDCLNT_E_DEVICE_INVALIDATED` on unplug or a format change), or no audio taken for 1 s: reopen at once; while no device opens, the mix runs on the clock (taps still fed), retried every second | stopping the call's far end until a device is back (the echo canceller would lose its reference)                                                                                                            |
| summing        | add the inputs in `f32`, then a soft clip from 0.977 (the call limiter's ceiling) towards 0.999, so one input passes untouched and a clip over a loud call bends instead of clipping; a boosted person is limited inside the call's mix first (§11.1)                | a limiter on the sum: the call already has one, and a second would pump the call when a clip plays                                                                                                          |
| lifetime       | the thread starts on a wake (an input became audible) or a `Hold` (the call); it closes the device after 2 s with neither                                                                                                                                            | a device open for the whole process                                                                                                                                                                         |

Name migration: a saved output (or input) whose id is not listed, but
whose saved name is, moves to that endpoint's id when the device list
arrives (`match_endpoint`: exact name, case-insensitive; a 31-character
name only when exactly one listed name starts with it). An unplugged
device keeps its id, and the default plays until it is back.

Measured with `cargo run -p yap-speaker --example probe`, which initialises
and closes the client without starting it, so nothing plays. On this
machine's default communications output (VB-Audio Matrix, 10 ms engine
period), a 10 or 20 ms request gives a 22 ms buffer, and the stream
latency plus a period is 10 ms. So a frame is heard 20-30 ms after it is
mixed (10-20 ms queued, 10 ms past the buffer). The call's old stream
asked for 40 ms and kept 30 ms ahead (40-50 ms). `waveOut` kept up to
80 ms of buffers before the audio engine's own latency. PERFORMANCE.md
"One speaker" has the end-to-end numbers.

Not verified on hardware (all live runs used `YAP_NULL_SPEAKER`): a real
default-device change, an unplug mid-call, and underruns at 20 ms ahead
on a loaded machine. The notification decisions and the reopen request
are unit-tested (`wasapi::tests`, `engine::tests`). `YAP_SPEAKER_AHEAD_MS`
raises the lead if a machine underruns.

Sources:

- https://learn.microsoft.com/windows/win32/coreaudio/stream-latency-during-playback
- https://learn.microsoft.com/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient
- https://learn.microsoft.com/windows/win32/coreaudio/audclnt-streamflags-xxx-constants
- https://learn.microsoft.com/windows/win32/coreaudio/recovering-from-an-invalid-device-error

## 18. Dependency audit (cargo-deny, 2026-11-08)

`rust/deny.toml` runs four checks on the Windows target only. Crates that
build only for Linux, macOS, Android or wasm (openssl, security-framework,
jni, r-efi) are not in the exe and are left out. README "Dependency audit"
says how to run it. The release workflow runs it before the build.

| check      | setting                                                                                                          | result                                  |
| ---------- | ---------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| advisories | RustSec database; vulnerabilities, `unmaintained = "all"`, unsound and yanked versions are errors; no ignores    | **none** (0 vulnerable, 0 unmaintained) |
| licenses   | our crates (GPL-3.0-only, `publish = false`) skipped; an allow list of exactly what the tree uses                | ok                                      |
| bans       | `multiple-versions = "warn"`, wildcards denied; the duplicates that stay are in `skip`, each with its reason     | ok, no unexplained duplicate            |
| sources    | crates.io only; no git or other registries (the patched crates are `[patch]` paths made by `patches/vendor.ps1`) | ok                                      |

**Licences in use** (`cargo deny list -l license`): MIT, Apache-2.0,
BSD-3-Clause, ISC, Zlib, 0BSD, BSL-1.0, Unlicense, Unicode-3.0,
BlueOak-1.0.0, CDLA-Permissive-2.0 (webpki-roots, through velopack's ureq)
and MPL-2.0 (symphonia; file-level copyleft, fine under our GPL-3.0).
Apache-2.0 WITH LLVM-exception, BSD-2-Clause and LGPL-2.1-or-later appear
only on crates that do not build for Windows, so they are not on the list.

### 18.1 Fixed

| duplicate                                                                                            | was                                                              | now                                                                                                                                                  |
| ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `windows` 0.61 and 0.62, and 10 `windows-*` crates with them                                         | ours and `webview2-com` 0.38.2 on 0.61, velopack on 0.62         | everything on **0.62.2**: `webview2-com` =0.39.1 (its release on 0.62); `windows` and `windows-core` 0.62 in the root, `yap-capture` and `yap-voice` |
| `sha2` 0.10 and 0.11, with `digest`, `block-buffer`, `crypto-common`, `cpufeatures`, `generic-array` | `yap-capture` (the signed hook's hash) on 0.10, velopack on 0.11 | `yap-capture` on 0.11 (the same `Sha256::digest` call)                                                                                               |
| `windows-sys` 0.59                                                                                   | `tempfile`, `winapi-util`                                        | gone after the lock update. 0.60 (keyring) and 0.61 are left                                                                                         |

26 duplicate warnings became 10, and §18.2 gives the reason for each.

In this code `windows` 0.62 changes one call: `Error::from_win32()` is now
`Error::from_thread()`. It had four call sites: two in `obs_ipc.rs`, one in
`webembed.rs` and one in the `hook_target` example. The Tauri desktop app
still pins `webview2-com` 0.38.2. The two apps do not share a lock, so they
do not need to match.

### 18.2 Left, and why

| crate                                                    | pulled by                                                   | why it stays                                         |
| -------------------------------------------------------- | ----------------------------------------------------------- | ---------------------------------------------------- |
| `thiserror` 1 (and `-impl`)                              | `cxx-qt` 0.10, `clang-format`                               | a cxx-qt major (0.11) is out of scope                |
| `bitflags` 1                                             | `symphonia-core` 0.5.5                                      | symphonia 0.6 has a new API (§9)                     |
| `windows-sys` 0.60                                       | `keyring` 3.6.3                                             | keyring 4 has a new API (separate credential stores) |
| `miniz_oxide` 0.8                                        | `png` 0.18.1 (latest)                                       | `flate2` is on 0.9 and png has not moved             |
| `getrandom` 0.2                                          | `ring` 0.17 (latest), under velopack's `ureq`               | not linked into the exe (§18.3)                      |
| `syn` 1                                                  | `ebml-iterable-specification-derive` 0.4 (yap-webm, latest) | a proc macro, build time only                        |
| `syn` 3                                                  | `cxx` 1.0.205's macros and build crates                     | build time only                                      |
| `reqwest` 0.12, `base64` 0.22, `codespan-reporting` 0.11 | `webrtc-sys-build` (downloads libwebrtc), `cxx-qt-build`    | build scripts only, never in the exe                 |

Not done: a blanket `cargo update`. It offers patch releases of cc, flate2,
mime, smallvec, uuid and zip. Those would add three crates and a second
`unicode-width`, and none of the current versions has an advisory.

### 18.3 Does velopack put ureq, rustls and ring in the exe?

velopack 1.3.39 uses `ureq` 3 with its default features (rustls, ring and
webpki-roots) for `UpdateManager`'s downloads. velopack has no feature that
turns this off, and a crate cannot turn off its dependency's default
features. So the four crates stay in the build graph and in this audit.

**They are not in the exe.** The app calls only
`VelopackApp::build()...run()` (`src/installer.rs`), and the linker drops
the rest. In the link map of the `native/gifdeps` build (`/MAP`):

- none of `ring`'s C or assembly objects is there: no `ring_core_0_17_*`
  symbol;
- there is no rustls handshake, ureq agent or webpki code, and no
  webpki-roots certificate data: the string "ISRG Root" is not in the exe;
- 19 symbols (1.1 KiB) carry those crates' names. They are generic std
  code that identical-code folding named after the first crate that used it:
  drop glue, `Read::read_exact`, `Vec::into_boxed_slice`;
- velopack itself is 132 KiB: zip reading, `VelopackApp::run` and the
  apply path.

The exe does contain DigiCert certificates and the strings ChaCha20 and
X25519. They come from BoringSSL and WebRTC (`third_party/boringssl`,
`rtc_base/ssl_roots.h`), not from Rust.

### 18.4 `gif` in yap-anim

Streamed GIFs (PERFORMANCE.md, pass 8) use `gif` 0.14.2 directly. It was
already in the lock through `image`'s `gif` feature, so no crate was added.
The crate's own row-by-row deinterlacing (`read_next_frame`) can stop early
with "image truncated" on an interlaced frame that Qt, Pillow and Edge
decode in full (for example a 33x37 frame). `gifstream.rs` therefore reads
each frame with a single `fill_buffer` call and puts the rows in order
itself. A test covers interlaced frames of seven sizes. The bug has not
been reported upstream yet.
