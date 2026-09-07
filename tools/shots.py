#!/usr/bin/env python3
"""Render the website's screenshots by running the real binary.

    .venv/bin/python tools/shots.py            # regenerate every shot
    .venv/bin/python tools/shots.py --check    # fail if any is out of date

A pasted screenshot is a lie with a timer on it: it was true when it was taken
and nothing fails when the status bar changes. These come out of the program,
against an emulated terminal, from a committed fixture vault — so a change to
what trafford draws shows up as a changed image in the diff of the commit that
caused it.

SVG rather than PNG, for four reasons that all matter here: the text is real
text, so it is selectable and searchable; it scales to any display without
going soft; it is a few kilobytes; and a reviewer can read the diff.

Needs pyte, like tools/probe.py:

    python3 -m venv .venv && .venv/bin/pip install pyte
"""

import argparse
import os
import pathlib
import shutil
import sys
import tempfile
import tomllib

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from probe import Session  # noqa: E402  (probe owns the pty plumbing)

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "site" / "shots.toml"
BIN = ROOT / "target" / "release" / "trafford"

# The terminal's own default, used for any cell the program left alone. Taken
# from Gotham, which is the theme the fixture pins — a shot whose background
# came from whoever ran it would differ between machines.
DEFAULT_FG = "c5c8c6"
DEFAULT_BG = "0a0f14"

# Type metrics. The advance width is what makes a monospaced SVG line up; it is
# a ratio of the font size rather than a measured value, because the renderer
# has no font to measure and the viewer may not have ours either.
FONT_SIZE = 15.0
ADVANCE = FONT_SIZE * 0.6
LINE_HEIGHT = FONT_SIZE * 1.45
PADDING = FONT_SIZE * 1.1
FONT_STACK = (
    "ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, "
    "'DejaVu Sans Mono', monospace"
)

# What the manifest is allowed to ask for, spelled rather than typed as control
# characters — a literal 0x0b in a TOML file is invisible in every diff.
KEYS = {
    "ctrl-a": b"\x01",
    "ctrl-b": b"\x02",
    "ctrl-e": b"\x05",
    "ctrl-f": b"\x06",
    "ctrl-g": b"\x07",
    "ctrl-j": b"\x0a",
    "ctrl-k": b"\x0b",
    "ctrl-l": b"\x0c",
    "ctrl-n": b"\x0e",
    "ctrl-p": b"\x10",
    "ctrl-t": b"\x14",
    "enter": b"\r",
    "esc": b"\x1b",
    "tab": b"\t",
    "down": b"\x1b[B",
    "up": b"\x1b[A",
}


def keystrokes(name):
    if name in KEYS:
        return KEYS[name]
    if len(name) == 1:
        return name.encode()
    raise SystemExit(f"shots.toml asks for an unknown key: {name!r}")


def capture(shot, vault):
    """Run the binary, send the keys, and read the screen back."""
    session = Session(str(BIN), str(vault), cols=shot["cols"], rows=shot["rows"])
    try:
        session.until(lambda s: "notes" in "\n".join(s.display), 30)
        for key in shot.get("keys", []):
            # Each key as its own write: a terminal delivers ESC glued to the
            # next byte as Alt+key, and two keys in one write are one keypress.
            session.send(keystrokes(key), 0.5)
        session.pump(0.5)
        return [
            [session.screen.buffer[y][x] for x in range(session.screen.columns)]
            for y in range(session.screen.lines)
        ]
    finally:
        session.close()


def runs(row):
    """Collapse a row into runs of one style, so the SVG is not one node per cell."""
    out, current, style = [], [], None
    for cell in row:
        here = (
            cell.fg if cell.fg != "default" else DEFAULT_FG,
            cell.bg if cell.bg != "default" else DEFAULT_BG,
            cell.bold,
            cell.italics,
            cell.underscore,
        )
        if here != style:
            if current:
                out.append((style, "".join(current)))
            current, style = [], here
        current.append(cell.data or " ")
    if current:
        out.append((style, "".join(current)))
    return out


def escape(text):
    return (
        text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    )


