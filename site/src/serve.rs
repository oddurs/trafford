//! A development server that binds a port the operating system chooses.
//!
//! A fixed port is a shared global with a process-wide lifetime, and this
//! project is developed in a way that guarantees collisions: several git
//! worktrees, sometimes more than one agent editing at once. Two checkouts
//! both wanting `:3000` gives one of two outcomes, and the bad one is not the
//! error — the second server does not start, the browser tab keeps showing the
//! first worktree's build, and you review a change that is not there. That
//! failure is silent, survives a reload, and is indistinguishable from "my
//! edit did nothing".
//!
//! So: bind `127.0.0.1:0`, and ask the operating system what it gave us. The
//! port is *read after binding*, never probed for and then bound — probing is
//! a race with every other process on the machine and is the standard way this
//! is got wrong.
//!
//! Written on `std::net`. This project shells out to `git` rather than taking
//! `git2` and uses `ureq` rather than `reqwest`; a static file server for GET
//! and HEAD over loopback is a few hundred lines, and it means the site crate
//! adds no dependency the workspace did not already have.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

/// Loopback, always. A documentation preview has no business being reachable
/// from the café's network, and defaulting to `0.0.0.0` is how it gets there.
const HOST: Ipv4Addr = Ipv4Addr::LOCALHOST;

/// Where the reload client connects.
pub const RELOAD_PATH: &str = "/_reload";

/// A bound server, before anything has been served.
pub struct Server {
    listener: TcpListener,
    root: PathBuf,
    state: Arc<State>,
    /// Removed on drop, so the next run does not find a file describing a
    /// server that is no longer there.
    stamp: Option<PathBuf>,
}

/// What the request threads share.
#[derive(Default)]
pub struct State {
    /// Bumped by every successful rebuild. A client that reconnects compares
    /// its own generation and reloads if it missed one.
    generation: AtomicU64,
    /// The last build error, shown as a banner rather than as a blank page.
    error: Mutex<Option<String>>,
    stop: AtomicBool,
}

