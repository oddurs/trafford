#!/usr/bin/env python3
"""Drive trafford in a pseudo-terminal and report what the screen really shows.

A TUI can compile, pass every unit test, and still be unusable: a pane can be
drawn over, the cursor can sit in the wrong column, a keystroke can go to the
wrong pane. None of that is visible from inside the process. This runs the real
binary against a real terminal emulator and reads the result back.

Every non-trivial bug found during the first week of this project came from
here — a panic on narrow terminals, CRLF files tearing the layout apart, the
cursor drifting on CJK text, and a stall on saving in a large vault.

    pip install pyte
    python3 tools/probe.py screens  <vault>   # dump what each keystroke draws
    python3 tools/probe.py cursor   <vault>   # where the cursor actually lands
    python3 tools/probe.py sizes    <vault>   # panic-hunt across terminal sizes
    python3 tools/probe.py timings  <vault>   # latency of startup, save, search

Pass --bin to point at a binary other than target/release/trafford.

Keys are raw bytes: b"\\x0b" is ctrl-k, b"\\x1b" is esc, b"\\r" is enter. Send
esc as its own step — a terminal delivers ESC glued to the next key as Alt+key,
which is a mistake that costs an afternoon if you make it silently.
"""

import argparse
import fcntl
import os
import pty
import select
import struct
import sys
import termios
import time

try:
    import pyte
except ImportError:
    sys.exit("this needs pyte: pip install pyte")

DEFAULT_BIN = "target/release/trafford"


class Session:
    """A running trafford attached to an emulated terminal."""

    def __init__(self, binary, vault, cols=120, rows=34):
        # Resolved before the fork: the child chdirs into the vault, after
        # which a relative binary path no longer points anywhere.
        binary = os.path.abspath(binary)
        vault = os.path.abspath(vault)
        self.screen = pyte.Screen(cols, rows)
        self.stream = pyte.ByteStream(self.screen)
        self.raw = b""
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.chdir(vault)
            os.environ["TERM"] = "xterm-256color"
            try:
                os.execv(binary, [binary, vault])
            except OSError as err:  # the parent only sees EIO otherwise
                os.write(2, f"failed to exec {binary}: {err}\n".encode())
                os._exit(127)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

    def pump(self, seconds):
        end = time.time() + seconds
        while time.time() < end:
            ready, _, _ = select.select([self.fd], [], [], 0.02)
            if not ready:
                continue
            try:
                chunk = os.read(self.fd, 1 << 20)
            except OSError:
                return False
            if not chunk:
                return False
            self.raw += chunk
            self.stream.feed(chunk)
        return True

    def until(self, predicate, limit=30.0):
        """Pump until `predicate(screen)` holds. Returns seconds, or None."""
        start = time.time()
        while time.time() - start < limit:
            ready, _, _ = select.select([self.fd], [], [], 0.02)
            if ready:
                try:
                    chunk = os.read(self.fd, 1 << 20)
                except OSError:
                    break
                if not chunk:
                    break
                self.raw += chunk
                self.stream.feed(chunk)
            if predicate(self.screen):
                return time.time() - start
        return None

    def send(self, keys, settle=0.45):
        try:
            os.write(self.fd, keys)
        except OSError:
            raise SystemExit(
                "trafford exited early:\n" + self.raw.decode("utf8", "replace")[-2000:]
            )
        self.pump(settle)

    def display(self):
        return "\n".join(line.rstrip() for line in self.screen.display)

    def panicked(self):
        return "panicked at" in self.raw.decode("utf8", "replace")

    def close(self):
        try:
            os.write(self.fd, b"\x11")  # ctrl-q
            self.pump(0.2)
            os.close(self.fd)
        except OSError:
            pass
        try:
            _, status = os.waitpid(self.pid, 0)
            return os.waitstatus_to_exitcode(status)
        except ChildProcessError:
            return 0


