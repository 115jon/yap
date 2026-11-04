"""Builds desktop-native/assets/fonts/*.ttf: static cuts of the web's fonts.

Qt 6.6 cannot set variation axes (QFont::setVariableAxis is 6.7+), so every
weight / opsz / wdth the UI uses is cut from the Google Fonts variable TTF
(the same upstream Fontsource serves the web) and renamed to its own "Yap *"
family, so Qt's weight matching never crosses cuts. Tracking is not baked:
QML sets font.letterSpacing. Coverage is the full face (no subsetting).

    uvx --from fonttools --with brotli python -I desktop-native/tools/build-fonts.py

Downloads (curl) are pinned to one google/fonts commit and checked by
SHA-256; the cache lives in desktop-native/tools/.cache/. When node_modules
has the Fontsource packages, each source's version string (name ID 5) must
match the WOFF2 the web loads.
"""

import hashlib
import subprocess
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

HERE = Path(__file__).resolve().parent
NATIVE = HERE.parent
ROOT = NATIVE.parent
CACHE = HERE / ".cache" / "fonts"
OUT = NATIVE / "assets" / "fonts"

COMMIT = "bbda7253fc363238b05d97515ca71f8ae49c4fa9"  # google/fonts main, 2026-11-04
RAW = f"https://raw.githubusercontent.com/google/fonts/{COMMIT}/ofl"

# key: (google/fonts path, sha256, Fontsource WOFF2 to compare versions with)
SOURCES = {
    "bricolage": ("bricolagegrotesque/BricolageGrotesque[opsz,wdth,wght].ttf",
                  "413e7357809ddd12fd80a96a8a396de0e401638d4acd3cb3e37532f0472ac682",
                  "bricolage-grotesque/files/bricolage-grotesque-latin-standard-normal.woff2"),
    "geist": ("geist/Geist[wght].ttf",
              "73894e0448cae90a92b6c2f8732b7bb9acb7b94c418bff559dad4a18e1de9659",
              "geist/files/geist-latin-wght-normal.woff2"),
    "geist-italic": ("geist/Geist-Italic[wght].ttf",
                     "a464348abe41a6392f18d04c0ac2ad3c26619357ff5fdd76aff3059116f45e62",
                     "geist/files/geist-latin-wght-italic.woff2"),
    "geist-mono": ("geistmono/GeistMono[wght].ttf",
                   "d00e590b8eb3a59acc329b2d044fd143ae935090b7da33199ebee27cc7de8196",
                   "geist-mono/files/geist-mono-latin-wght-normal.woff2"),
    "martian": ("martianmono/MartianMono[wdth,wght].ttf",
                "c3467843ec1c2574b05fbcfd7147c7bfbcf63ddca8fc2bcb9d117f1bfb1b22e7",
                "martian-mono/files/martian-mono-latin-standard-normal.woff2"),
}
LICENSES = {"bricolage": "bricolagegrotesque", "geist": "geist", "geist-mono": "geistmono", "martian": "martianmono"}