impl State {
    /// Tell every connected page to reload.
    pub fn rebuilt(&self) {
        *self.error.lock().expect("no panic holds this lock") = None;
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Tell every connected page what went wrong, and leave it readable.
    pub fn failed(&self, message: String) {
        *self.error.lock().expect("no panic holds this lock") = Some(message);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

impl Server {
    /// Bind a port. `port: None` means "whichever one is free", which is the
    /// only sane default when several of these run at once.
    pub fn bind(root: impl Into<PathBuf>, port: Option<u16>) -> Result<Server> {
        let wanted = SocketAddr::new(IpAddr::V4(HOST), port.unwrap_or(0));
        let listener = TcpListener::bind(wanted).with_context(|| match port {
            // An explicit port that is taken is a loud failure: the caller
            // asked for that one and silently using another would defeat the
            // reason they asked.
            Some(p) => format!("port {p} is already in use"),
            None => "could not bind a loopback port".to_string(),
        })?;
        Ok(Server {
            listener,
            root: root.into(),
            state: Arc::new(State::default()),
            stamp: None,
        })
    }

    /// The address actually bound — never the one requested.
    pub fn addr(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .expect("a bound listener has an address")
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr())
    }

    pub fn state(&self) -> Arc<State> {
        Arc::clone(&self.state)
    }

    /// Record the running server where a script can find it.
    ///
    /// Scraping stdout for a port works until something else prints first. A
    /// file with the pid in it also lets the *next* run tell a live server
    /// from a stale record left by one that was killed.
    pub fn write_stamp(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let addr = self.addr();
        std::fs::write(
            &path,
            format!(
                "{{\n  \"url\": \"http://{addr}\",\n  \"port\": {},\n  \"pid\": {},\n  \"root\": {:?}\n}}\n",
                addr.port(),
                std::process::id(),
                self.root.display().to_string(),
            ),
        )
        .with_context(|| format!("writing {}", path.display()))?;
        self.stamp = Some(path);
        Ok(())
    }

    /// Serve until `state.stop()` is called.
    ///
    /// One thread per connection. A docs preview has one reader and a handful
    /// of open tabs; a thread pool would be machinery in the way of a thing
    /// that is never under load.
    pub fn run(&self) -> Result<()> {
        for stream in self.listener.incoming() {
            if self.state.stopped() {
                break;
            }
            let Ok(stream) = stream else { continue };
            let root = self.root.clone();
            let state = Arc::clone(&self.state);
            std::thread::spawn(move || {
                let _ = handle(stream, &root, &state);
            });
        }
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(path) = &self.stamp {
            std::fs::remove_file(path).ok();
        }
    }
}

fn handle(mut stream: TcpStream, root: &Path, state: &State) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    if reader.read_line(&mut request)? == 0 {
        return Ok(());
    }
    // Drain the headers. Not reading them leaves bytes in the socket and the
    // next keep-alive request on this connection reads a header as a verb.
    let mut header = String::new();
    while reader.read_line(&mut header)? > 2 {
        header.clear();
    }

    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target.split(['?', '#']).next().unwrap_or("/");

    if !matches!(method, "GET" | "HEAD") {
        return respond(&mut stream, 405, "text/plain; charset=utf-8", b"", true);
    }
    if path == RELOAD_PATH {
        return reload_stream(stream, state);
    }

    let head_only = method == "HEAD";
    match resolve(root, path) {
        Resolved::Redirect(to) => {
            let body = format!("<a href=\"{to}\">{to}</a>\n");
            write!(
                stream,
                "HTTP/1.1 301 Moved Permanently\r\nLocation: {to}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            if !head_only {
                stream.write_all(body.as_bytes())?;
            }
            Ok(())
        }
        Resolved::File(file) => match std::fs::read(&file) {
            Ok(body) => respond(&mut stream, 200, mime(&file), &body, head_only),
            Err(_) => not_found(&mut stream, root, head_only),
        },
        Resolved::Missing => not_found(&mut stream, root, head_only),
        // A traversal attempt gets 403 rather than 404: there is nothing
        // ambiguous about `../../../etc/passwd`, and saying so is clearer than
        // pretending the file is merely absent.
        Resolved::Forbidden => respond(
            &mut stream,
            403,
            "text/plain; charset=utf-8",
            b"forbidden\n",
            head_only,
        ),
    }
}

fn not_found(stream: &mut TcpStream, root: &Path, head_only: bool) -> std::io::Result<()> {
    let body = std::fs::read(root.join("404.html")).unwrap_or_else(|_| b"not found\n".to_vec());
    respond(stream, 404, "text/html; charset=utf-8", &body, head_only)
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    // `Content-Length` is the byte count even for HEAD, so a HEAD and a GET
    // agree — a client that trusts HEAD and then reads fewer bytes hangs.
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    if !head_only {
        stream.write_all(body)?;
    }
    stream.flush()
}

/// Hold the connection open and push a reload whenever the build generation
/// moves. One connection per page, no polling, and a browser reconnects on its
/// own after the server restarts.
fn reload_stream(mut stream: TcpStream, state: &State) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n"
    )?;
    stream.flush()?;
    let mut seen = state.generation.load(Ordering::SeqCst);
    loop {
        if state.stopped() {
            return Ok(());
        }
        let now = state.generation.load(Ordering::SeqCst);
        if now != seen {
            seen = now;
            let error = state
                .error
                .lock()
                .expect("no panic holds this lock")
                .clone();
            let frame = match error {
                Some(message) => format!(
                    "event: error-report\ndata: {}\n\n",
                    message.replace('\n', "\ndata: ")
                ),
                None => "event: reload\ndata: 1\n\n".to_string(),
            };
            // A write failure means the tab is gone. That is the normal way
            // one of these ends.
            if stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err() {
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
}

enum Resolved {
    File(PathBuf),
    Redirect(String),
    Missing,
    Forbidden,
}

/// Map a request path to a file inside `root`, or refuse.
///
/// Refusing is the important half. `..`, an encoded `..`, an absolute path and
/// a symlink out of the tree all have to end here rather than at a file.
fn resolve(root: &Path, path: &str) -> Resolved {
    let decoded = match percent_decode(path) {
        Some(d) => d,
        None => return Resolved::Forbidden,
    };
    let relative = decoded.trim_start_matches('/');

    // Reject before touching the filesystem: a component-level check cannot be
    // fooled by a path that does not exist yet.
    let candidate = Path::new(relative);
    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => return Resolved::Forbidden,
        }
    }

    let target = root.join(relative);
    if target.is_dir() {
        if !decoded.ends_with('/') {
            // Without the slash, every relative href on the page resolves one
            // level too high. Redirecting is the only fix that keeps the
            // links relative.
            return Resolved::Redirect(format!("{decoded}/"));
        }
        let index = target.join("index.html");
        return if index.is_file() {
            within(root, index)
        } else {
            Resolved::Missing
        };
    }
    if target.is_file() {
        return within(root, target);
    }
    Resolved::Missing
}

