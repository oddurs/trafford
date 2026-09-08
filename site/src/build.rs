//! Turning `docs/` — which is a vault — into a directory of HTML.
//!
//! Three properties are worth more here than any feature, because they are
//! what let everything downstream trust the output:
//!
//!   * **Deterministic.** Same commit, byte-identical tree. Every iteration is
//!     over a sorted collection and nothing writes a timestamp, which is what
//!     lets CI regenerate and `git diff --exit-code`.
//!   * **Atomic.** The tree is built beside the live one and swapped into
//!     place, so a failed build leaves the last good site standing and the
//!     development server never serves a half-written page.
//!   * **Loud.** A link that resolves to nothing fails the build with a file
//!     and a line. A 404 found by a reader is a bug that got all the way out.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use trafford::vault::note::{frontmatter_block, slug};
use trafford::vault::{Note, Vault};

use crate::html::{self, Ctx, PageLink, Problem, ASSET_DIR};
use crate::shell::{self, Assets, NavItem};

/// The stylesheet and behaviour, checked into the repo and read at build time.
const STYLE: &str = include_str!("../assets/site.css");
const SCRIPT: &str = include_str!("../assets/site.js");

/// The typeface, subset by `tools/subset-font.py` to the characters the built
/// site actually draws. Self-hosted because a font service is a request
/// leaving the origin, which no page here makes.
const FONT: &[u8] = include_bytes!("../assets/fonts/jetbrains-mono.woff2");

/// What `site.css` writes instead of a font URL, since the URL carries a hash
/// of the font's own bytes and is only known at build time. Relative to the
/// stylesheet, which is where a CSS `url()` resolves from.
const FONT_URL: &str = "FONT_URL";

pub struct Options {
    /// The vault to render.
    pub docs: PathBuf,
    /// Where the finished tree goes. Replaced atomically.
    pub out: PathBuf,
    /// Absolute origin, for canonical links and the sitemap. A local build has
    /// none, and a canonical tag pointing at a random port is worse than none.
    pub site_url: Option<String>,
    /// Inject the live-reload client. Only `serve` sets this.
    pub reload: bool,
    /// Build the API documentation and publish it under `/api`.
    ///
    /// Off by default, and deliberately: `cargo doc` over the workspace is
    /// tens of seconds, and the watch loop in `serve` lives or dies on the
    /// rebuild staying under a second. CI and the deploy turn it on.
    pub api: bool,
}

impl Options {
    pub fn new(docs: impl Into<PathBuf>, out: impl Into<PathBuf>) -> Options {
        Options {
            docs: docs.into(),
            out: out.into(),
            site_url: None,
            reload: false,
            api: false,
        }
    }
}

#[derive(Debug)]
pub struct Built {
    /// Root-relative page paths, sorted. The smoke tests walk this.
    pub pages: Vec<String>,
    pub bytes: u64,
}

/// What a note said about itself in its frontmatter.
struct Meta {
    layout: String,
    section: String,
    order: i64,
    description: Option<String>,
    tagline: Option<String>,
    headline: Option<String>,
    install: Option<String>,
    screenshot: Option<String>,
    cast: Option<String>,
}

fn meta(note: &Note) -> Meta {
    let lines: Vec<String> = note.text.lines().map(str::to_string).collect();
    // A property holds a list, because `tags:` does. None of the keys read
    // here is ever more than one value, but joining rather than taking the
    // first means a key that grows one does not silently lose the rest.
    let pairs: BTreeMap<String, String> = frontmatter_block(&lines)
        .map(|(p, _)| p.into_iter().map(|(k, v)| (k, v.join(", "))).collect())
        .unwrap_or_default();
    let get = |k: &str| pairs.get(k).map(|v| v.trim().trim_matches('"').to_string());
    Meta {
        layout: get("layout").unwrap_or_else(|| "doc".into()),
        section: get("section").unwrap_or_else(|| "Documentation".into()),
        order: get("order").and_then(|v| v.parse().ok()).unwrap_or(100),
        description: get("description"),
        tagline: get("tagline"),
        headline: get("headline"),
        install: get("install"),
        screenshot: get("screenshot"),
        cast: get("cast"),
    }
}

