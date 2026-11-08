"""Renders the Velopack installer splash (docs/INSTALLER.md, "Splash").

    python tools/build-splash.py --version 0.2.12 --out build/installer/splash.png

The Tauri app's installer window (desktop/installer: MainWindow.xaml,
Ui/Styles.xaml, Ui/Theme.cs, Ui/Dither.cs) on a 440 x 300 card, in its dark
palette: the Ply-0 ground with a 1 px line-strong edge, the "YAP SETUP"
silkscreen label, Yappy in accent ink through the 32-cell Bayer ramp, the
talk LED with its status word, "Installing Yap Native" and the status line.

Velopack draws no text: it shows this image in a borderless, topmost window,
scaled by the monitor's DPI (the image counts as 96 DPI, Lanczos, at most
70 % of the monitor), and paints its progress bar over the bottom edge, a
solid strip 12 px x DPI tall and the full width, in --splashProgressColor
(tools/package.ps1 passes the accent). The image keeps that strip as the
old meter's tint-2 track, so the bar fills a track rather than covering the
art. Type is heavier than the WPF window's and the mark's cells are 3 px,
so the upscaled card stays legible and hard-edged.

Everything is drawn at 4x and box-filtered down: shapes on whole 1x pixels
stay hard, and text gets 16-sample coverage. Needs Pillow; the fonts are the
app's own static cuts (assets/fonts, tools/build-fonts.py) and the mark is
assets/svg/splash-logo.svg.
"""

import argparse
import re
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "assets" / "fonts"
MARK_SVG = ROOT / "assets" / "svg" / "splash-logo.svg"

W, H = 440, 300
SS = 4  # supersampling
BAR = 12  # Velopack's progress strip at 96 DPI


def mix(color, alpha, ground=(0, 0, 0)):
    """ink at `alpha` over the ground, as tokens.css mixes its tints."""
    return tuple(round(c * alpha + g * (1 - alpha)) for c, g in zip(color, ground))


def hex_rgb(value):
    value = value.lstrip("#")
    return tuple(int(value[i:i + 2], 16) for i in (0, 2, 4))


# Ui/Theme.cs, the dark palette.
PLY0 = hex_rgb("#000000")
INK1 = hex_rgb("#f2f2f3")
INK2 = hex_rgb("#b4b4ba")
INK3 = hex_rgb("#8b8b92")
ACCENT = hex_rgb("#ff8ad8")
TALK = hex_rgb("#3ddc84")
LINE_STRONG = mix(INK1, 0.14)
TINT2 = mix(INK1, 0.10)
TALK_HALO = mix(TALK, 0.22)

# Ui/Dither.cs: Bayer 8x8 and the mark's diagonal ramp.
BAYER8 = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
]
CELLS = 32


def bayer_lit(x, y, coverage):
    return coverage * 64 > BAYER8[y % 8][x % 8] + 0.5


def ramp_lit(x, y):
    d = (x + y) / (2.0 * (CELLS - 1))
    return bayer_lit(x, y, 0.98 - d * 0.3)


def svg_polygons(path):
    """The mark's filled polygons and its cut-outs (fill #000000), from the
    brand SVG's M/L/Z paths on the 32-unit grid."""
    text = path.read_text(encoding="utf-8")
    fills, holes = [], []
    for d, fill in re.findall(r'<path d="([^"]+)" fill="([^"]+)"', text):
        for figure in re.findall(r"M([^Z]+)Z", d):
            points = [tuple(float(v) for v in pair.split()) for pair in re.split(r"L", figure.strip())]
            (holes if fill.lower() == "#000000" else fills).append(points)
    if not fills:
        raise SystemExit(f"no mark in {path}")
    return fills, holes


def font(name, px):
    return ImageFont.truetype(str(FONTS / name), px * SS)


def text(draw, xy, value, face, fill, tracking=0.0):
    """Text with its top-left at xy (1x pixels), optional letter spacing in em."""
    x, y = xy[0] * SS, xy[1] * SS
    if not tracking:
        draw.text((x, y), value, font=face, fill=fill)
        return
    em = face.size
    for ch in value:
        draw.text((x, y), ch, font=face, fill=fill)
        x += face.getlength(ch) + tracking * em


def text_width(value, face, tracking=0.0):
    return (sum(face.getlength(ch) for ch in value) + tracking * face.size * max(0, len(value) - 1)) / SS


