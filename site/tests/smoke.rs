//! Build the real site, serve it, and ask it for every page.
//!
//! The failure modes this catches are not compile errors and are not visible
//! from inside the process that produced them: a page that builds and then
//! 500s for a path with a trailing slash, a stylesheet served as `text/plain`
//! so the page arrives unstyled, an asset written but never linked. That is
//! the same reason `tools/probe.py` exists for the terminal — none of it can
//! be seen from where it was made.
//!
//! Every server here binds port zero, so these run in parallel, in any number,
//! on a machine already running two development servers. A fixed test port
//! would make that impossible and would fail on a laptop where something
//! unrelated already held it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use trafford::testing::TempDir;
use trafford_site::build::{self, Options};
use trafford_site::serve::{self, Server};

/// The project's actual `docs/`, not a fixture: what is being tested is that
/// *this site* serves, and a fixture would pass while the real tree 404s.
fn docs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root")
        .join("docs")
}

struct Site {
    _out: TempDir,
    root: PathBuf,
    url: String,
    pages: Vec<String>,
}

fn built_and_served() -> Site {
    let out = TempDir::new();
    let root = out.path().join("site");
    let built = build::build(&Options::new(docs(), &root)).expect("the site builds");
    let server = Server::bind(&root, None).expect("an ephemeral port");
    let url = server.url();
    std::thread::spawn(move || {
        let _ = server.run();
    });
    Site {
        _out: out,
        root,
        url,
        pages: built.pages,
    }
}

#[test]
fn every_page_the_build_produced_is_served() {
    let site = built_and_served();
    assert!(site.pages.len() > 5, "only {} pages", site.pages.len());
    for page in &site.pages {
        let url = format!("{}/{page}", site.url);
        let (status, headers, body) = serve::get(&url).expect("a response");
        assert_eq!(status, 200, "{url}");
        assert_eq!(headers["content-type"], "text/html; charset=utf-8", "{url}");
        assert!(body.len() > 500, "{url} returned {} bytes", body.len());
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("<main"), "{url} has no main element");
        assert!(text.contains("</html>"), "{url} is truncated");
    }
}

/// The manifest is what CI and the search index read. If it disagrees with
/// what was written, both are working from a list that is not the site.
#[test]
fn the_manifest_lists_exactly_what_was_built() {
    let site = built_and_served();
    let manifest = std::fs::read_to_string(site.root.join("pages.json")).unwrap();
    let listed: BTreeSet<String> = manifest
        .lines()
        .filter_map(|l| l.trim().trim_end_matches(',').strip_prefix('"'))
        .filter_map(|l| l.strip_suffix('"'))
        .map(str::to_string)
        .collect();
    let expected: BTreeSet<String> = site.pages.iter().map(|p| format!("/{p}")).collect();
    assert_eq!(listed, expected);
}

/// Every asset a page asks for has to exist. A hashed filename that does not
/// match what was written is an unstyled site, and nothing else notices.
#[test]
fn every_asset_a_page_links_to_is_served() {
    let site = built_and_served();
    let (_, _, body) = serve::get(&format!("{}/", site.url)).unwrap();
    let html = String::from_utf8_lossy(&body).to_string();

    let mut checked = 0;
    for attr in ["href=\"", "src=\""] {
        for part in html.split(attr).skip(1) {
            let value = part.split('"').next().unwrap_or("");
            // Local assets only: an external link is not this test's business
            // and a fragment is not a request.
            if !value.starts_with("assets/") {
                continue;
            }
            let (status, headers, _) =
                serve::get(&format!("{}/{value}", site.url)).expect("a response");
            assert_eq!(status, 200, "{value}");
            assert_ne!(
                headers["content-type"], "application/octet-stream",
                "{value} has no content type of its own"
            );
            checked += 1;
        }
    }
    assert!(checked >= 2, "only checked {checked} assets");
}

#[test]
fn a_directory_url_without_its_slash_redirects() {
    let site = built_and_served();
    let (status, headers, _) = serve::get(&format!("{}/docs/getting-started", site.url)).unwrap();
    assert_eq!(status, 301);
    assert_eq!(headers["location"], "/docs/getting-started/");
}

#[test]
fn a_missing_page_gets_the_styled_404() {
    let site = built_and_served();
    let (status, _, body) = serve::get(&format!("{}/no/such/page/", site.url)).unwrap();
    assert_eq!(status, 404);
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("stylesheet"), "the 404 is unstyled");
}

/// Two of these at once, which is the case a fixed port makes impossible and
/// the case this project is always in.
#[test]
fn two_sites_serve_at_once_without_shadowing_each_other() {
    let a = built_and_served();
    let b = built_and_served();
    assert_ne!(a.url, b.url);
    for site in [&a, &b] {
        let (status, _, _) = serve::get(&format!("{}/", site.url)).unwrap();
        assert_eq!(status, 200);
    }
}

/// Nothing on the deployed site may reach another host. A CDN, a font service
/// or an analytics tag is a request that can be down, slow, or watching.
#[test]
fn no_page_asks_the_network_for_anything() {
    let site = built_and_served();
    for page in &site.pages {
        let (_, _, body) = serve::get(&format!("{}/{page}", site.url)).unwrap();
        let html = String::from_utf8_lossy(&body).to_string();
        for attr in ["href=\"", "src=\""] {
            for part in html.split(attr).skip(1) {
                let value = part.split('"').next().unwrap_or("");
                let external = value.starts_with("http://") || value.starts_with("https://");
                // A link a reader clicks is fine; a resource the page *loads*
                // is not, and only the latter appears in `src`.
                if external && attr == "src=\"" {
                    panic!("/{page} loads {value} from another host");
                }
            }
        }
        assert!(
            !html.contains("fonts.googleapis.com") && !html.contains("cdn."),
            "/{page} references a CDN"
        );
    }
}

