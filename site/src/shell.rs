//! The HTML around a rendered note.
//!
//! Written as `format!` rather than through a template engine. A template
//! language is a second syntax, a second set of escaping rules, and a build
//! dependency, and this site has two page shapes. When it has twelve, revisit
//! it — until then the compiler checking the holes is worth more than the
//! tidiness.

use std::fmt::Write as _;

use crate::html::{escape, escape_attr, Ctx, TocEntry};

/// One entry in the sidebar.
pub struct NavItem {
    pub title: String,
    /// Root-relative, with a trailing slash.
    pub url: String,
    pub section: String,
    pub current: bool,
}

/// Everything the shell needs that is not the body.
pub struct Page<'a> {
    pub title: String,
    pub description: String,
    /// Root-relative path of this page, for the canonical URL.
    pub url: String,
    pub body: String,
    pub toc: &'a [TocEntry],
    pub nav: &'a [NavItem],
    pub assets: &'a Assets,
    /// Absolute origin, when there is one. Local builds have none, and a
    /// canonical tag pointing at localhost is worse than no canonical tag.
    pub site_url: Option<&'a str>,
    /// Injected by `serve` and never by `build`, so a deployed page cannot
    /// carry development code.
    pub reload: bool,
}

/// The hashed names the assets were written under.
pub struct Assets {
    pub css: String,
    pub js: String,
    pub icon: String,
    pub font: String,
}

/// The whole document for a documentation page.
pub fn doc(ctx: &Ctx<'_>, page: &Page<'_>) -> String {
    let mut main = String::new();
    let _ = write!(main, "<article class=\"prose\">\n{}\n</article>", page.body);

    let mut aside = String::new();
    if !page.toc.is_empty() {
        aside.push_str("<nav class=\"toc\" aria-label=\"On this page\">\n<p class=\"toc-title\">On this page</p>\n<ul>\n");
        for entry in page.toc {
            let _ = writeln!(
                aside,
                "<li class=\"toc-{}\"><a href=\"#{}\">{}</a></li>",
                entry.level,
                escape_attr(&entry.anchor),
                escape(&entry.text)
            );
        }
        aside.push_str("</ul>\n</nav>\n");
    }

    let content = format!(
        "<div class=\"layout\">\n{sidebar}\n<main id=\"content\">\n{main}\n</main>\n<div class=\"rail\">{aside}</div>\n</div>",
        sidebar = sidebar(ctx, page.nav),
    );
    document(ctx, page, &content, "doc")
}

/// The landing page: the claim, the program, then the note's own body.
///
/// `closing` is put inside the last section rather than after it — the last
/// section is the page's closing claim, and the command belongs under the
/// claim rather than in a block of its own beneath it.
pub fn landing(ctx: &Ctx<'_>, page: &Page<'_>, hero: &str, closing: &str) -> String {
    let body = match page.body.rfind("</section>") {
        Some(at) => format!("{}{closing}{}", &page.body[..at], &page.body[at..]),
        None => page.body.clone(),
    };
    let content = format!(
        "<main id=\"content\" class=\"landing\">\n{hero}\n<div class=\"prose\">\n{body}\n</div>\n</main>"
    );
    document(ctx, page, &content, "landing")
}

fn sidebar(ctx: &Ctx<'_>, nav: &[NavItem]) -> String {
    let mut out =
        String::from("<nav class=\"sidebar\" aria-label=\"Documentation\">\n<ul class=\"nav\">\n");
    let mut section = String::new();
    for item in nav {
        if item.section != section {
            section = item.section.clone();
            let _ = writeln!(out, "<li class=\"nav-section\">{}</li>", escape(&section));
        }
        let current = if item.current {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "<li><a href=\"{}\"{current}>{}</a></li>",
            escape_attr(&ctx.href(&item.url)),
            escape(&item.title)
        );
    }
    out.push_str("</ul>\n</nav>\n");
    out
}

fn document(ctx: &Ctx<'_>, page: &Page<'_>, content: &str, kind: &str) -> String {
    let canonical = match page.site_url {
        Some(origin) => format!(
            "\n<link rel=\"canonical\" href=\"{}\">",
            escape_attr(&format!("{}/{}", origin.trim_end_matches('/'), page.url))
        ),
        None => String::new(),
    };
    let reload = if page.reload { RELOAD_SCRIPT } else { "" };
    let title = if page.title == "trafford" {
        "trafford — a terminal knowledge base".to_string()
    } else {
        format!("{} — trafford", page.title)
    };

    format!(
        r##"<!doctype html>
<html lang="en" class="{kind}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="{description}">
<meta property="og:title" content="{og_title}">
<meta property="og:description" content="{description}">
<meta property="og:type" content="website">{canonical}
<link rel="icon" href="{icon}" type="image/svg+xml">
<link rel="preload" href="{font}" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="{css}">
{theme_script}
</head>
<body>
<a class="skip" href="#content">Skip to content</a>
{header}
{content}
{footer}
<script src="{js}" defer></script>{reload}
</body>
</html>
"##,
        title = escape(&title),
        og_title = escape_attr(&title),
        description = escape_attr(&page.description),
        icon = escape_attr(&ctx.href(&page.assets.icon)),
        font = escape_attr(&ctx.href(&page.assets.font)),
        css = escape_attr(&ctx.href(&page.assets.css)),
        js = escape_attr(&ctx.href(&page.assets.js)),
        theme_script = THEME_SCRIPT,
        header = header(ctx),
        footer = footer(ctx),
    )
}

