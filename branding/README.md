# Branding

The Yap mark is **Yappy**, a pixel dog mid-bark, drawn by hand on a 32-unit
grid: Yap Pink `#ff3d8b` with black features. The vector sources live here:

- `yappy-master.svg`: the 32-grid master (the icon at 32 px and up).
- `yappy-24.svg`, `yappy-16.svg`: hand-pixelled 24 and 16 px versions. Use
  these at those sizes; a resized master blurs.
- `yappy-android-foreground.svg`, `yappy-android-monochrome.svg`: the Android
  adaptive layers (108 dp canvas, glyph in the 66 dp safe zone, plate
  `#0b0b0d`).
- `yappy-lockup-on-dark.svg`, `yappy-lockup-on-light.svg`: mark and wordmark.

`#ff3d8b` is brand only: the mark and the `brand-2` slot. It is never an
interaction or state colour (see DESIGN.md, Colors).

## App icon

`app-icon.png` (1024 px, the master) and the `app-icon-<size>.png` files
(16 and 24 are the hand-pixelled art; 32, 48, 64, 96, 128 and 256 are exact
grid multiples) are the sources for the shipped icons:

- `public/favicon.ico`
- `public/favicon-dev.ico`, the green dev-server variant, from
  `app-icon-dev-16/32/48.png`
- `native/assets/app/yap.ico`, the Windows client's executable icon

The script that generated them belonged to the Tauri app and was removed
with it (it lives on the `archive/tauri-desktop` branch). Until a
favicon-only replacement lands, rebuild these by hand from the sized PNGs.

## Rendered from the SVGs, not by the script

These were rendered once from the SVGs above at exact sizes; re-render them
if the art changes:

- `app-icon*.png` (including the dev variants: the same art in talk green
  `#3ddc84`) and `public/favicon-dev.png`.
- `public/apple-touch-icon.png` (180), `public/icon-192.png`,
  `public/icon-512.png` (the web manifest): the adaptive composition on the
  `#0b0b0d` plate.
- `public/og-image.png` (1200 x 630): the dark lockup centred on `#0b0b0d`.

## Drawn in code from the same grid

- `src/assets/splash-logo.svg` (master) and `public/icons/home-*.svg`
  (24 px art, the rail home mark).
- `src/components/ui/yap-bark.tsx`: the dithered, barking loader mark.
- `native/mobile/android/res/`: the Qt Android app's adaptive
  launcher layers.
