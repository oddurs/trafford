#!/usr/bin/env python3
"""Check the external links in a built site.

    python3 tools/linkcheck.py target/site

Internal links are not checked here, and deliberately: the build already
refuses to produce a page with one that resolves to nothing, which is a better
place to catch it — it fails the commit rather than a job that runs later.

What is left is links off the site, and those fail for reasons that have
nothing to do with the change under review: an expired certificate, a rate
limit, a host that is down this morning. Breaking someone's pull request on one
of those is how link checking gets switched off, so this runs weekly on its own
and opens an issue rather than failing a build.

Standard library only, so CI needs no install step for it.
"""

import pathlib
import re
import ssl
import sys
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

HREF = re.compile(r'(?:href|src)="(https?://[^"]+)"')
TIMEOUT = 15
# Some hosts refuse a bare urllib request, which is a fact about them rather
# than about the link.
HEADERS = {"User-Agent": "trafford-linkcheck (+https://github.com/oddurs/trafford)"}


def links(root):
    found = {}
    for path in sorted(root.rglob("*.html")):
        # rustdoc's output is generated, huge, and links out to doc.rust-lang.org
        # on every page; checking it would be thousands of requests to say
        # nothing about this project's own writing.
        if "api" in path.relative_to(root).parts:
            continue
        for url in HREF.findall(path.read_text(errors="replace")):
            found.setdefault(url, set()).add(str(path.relative_to(root)))
    return found


def check(url):
    """HEAD, then GET if the host does not like HEAD."""
    context = ssl.create_default_context()
    for method in ("HEAD", "GET"):
        request = urllib.request.Request(url, method=method, headers=HEADERS)
        try:
            with urllib.request.urlopen(request, timeout=TIMEOUT, context=context) as r:
                if r.status < 400:
                    return None
                last = f"HTTP {r.status}"
        except urllib.error.HTTPError as e:
            # 403 and 405 from a HEAD are usually the host's opinion of HEAD.
            if e.code in (403, 405) and method == "HEAD":
                last = f"HTTP {e.code}"
                continue
            last = f"HTTP {e.code}"
        except Exception as e:  # noqa: BLE001 — a link check has no better answer
            last = type(e).__name__ + f": {e}"
        if method == "GET":
            return last
    return last


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "target/site")
    if not root.is_dir():
        raise SystemExit(f"{root} is not a directory — build the site first")

    found = links(root)
    print(f"checking {len(found)} external link(s) in {root}")
    with ThreadPoolExecutor(max_workers=8) as pool:
        results = list(pool.map(check, found))

    broken = [(url, why, found[url]) for url, why in zip(found, results) if why]
    for url, why, pages in sorted(broken):
        where = ", ".join(sorted(pages))
        print(f"  {url}\n    {why}\n    linked from {where}")
    if broken:
        raise SystemExit(f"{len(broken)} external link(s) are unreachable")
    print("every external link answered")


if __name__ == "__main__":
    main()
