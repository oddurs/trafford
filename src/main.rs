mod app;
mod config;
mod editor;
mod git;
mod keymap;
mod llm;
#[cfg(test)]
mod testing;
mod tree;
mod ui;
mod vault;

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
use std::io::{self, Stdout};
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
    let app = App::new(vault, cfg);
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

fn setup_terminal() -> Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        SetCursorStyle::DefaultUserShape
    )?;
    Ok(())
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
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        app.poll_assistant();
        app.tick();

        if app.should_quit {
            return Ok(());
        }
    }
}