fn header(ctx: &Ctx<'_>) -> String {
    format!(
        r#"<header class="top">
<a class="brand" href="{home}">{mark}<span class="wordmark">trafford</span></a>
<nav class="top-nav">
<a href="{docs}">Docs</a>
<a href="{keys}">Keys</a>
<a href="{themes}">Themes</a>
<a href="https://github.com/oddurs/trafford" rel="noreferrer noopener">GitHub</a>
</nav>
<button class="theme-toggle" type="button" hidden aria-label="Change theme">
<span class="theme-name">theme</span>
</button>
</header>"#,
        home = escape_attr(&ctx.href("")),
        docs = escape_attr(&ctx.href("docs/getting-started/")),
        keys = escape_attr(&ctx.href("docs/keys/")),
        themes = escape_attr(&ctx.href("docs/themes/")),
        // `currentColor`, so the mark takes the accent from the stylesheet and
        // changes with the theme like everything else.
        mark = crate::build::mark("currentColor", None),
    )
}

/// A sitemap rather than a line.
///
/// The thing an open-source site has that a company's cannot fake is that the
/// work is visible: the roadmap, the source, the licence, the item that argued
/// for whatever you are reading. A footer is where that goes.
fn footer(ctx: &Ctx<'_>) -> String {
    let column = |title: &str, links: &[(&str, &str)]| -> String {
        let items: String = links
            .iter()
            .map(|(label, href)| {
                let external = href.starts_with("http");
                format!(
                    "<li><a href=\"{}\"{}>{}</a></li>",
                    escape_attr(&if external {
                        (*href).to_string()
                    } else {
                        ctx.href(href)
                    }),
                    if external {
                        " rel=\"noreferrer noopener\""
                    } else {
                        ""
                    },
                    escape(label)
                )
            })
            .collect();
        format!("<div><p class=\"foot-title\">{title}</p><ul>{items}</ul></div>")
    };

    format!(
        r#"<footer class="bottom">
<div class="foot-grid">
<div class="foot-brand">
<a class="brand" href="{home}">{mark}<span class="wordmark">trafford</span></a>
<p>A terminal knowledge base. MIT licensed, one binary, no telemetry.</p>
<p class="foot-fine">This site is generated by the program it documents.</p>
</div>
{start}
{reference}
{project}
</div>
</footer>"#,
        home = escape_attr(&ctx.href("")),
        mark = crate::build::mark("currentColor", None),
        start = column(
            "Start",
            &[
                ("Getting started", "docs/getting-started/"),
                ("The vault", "docs/the-vault/"),
                ("Editing", "docs/editing/"),
                ("Reading", "docs/reading/"),
            ],
        ),
        reference = column(
            "Reference",
            &[
                ("Keys", "docs/keys/"),
                ("Configuration", "docs/configuration/"),
                ("Themes", "docs/themes/"),
                ("API", "api/"),
            ],
        ),
        project = column(
            "Project",
            &[
                ("Source", "https://github.com/oddurs/trafford"),
                (
                    "Roadmap",
                    "https://github.com/oddurs/trafford/blob/main/ROADMAP.md"
                ),
                ("Working with agents", "docs/version-control-with-agents/",),
                (
                    "Licence",
                    "https://github.com/oddurs/trafford/blob/main/LICENSE"
                ),
            ],
        ),
    )
}

/// Applied before the first paint, so a reader who chose light does not get a
/// dark flash on every navigation. Inline for the same reason: an external
/// file is a round trip that happens after the page has already been painted.
const THEME_SCRIPT: &str = r#"<script>
(function () {
  try {
    var t = localStorage.getItem("trafford-theme");
    if (t) document.documentElement.setAttribute("data-theme", t);
  } catch (e) {}
})();
</script>"#;

/// Only `serve` ever injects this. It is not written to disk by a build, which
/// is what keeps a deployed page free of development code.
const RELOAD_SCRIPT: &str = r#"
<script>
(function () {
  var banner;
  function show(text) {
    if (!banner) {
      banner = document.createElement("div");
      banner.className = "build-error";
      document.body.appendChild(banner);
    }
    banner.textContent = text;
  }
  var es = new EventSource("/_reload");
  es.addEventListener("reload", function () { location.reload(); });
  es.addEventListener("error-report", function (e) { show(e.data); });
  es.addEventListener("ok", function () { if (banner) { banner.remove(); banner = null; } });
})();
</script>"#;
