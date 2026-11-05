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

- Fork `desktop/scripts/build-installer-fonts.py` as
  `desktop-native/scripts/build-fonts.py`. PARITY_UI §3.4 lists the changes:
  no `track()`, the full instance list, and full coverage instead of the
  Latin subset.
- **Input:** `desktop-native/assets/fonts/src/*.ttf` (the vendored Google
  Fonts variable TTFs, with `OFL.txt` each).
- **Output:** `desktop-native/assets/fonts/*.ttf`, committed. Commit the
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

- About 60 lines of C++ in `desktop-native/cpp/iconprovider.{h,cpp}`,
  registered with `engine.addImageProvider("icon", ...)`.
- It loads `:/icons/<concept>-<bold|fill>.svg`.
- It reads `requestedSize`, so `Image { sourceSize }` drives the
  rasterisation size. Set `smooth: false` at integer DPR to avoid blur, and
  round sizes to the device pixel.
- `QSvgRenderer` comes from `Qt6Svg.lib` (present in the 6.6.3 build). The
  `qsvg` image plugin is present too.

Generator:

- Extend `scripts/generate-phosphor-icons.mjs` with `--qml`.
- For each concept it writes `desktop-native/assets/icons/<concept>-bold.svg`
  (and `-fill.svg` when `fill: true`) with `rotate` applied. The provider
  composites the slash, using the same constants as `Icon.tsx`.
- It writes `desktop-native/assets/icons/manifest.json` with the concept,
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
- Copy them to `desktop-native/assets/svg/` and draw them with
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

Build `desktop-native/rust/qml/ui/` with these components:

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
  `desktop-native/scripts/build-qt.ps1` that records the exact configure
  line. Publish the install prefix as an artifact (an internal CDN zip) so
  later machines get a "prebuilt" without rebuilding.
- Switch `-QtDir` once `build.ps1` passes and a fixture pixel diff is no
  worse.
- After the switch, adopt `Popup.Window`, `RectangularShadow` and variable
  fonts, in that order.

## 7. Next steps (vendoring)

1. `desktop-native/assets/fonts/src/`: download at a pinned google/fonts commit:
   - `BricolageGrotesque[opsz,wdth,wght].ttf`
   - `Geist[wght].ttf` and `Geist-Italic[wght].ttf`
   - `GeistMono[wght].ttf`
   - `MartianMono[wdth,wght].ttf`
   - each family's `OFL.txt`

   Record the commit and the name-ID-5 versions in `assets/fonts/SOURCES.md`.

2. `desktop-native/scripts/build-fonts.py`: fork the installer script:
   - remove tracking;
   - use the instance table in §1.3 and full coverage;
   - check versions against `node_modules/@fontsource-variable/*`.

   Commit the output to `desktop-native/assets/fonts/`.

3. `scripts/generate-phosphor-icons.mjs --qml`: write
   `desktop-native/assets/icons/*.svg` and `manifest.json`. Add
   `cpp/iconprovider.{h,cpp}` and `qml/ui/Icon.qml`.
4. Copy the theme SVGs (§2.3) to `desktop-native/assets/svg/`.
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
   `desktop-native/assets/emoji/` or `include_bytes!` it in Rust. Add the
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

| area       | pick                                                                                                                                                              | runner-up / rejected                                                                                                                                                   |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Decoding   | `symphonia` 0.5.5 (2025-10-11; MPL-2.0; pure Rust), features `mp3`, `aac`, `isomp4`, `ogg`, `vorbis`, `wav`, `pcm`, `flac`, `mkv`                                 | 0.6.1 (2026-08-13) has a new API; move when the crate is ready for it. FFmpeg bindings: a large C dependency to build `/MT`                                            |
| Opus       | `extern "C"` to `opus_decoder_create` / `opus_decode_float` / `opus_decoder_destroy`, resolved from the libopus inside LiveKit's `webrtc.lib` (feature `libopus`) | `audiopus` / `opus` crates build or link a second libopus (duplicate symbols next to `webrtc.lib`)                                                                     |
| Resampling | `rubato` 5.0.1 (2026-10-01; MIT/Apache), the synchronous FFT resampler (`Fft`, `FixedSync::Input`, 1024-frame chunks)                                             | linear or cubic interpolation: audible aliasing and treble loss on 44.1 → 48 kHz music                                                                                 |
| Output     | `waveOut` through `windows-sys` 0.61 (as `src/sound.rs`), one thread, four 20 ms buffers (80 ms), 48 kHz stereo 16-bit                                            | `cpal` (WASAPI): more code and threads for the same shared-mode result. WASAPI directly: only needed for exclusive mode or lower latency, which playback does not need |

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

- One `yap-audio` thread mixes 20 ms blocks and writes them to `waveOut`
  with an event callback. Four buffers keep the latency at 80 ms.
- The device closes after 2 s with nothing audible and reopens on the next
  play.
- `set_output_device` matches the endpoint name against `waveOut`'s
  31-character names: exact first, then by prefix, case-insensitively. No
  match falls back to `WAVE_MAPPER`.
- The final mix is soft clipped: identity up to 0.8, then a tanh shoulder
  towards 0.98. Overlapping clips never wrap or hard clip.
- Known limit: the output stays on the device it opened. A change of the
  system default device mid-stream is not followed until the device closes
  (2 s idle) and reopens on the next play. Choosing a device in the voice
  settings reopens it at once.

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

| area                   | pick                                                                                                                                                             | why / rejected                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Chat video             | Qt Multimedia `MediaPlayer` + `VideoOutput` on Windows Media Foundation (the only backend in the static Qt 6.6.3)                                                | Plays mp4 (H.264/AAC) from a URL with seeking, including `/api/proxy-media`'s synthetic 206 ranges (live: TikTok, X, a direct mp4). No seekable `QIODevice` was needed: Media Foundation's own HTTP source does the ranges.                                                                                                                                                                                                                                                                        |
| webm (nameplates)      | the still frame                                                                                                                                                  | Media Foundation's media engine plays the VP9 webm on this PC (probed through WinRT `MediaPlayer`: 448x84, seekable), but Qt's backend opens sources through `IMFSourceResolver`, which has no webm byte-stream handler: the player sits in `LoadingMedia` forever, with no error. libvpx (inside libwebrtc) plus a matroska demuxer into `VideoHub` would work but is a decoder pipeline for a hover effect; the web itself shows the still at rest. `NameplateLayer` plays only non-webm motion. |
| APNG and animated WebP | `image` 0.25.10 (`png`, `webp` features, no defaults) in the new `yap-anim` crate, behind `src/anim.rs` and the `AnimatedPicture` item (`cpp/animatedpicture.*`) | Qt's PNG reader shows only APNG's default image; decorations and effects are APNG. One decoder for both formats: frames composited, scaled to the drawn size (never up), premultiplied, delays clamped as browsers do. Decoded on the thread pool; a shared 96 MB cache keyed by URL and size; frames tick only while playing, on screen and with the app active. Qt's `qwebp` could animate WebP too, but one path is simpler.                                                                    |
| Web players            | `webview2-com` =0.38.2 (Tauri's version, `windows` 0.61, already in the lock)                                                                                    | The web's iframes need a browser. Each player is a WebView2 controller in a child window of ours, placed every rendered frame over its QML item and clipped to the chat list; hidden under popups. The loader is linked statically from `WebView2LoaderStatic.lib` (no DLL; `webview2-com-sys`'s own binding would import `WebView2Loader.dll`).                                                                                                                                                   |

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