def cap_top(face):
    """Offset from the draw origin to the top of capitals (1x px)."""
    return face.getbbox("H")[1] / SS


def render(version):
    img = Image.new("RGB", (W * SS, H * SS), PLY0)
    draw = ImageDraw.Draw(img)

    def rect(x0, y0, x1, y1, fill):
        draw.rectangle([x0 * SS, y0 * SS, x1 * SS - 1, y1 * SS - 1], fill=fill)

    # The edge, then Velopack's track along the bottom (inside the edge's
    # sides; the bar covers the bottom edge where it runs).
    rect(0, 0, W, H, LINE_STRONG)
    rect(1, 1, W - 1, H - 1, PLY0)
    rect(0, H - BAR, W, H, TINT2)

    # Title bar (40 px): the silkscreen label, and the version where the
    # window keys were.
    silk = font("MartianSilk-Medium.ttf", 12)
    label_y = 20 - (silk.getbbox("H")[3] - silk.getbbox("H")[1]) / SS / 2 - cap_top(silk)
    text(draw, (16, label_y), "YAP SETUP", silk, INK3, tracking=0.08)
    num = font("MartianNum-Medium.ttf", 12)
    stamp = f"v{version}"
    num_y = 20 - (num.getbbox("0")[3] - num.getbbox("0")[1]) / SS / 2 - num.getbbox("0")[1] / SS
    text(draw, (W - 16 - text_width(stamp, num), num_y), stamp, num, INK3)

    # Yappy: 96 px, 3 px cells, accent ink through the ramp, the cut-outs in
    # the ground (DitherMark at rest: the icon's pose, jaw open).
    mark_x, mark_y, cell = 32, 48, 3
    fills, holes = svg_polygons(MARK_SVG)
    # Which cells the mark covers: sampled at each cell's centre on a 16x
    # mask (Pillow's polygon fill would take the far edges' cells too).
    fine = Image.new("L", (CELLS * 16, CELLS * 16), 0)
    fdraw = ImageDraw.Draw(fine)
    for poly in fills:
        fdraw.polygon([(px * 16, py * 16) for px, py in poly], fill=255)
    for poly in holes:
        fdraw.polygon([(px * 16, py * 16) for px, py in poly], fill=0)
    for cy in range(CELLS):
        for cx in range(CELLS):
            inside = fine.getpixel((cx * 16 + 8, cy * 16 + 8)) > 127
            if inside and ramp_lit(cx, cy):
                x0, y0 = mark_x + cx * cell, mark_y + cy * cell
                rect(x0, y0, x0 + cell, y0 + cell, ACCENT)

    # Kicker: the talk LED (10 px halo, 6 px dot) and its status word.
    kick_y = mark_y + CELLS * cell + 24  # 168
    led_x = 32
    draw.ellipse([led_x * SS, (kick_y + 2) * SS, (led_x + 10) * SS - 1, (kick_y + 12) * SS - 1], fill=TALK_HALO)
    draw.ellipse([(led_x + 2) * SS, (kick_y + 4) * SS, (led_x + 8) * SS - 1, (kick_y + 10) * SS - 1], fill=TALK)
    word_y = kick_y + 7 - (silk.getbbox("H")[3] - silk.getbbox("H")[1]) / SS / 2 - cap_top(silk)
    text(draw, (led_x + 18, word_y), "INSTALLING", silk, INK3, tracking=0.08)

    # "Installing Yap Native": Bricolage Title ExtraBold (DialogTitle, a size up).
    title = font("BricolageTitle-ExtraBold.ttf", 30)
    title_top = kick_y + 14 + 12
    text(draw, (32, title_top - cap_top(title)), "Installing Yap Native", title, INK1)

    # The status line (Message): Geist, a weight up for the upscale.
    body = font("Geist-Medium.ttf", 15)
    body_top = title_top + 22 + 14
    text(draw, (32, body_top - cap_top(body)), "Setting it up for this Windows account.", body, INK2)

    return img.resize((W, H), Image.Resampling.BOX)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--version", required=True, help="the version stamped in the title bar")
    parser.add_argument("--out", required=True, type=Path, help="the PNG to write")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9A-Za-z.+-]{1,32}", args.version):
        raise SystemExit(f"odd version: {args.version!r}")
    args.out.parent.mkdir(parents=True, exist_ok=True)
    render(args.version).save(args.out, optimize=True)
    print(f"splash {W}x{H} -> {args.out}")


if __name__ == "__main__":
    main()
