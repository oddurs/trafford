#!/usr/bin/env python3
"""Look at the built site one section at a time, and measure it.

    .venv/bin/python tools/look.py                 # every section, 1440px wide
    .venv/bin/python tools/look.py --width 420     # a phone
    .venv/bin/python tools/look.py --page docs/reading/

`tools/probe.py` does this for the terminal: run the real thing, read the
screen back, and assert on what is actually drawn rather than on what the code
looks like. This is the same idea pointed at the website, and it exists for the
same reason — a full-page screenshot at 5000 pixels tall tells you a section is
*there*, and nothing about whether it reads.

Writes one PNG per section, and prints what it measured: anything wider than
the viewport, text too small or too low-contrast to read, and the ratio of
empty space to content, which is the number that catches a section that has
technically laid out and visibly has not.

Needs playwright:

    .venv/bin/pip install playwright && .venv/bin/python -m playwright install chromium
"""

import argparse
import functools
import http.server
import pathlib
import socketserver
import threading

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "target" / "site"

# What a reader is entitled to. Anything under these is reported.
MIN_FONT_PX = 11.0
MIN_CONTRAST = 4.5


def serve(directory):
    """A static server on a port the operating system picks.

    The same reason the development server binds zero: several of these run at
    once here, and a fixed port means looking at another checkout's build.
    """
    handler = functools.partial(
        http.server.SimpleHTTPRequestHandler, directory=str(directory)
    )
    httpd = socketserver.TCPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd, httpd.server_address[1]