def to_svg(rows, title):
    cols = len(rows[0])
    width = cols * ADVANCE + PADDING * 2
    height = len(rows) * LINE_HEIGHT + PADDING * 2

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0f} {height:.0f}" '
        f'width="{width:.0f}" height="{height:.0f}" role="img" '
        f'aria-label="{escape(title)}" font-family="{FONT_STACK}" '
        f'font-size="{FONT_SIZE:.1f}">',
        f"<title>{escape(title)}</title>",
        f'<rect width="100%" height="100%" fill="#{DEFAULT_BG}" rx="6"/>',
    ]

    # Backgrounds first, as one pass, so a coloured run sits under its text
    # rather than beside it.
    for y, row in enumerate(rows):
        x = 0
        for (fg, bg, bold, italic, under), text in runs(row):
            if bg != DEFAULT_BG:
                parts.append(
                    f'<rect x="{PADDING + x * ADVANCE:.1f}" '
                    f'y="{PADDING + y * LINE_HEIGHT:.1f}" '
                    f'width="{len(text) * ADVANCE:.1f}" height="{LINE_HEIGHT:.1f}" '
                    f'fill="#{bg}"/>'
                )
            x += len(text)

    for y, row in enumerate(rows):
        # A row of nothing but spaces contributes no text node at all.
        if not any((c.data or " ").strip() for c in row):
            continue
        baseline = PADDING + y * LINE_HEIGHT + FONT_SIZE * 0.8
        x = 0
        line = []
        for (fg, bg, bold, italic, under), text in runs(row):
            if text.strip():
                attrs = [
                    f'x="{PADDING + x * ADVANCE:.1f}"',
                    f'fill="#{fg}"',
                    'xml:space="preserve"',
                ]
                if bold:
                    attrs.append('font-weight="700"')
                if italic:
                    attrs.append('font-style="italic"')
                if under:
                    attrs.append('text-decoration="underline"')
                line.append(f'<tspan {" ".join(attrs)}>{escape(text)}</tspan>')
            x += len(text)
        parts.append(f'<text y="{baseline:.1f}">{"".join(line)}</text>')

    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if a committed shot no longer matches what the program draws",
    )
    parser.add_argument("--bin", default=str(BIN))
    args = parser.parse_args()

    if not pathlib.Path(args.bin).exists():
        raise SystemExit(
            f"{args.bin} does not exist — run `cargo build --release` first"
        )

    manifest = tomllib.loads(MANIFEST.read_text())
    base = MANIFEST.parent
    out_dir = (base / manifest["out"]).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    stale = []
    with tempfile.TemporaryDirectory(prefix="trafford-shots-") as tmp:
        # Outside the repository, deliberately. Run in place and trafford walks
        # up, finds *this* project's git repo, and puts its dirty-file count in
        # the status bar — so the shot changes every time anyone edits
        # anything. That is how this was found.
        vault = pathlib.Path(tmp) / "vault"
        shutil.copytree(base / manifest["vault"], vault)
        # And no user config either. `Theme::resolve` looks in
        # `~/.config/trafford/themes/` *before* the built-ins, so a developer
        # with their own gotham.toml would take a different screenshot from CI
        # and neither would be wrong. An empty home removes the question.
        home = pathlib.Path(tmp) / "home"
        home.mkdir()
        os.environ["HOME"] = str(home)
        os.environ["XDG_CONFIG_HOME"] = str(home / ".config")
        shots = [(shot, to_svg(capture(shot, vault), shot["title"])) for shot in manifest["shot"]]

    for shot, svg in shots:
        dest = out_dir / f"{shot['name']}.svg"
        current = dest.read_text() if dest.exists() else None
        if current == svg:
            print(f"  {dest.relative_to(ROOT)} is current")
            continue
        if args.check:
            stale.append(dest.relative_to(ROOT))
            continue
        dest.write_text(svg)
        print(f"  wrote {dest.relative_to(ROOT)} ({len(svg) // 1024} KiB)")

    if stale:
        names = "\n".join(f"  {p}" for p in stale)
        raise SystemExit(
            "these screenshots no longer match what trafford draws:\n"
            f"{names}\n\nRun: .venv/bin/python tools/shots.py"
        )


if __name__ == "__main__":
    os.environ.pop("TRAFFORD_VAULT", None)
    main()
