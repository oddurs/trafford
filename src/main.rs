mod app;
mod clipboard;
mod config;
mod editor;
mod git;
mod keymap;
mod layout;
mod llm;
mod mouse;
#[cfg(test)]
mod testing;
mod tree;
mod ui;
mod vault;
mod watch;

use anyhow::{Context, Result};
use app::App;
use config::Config;
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use editor::Mode;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout, Write};

/// Ask the terminal for button presses (1000), motion *while a button is
/// held* (1002), and SGR coordinates (1006), which lift the 223-column limit
/// of the original encoding.
///
/// Deliberately not crossterm's `EnableMouseCapture`: that also sends 1003,
/// which reports every pointer movement whether a button is down or not. This
/// loop redraws on each event, so all-motion tracking would burn CPU merely
/// for waving the mouse over the window — and nothing here reacts to a hover.
const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1006h";
const MOUSE_OFF: &str = "\x1b[?1006l\x1b[?1002l\x1b[?1000l";
use std::path::{Path, PathBuf};
use std::time::Duration;
use vault::Vault;

const USAGE: &str = "\
trafford — a terminal knowledge base

USAGE
    trafford [PATH]         open the vault at PATH (default: $TRAFFORD_VAULT or .)
    trafford init [PATH]    create a vault: starter notes, config, and a git repo
    trafford --help
    trafford --version

ENVIRONMENT
    TRAFFORD_VAULT      default vault path
    ANTHROPIC_API_KEY   enables the assistant pane

Configuration lives in <vault>/.trafford/config.toml so it travels with the vault.
";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("--help") | Some("-h") => {
            print!("{USAGE}");
            return Ok(());
        }
        Some("--version") | Some("-V") => {
            println!("trafford {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("init") => {
            let path = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            return init_vault(&path);
        }
        _ => {}
    }

    let root = args
        .first()
        .map(PathBuf::from)
        .or_else(|| std::env::var("TRAFFORD_VAULT").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));

    if !root.exists() {
        anyhow::bail!(
            "{} does not exist — run `trafford init {}` to create a vault there",
            root.display(),
            root.display()
        );
    }

    let vault = Vault::open(&root)?;
    let cfg = Config::load(&vault.root);
    let watching = watch::Watcher::start(&vault.root);
    let mut app = App::new(vault, cfg);
    match watching {
        Ok(w) => app.watcher = Some(w),
        // A vault that is not watched still works; it just needs `reindex`.
        // Worth saying once rather than leaving the reader to wonder why
        // nothing updates.
        Err(err) => app.set_status(format!("not watching the vault: {err}")),
    }
    run(app)
}

/// Scaffold a vault: a couple of notes that explain themselves, a config file,
/// a .gitignore, and a git repository if one is not already there.
fn init_vault(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))?;
    let root = path.canonicalize()?;

    let welcome = root.join("Welcome.md");
    if !welcome.exists() {
        std::fs::write(
            &welcome,
            "\
---
tags: [meta]
---
# Welcome

This is a trafford vault: a folder of markdown files, linked with `[[wikilinks]]`,
versioned with git.

- Press `ctrl-k` for the command palette, `f1` for every key.
- Press `enter` on [[How linking works]] to follow the link.
- A link to a note that does not exist yet, like [[Someday]], shows in red —
  press `enter` on it to write it.

## Tasks

- [ ] Open the command palette
- [ ] Write your first note with `ctrl-n`
- [ ] Commit the vault with `ctrl-g`
",
        )?;
    }

    let linking = root.join("How linking works.md");
    if !linking.exists() {
        std::fs::write(
            &linking,
            "\
---
tags: [meta]
---
# How linking works

Write `[[Note name]]` anywhere and trafford resolves it against the vault: first
by exact path, then by filename. `[[folder/Note#Heading|shown text]]` works too.

Every note's inbound links appear in the context pane on the right, so the
backlinks are always one glance away. Renaming a note with the palette rewrites
the links that pointed at it.

Back to [[Welcome]].
",
        )?;
    }

    Config::write_default(&root)?;

    let gitignore = root.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(
            &gitignore,
            "# trafford\n.DS_Store\n.trafford/cache/\n*.swp\n",
        )?;
    }

    match git::Repo::discover(&root) {
        Some(repo) => println!(
            "vault ready in {} (git repo at {})",
            root.display(),
            repo.root.display()
        ),
        None => {
            git::Repo::init(&root)?;
            println!("vault ready in {} (initialised a git repo)", root.display());
        }
    }
    println!("run `trafford {}` to open it", root.display());
    Ok(())
}

type Term = Terminal<CrosstermBackend<Stdout>>;

/// Take over the terminal: raw mode, the alternate screen, and the mouse
/// modes we use. Paired with [`release_terminal`], which must undo all three —
/// a program that keeps any of them after exiting leaves the user's shell
/// unusable, which is the worst failure this could have.
fn claim_terminal() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    write!(stdout, "{MOUSE_ON}")?;
    stdout.flush()?;
    Ok(())
}

fn release_terminal() -> Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "{MOUSE_OFF}")?;
    stdout.flush()?;
    disable_raw_mode()?;
    execute!(
        stdout,
        LeaveAlternateScreen,
        SetCursorStyle::DefaultUserShape
    )?;
    Ok(())
}

fn setup_terminal() -> Result<Term> {
    claim_terminal()?;
    let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    Ok(terminal)
}

/// Hand the terminal to another program, then take it back.
///
/// Everything `claim_terminal` turned on has to be turned off first and back
/// on afterwards, or the guest program runs against a terminal in a state it
/// did not ask for. The screen is cleared on return because the guest drew
/// over the alternate screen we are about to reuse.
fn run_in_terminal(terminal: &mut Term, program: &str, args: &[String]) -> Result<()> {
    release_terminal()?;
    let status = std::process::Command::new(program).args(args).status();
    claim_terminal()?;
    terminal.clear()?;
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(anyhow::anyhow!("{program} exited with {status}")),
        Err(err) => Err(anyhow::anyhow!("could not run {program}: {err}")),
    }
}

fn restore_terminal() -> Result<()> {
    release_terminal()
}

fn run(mut app: App) -> Result<()> {
    // Leave the terminal usable even if something panics mid-draw.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        default_hook(info);
    }));

    let mut terminal = setup_terminal()?;
    let result = event_loop(&mut terminal, &mut app);
    restore_terminal()?;
    terminal.show_cursor()?;
    result
}

fn event_loop(terminal: &mut Term, app: &mut App) -> Result<()> {
    let mut last_mode = Mode::Insert; // force the cursor style to be set once
    loop {
        if app.editor.mode != last_mode {
            let style = if app.editor.mode.is_insert() {
                SetCursorStyle::BlinkingBar
            } else {
                SetCursorStyle::SteadyBlock
            };
            let _ = execute!(io::stdout(), style);
            last_mode = app.editor.mode;
        }

        terminal.draw(|f| ui::draw(f, app))?;

        // A short poll keeps streamed assistant tokens arriving smoothly.
        if event::poll(Duration::from_millis(60))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        // Handing the terminal over has to happen here, between draws, where
        // the terminal is owned and nothing is mid-render.
        if let Some((program, args)) = app.pending_suspend.take() {
            if let Err(err) = run_in_terminal(terminal, &program, &args) {
                app.set_status(err.to_string());
            }
            // The guest may have changed the file on disk.
            if let Err(err) = app.reload_after_external() {
                app.set_status(format!("reindex failed: {err}"));
            }
        }

        app.poll_assistant();
        app.absorb_disk_changes();
        app.tick();

        if app.should_quit {
            return Ok(());
        }
    }
}