def cmd_screens(args):
    """Dump the screen after each keystroke, to read the UI as a user sees it."""
    steps = [
        ("opening screen", None),
        ("command palette", b"\x0b"),
        ("filtered to 'git'", b"git"),
        ("dismissed", b"\x1b"),
        ("help", b"\x1bOP"),
        ("dismissed", b"\x1b"),
        ("quick switcher", b"\x10"),
        ("git panel", b"\x1b\x07"),
        ("diff of the selected file", b"d"),
        ("back to the git panel", b"\x1b"),
        ("assistant", b"\x1b\x0a"),
    ]
    session = Session(args.bin, args.vault)
    session.pump(0.9)
    for label, keys in steps:
        if keys:
            session.send(keys)
        print("=" * session.screen.columns)
        print(f"### {label}")
        print("=" * session.screen.columns)
        print(session.display())
    session.close()


def cmd_cursor(args):
    """Report where the terminal cursor lands — catches column drift."""
    probes = [
        ("end of the first line", [b"1G", b"$"]),
        ("end of the last line", [b"G", b"$"]),
        ("start of the last line", [b"G", b"0"]),
    ]
    for label, keys in probes:
        session = Session(args.bin, args.vault)
        session.until(lambda s: "notes" in "\n".join(s.display), 30)
        for key in keys:
            session.send(key, 0.3)
        cursor = session.screen.cursor
        print(f"### {label}: column {cursor.x}, row {cursor.y}")
        for line in session.screen.display[:6]:
            print("    " + line.rstrip())
        print()
        session.close()


def cmd_sizes(args):
    """Open every pane at hostile terminal sizes and report any panic."""
    sizes = [(200, 50), (120, 34), (80, 24), (60, 20), (40, 15), (30, 12), (20, 10), (12, 6), (5, 3)]
    bad = 0
    for cols, rows in sizes:
        session = Session(args.bin, args.vault, cols, rows)
        session.pump(0.7)
        for keys in [b"\x0a", b"\x0b", b"\x1b", b"\x07", b"\x1b"]:
            session.send(keys, 0.25)
        panicked = session.panicked()
        code = session.close()
        bad += panicked
        print(f"{'PANIC' if panicked else 'ok   '} {cols:>3}x{rows:<3} exit={code}")
    return 1 if bad else 0


def cmd_timings(args):
    """Latency of the things a user waits on."""
    session = Session(args.bin, args.vault)
    startup = session.until(lambda s: "notes" in "\n".join(s.display), 60)
    print(f"  startup   {fmt(startup)}")

    session.send(b"\x10", 0.3)  # quick switcher
    opened = session.until(lambda s: "Open note" in "\n".join(s.display), 30)
    print(f"  switcher  {fmt(opened)}")
    session.send(b"\x1b", 0.2)

    session.send(b"Gox", 0.2)  # append a character
    session.send(b"\x1b", 0.1)
    os.write(session.fd, b"\x13")  # ctrl-s
    saved = session.until(lambda s: "saved" in "\n".join(s.display), 60)
    print(f"  save      {fmt(saved)}")

    os.write(session.fd, b"\x06the")  # search
    found = session.until(lambda s: "hits" in "\n".join(s.display), 60)
    print(f"  search    {fmt(found)}")
    session.close()


def fmt(seconds):
    return "timeout" if seconds is None else f"{seconds * 1000:7.0f} ms"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("mode", choices=["screens", "cursor", "sizes", "timings"])
    parser.add_argument("vault", help="path to a vault to open")
    parser.add_argument("--bin", default=DEFAULT_BIN, help=f"binary to run (default {DEFAULT_BIN})")
    args = parser.parse_args()
    if not os.path.exists(args.bin):
        sys.exit(f"{args.bin} not found — cargo build --release first")
    return {
        "screens": cmd_screens,
        "cursor": cmd_cursor,
        "sizes": cmd_sizes,
        "timings": cmd_timings,
    }[args.mode](args) or 0


if __name__ == "__main__":
    sys.exit(main())
