#!/usr/bin/env python3
"""Cut the site's typeface down to the characters the site actually draws.

    .venv/bin/python tools/subset-font.py           # regenerate the subset
    .venv/bin/python tools/subset-font.py --check   # fail if a glyph is missing

A web font service is a request leaving the origin, which this site does not
make — `no_page_asks_the_network_for_anything` fails the build over it. So the
font is self-hosted, and a self-hosted font has to be small enough to justify
itself: the full variable JetBrains Mono is 303 KiB and the page it would sit
on is 57.

The character set is *derived from the built site*, not guessed. The failure
this avoids is quiet: prose gains a character the subset does not have, the
browser falls back mid-word, and it looks like a rendering bug in one glyph.

Needs fonttools:

    .venv/bin/pip install "fonttools[woff]" brotli
"""

import argparse
import pathlib
import re
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "target" / "site"
SOURCE = ROOT / "site" / "assets" / "fonts" / "JetBrainsMono[wght].ttf"
DEST = ROOT / "site" / "assets" / "fonts" / "jetbrains-mono.woff2"

# Everything a page might grow tomorrow without anyone thinking about the font:
# the printable ASCII the prose is mostly made of, the punctuation this project
# actually writes with (em dashes and curly quotes are everywhere), and the box
# drawing and arrows the screenshots use.
ALWAYS = (
    "".join(chr(c) for c in range(0x20, 0x7F))
    + "—–…‘’“”·•×÷°±≤≥≠→←↑↓⌘⇧⌥⌃"
    + "▌▎│─┌┐└┘├┤┬┴┼╭╮╰╯▸▾▴▪●○◦"
    + "áàâäãåéèêëíìîïóòôöõúùûüñçß"
    + "ÁÀÂÄÃÅÉÈÊËÍÌÎÏÓÒÔÖÕÚÙÛÜÑÇ"
)

TAG = re.compile(r"<[^>]+>")
SCRIPT = re.compile(r"<(script|style)\b.*?</\1>", re.S | re.I)
ENTITY = {"&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&#39;": "'"}


def drawn_characters():
    """Every character the built site puts on a screen."""
    if not SITE.is_dir():
        raise SystemExit(f"{SITE} does not exist — build the site first")
    found = set(ALWAYS)
    for path in sorted(SITE.rglob("*.html")):
        # rustdoc is thousands of pages of the same characters and is not
        # styled by this font.
        if "api" in path.relative_to(SITE).parts:
            continue
        html = path.read_text(errors="replace")
        html = SCRIPT.sub(" ", html)
        # Attribute values are drawn too — alt text, aria labels, the title.
        text = TAG.sub(" ", html) + " ".join(re.findall(r'"([^"]*)"', html))
        for entity, char in ENTITY.items():
            text = text.replace(entity, char)
        found.update(text)
    return {c for c in found if c.isprintable() and c != " "} | {" "}


def font_characters(path):
    from fontTools.ttLib import TTFont

    with TTFont(path) as font:
        return {chr(c) for table in font["cmap"].tables for c in table.cmap}


def advance_em(path):
    """The advance width of a monospaced glyph, as a fraction of the em.

    This is the number the fallback stack is chosen against. Every family in
    `--mono` advances at 0.6em, so a swap when the webfont lands moves nothing.
    Replace the font with one at 0.55em and every line reflows on load — which
    is exactly the layout shift this is meant to prevent, and it is invisible
    on a machine where the font is already cached.
    """
    from fontTools.ttLib import TTFont

    with TTFont(path) as font:
        upem = font["head"].unitsPerEm
        # `m` rather than a random glyph: it is the widest thing in a
        # proportional face and identical to the rest in a monospaced one.
        width = font["hmtx"]["m"][0]
        return width / upem


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    if not SOURCE.exists():
        raise SystemExit(f"{SOURCE} is missing")

    wanted = drawn_characters()

    if args.check:
        if not DEST.exists():
            raise SystemExit(f"{DEST} has not been generated")
        have = font_characters(DEST)
        missing = sorted(wanted - have)
        if missing:
            shown = " ".join(f"{c!r}(U+{ord(c):04X})" for c in missing[:20])
            raise SystemExit(
                f"{len(missing)} character(s) the site draws are not in the "
                f"subset:\n  {shown}\n\nRun: .venv/bin/python tools/subset-font.py"
            )
        advance = advance_em(DEST)
        if abs(advance - 0.6) > 0.001:
            raise SystemExit(
                f"the font advances at {advance:.4f}em, not 0.6em — every "
                "fallback in the stack is 0.6em, so this one would reflow the "
                "page when it loads"
            )
        print(
            f"the subset covers all {len(wanted)} characters the site draws, "
            f"at {advance:.2f}em"
        )
        return

    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
        f.write("".join(sorted(wanted)))
        text_file = f.name

    subprocess.run(
        [
            sys.executable,
            "-m",
            "fontTools.subset",
            str(SOURCE),
            f"--text-file={text_file}",
            "--flavor=woff2",
            f"--output-file={DEST}",
            # Keep the weight axis: the site uses 400 and 600, and one variable
            # file is smaller than two static ones.
            "--layout-features=kern,liga,calt",
            "--no-hinting",
            "--desubroutinize",
            "--drop-tables+=DSIG",
            "--name-IDs=*",
        ],
        check=True,
    )
    pathlib.Path(text_file).unlink()

    size = DEST.stat().st_size
    print(f"  {DEST.relative_to(ROOT)}  {size / 1024:.1f} KiB, {len(wanted)} characters")
    if size > 45 * 1024:
        raise SystemExit(
            f"the subset is {size / 1024:.1f} KiB, over the 45 KiB this was "
            "worth doing under"
        )


if __name__ == "__main__":
    main()