/// Build the site. Returns what was written, or every problem found.
pub fn build(opts: &Options) -> Result<Built> {
    let vault =
        Vault::open(&opts.docs).with_context(|| format!("reading {}", opts.docs.display()))?;
    if vault.notes.is_empty() {
        bail!("{} holds no notes", opts.docs.display());
    }

    // Sorted by id, so page order — and therefore every byte downstream — does
    // not depend on the order the filesystem handed the files back.
    let mut notes: Vec<&Note> = vault.notes.iter().collect();
    notes.sort_by(|a, b| a.id.cmp(&b.id));

    // Frontmatter is parsed once per note, here, and read from this map
    // everywhere else. `url_for` used to re-parse on every call and is called
    // once per navigation entry per page, which is quadratic in a docs tree
    // that only has to grow a little to notice.
    let metas: BTreeMap<String, Meta> = notes.iter().map(|n| (n.id.clone(), meta(n))).collect();
    let pages: BTreeMap<String, PageLink> = notes
        .iter()
        .map(|note| {
            (
                note.id.clone(),
                PageLink {
                    url: url_for(note, &metas[&note.id]),
                    title: note.title.clone(),
                },
            )
        })
        .collect();

    let font = hashed_bytes("assets/fonts/jetbrains-mono", "woff2", FONT);
    let css = stylesheet(&font)?;
    let assets = Assets {
        css: hashed("assets/site", "css", &css),
        js: hashed("assets/site", "js", SCRIPT),
        icon: "assets/favicon.svg".to_string(),
        font,
    };

    // Navigation is every page but the landing one, in the order the notes ask
    // for and then alphabetically — a stable order rather than a lucky one.
    let mut nav_source: Vec<(&Note, &Meta)> = notes
        .iter()
        .map(|n| (*n, &metas[&n.id]))
        .filter(|(_, m)| m.layout != "landing")
        .collect();
    nav_source
        .sort_by(|(a, ma), (b, mb)| (ma.order, &a.title, &a.id).cmp(&(mb.order, &b.title, &b.id)));

    let staging = staging_dir(&opts.out);
    if staging.exists() {
        fs::remove_dir_all(&staging).ok();
    }
    fs::create_dir_all(&staging).with_context(|| format!("creating {}", staging.display()))?;

    let mut problems: Vec<Problem> = Vec::new();
    let mut written: Vec<(String, Vec<u8>)> = Vec::new();
    let mut page_paths: Vec<String> = Vec::new();

    for note in &notes {
        let m = &metas[&note.id];
        let url = pages[&note.id].url.clone();
        let depth = url.matches('/').count();
        let ctx = Ctx {
            vault: &vault,
            pages: &pages,
            depth,
        };
        let rendered = if m.layout == "landing" {
            html::render_sectioned(note, &ctx, &mut problems)
        } else {
            html::render(note, &ctx, &mut problems)
        };
        let nav: Vec<NavItem> = nav_source
            .iter()
            .map(|(n, nm)| NavItem {
                title: n.title.clone(),
                url: pages[&n.id].url.clone(),
                section: nm.section.clone(),
                current: n.id == note.id,
            })
            .collect();
        let page = shell::Page {
            title: note.title.clone(),
            description: m
                .description
                .clone()
                .or_else(|| m.tagline.clone())
                .unwrap_or_else(|| rendered.summary.clone()),
            url: url.clone(),
            body: rendered.html,
            toc: &rendered.toc,
            nav: &nav,
            assets: &assets,
            site_url: opts.site_url.as_deref(),
            reload: opts.reload,
        };
        let doc = if m.layout == "landing" {
            let hero = hero(&ctx, note, m, &vault, &mut problems);
            shell::landing(&ctx, &page, &hero, &install(m))
        } else {
            shell::doc(&ctx, &page)
        };
        written.push((format!("{url}index.html"), doc.into_bytes()));
        page_paths.push(url);
    }

    if !problems.is_empty() {
        problems.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        let list = problems
            .iter()
            .map(|p| format!("  {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        // Nothing has been swapped into place, so the last good site is still
        // there and the reader sees no part of this.
        fs::remove_dir_all(&staging).ok();
        bail!(
            "{} problem(s) in {}:\n{list}",
            problems.len(),
            opts.docs.display()
        );
    }

    written.push((assets.css.clone(), css.into_bytes()));
    written.push((assets.js.clone(), SCRIPT.as_bytes().to_vec()));
    written.push((assets.font.clone(), FONT.to_vec()));
    written.push((assets.icon.clone(), favicon()?.into_bytes()));
    written.push((
        "design/index.html".into(),
        specimen_page(&vault, &pages, &nav_source, &assets, opts)?.into_bytes(),
    ));
    page_paths.push("design/".into());
    written.push(("404.html".into(), not_found(&assets)?.into_bytes()));
    written.push(("pages.json".into(), manifest(&page_paths).into_bytes()));
    if let Some(origin) = &opts.site_url {
        written.push((
            "sitemap.xml".into(),
            sitemap(origin, &page_paths).into_bytes(),
        ));
        written.push((
            "robots.txt".into(),
            format!("User-agent: *\nAllow: /\nSitemap: {origin}/sitemap.xml\n").into_bytes(),
        ));
    }

    for (rel, body) in &written {
        write_file(&staging.join(rel), body)?;
    }
    let copied = copy_attachments(&opts.docs, &staging.join(ASSET_DIR))?;
    let api = if opts.api { build_api(&staging)? } else { 0 };
    check_api_links(&written, &staging, opts.api)?;

    swap(&staging, &opts.out)?;

    page_paths.sort();
    let bytes = written.iter().map(|(_, b)| b.len() as u64).sum::<u64>() + copied + api;
    Ok(Built {
        pages: page_paths,
        bytes,
    })
}

/// The design system, as a page.
///
/// Generated rather than written, from the same constants that generate the
/// stylesheet — a specimen that can disagree with what the site uses is worse
/// than none.
fn specimen_page(
    vault: &Vault,
    pages: &BTreeMap<String, PageLink>,
    nav_source: &[(&Note, &Meta)],
    assets: &Assets,
    opts: &Options,
) -> Result<String> {
    let ctx = Ctx {
        vault,
        pages,
        depth: 1,
    };
    let body = crate::design::specimen(&crate::palette::swatches()?);
    let nav: Vec<NavItem> = nav_source
        .iter()
        .map(|(n, nm)| NavItem {
            title: n.title.clone(),
            url: pages[&n.id].url.clone(),
            section: nm.section.clone(),
            current: false,
        })
        .collect();
    let page = shell::Page {
        title: "The design system".into(),
        description:
            "Every measurement trafford's site makes, generated from one source and drawn at the size it is."
                .into(),
        url: "design/".into(),
        body,
        toc: &[],
        nav: &nav,
        assets,
        site_url: opts.site_url.as_deref(),
        reload: opts.reload,
    };
    Ok(shell::doc(&ctx, &page))
}

/// Where a note lands. The landing page is the root; everything else is a
/// directory with an `index.html`, so its URL ends in a slash and relative
/// links from it are predictable.
fn url_for(note: &Note, m: &Meta) -> String {
    if m.layout == "landing" {
        String::new()
    } else {
        format!("docs/{}/", slug(note.stem()))
    }
}

/// The one screen a reader gets before they decide.
///
/// A claim, a sentence under it, the command, and then the program at full
/// width. The claim is the biggest thing on the page: a product name in that
/// position tells a reader what something is called rather than what it does.
fn hero(
    ctx: &Ctx<'_>,
    note: &Note,
    m: &Meta,
    vault: &Vault,
    problems: &mut Vec<Problem>,
) -> String {
    let mut out = String::from("<section class=\"hero\">\n");
    let _ = writeln!(
        out,
        "<h1>{}</h1>",
        html::escape(m.headline.as_deref().unwrap_or(&note.title))
    );
    if let Some(tagline) = &m.tagline {
        let _ = writeln!(out, "<p class=\"tagline\">{}</p>", html::escape(tagline));
    }
    let _ = writeln!(
        out,
        "<div class=\"actions\">{}<a class=\"secondary\" href=\"{}\">Read the docs →</a></div>\n{}",
        install(m),
        html::escape_attr(&ctx.href("docs/getting-started/")),
        facts().unwrap_or_default(),
    );
    if let Some(shot) = &m.screenshot {
        match vault.attachment(shot) {
            Some(rel) => {
                // The still is the content; the cast is an enhancement layered
                // over it. With no JavaScript, with reduced motion, or before
                // the frames arrive, this is what a reader sees — which is why
                // the still stays generated even now that the cast exists.
                let cast = match &m.cast {
                    Some(name) => match vault.attachment(name) {
                        Some(cast_rel) => format!(
                            " data-cast=\"{}\"",
                            html::escape_attr(&ctx.href(&format!("{ASSET_DIR}/{cast_rel}")))
                        ),
                        None => {
                            problems.push(Problem {
                                file: note.id.clone(),
                                line: 1,
                                message: format!("cast: {name} is not in the vault"),
                            });
                            String::new()
                        }
                    },
                    None => String::new(),
                };
                let _ = writeln!(
                    out,
                    "<figure class=\"shot\"{cast}>{}</figure>",
                    inline_or_img(ctx, &vault.root, rel)
                );
            }
            None => problems.push(Problem {
                file: note.id.clone(),
                line: 1,
                message: format!("screenshot: {shot} is not in the vault"),
            }),
        }
    }
    out.push_str("</section>\n");
    out
}

/// Facts about the project, counted rather than typed.
///
/// A number on a landing page is a claim, and a claim that is typed goes stale
/// the first time it changes and nobody notices. These are read out of the
/// repository at build time, so they are either right or the build is wrong.
fn facts() -> Result<String> {
    let root = workspace_root();
    let manifest = fs::read_to_string(root.join("trafford/Cargo.toml"))
        .context("reading the application's manifest for the dependency count")?;
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap_or("")
        .lines()
        .take_while(|l| !l.trim_start().starts_with('['))
        .filter(|l| l.contains('=') && !l.trim_start().starts_with('#'))
        .count();

    let mut tests = 0;
    let mut stack = vec![
        root.join("trafford/src"),
        root.join("site/src"),
        root.join("site/tests"),
    ];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            let mut entries: Vec<PathBuf> = fs::read_dir(&path)
                .with_context(|| format!("reading {}", path.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect();
            entries.sort();
            stack.extend(entries);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            tests += fs::read_to_string(&path)?.matches("#[test]").count();
        }
    }

    Ok(format!(
        "<p class=\"facts\">MIT licensed · {deps} direct dependencies · {tests} tests · no telemetry</p>"
    ))
}

/// The command, with a button that copies it.
///
/// For a program you install with one line, the command *is* the call to
/// action — a solid button here would be a link to a page showing this.
fn install(m: &Meta) -> String {
    match &m.install {
        Some(cmd) => format!(
            "<div class=\"install\"><code>{cmd}</code><button type=\"button\" class=\"copy\" data-copy=\"{attr}\" hidden>Copy</button></div>",
            cmd = html::escape(cmd),
            attr = html::escape_attr(cmd),
        ),
        None => String::new(),
    }
}

/// An SVG screenshot goes into the page rather than beside it: it is a few
/// kilobytes, it scales to any display, and inlining it means the first paint
/// needs no second request. Anything else is an `<img>`.
fn inline_or_img(ctx: &Ctx<'_>, root: &Path, rel: &str) -> String {
    if rel.to_lowercase().ends_with(".svg") {
        if let Ok(body) = fs::read_to_string(root.join(rel)) {
            return body
                .lines()
                .filter(|l| !l.trim_start().starts_with("<?xml"))
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
    format!(
        "<img src=\"{}\" alt=\"trafford, running\">",
        html::escape_attr(&ctx.href(&format!("{ASSET_DIR}/{rel}")))
    )
}

/// The generated palette, then the hand-written rules, with the font URL filled
/// in. One file: a second request for the palette would be a round trip before
/// the page has a colour.
fn stylesheet(font: &str) -> Result<String> {
    // `assets/fonts/x.woff2` seen from `assets/site.css` is `fonts/x.woff2`.
    let relative = font
        .strip_prefix("assets/")
        .expect("the font is written under assets/");
    Ok(format!(
        "{}\n{}\n{}",
        crate::palette::stylesheet()?,
        crate::design::stylesheet(),
        STYLE.replace(FONT_URL, relative)
    ))
}

/// The mark: the bar trafford draws beside the note you are reading, and three
/// lines of a note beside it.
///
/// One geometry, used by the favicon and by the header, because a logo that is
/// drawn twice is a logo that is drawn differently. `fill` lets the header take
/// the accent from the stylesheet — the favicon cannot, since a tab icon has no
/// cascade to read from.
pub fn mark(fill: &str, background: Option<&str>) -> String {
    let ground = match background {
        Some(bg) => format!("<rect width=\"32\" height=\"32\" rx=\"6\" fill=\"{bg}\"/>"),
        None => String::new(),
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\" aria-hidden=\"true\">\
{ground}\
<rect x=\"8\" y=\"7\" width=\"4\" height=\"18\" fill=\"{fill}\"/>\
<rect x=\"15\" y=\"7\" width=\"9\" height=\"3\" fill=\"{fill}\" opacity=\".8\"/>\
<rect x=\"15\" y=\"14\" width=\"9\" height=\"3\" fill=\"{fill}\" opacity=\".55\"/>\
<rect x=\"15\" y=\"21\" width=\"6\" height=\"3\" fill=\"{fill}\" opacity=\".35\"/>\
</svg>"
    )
}

/// The mark as a tab icon, in the default theme's accent. A favicon has no
/// stylesheet to read a colour from, so this one is baked.
fn favicon() -> Result<String> {
    let (_, theme) = crate::palette::translatable()?
        .into_iter()
        .find(|(_, t)| t.dark)
        .ok_or_else(|| anyhow!("no dark theme for the favicon"))?;
    Ok(format!(
        "{}\n",
        mark(
            &crate::palette::hex_of(theme.accent)?,
            Some(&crate::palette::hex_of(theme.bg)?)
        )
    ))
}

/// GitHub Pages serves this for anything it cannot find.
fn not_found(assets: &Assets) -> Result<String> {
    // Absolute asset paths, because a 404 is served at a URL nobody planned
    // and a relative path from `/a/b/c/nope` would climb out of the site.
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Not found — trafford</title>
<link rel="stylesheet" href="/{css}">
</head>
<body>
<main id="content" class="notfound">
<h1>404</h1>
<p>There is no page at that address.</p>
<p><a href="/">Back to the start</a></p>
</main>
</body>
</html>
"#,
        css = assets.css
    ))
}

/// The pages, for our own tooling. Sorted, so it diffs cleanly.
fn manifest(pages: &[String]) -> String {
    let mut sorted: Vec<&String> = pages.iter().collect();
    sorted.sort();
    let list = sorted
        .iter()
        .map(|p| format!("  \"/{p}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    format!("[\n{list}\n]\n")
}

fn sitemap(origin: &str, pages: &[String]) -> String {
    let origin = origin.trim_end_matches('/');
    let mut sorted: Vec<&String> = pages.iter().collect();
    sorted.sort();
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in sorted {
        // No `lastmod`: it would be a timestamp, and a timestamp is the end of
        // a byte-identical rebuild.
        let _ = writeln!(out, "  <url><loc>{origin}/{page}</loc></url>");
    }
    out.push_str("</urlset>\n");
    out
}

/// Copy everything that is not a note: images, casts, anything embedded.
fn copy_attachments(from: &Path, to: &Path) -> Result<u64> {
    let mut bytes = 0;
    let mut stack = vec![from.to_path_buf()];
    let mut files: Vec<PathBuf> = Vec::new();
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) != Some("md") {
                files.push(path);
            }
        }
    }
    files.sort();
    for path in files {
        let rel = path.strip_prefix(from).unwrap_or(&path);
        let body = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        bytes += body.len() as u64;
        write_file(&to.join(rel), &body)?;
    }
    Ok(bytes)
}

/// Run `cargo doc` and put its output under `/api`.
///
/// Shelling out to cargo rather than linking rustdoc, for the same reason git
/// is shelled out to: it is the tool the reader already has, with their
/// toolchain and their flags. `RUSTDOCFLAGS=-D warnings` is CI's business, not
/// this function's.
fn build_api(staging: &Path) -> Result<u64> {
    let workspace = workspace_root();
    let status = std::process::Command::new(std::env::var("CARGO").unwrap_or("cargo".into()))
        .args(["doc", "--no-deps", "--workspace", "--quiet"])
        .current_dir(&workspace)
        .status()
        .context("running `cargo doc`")?;
    if !status.success() {
        bail!("`cargo doc` failed");
    }
    let from = workspace.join("target").join("doc");
    let to = staging.join("api");
    let bytes = copy_tree(&from, &to)?;
    // rustdoc's own entry point is a path nobody types.
    write_file(
        &to.join("index.html"),
        b"<!doctype html><meta charset=\"utf-8\">\
<title>trafford API</title>\
<meta http-equiv=\"refresh\" content=\"0; url=trafford/index.html\">\
<a href=\"trafford/index.html\">trafford</a>\n",
    )?;
    Ok(bytes)
}

/// The workspace root, found from this crate rather than from the working
/// directory — `site build` should work from anywhere.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// Every prose link into `/api` points at a page rustdoc actually wrote.
///
/// This is what makes a renamed type a build failure rather than a 404 for a
/// reader. Without `--api` there is nothing to check against, so the links are
/// reported rather than silently believed.
fn check_api_links(written: &[(String, Vec<u8>)], staging: &Path, api: bool) -> Result<()> {
    let mut missing = Vec::new();
    let mut skipped = 0;
    for (rel, body) in written {
        if !rel.ends_with(".html") {
            continue;
        }
        let html = String::from_utf8_lossy(body);
        for part in html.split("href=\"").skip(1) {
            let value = part.split('"').next().unwrap_or("");
            let Some(target) = value.split('#').next() else {
                continue;
            };
            let Some(at) = target.find("api/") else {
                continue;
            };
            if target.starts_with("http") {
                continue;
            }
            if !api {
                skipped += 1;
                continue;
            }
            if !staging.join(&target[at..]).exists() {
                missing.push(format!("  {rel} -> {target}"));
            }
        }
    }
    if !missing.is_empty() {
        bail!(
            "{} link(s) into /api point at pages rustdoc did not write:\n{}",
            missing.len(),
            missing.join("\n")
        );
    }
    if skipped > 0 {
        eprintln!("note: {skipped} link(s) into /api not checked — build with --api to check them");
    }
    Ok(())
}

/// Copy a directory verbatim. Sorted, so the walk is deterministic even though
/// nothing downstream depends on the order.
fn copy_tree(from: &Path, to: &Path) -> Result<u64> {
    let mut bytes = 0;
    let mut stack = vec![from.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(from).unwrap_or(&path);
                let body = fs::read(&path)?;
                bytes += body.len() as u64;
                write_file(&to.join(rel), &body)?;
            }
        }
    }
    Ok(bytes)
}

