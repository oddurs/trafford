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
import difflib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from probe import Session  # noqa: E402  (probe owns the pty plumbing)

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "site" / "shots.toml"
BIN = ROOT / "target" / "release" / "trafford"

# A theme whose every role is a different colour, used to record with.
#
# The recordings carry *roles* rather than colours, so the website can draw
# them in whatever theme the reader has chosen — which is the whole claim the
# theme section makes, and it was false: every recording was gotham, so
# choosing Paper gave a cream page with six dark terminals pasted onto it.
#
# Roles cannot be recovered from a normal recording, because a palette is not
# injective: gotham draws `faint` and `selection` in one hex, and `link` and
# `muted` in another, and in paper those pairs are nowhere near each other. So
# the capture runs against a theme that gives every role a colour of its own,
# and the mapping back is exact by construction.
SENTINEL = "probe"

# What the root `<svg>` of a role recording wears, so its palette rules can be
# scoped to it and reach nothing else on the page.
SVG_CLASS = "shot-palette"

# `Theme`'s field names are not quite the theme file's keys.
TOML_KEY = {"bg": "background", "fg": "text"}

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


def palettes():
    """Every theme's roles, asked of the program rather than parsed here.

    A theme file may leave a role unstated and have it derived, so a second
    reader of the format would get a different answer from the one the app
    draws with. `site palette` is that answer.
    """
    out = subprocess.run(
        ["cargo", "run", "--quiet", "-p", "trafford-site", "--", "palette"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(out.stdout)


def sentinel_colours(roles):
    """One unmistakable colour per role. A grey ramp: unique, and nothing else
    on the screen is anywhere near it."""
    return {role: f"{i + 1:02x}{i + 1:02x}{i + 1:02x}" for i, role in enumerate(roles)}


def write_sentinel_theme(vault, roles):
    """Into the vault, which is the first place `Theme::resolve` looks.

    Not the user config directory: that is `~/Library/Application Support` on
    macOS and `~/.config` on Linux, so a theme written to one of them is
    invisible on the other — which is exactly what happened, silently. Every
    recording came out in two colours and nothing failed. The vault path is
    the same everywhere.
    """
    directory = vault / ".trafford" / "themes"
    directory.mkdir(parents=True, exist_ok=True)
    lines = ['name = "Probe"', "dark = true"]
    for role, colour in sentinel_colours(roles).items():
        lines.append(f'{TOML_KEY.get(role, role.replace("-", "_"))} = "#{colour}"')
    (directory / f"{SENTINEL}.toml").write_text("\n".join(lines) + "\n")


def as_roles(rows, roles):
    """Replace every captured colour with the role that produced it.

    A colour that is not a sentinel means something drew with a value that did
    not come from the theme. That is worth saying out loud rather than
    silently freezing one theme's colour into a recording that claims to
    follow the reader's.
    """
    back = {colour: role for role, colour in sentinel_colours(roles).items()}
    unknown = set()

    def role_of(colour, default):
        if colour == "default":
            return default
        if colour in back:
            return back[colour]
        unknown.add(colour)
        return default

    # Rebuilt rather than mutated in place: pyte's cells are namedtuples, and
    # two identical cells are equal, so anything that looks a cell up by value
    # rewrites the wrong one.
    rows = [
        [cell._replace(fg=role_of(cell.fg, "fg"), bg=role_of(cell.bg, "bg")) for cell in row]
        for row in rows
    ]
    if unknown:
        shown = " ".join(sorted(f"#{c}" for c in unknown))
        print(f"  note: drawn with colours no theme role explains: {shown}")
    return rows


def prepare(spec, base, tmp, index, roles):
    """A fresh vault for one recording.

    Per recording rather than one shared copy: a shot can ask for a theme or
    for a dirty working tree, and neither should leak into the next one. The
    copy is also what keeps the recordings out of *this* repository — in place,
    trafford walks up, finds it, and puts its dirty-file count in the status
    bar of every shot.
    """
    vault = pathlib.Path(tmp) / f"{index:02d}-{spec['name']}"
    shutil.copytree(base, vault)

    # A shot that names a theme means it: the strip is three palettes shown at
    # once, and must not follow the reader's. Everything else records through
    # the sentinel theme and comes out wearing roles.
    theme = spec.get("theme", SENTINEL)
    if theme == SENTINEL:
        write_sentinel_theme(vault, roles)
    config = vault / ".trafford" / "config.toml"
    config.write_text(
        re.sub(r'theme = "[^"]*"', f'theme = "{theme}"', config.read_text())
    )

    make_repo(vault)

    # A clean tree makes a poor argument for a git panel.
    for name, addition in spec.get("dirty", {}).items():
        path = vault / name
        path.write_text(path.read_text() + addition)

    return vault


def make_repo(vault):
    """Make the fixture a git repository, so the status bar has something true
    to say.

    Without it the status bar reads `no git`, which is an odd thing for a
    landing page to show under a claim about git being in the status bar. It is
    created here rather than committed because a `.git` directory inside this
    repository is not a thing git will track.

    Everything that could vary is pinned: the branch name, the identity, and
    both dates. The global config is taken out of the picture entirely — a
    developer's signing key or a template directory would otherwise reach in.
    """
    # A fixed date, which fixes the commit hash. See the git shot in
    # `shots.toml` for why no recording shows a commit's *age*.
    fixed = {
        **os.environ,
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_CONFIG_SYSTEM": "/dev/null",
        "GIT_AUTHOR_DATE": "2026-01-01T00:00:00+00:00",
        "GIT_COMMITTER_DATE": "2026-01-01T00:00:00+00:00",
    }
    identity = [
        "-c",
        "user.name=trafford",
        "-c",
        "user.email=trafford@example.com",
        "-c",
        "commit.gpgsign=false",
    ]
    run = lambda *args: subprocess.run(
        ["git", *identity, *args], cwd=vault, env=fixed, check=True,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    run("init", "-b", "main")
    run("add", "-A")
    run("commit", "-m", "the vault")


def settle(session, limit=6.0):
    """Wait for the screen to stop moving.

    A fixed pause is a guess, and the guess broke the moment the fixture became
    a git repository: startup grew a git poll, the first keystroke arrived
    before the program was listening, and the shot silently showed whatever
    note happened to be open instead of the one it asked for. Nothing failed —
    it just recorded the wrong thing.
    """
    previous = None
    waited = 0.0
    while waited < limit:
        session.pump(0.25)
        waited += 0.25
        now = session.display()
        if now == previous:
            return
        previous = now


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
        settle(session)
        for key in shot.get("keys", []):
            # Each key as its own write: a terminal delivers ESC glued to the
            # next byte as Alt+key, and two keys in one write are one keypress.
            session.send(keystrokes(key), 0.35)
            settle(session, 2.0)
        return [
            [session.screen.buffer[y][x] for x in range(session.screen.columns)]
            for y in range(session.screen.lines)
        ]
    finally:
        session.close()


def record(cast, vault):
    """Run the binary and keep every frame, not only the last.

    One frame per keystroke, because the program redraws once per event — so
    "hold `l` to walk down the tree" is several steps in the manifest rather
    than an animation this has to sample.
    """
    session = Session(str(BIN), str(vault), cols=cast["cols"], rows=cast["rows"])
    frames = []

    def grab(hold):
        frames.append(
            (
                hold,
                [
                    [session.screen.buffer[y][x] for x in range(session.screen.columns)]
                    for y in range(session.screen.lines)
                ],
            )
        )

    try:
        session.until(lambda s: "notes" in "\n".join(s.display), 30)
        settle(session)
        # Keys sent before the first frame is kept. Two recordings that both
        # began by opening a note through the picker were identical for their
        # first two seconds, and side by side that reads as one broken player
        # rather than as two recordings. Whatever a cast is *about* should be
        # its opening frame.
        for key in cast.get("setup", []):
            session.send(keystrokes(key), 0.25)
            settle(session, 2.0)
        default_hold = cast.get("hold", 900)
        grab(default_hold)
        for step in cast["steps"]:
            session.send(keystrokes(step["key"]), 0.25)
            settle(session, 2.0)
            # The cast's own `hold` is the default here as well as above. It
            # used to default only for the opening frame, so a step that left
            # it out raised `KeyError` a hundred lines from the manifest that
            # caused it — for a key the manifest documents as the default.
            grab(step.get("hold", default_hold))
        return frames
    finally:
        session.close()


def to_cast(frames, cast):
    """A frame format, not a video.

    A style table, then rows of runs, and every frame after the first carries
    only the rows that *changed*. Twenty full frames of a 108x24 terminal would
    be megabytes; the diff is tens of kilobytes, and the text stays real text.
    """
    styles, index = [], {}

    def style_id(style):
        if style not in index:
            index[style] = len(styles)
            styles.append(list(style))
        return index[style]

    out, previous = [], None
    for hold, grid in frames:
        rows = {}
        for y, row in enumerate(grid):
            encoded = [[style_id(style), text] for style, text in runs(row)]
            if previous is None or previous[y] != encoded:
                rows[str(y)] = encoded
        previous = [
            [[style_id(style), text] for style, text in runs(row)] for row in grid
        ]
        out.append({"d": hold, "rows": rows})

    return {
        "title": cast["title"],
        "cols": cast["cols"],
        "rows": cast["rows"],
        "loop": cast.get("loop_pause", 1500),
        # A role recording names its ground; a literal one carries the hex.
        # `site.js` reads this to decide which it is holding.
        "bg": "bg" if cast.get("roles", True) else DEFAULT_BG,
        "styles": styles,
        "frames": out,
    }


def runs(row):
    """Collapse a row into runs of one style, so the SVG is not one node per cell."""
    out, current, style = [], [], None
    for cell in row:
        # `as_roles` has already turned a role recording's defaults into "fg"
        # and "bg"; what is left here is a literal capture.
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


def role_style(palette, used):
    """The stylesheet an SVG carries so it can wear two themes.

    An `<img>` cannot see the page's `data-theme`, but an SVG *does* honour a
    `<style>` of its own, media queries included — so one file covers the two
    palettes a reader gets without choosing: the dark default, and the light
    one their system asks for. An explicit choice of the third theme is the
    player's job, and the player repaints the moment it starts.

    Every rule is scoped under the root's own class. The hero is *inlined* into
    the page rather than loaded as an image, so a bare `.accent{fill:…}` here
    is a document-wide rule — and the page has other inline SVG for it to
    repaint.
    """
    themes = palette["themes"]
    dark = next(n for n in themes if n == "gotham")
    light = next((n for n in themes if n == "paper"), dark)
    # `var(--term-r, #hex)` rather than a bare hex, because these files are
    # read two ways. The hero is inlined into the page, where the variable is
    # defined and follows whichever of the three themes the reader picked. The
    # rest load as `<img>`, where nothing of the page reaches in and the
    # fallback is what draws — which is why the media query below swaps the
    # *fallback* rather than the variable.
    rules = "".join(
        f".{SVG_CLASS} .{r}{{fill:var(--term-{r},#{themes[dark][r]})}}" for r in used
    )
    swap = "".join(
        f".{SVG_CLASS} .{r}{{fill:var(--term-{r},#{themes[light][r]})}}" for r in used
    )
    return (
        f"<style>{rules}"
        f"@media(prefers-color-scheme:light){{{swap}}}</style>"
    )


def to_svg(rows, title, palette=None):
    cols = len(rows[0])
    width = cols * ADVANCE + PADDING * 2
    height = len(rows) * LINE_HEIGHT + PADDING * 2

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0f} {height:.0f}" '
        f'width="{width:.0f}" height="{height:.0f}"{" class=" + chr(34) + SVG_CLASS + chr(34) if palette else ""} role="img" '
        f'aria-label="{escape(title)}" font-family="{FONT_STACK}" '
        f'font-size="{FONT_SIZE:.1f}">',
        f"<title>{escape(title)}</title>",
    ]
    if palette:
        used = sorted(
            {c.fg for row in rows for c in row} | {c.bg for row in rows for c in row}
        )
        parts.append(role_style(palette, used))
        parts.append('<rect width="100%" height="100%" class="bg" rx="6"/>')
    else:
        parts.append(f'<rect width="100%" height="100%" fill="#{DEFAULT_BG}" rx="6"/>')

    ground = "bg" if palette else DEFAULT_BG
    paint = (lambda c: f'class="{c}"') if palette else (lambda c: f'fill="#{c}"')

    # Backgrounds first, as one pass, so a coloured run sits under its text
    # rather than beside it.
    for y, row in enumerate(rows):
        x = 0
        for (fg, bg, bold, italic, under), text in runs(row):
            if bg != ground:
                parts.append(
                    f'<rect x="{PADDING + x * ADVANCE:.1f}" '
                    f'y="{PADDING + y * LINE_HEIGHT:.1f}" '
                    f'width="{len(text) * ADVANCE:.1f}" height="{LINE_HEIGHT:.1f}" '
                    f'{paint(bg)}/>'
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
                # `textLength` forces the run to occupy exactly the cells it
                # occupied in the terminal. Without it the run is drawn at the
                # viewer's own advance width, which is only *approximately*
                # 0.6em — so runs creep and a styled word ends up touching the
                # one after it. That looked like the program had dropped a
                # space, which is a bad thing for a screenshot to imply.
                attrs = [
                    f'x="{PADDING + x * ADVANCE:.1f}"',
                    f'textLength="{len(text) * ADVANCE:.1f}"',
                    'lengthAdjust="spacingAndGlyphs"',
                    paint(fg),
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


def explain(dest, committed, fresh, limit=14):
    """The first few lines that differ, in a form worth reading.

    A cast is one line of JSON, so a line diff says "line 1 changed". Compare
    the frames instead and name the first row that moved.
    """
    if dest.suffix == ".json":
        try:
            a, b = json.loads(committed or "{}"), json.loads(fresh)
        except json.JSONDecodeError:
            return [f"{dest.name}: not valid JSON"]
        out = [f"{dest.name}: {len(a.get('frames', []))} frames committed, "
               f"{len(b.get('frames', []))} fresh"]
        for i, (x, y) in enumerate(zip(a.get("frames", []), b.get("frames", []))):
            if x == y:
                continue
            rows = sorted(set(x["rows"]) | set(y["rows"]), key=int)
            for r in rows:
                if x["rows"].get(r) != y["rows"].get(r):
                    text = lambda f: "".join(run[1] for run in f["rows"].get(r, []))
                    out.append(f"  frame {i}, row {r}:")
                    out.append(f"    committed  {text(x)[:90]!r}")
                    out.append(f"    fresh      {text(y)[:90]!r}")
                    return out[:limit]
            out.append(f"  frame {i} differs only in styling")
            return out[:limit]
        return out[:limit]

    diff = difflib.unified_diff(
        committed.splitlines(), fresh.splitlines(),
        "committed", "fresh", lineterm="", n=1,
    )
    return [f"{dest.name}:"] + [line[:120] for line in list(diff)[2:limit]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if a committed shot no longer matches what the program draws",
    )
    parser.add_argument("--bin", default=str(BIN))
    args = parser.parse_args()

    if args.bin == str(BIN):
        # Build it here rather than trusting whatever is in `target/`. The
        # recordings exist to match what the program draws, and this tool
        # happily recorded a stale binary once — after a branch switch, so the
        # shots matched a build from another branch and `--check` said they
        # were current. A no-op when it already is.
        subprocess.run(
            ["cargo", "build", "--release", "-p", "trafford"],
            cwd=ROOT,
            check=True,
        )
    elif not pathlib.Path(args.bin).exists():
        raise SystemExit(f"{args.bin} does not exist")

    manifest = tomllib.loads(MANIFEST.read_text())
    base = MANIFEST.parent
    out_dir = (base / manifest["out"]).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    palette = palettes()

    stale = []
    stale_pairs = []
    with tempfile.TemporaryDirectory(prefix="trafford-shots-") as tmp:
        # No user config either: `Theme::resolve` looks in
        # `~/.config/trafford/themes/` *before* the built-ins, so a developer
        # with their own gotham.toml would take a different screenshot from CI
        # and neither would be wrong.
        home = pathlib.Path(tmp) / "home"
        home.mkdir()
        os.environ["HOME"] = str(home)
        os.environ["XDG_CONFIG_HOME"] = str(home / ".config")
        fixture = base / manifest["vault"]

        # A shot naming a theme is drawn in it; everything else is drawn in
        # roles and coloured by whoever is reading.
        roles_of = lambda spec, rows: rows if "theme" in spec else as_roles(rows, palette["roles"])
        wear = lambda spec: None if "theme" in spec else palette

        shots = []
        for i, shot in enumerate(manifest["shot"]):
            vault = prepare(shot, fixture, tmp, i, palette["roles"])
            rows = roles_of(shot, capture(shot, vault))
            shots.append(
                (out_dir / f"{shot['name']}.svg", to_svg(rows, shot["title"], wear(shot)))
            )
        for i, cast in enumerate(manifest.get("cast", []), start=100):
            vault = prepare(cast, fixture, tmp, i, palette["roles"])
            frames = [(hold, roles_of(cast, rows)) for hold, rows in record(cast, vault)]
            cast = {**cast, "roles": "theme" not in cast}
            shots.append(
                (
                    out_dir / f"{cast['name']}.cast.json",
                    json.dumps(to_cast(frames, cast), separators=(",", ":")) + "\n",
                )
            )
            # The poster comes out of the same recording, so it cannot drift
            # from the cast it stands in for. Default to the frame the cast
            # ends on, which is the one worth being still.
            poster = frames[cast.get("poster", len(frames) - 1)][1]
            shots.append(
                (out_dir / f"{cast['name']}.svg", to_svg(poster, cast["title"], wear(cast)))
            )

    for dest, body in shots:
        current = dest.read_text() if dest.exists() else None
        if current == body:
            print(f"  {dest.relative_to(ROOT)} is current")
            continue
        if args.check:
            stale.append(dest.relative_to(ROOT))
            stale_pairs.append((dest, current or "", body))
            continue
        dest.write_text(body)
        print(f"  wrote {dest.relative_to(ROOT)} ({len(body) // 1024} KiB)")

    if stale:
        names = "\n".join(f"  {p}" for p in stale)
        # Say *what* changed, not only that something did. A check that fails
        # on a machine you do not have — CI, someone else's laptop — is a check
        # you cannot act on, and this one had to be diagnosed by guessing once.
        print("\nthe first difference:")
        for line in explain(*stale_pairs[0]):
            print(f"  {line}")
        raise SystemExit(
            "\nthese recordings no longer match what trafford draws:\n"
            f"{names}\n\nRun: .venv/bin/python tools/shots.py"
        )


if __name__ == "__main__":
    os.environ.pop("TRAFFORD_VAULT", None)
    main()