# (file, family, source, axes, italic). One family per cut; Theme.qml names
# them per type role (PARITY_UI.md §3.3, LIBRARIES.md §1.3).
INSTANCES = [
    ("BricolageKey-SemiBold.ttf", "Yap Bricolage Key", "bricolage", {"wght": 600, "opsz": 15, "wdth": 100}, False),
    ("BricolageKey-Bold.ttf", "Yap Bricolage Key Bold", "bricolage", {"wght": 700, "opsz": 15, "wdth": 100}, False),
    ("BricolageKey-ExtraBold.ttf", "Yap Bricolage Key ExtraBold", "bricolage", {"wght": 800, "opsz": 15, "wdth": 100}, False),
    ("BricolageHead-Bold.ttf", "Yap Bricolage Head", "bricolage", {"wght": 700, "opsz": 20, "wdth": 100}, False),
    ("BricolageHead-ExtraBold.ttf", "Yap Bricolage Head ExtraBold", "bricolage", {"wght": 800, "opsz": 18, "wdth": 100}, False),
    ("BricolageTitle-ExtraBold.ttf", "Yap Bricolage Title", "bricolage", {"wght": 800, "opsz": 28, "wdth": 100}, False),
    ("BricolagePoster-ExtraBold.ttf", "Yap Bricolage Poster", "bricolage", {"wght": 800, "opsz": 40, "wdth": 100}, False),
    ("Geist-Regular.ttf", "Yap Geist", "geist", {"wght": 400}, False),
    ("Geist-Medium.ttf", "Yap Geist Medium", "geist", {"wght": 500}, False),
    ("Geist-SemiBold.ttf", "Yap Geist SemiBold", "geist", {"wght": 600}, False),
    ("Geist-Italic.ttf", "Yap Geist Italic", "geist-italic", {"wght": 400}, True),
    ("GeistMono-Regular.ttf", "Yap Geist Mono", "geist-mono", {"wght": 400}, False),
    ("MartianSilk-Medium.ttf", "Yap Martian Silk", "martian", {"wght": 500, "wdth": 90}, False),
    ("MartianNum-Medium.ttf", "Yap Martian Num", "martian", {"wght": 500, "wdth": 100}, False),
    ("MartianNum-SemiBold.ttf", "Yap Martian Num SemiBold", "martian", {"wght": 600, "wdth": 100}, False),
]


def fetch(url, dest, sha256=None):
    if not dest.exists():
        dest.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["curl", "-sSfL", "-o", str(dest), url.replace("[", "%5B").replace("]", "%5D")], check=True)
    if sha256 and hashlib.sha256(dest.read_bytes()).hexdigest() != sha256:
        dest.unlink()
        sys.exit(f"{dest.name}: SHA-256 mismatch")
    return dest


def version(font):
    return font["name"].getDebugName(5)


def rename(font, family, italic):
    name = font["name"]
    keep = {0, 7, 8, 9, 11, 12, 13, 14}  # copyright, trademark, maker, license text and URL
    name.names = [n for n in name.names if n.nameID in keep]
    style = "Italic" if italic else "Regular"
    ps = family.replace(" ", "") + "-" + style
    for name_id, value in ((1, family), (2, style), (3, f"{family};{style};{COMMIT[:7]}"), (4, f"{family} {style}" if italic else family), (6, ps)):
        name.setName(value, name_id, 3, 1, 0x409)
    name.setName("This Font Software is licensed under the SIL Open Font License, Version 1.1.", 13, 3, 1, 0x409)
    name.setName("https://openfontlicense.org", 14, 3, 1, 0x409)


def main():
    modules = ROOT / "node_modules" / "@fontsource-variable"
    paths = {}
    for key, (path, sha, woff2) in SOURCES.items():
        paths[key] = fetch(f"{RAW}/{path}", CACHE / Path(path).name, sha)
        web = modules / woff2
        if web.exists():
            ours, theirs = version(TTFont(paths[key])), version(TTFont(web))
            if ours != theirs:
                sys.exit(f"{key}: google/fonts has '{ours}', the web serves '{theirs}'")
            print(f"{key}: {ours} (matches Fontsource)")
        else:
            print(f"{key}: {version(TTFont(paths[key]))} (no Fontsource copy to compare)")

    OUT.mkdir(parents=True, exist_ok=True)
    for key, folder in LICENSES.items():
        fetch(f"{RAW}/{folder}/OFL.txt", OUT / f"OFL-{key}.txt")

    for file_name, family, source, axes, italic in INSTANCES:
        font = instantiateVariableFont(TTFont(paths[source]), axes)
        rename(font, family, italic)
        os2 = font["OS/2"]
        os2.usWeightClass = axes["wght"]
        # REGULAR (bit 6) or ITALIC (bit 0); never BOLD, so GDI does not
        # treat a 600-800 cut as the bold of another family.
        os2.fsSelection = (os2.fsSelection & ~0b1100001) | (0b1 if italic else 0b1000000)
        font["head"].macStyle = 0b10 if italic else 0
        if "STAT" in font:
            del font["STAT"]
        font.save(OUT / file_name)
        print(f"{file_name}: {family}, {(OUT / file_name).stat().st_size} bytes")


if __name__ == "__main__":
    main()