fn write_file(path: &Path, body: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, body).with_context(|| format!("writing {}", path.display()))
}

fn staging_dir(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".next-{}", std::process::id()));
    out.with_file_name(name)
}

/// Move `staging` onto `out`, keeping the old tree until the new one is in
/// place.
///
/// `rename` onto a non-empty directory fails, so the old tree steps aside
/// first. The window where neither is at `out` is two renames wide; a reader
/// hitting it gets one 404 rather than a page assembled from both.
fn swap(staging: &Path, out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut previous = out.file_name().unwrap_or_default().to_os_string();
    previous.push(format!(".prev-{}", std::process::id()));
    let previous = out.with_file_name(previous);
    let had_old = out.exists();
    if had_old {
        fs::rename(out, &previous)
            .with_context(|| format!("moving the previous {} aside", out.display()))?;
    }
    match fs::rename(staging, out) {
        Ok(()) => {
            if had_old {
                fs::remove_dir_all(&previous).ok();
            }
            Ok(())
        }
        Err(e) => {
            // Put the old site back rather than leaving nothing at all.
            if had_old {
                fs::rename(&previous, out).ok();
            }
            Err(e).with_context(|| format!("moving the new tree to {}", out.display()))
        }
    }
}

/// `assets/site.<hash>.css`. The name changes when the bytes do, which is what
/// lets the server hand out a year-long cache header without lying.
fn hashed(stem: &str, ext: &str, body: &str) -> String {
    hashed_bytes(stem, ext, body.as_bytes())
}