/// The reload client is injected by `serve` and never written by `build`.
#[test]
fn a_built_page_carries_no_development_code() {
    let site = built_and_served();
    for page in &site.pages {
        let (_, _, body) = serve::get(&format!("{}/{page}", site.url)).unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(
            !html.contains(serve::RELOAD_PATH),
            "/{page} carries the reload client"
        );
        assert!(
            !html.contains("EventSource"),
            "/{page} opens an event stream"
        );
    }
}

/// Every control on the page is an enhancement, and a reader without
/// JavaScript must not be shown one that does nothing.
///
/// The cast player is the case that made this worth pinning: it replaces the
/// still screenshot, so if it ever became part of the markup rather than
/// something script builds, a reader with no JavaScript would get an empty box
/// where the picture was.
#[test]
fn a_page_without_javascript_has_no_dead_controls() {
    let site = built_and_served();
    for page in &site.pages {
        let (_, _, body) = serve::get(&format!("{}/{page}", site.url)).unwrap();
        let html = String::from_utf8_lossy(&body).to_string();
        for button in html.split("<button").skip(1) {
            let tag = button.split('>').next().unwrap_or("");
            assert!(
                tag.contains(" hidden"),
                "/{page} ships a button script has to reveal, without `hidden`: <button{tag}>"
            );
        }
        assert!(
            !html.contains("cast-screen"),
            "/{page} has the cast in its markup; it must be built by script"
        );
    }
}

/// Nothing heavy is fetched before it is needed.
///
/// Six recordings is 132 KiB. If the browser were told to fetch them — a
/// `src` rather than a `data-` attribute — the landing page would be a
/// megabyte before a word of it was read. The casts are named in an attribute
/// the browser ignores and asked for by script on approach, and every poster
/// says `loading="lazy"`.
#[test]
fn nothing_heavy_is_fetched_before_it_is_needed() {
    let site = built_and_served();
    for page in &site.pages {
        let (_, _, body) = serve::get(&format!("{}/{page}", site.url)).unwrap();
        let html = String::from_utf8_lossy(&body).to_string();

        for attr in ["src=\"", "href=\""] {
            for part in html.split(attr).skip(1) {
                let value = part.split('"').next().unwrap_or("");
                assert!(
                    !value.ends_with(".cast.json"),
                    "/{page} tells the browser to fetch {value} on load"
                );
            }
        }
        for img in html.split("<img").skip(1) {
            let tag = img.split('>').next().unwrap_or("");
            assert!(
                tag.contains("loading=\"lazy\""),
                "/{page} has an image that loads eagerly: <img{tag}>"
            );
        }
    }
}

/// Anchors are what a `[[Note#Heading]]` link lands on, and the build already
/// refuses a link to a heading that is not there. This is the other half: the
/// heading it *is* there, with the id the link was rewritten to.
#[test]
fn every_in_page_anchor_a_page_links_to_exists_on_the_page_it_names() {
    let site = built_and_served();
    let mut pages: Vec<(String, String)> = Vec::new();
    for page in &site.pages {
        let (_, _, body) = serve::get(&format!("{}/{page}", site.url)).unwrap();
        pages.push((page.clone(), String::from_utf8_lossy(&body).to_string()));
    }
    let ids: Vec<(String, BTreeSet<String>)> = pages
        .iter()
        .map(|(page, html)| {
            let ids = html
                .split("id=\"")
                .skip(1)
                .filter_map(|p| p.split('"').next())
                .map(str::to_string)
                .collect();
            (page.clone(), ids)
        })
        .collect();

    for (page, html) in &pages {
        for part in html.split("href=\"").skip(1) {
            let value = part.split('"').next().unwrap_or("");
            let Some((path, anchor)) = value.split_once('#') else {
                continue;
            };
            if anchor.is_empty() || anchor == "content" {
                continue;
            }
            // Resolve the relative href back to a page in the manifest.
            let target = if path.is_empty() {
                page.clone()
            } else {
                match normalise(page, path) {
                    Some(t) => t,
                    None => continue,
                }
            };
            let Some((_, found)) = ids.iter().find(|(p, _)| *p == target) else {
                continue;
            };
            assert!(
                found.contains(anchor),
                "/{page} links to #{anchor} on /{target}, which has no such id"
            );
        }
    }
}

/// `../../docs/keys/` seen from `docs/reading/` is `docs/keys/`.
fn normalise(from: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(format!("{}/", parts.join("/")).replace("//", "/"))
}

/// A guard on the guard: the smoke tests are worthless if the server they use
/// would serve anything at all.
#[test]
fn the_server_is_still_refusing_to_leave_its_root() {
    let site = built_and_served();
    let (status, _, body) = serve::get(&format!("{}/../../Cargo.toml", site.url)).unwrap();
    assert!(status == 403 || status == 404);
    assert!(!String::from_utf8_lossy(&body).contains("[workspace]"));
}
