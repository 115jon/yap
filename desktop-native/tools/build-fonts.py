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

The Yap display-name faces (NAME_FACES: Silkscreen, Unbounded, Syne,
Instrument Serif, VT323, Rubik Glitch, Major Mono Display) are the web's
own Fontsource files, one weight each and latin only as the web loads them
(src/display-name-fonts.css), pinned to their package version and SHA-256
and converted from WOFF2 to TTF with one "Yap Name *" family each. (The
eight Classic faces, `Display*.ttf`, were converted the same way by hand.)
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


# The Yap display-name faces: (file, family, package, WOFF2 in the
# package's files/, sha256, italic). Families are what DisplayName.qml asks
# for; the weight stays the source's (Unbounded 700, Syne 800).
FONTSOURCE_VERSION = "5.3.0"
FONTSOURCE = "https://cdn.jsdelivr.net/npm/@fontsource/{package}@" + FONTSOURCE_VERSION + "/{path}"
NAME_FACES = [
    ("DisplaySilkscreen-Regular.ttf", "Yap Name Silkscreen", "silkscreen",
     "silkscreen-latin-400-normal.woff2",
     "e6c72ea4702249202bcdd79d3343057e4e25ef1f04e3fcffd8602ab53b40b4cc", False),
    ("DisplayUnbounded-Bold.ttf", "Yap Name Unbounded", "unbounded",
     "unbounded-latin-700-normal.woff2",
     "477fa23002517a32f232316d859e7b672a044f20073babec8ed3dbcfdb8e041a", False),
    ("DisplaySyne-ExtraBold.ttf", "Yap Name Syne", "syne",
     "syne-latin-800-normal.woff2",
     "1a340e84b78c7e1e7ed24306d682fdcd6dc8cc6cb52b158fbaf22c03f7f001c3", False),
    ("DisplayInstrumentSerif-Italic.ttf", "Yap Name Instrument Serif", "instrument-serif",
     "instrument-serif-latin-400-italic.woff2",
     "5a51946dfffa82972bc98745359c46761515641fda557c25116459a9f83da4a7", True),
    ("DisplayVT323-Regular.ttf", "Yap Name VT323", "vt323",
     "vt323-latin-400-normal.woff2",
     "8ddbebcc1048154132e1d78eb9b1f7850bca1b7d857035ccf1cb4318ebc615b6", False),
    ("DisplayRubikGlitch-Regular.ttf", "Yap Name Rubik Glitch", "rubik-glitch",
     "rubik-glitch-latin-400-normal.woff2",
     "ce7442b4b24dec89c46dee8926494d916d16c42ad038b9a14dceb96293773a5c", False),
    ("DisplayMajorMono-Regular.ttf", "Yap Name Major Mono", "major-mono-display",
     "major-mono-display-latin-400-normal.woff2",
     "3fcb93251028559c08304b621be8d132d1360b0efae44c0f282181fb1e9800f3", False),
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


def style_bits(font, italic):
    """REGULAR (bit 6) or ITALIC (bit 0); never BOLD, so GDI does not
    treat a 600-800 cut as the bold of another family."""
    os2 = font["OS/2"]
    os2.fsSelection = (os2.fsSelection & ~0b1100001) | (0b1 if italic else 0b1000000)
    font["head"].macStyle = 0b10 if italic else 0


def name_faces():
    modules = ROOT / "node_modules" / "@fontsource"
    for file_name, family, package, woff2, sha, italic in NAME_FACES:
        source = fetch(FONTSOURCE.format(package=package, path=f"files/{woff2}"),
                       CACHE / "fontsource" / woff2, sha)
        web = modules / package / "files" / woff2
        if web.exists() and hashlib.sha256(web.read_bytes()).hexdigest() != sha:
            sys.exit(f"{woff2}: node_modules has another file than the pinned one")
        fetch(FONTSOURCE.format(package=package, path="LICENSE"), OUT / f"OFL-{package}.txt")
        font = TTFont(source)
        font.flavor = None
        rename(font, family, italic)
        style_bits(font, italic)
        font.save(OUT / file_name)
        print(f"{file_name}: {family} {font['OS/2'].usWeightClass}, {(OUT / file_name).stat().st_size} bytes")


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
        font["OS/2"].usWeightClass = axes["wght"]
        style_bits(font, italic)
        if "STAT" in font:
            del font["STAT"]
        font.save(OUT / file_name)
        print(f"{file_name}: {family}, {(OUT / file_name).stat().st_size} bytes")

    name_faces()


if __name__ == "__main__":
    main()