fn hashed_bytes(stem: &str, ext: &str, body: &[u8]) -> String {
    format!("{stem}.{}.{ext}", fnv(body))
}

/// FNV-1a, eight hex digits. A cache-busting name, not a checksum — no
/// dependency is worth taking for this.
fn fnv(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", (hash >> 32) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use trafford::testing::TempDir;

    fn vault_with(files: &[(&str, &str)]) -> TempDir {
        TempDir::with_files(files)
    }

    const LANDING: &str =
        "---\nlayout: landing\ntagline: notes in a terminal\n---\n\n# trafford\n\nbody\n";

    #[test]
    fn a_build_writes_a_page_per_note() {
        let src = vault_with(&[
            ("index.md", LANDING),
            ("getting-started.md", "# Getting started\n\ntext\n"),
        ]);
        let out = TempDir::new();
        let built = build(&Options::new(src.path(), out.path().join("site"))).unwrap();
        // The landing page, the note, and the specimen — which is a page of
        // the site rather than a note in the vault, because its content is the
        // design tokens and there is nothing for anyone to write.
        assert_eq!(
            built.pages,
            vec![
                "".to_string(),
                "design/".to_string(),
                "docs/getting-started/".to_string()
            ]
        );
        assert!(out.path().join("site/index.html").exists());
        assert!(out
            .path()
            .join("site/docs/getting-started/index.html")
            .exists());
        assert!(out.path().join("site/404.html").exists());
    }

    /// Same input, same bytes. Everything downstream — the CI check that the
    /// generated assets are current, a cache header, a diff in a pull request
    /// — rests on this one property.
    #[test]
    fn two_builds_of_the_same_source_agree_byte_for_byte() {
        let src = vault_with(&[
            ("index.md", LANDING),
            ("a.md", "# A\n\nsee [[b]]\n"),
            ("b.md", "# B\n\ntext\n"),
        ]);
        let out = TempDir::new();
        build(&Options::new(src.path(), out.path().join("site"))).unwrap();
        let first = read_tree(&out.path().join("site"));
        build(&Options::new(src.path(), out.path().join("site"))).unwrap();
        let second = read_tree(&out.path().join("site"));
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn a_broken_link_fails_the_build_and_names_the_line() {
        let src = vault_with(&[("index.md", LANDING), ("a.md", "# A\n\nsee [[Gone]]\n")]);
        let out = TempDir::new();
        let err = build(&Options::new(src.path(), out.path().join("site")))
            .unwrap_err()
            .to_string();
        assert!(err.contains("a.md:3"), "{err}");
        assert!(err.contains("Gone"), "{err}");
    }

    /// The previous site stays up. Anything else means one typo takes the docs
    /// down until it is found.
    #[test]
    fn a_failed_build_leaves_the_last_good_tree_in_place() {
        let good = vault_with(&[("index.md", LANDING), ("a.md", "# A\n\nfine\n")]);
        let out = TempDir::new();
        let target = out.path().join("site");
        build(&Options::new(good.path(), &target)).unwrap();
        let before = read_tree(&target);

        let bad = vault_with(&[("index.md", LANDING), ("a.md", "# A\n\n[[Gone]]\n")]);
        assert!(build(&Options::new(bad.path(), &target)).is_err());
        assert_eq!(read_tree(&target), before);
    }

    #[test]
    fn assets_are_named_by_their_contents() {
        let src = vault_with(&[("index.md", LANDING)]);
        let out = TempDir::new();
        build(&Options::new(src.path(), out.path().join("site"))).unwrap();
        let index = fs::read_to_string(out.path().join("site/index.html")).unwrap();
        let name = index
            .split("href=\"")
            .find(|s| s.starts_with("assets/site."))
            .and_then(|s| s.split('"').next())
            .expect("a stylesheet link");
        assert!(out.path().join("site").join(name).exists(), "{name}");
        assert_eq!(name.matches('.').count(), 2, "no hash in {name}");
    }

    /// A build never writes development code. The reload client is injected by
    /// the server and by nothing else, and this is what pins it.
    #[test]
    fn nothing_a_build_writes_mentions_the_reload_channel() {
        let src = vault_with(&[("index.md", LANDING), ("a.md", "# A\n\ntext\n")]);
        let out = TempDir::new();
        build(&Options::new(src.path(), out.path().join("site"))).unwrap();
        for (_, body) in read_tree(&out.path().join("site")) {
            let text = String::from_utf8_lossy(&body).to_string();
            assert!(!text.contains("_reload"), "a build wrote the reload client");
        }
    }

    #[test]
    fn a_sitemap_appears_only_when_there_is_an_origin_to_put_in_it() {
        let src = vault_with(&[("index.md", LANDING)]);
        let out = TempDir::new();
        let target = out.path().join("site");
        build(&Options::new(src.path(), &target)).unwrap();
        assert!(!target.join("sitemap.xml").exists());

        let mut opts = Options::new(src.path(), &target);
        opts.site_url = Some("https://example.com/trafford".into());
        build(&opts).unwrap();
        let map = fs::read_to_string(target.join("sitemap.xml")).unwrap();
        assert!(
            map.contains("<loc>https://example.com/trafford/</loc>"),
            "{map}"
        );
    }

    fn read_tree(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .to_string();
                    out.push((rel, fs::read(&path).unwrap()));
                }
            }
        }
        out.sort();
        out
    }
}
