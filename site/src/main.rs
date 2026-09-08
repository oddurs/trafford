use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use trafford_site::build::{self, Options};
use trafford_site::{keys, palette, serve, watch};

const USAGE: &str = "\
site — build and serve trafford's documentation and landing page

USAGE
    site build [OPTIONS]    write the site to the output directory
    site serve [OPTIONS]    build it, then serve it and rebuild as you write
    site sync               regenerate the docs that are generated from code
    site palette            every theme's roles as JSON, for tools/shots.py

OPTIONS
    --docs <DIR>       source vault (default: docs)
    --out <DIR>        output directory (default: target/site)
    --site-url <URL>   absolute origin, for canonical links and the sitemap
    --port <N>         serve on this port instead of one the OS chooses
    --open             open a browser at the served URL
    --api              also build rustdoc and publish it under /api

    `serve` with no --port binds 127.0.0.1:0 and reports what it got, so any
    number of these can run at once — one per worktree, one per agent — without
    one silently shadowing another. The URL is also written to
    <out>/../.serve.json for anything that would rather not scrape stdout.
";

/// How often the watcher walks the source tree.
const POLL: Duration = Duration::from_millis(200);

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str).unwrap_or("build") {
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        "build" => {
            let (opts, _) = parse(&args[1..])?;
            let built = build::build(&opts)?;
            println!(
                "built {} page(s), {} KiB → {}",
                built.pages.len(),
                built.bytes / 1024,
                opts.out.display()
            );
            Ok(())
        }
        "serve" => {
            let (opts, flags) = parse(&args[1..])?;
            serve_it(opts, flags)
        }
        "sync" => sync(&args[1..]),
        "palette" => {
            print!("{}", palette::as_json()?);
            Ok(())
        }
        other => bail!("unknown command `{other}`\n\n{USAGE}"),
    }
}

#[derive(Default)]
struct Flags {
    port: Option<u16>,
    open: bool,
}

fn parse(args: &[String]) -> Result<(Options, Flags)> {
    let mut opts = Options::new("docs", Path::new("target").join("site"));
    let mut flags = Flags::default();
    let mut i = 0;
    while i < args.len() {
        let value = || -> Result<String> {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| anyhow!("{} wants a value", args[i]))
        };
        match args[i].as_str() {
            "--docs" => {
                opts.docs = value()?.into();
                i += 2;
            }
            "--out" => {
                opts.out = value()?.into();
                i += 2;
            }
            "--site-url" => {
                opts.site_url = Some(value()?);
                i += 2;
            }
            "--port" => {
                flags.port = Some(value()?.parse()?);
                i += 2;
            }
            "--open" => {
                flags.open = true;
                i += 1;
            }
            "--api" => {
                opts.api = true;
                i += 1;
            }
            other => bail!("unknown option `{other}`\n\n{USAGE}"),
        }
    }
    Ok((opts, flags))
}

fn serve_it(mut opts: Options, flags: Flags) -> Result<()> {
    // The reload client exists only here. A build never writes it.
    opts.reload = true;

    let first = build::build(&opts);
    if let Err(e) = &first {
        // Serve anyway: the banner is how the error reaches the browser, and a
        // server that refuses to start over a typo is a worse tool.
        eprintln!("build failed:\n{e:#}");
    }

    let mut server = serve::Server::bind(&opts.out, flags.port)?;
    let stamp = opts
        .out
        .parent()
        .unwrap_or(Path::new("."))
        .join(".serve.json");
    server.write_stamp(&stamp)?;
    let url = server.url();
    let state = server.state();
    if let Err(e) = first {
        state.failed(format!("{e:#}"));
    }

    let sources = watch::sources(&opts.docs, Path::new(env!("CARGO_MANIFEST_DIR")));
    println!("  {url}   ← {}", opts.docs.display());
    println!(
        "  {} watched · {}",
        counted(&sources, &opts.out),
        stamp.display()
    );

    // No signal handler. Removing the record on Ctrl-C would need unsafe FFI
    // or a dependency, and it would still not survive `kill -9` — so the
    // record is never trusted on its own. `serve::alive` asks the port, which
    // is the thing that actually matters, and the next server overwrites the
    // file regardless.
    let stopping = Arc::new(AtomicBool::new(false));

    if flags.open {
        open_browser(&url);
    }

    let watcher = {
        let opts = Options {
            docs: opts.docs.clone(),
            out: opts.out.clone(),
            site_url: opts.site_url.clone(),
            reload: true,
            // Never in the watch loop: `cargo doc` is tens of seconds and the
            // reason to have a watch loop is that it is not.
            api: false,
        };
        let state = Arc::clone(&state);
        let stopping = Arc::clone(&stopping);
        let skip = vec![opts.out.clone()];
        std::thread::spawn(move || {
            watch::watch(
                &sources,
                &skip,
                POLL,
                || stopping.load(Ordering::SeqCst),
                || match build::build(&opts) {
                    Ok(built) => {
                        println!("rebuilt {} page(s)", built.pages.len());
                        state.rebuilt();
                    }
                    Err(e) => {
                        eprintln!("build failed:\n{e:#}");
                        state.failed(format!("{e:#}"));
                    }
                },
            );
        })
    };

    server.run()?;
    stopping.store(true, Ordering::SeqCst);
    let _ = watcher.join();
    Ok(())
}

fn counted(sources: &[PathBuf], out: &Path) -> String {
    let n = watch::snapshot(sources, &[out.to_path_buf()]).len();
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

/// Regenerate the documentation that is written from code rather than by hand.
///
/// Run by a person when they change a keybinding, and by CI to check that they
/// did — `site sync && git diff --exit-code` is the whole enforcement.
fn sync(args: &[String]) -> Result<()> {
    let docs: PathBuf = match args {
        [] => "docs".into(),
        [flag, value] if flag == "--docs" => value.into(),
        _ => bail!("usage: site sync [--docs DIR]"),
    };
    let path = docs.join(keys::PATH);
    let body = keys::page();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current == body {
        println!("{} is current", path.display());
        return Ok(());
    }
    std::fs::write(&path, body)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn open_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(url).spawn();
}