MEASURE = """
() => {
  const luminance = (rgb) => {
    const [r, g, b] = rgb.map((v) => {
      v /= 255;
      return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const parse = (c) => (c.match(/\\d+/g) || []).slice(0, 3).map(Number);
  const ground = (el) => {
    for (let n = el; n; n = n.parentElement) {
      const bg = getComputedStyle(n).backgroundColor;
      if (bg && !bg.startsWith("rgba(0, 0, 0, 0)")) return parse(bg);
    }
    return [0, 0, 0];
  };
  const contrast = (a, b) => {
    const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
    return (x + 0.05) / (y + 0.05);
  };

  const doc = document.documentElement;
  const out = {
    viewport: doc.clientWidth,
    scrollWidth: doc.scrollWidth,
    overflow: [],
    small: [],
    faint: [],
    sections: [],
  };

  document.querySelectorAll("body *").forEach((el) => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return;
    // A box that scrolls its own content is allowed to hold something wider.
    const scroller = el.closest(
      ".shot, .table-scroll, pre, .install, .cast, figure"
    );
    if (r.right > doc.clientWidth + 1 && !scroller) {
      out.overflow.push(el.tagName + "." + (el.className || "").toString().slice(0, 40));
    }
    // A recording is a picture of a terminal: its colours are the terminal's,
    // on the terminal's own ground, and neither is this stylesheet's business.
    // Checking them reported forty false failures and hid the one real one.
    if (el.closest("svg, .cast-screen")) return;
    if (!el.children.length && el.textContent.trim()) {
      const style = getComputedStyle(el);
      const size = parseFloat(style.fontSize);
      const label = (el.textContent.trim().slice(0, 34) || "").replace(/\\s+/g, " ");
      if (size < %MIN_FONT%) out.small.push(size.toFixed(1) + "px  " + label);
      const ratio = contrast(parse(style.color), ground(el));
      if (ratio < %MIN_CONTRAST% && size < 24) {
        out.faint.push(ratio.toFixed(2) + ":1  " + label);
      }
    }
  });

  document.querySelectorAll("section, header.top, footer.bottom").forEach((el, i) => {
    const r = el.getBoundingClientRect();
    // How much of the section's height is doing nothing: the tallest run of
    // vertical space no child occupies.
    const kids = [...el.children].map((k) => k.getBoundingClientRect());
    let filled = 0;
    kids.forEach((k) => (filled += k.height));
    out.sections.push({
      i,
      name:
        (el.querySelector("h1, h2, .foot-title, .brand") || {}).textContent ||
        el.className,
      height: Math.round(r.height),
      children: kids.length,
      filled: Math.round(filled),
    });
  });
  return out;
}
""".replace("%MIN_FONT%", str(MIN_FONT_PX)).replace("%MIN_CONTRAST%", str(MIN_CONTRAST))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--width", type=int, default=1440)
    parser.add_argument("--height", type=int, default=1000)
    parser.add_argument("--page", default="")
    parser.add_argument("--theme", default="", help="force a data-theme")
    # Playwright defaults to a light preference, which is a real reader and not
    # the one this site is designed against first. Both are worth looking at.
    parser.add_argument("--scheme", default="dark", choices=["dark", "light"])
    parser.add_argument("--out", default=None)
    parser.add_argument("--settle", type=float, default=2.5)
    args = parser.parse_args()

    from playwright.sync_api import sync_playwright

    out_dir = (pathlib.Path(args.out) if args.out else ROOT / "target" / "look").resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    for old in out_dir.glob("*.png"):
        old.unlink()

    httpd, port = serve(SITE)
    url = f"http://127.0.0.1:{port}/{args.page}"

    with sync_playwright() as pw:
        browser = pw.chromium.launch()
        page = browser.new_page(
            viewport={"width": args.width, "height": args.height},
            color_scheme=args.scheme,
        )
        page.goto(url, wait_until="load")
        if args.theme:
            page.evaluate(
                "(t) => document.documentElement.setAttribute('data-theme', t)",
                args.theme,
            )
        # Recordings only load when they are scrolled to, so walk the page
        # before measuring anything.
        page.evaluate(
            """async () => {
                const step = window.innerHeight * 0.8;
                for (let y = 0; y < document.body.scrollHeight; y += step) {
                    window.scrollTo(0, y);
                    await new Promise((r) => setTimeout(r, 120));
                }
                window.scrollTo(0, 0);
            }"""
        )
        page.wait_for_timeout(int(args.settle * 1000))

        report = page.evaluate(MEASURE)

        blocks = page.locator("header.top, main section, footer.bottom")
        count = blocks.count()
        for i in range(count):
            el = blocks.nth(i)
            name = (
                el.evaluate(
                    "(e) => (e.querySelector('h1,h2') || {}).textContent || e.className"
                )
                or f"block-{i}"
            )
            slug = "".join(
                c if c.isalnum() else "-" for c in name.strip().lower()
            ).strip("-")[:44]
            el.scroll_into_view_if_needed()
            page.wait_for_timeout(400)
            el.screenshot(path=str(out_dir / f"{i:02d}-{slug or 'block'}.png"))

        browser.close()
    httpd.shutdown()

    print(f"{count} block(s) → {out_dir}")
    print(f"viewport {report['viewport']}  scrollWidth {report['scrollWidth']}")
    if report["scrollWidth"] > report["viewport"] + 1:
        print("  ! the page scrolls sideways")
    for label, rows in (
        ("wider than the viewport", report["overflow"]),
        (f"under {MIN_FONT_PX}px", report["small"]),
        (f"under {MIN_CONTRAST}:1", report["faint"]),
    ):
        unique = sorted(set(rows))
        if unique:
            print(f"\n  {label}:")
            for row in unique[:12]:
                print(f"    {row}")
            if len(unique) > 12:
                print(f"    … and {len(unique) - 12} more")

    print("\n  sections (height / children / filled):")
    for s in report["sections"]:
        name = " ".join(str(s["name"]).split())[:38]
        air = s["height"] - s["filled"]
        flag = "  ← mostly air" if s["height"] > 300 and air > s["height"] * 0.45 else ""
        print(f"    {s['height']:5}px  {s['children']:2} kids  {name:40}{flag}")


if __name__ == "__main__":
    main()