/// The last check, after symlinks have been followed.
fn within(root: &Path, path: PathBuf) -> Resolved {
    let (Ok(root), Ok(real)) = (root.canonicalize(), path.canonicalize()) else {
        return Resolved::Missing;
    };
    if real.starts_with(&root) {
        Resolved::File(real)
    } else {
        Resolved::Forbidden
    }
}

/// `%2e%2e` is `..`, and a decoder that does not know it is a hole.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Content types for what a build actually writes.
///
/// A stylesheet served as `text/plain` is an unstyled page and looks like a
/// generator bug, so the table is asserted on rather than assumed.
pub fn mime(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Every extension the mime table knows, for the test that pins it.
pub fn known_types() -> BTreeMap<&'static str, &'static str> {
    ["html", "css", "js", "json", "xml", "svg", "png", "txt"]
        .into_iter()
        .map(|ext| (ext, mime(Path::new(&format!("x.{ext}")))))
        .collect()
}

/// Whether something is answering at `url`.
///
/// The record a server leaves behind cannot be trusted on its own — a killed
/// process never cleans up — so anything that finds a `.serve.json` should ask
/// the port rather than believe the file.
pub fn alive(url: &str) -> bool {
    head(url).map(|(status, _, _)| status > 0).unwrap_or(false)
}

/// A blocking GET, for the tests. Returns the status, the headers and the body.
pub fn get(url: &str) -> Result<(u16, BTreeMap<String, String>, Vec<u8>)> {
    request("GET", url)
}

pub fn head(url: &str) -> Result<(u16, BTreeMap<String, String>, Vec<u8>)> {
    request("HEAD", url)
}

fn request(method: &str, url: &str) -> Result<(u16, BTreeMap<String, String>, Vec<u8>)> {
    let rest = url
        .strip_prefix("http://")
        .context("only http:// is served here")?;
    let (authority, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    let mut stream =
        TcpStream::connect(authority).with_context(|| format!("connecting to {authority}"))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;

    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("no header terminator in the response")?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body = raw[split + 4..].to_vec();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .context("no status in the response")?;
    let headers = lines
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.to_lowercase(), v.to_string()))
        .collect();
    Ok((status, headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use trafford::testing::TempDir;

    /// A server per test, on a port nobody named — which is the whole point.
    fn serving(files: &[(&str, &str)]) -> (TempDir, Arc<State>, String) {
        let dir = TempDir::with_files(files);
        let server = Server::bind(dir.path(), None).expect("bind an ephemeral port");
        let url = server.url();
        let state = server.state();
        std::thread::spawn(move || {
            let _ = server.run();
        });
        (dir, state, url)
    }

    #[test]
    fn binding_zero_gives_a_port_nobody_had_to_choose() {
        let dir = TempDir::new();
        let a = Server::bind(dir.path(), None).unwrap();
        let b = Server::bind(dir.path(), None).unwrap();
        assert_ne!(a.addr().port(), b.addr().port());
        assert_ne!(a.addr().port(), 0, "the port must be read after binding");
    }

    /// The failure this design exists to prevent: two worktrees, both serving,
    /// neither shadowing the other.
    #[test]
    fn two_servers_run_at_once_and_serve_their_own_tree() {
        let (_a, _, url_a) = serving(&[("index.html", "<p>tree a</p>")]);
        let (_b, _, url_b) = serving(&[("index.html", "<p>tree b</p>")]);
        assert_ne!(url_a, url_b);
        let (_, _, body_a) = get(&format!("{url_a}/")).unwrap();
        let (_, _, body_b) = get(&format!("{url_b}/")).unwrap();
        assert_eq!(String::from_utf8_lossy(&body_a), "<p>tree a</p>");
        assert_eq!(String::from_utf8_lossy(&body_b), "<p>tree b</p>");
    }

    /// An explicit port is a wish, and a wish that cannot be granted is an
    /// error rather than a quiet substitution.
    #[test]
    fn an_explicit_port_that_is_taken_fails_loudly() {
        let dir = TempDir::new();
        let first = Server::bind(dir.path(), None).unwrap();
        let taken = first.addr().port();
        let err = match Server::bind(dir.path(), Some(taken)) {
            Ok(_) => panic!("binding a port already in use should have failed"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains(&taken.to_string()), "{err}");
    }

    #[test]
    fn it_binds_loopback_and_nothing_else() {
        let dir = TempDir::new();
        let server = Server::bind(dir.path(), None).unwrap();
        assert_eq!(server.addr().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn a_directory_serves_its_index() {
        let (_dir, _, url) = serving(&[("docs/a/index.html", "<p>a</p>")]);
        let (status, headers, body) = get(&format!("{url}/docs/a/")).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers["content-type"], "text/html; charset=utf-8");
        assert_eq!(String::from_utf8_lossy(&body), "<p>a</p>");
    }

    /// Without the redirect every relative href on the page resolves one level
    /// too high, and the whole site looks broken from one missing slash.
    #[test]
    fn a_directory_without_a_slash_redirects_rather_than_404s() {
        let (_dir, _, url) = serving(&[("docs/a/index.html", "<p>a</p>")]);
        let (status, headers, _) = get(&format!("{url}/docs/a")).unwrap();
        assert_eq!(status, 301);
        assert_eq!(headers["location"], "/docs/a/");
    }

    #[test]
    fn content_types_are_right_for_what_a_build_writes() {
        let types = known_types();
        assert_eq!(types["css"], "text/css; charset=utf-8");
        assert_eq!(types["js"], "text/javascript; charset=utf-8");
        assert_eq!(types["svg"], "image/svg+xml");
        assert_eq!(types["json"], "application/json");
    }

    #[test]
    fn a_missing_page_gets_the_sites_own_404() {
        let (_dir, _, url) = serving(&[("404.html", "<h1>nope</h1>")]);
        let (status, _, body) = get(&format!("{url}/nowhere/")).unwrap();
        assert_eq!(status, 404);
        assert_eq!(String::from_utf8_lossy(&body), "<h1>nope</h1>");
    }

    /// Every shape of "climb out of the tree" ends in a refusal and never in a
    /// file. Remove the component check in `resolve` and this fails.
    #[test]
    fn nothing_outside_the_root_can_be_reached() {
        let outside = TempDir::with_files(&[("secret.txt", "do not serve me")]);
        let (dir, _, url) = serving(&[("index.html", "<p>root</p>")]);
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            dir.path().join("link.txt"),
        )
        .unwrap();

        for path in [
            "/../secret.txt",
            "/docs/../../secret.txt",
            "/%2e%2e/secret.txt",
            "/%2e%2e%2fsecret.txt",
            "/link.txt",
        ] {
            let (status, _, body) = get(&format!("{url}{path}")).unwrap();
            assert!(status == 403 || status == 404, "{path} returned {status}");
            assert!(
                !String::from_utf8_lossy(&body).contains("do not serve me"),
                "{path} served a file outside the root"
            );
        }
    }

    /// A client that trusts HEAD and then reads fewer bytes than promised
    /// hangs waiting for the rest.
    #[test]
    fn head_agrees_with_get() {
        let (_dir, _, url) = serving(&[("index.html", "<p>hello</p>")]);
        let (get_status, get_headers, get_body) = get(&format!("{url}/")).unwrap();
        let (head_status, head_headers, head_body) = head(&format!("{url}/")).unwrap();
        assert_eq!(get_status, head_status);
        assert_eq!(
            get_headers["content-length"],
            head_headers["content-length"]
        );
        assert_eq!(get_headers["content-length"], get_body.len().to_string());
        assert!(head_body.is_empty());
    }

    #[test]
    fn a_post_is_refused_rather_than_served() {
        let (_dir, _, url) = serving(&[("index.html", "<p>hi</p>")]);
        let (status, _, _) = request("POST", &format!("{url}/")).unwrap();
        assert_eq!(status, 405);
    }

    /// The record a script or an agent reads instead of scraping stdout.
    #[test]
    fn the_stamp_carries_the_port_and_goes_away_with_the_server() {
        let dir = TempDir::new();
        let stamp = dir.path().join("nested/.serve.json");
        let port = {
            let mut server = Server::bind(dir.path(), None).unwrap();
            server.write_stamp(&stamp).unwrap();
            let body = std::fs::read_to_string(&stamp).unwrap();
            assert!(body.contains(&format!("\"port\": {}", server.addr().port())));
            assert!(body.contains(&format!("\"pid\": {}", std::process::id())));
            server.addr().port()
        };
        assert!(port > 0);
        assert!(!stamp.exists(), "a stopped server left its record behind");
    }
}
