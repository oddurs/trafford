//! Colour themes.
//!
//! A theme is a set of *roles* — what a thing is for, not what colour it is —
//! so a palette can be swapped without touching the drawing code. Themes come
//! from three places, all through the same parser:
//!
//!   1. the four built in below, which are ordinary theme files compiled in;
//!   2. `.toml` files in `<vault>/.trafford/themes/` or
//!      `~/.config/trafford/themes/`, so a theme can travel with the notes;
//!   3. **Ghostty theme files**, read directly. If you already picked a theme
//!      for your terminal, trafford can wear the same one rather than making
//!      you transcribe it.

use anyhow::{anyhow, Context, Result};
use ratatui::style::{Modifier, Style};

/// Re-exported so a consumer of a `Theme` does not have to depend on the
/// terminal library to read one. The site does exactly that.
pub use ratatui::style::Color;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Theme files shipped with trafford. They are parsed at startup like any
/// other, so there is one code path and built-ins cannot drift from the
/// format users write.
const BUILTIN: &[(&str, &str)] = &[
    ("gotham", include_str!("../../themes/gotham.toml")),
    ("night", include_str!("../../themes/night.toml")),
    ("paper", include_str!("../../themes/paper.toml")),
    ("mono", include_str!("../../themes/mono.toml")),
];

pub const DEFAULT: &str = "gotham";

/// The parsed form of a theme file. Every colour is optional so a theme can
/// state only what it cares about; [`Theme::from_file`] fills the rest in.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    name: Option<String>,
    dark: Option<bool>,
    background: Option<String>,
    surface: Option<String>,
    overlay: Option<String>,
    text: Option<String>,
    muted: Option<String>,
    faint: Option<String>,
    border: Option<String>,
    border_focus: Option<String>,
    selection: Option<String>,
    cursorline: Option<String>,
    accent: Option<String>,
    secondary: Option<String>,
    heading: Option<String>,
    link: Option<String>,
    broken: Option<String>,
    code: Option<String>,
    tag: Option<String>,
    added: Option<String>,
    removed: Option<String>,
    modified: Option<String>,
    mode_normal: Option<String>,
    mode_insert: Option<String>,
    mode_visual: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    /// Whether the palette is meant for a dark ground.
    ///
    /// The terminal never asks: it draws on whatever `bg` says. The docs site
    /// does, because a browser has a `prefers-color-scheme` to honour and a
    /// reader whose system is set to light should not be handed Gotham.
    pub dark: bool,
    pub bg: Color,
    pub surface: Color,
    pub overlay: Color,
    pub fg: Color,
    pub muted: Color,
    pub faint: Color,
    pub border: Color,
    pub border_focus: Color,
    pub selection: Color,
    pub cursorline: Color,
    pub accent: Color,
    pub secondary: Color,
    pub heading: Color,
    pub link: Color,
    pub broken: Color,
    pub code: Color,
    pub tag: Color,
    pub added: Color,
    pub removed: Color,
    pub modified: Color,
    pub mode_normal: Color,
    pub mode_insert: Color,
    pub mode_visual: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::builtin(DEFAULT).expect("the default theme must parse")
    }
}

impl Theme {
    pub fn builtin(name: &str) -> Option<Theme> {
        let (_, body) = BUILTIN.iter().find(|(n, _)| *n == name)?;
        Theme::from_toml(body).ok()
    }

    pub fn builtin_names() -> Vec<&'static str> {
        BUILTIN.iter().map(|(n, _)| *n).collect()
    }

    pub fn from_toml(body: &str) -> Result<Theme> {
        let file: ThemeFile = toml::from_str(body).context("parsing theme")?;
        Theme::from_file(file)
    }

    fn from_file(file: ThemeFile) -> Result<Theme> {
        let dark = file.dark.unwrap_or(true);
        let bg = parse_or(
            file.background.as_deref(),
            if dark { "#101014" } else { "#f7f5f0" },
        )?;
        let fg = parse_or(
            file.text.as_deref(),
            if dark { "#d0d0d0" } else { "#202020" },
        )?;

        // Anything a theme leaves unstated is derived by mixing the two
        // colours it must state, so a five-line theme file still produces a
        // coherent set rather than a patchwork of defaults.
        let derive = |stated: Option<&str>, t: f32, fallback: Color| -> Result<Color> {
            Ok(match parse(stated)? {
                Some(c) => c,
                None => blend(bg, fg, t).unwrap_or(fallback),
            })
        };

        let accent = parse_or(file.accent.as_deref(), "#e0a458")?;
        Ok(Theme {
            dark,
            name: "",
            bg,
            fg,
            surface: derive(file.surface.as_deref(), 0.05, bg)?,
            overlay: derive(file.overlay.as_deref(), 0.10, bg)?,
            muted: derive(file.muted.as_deref(), 0.60, Color::Gray)?,
            faint: derive(file.faint.as_deref(), 0.35, Color::DarkGray)?,
            border: derive(file.border.as_deref(), 0.18, Color::DarkGray)?,
            border_focus: derive(file.border_focus.as_deref(), 0.45, Color::Gray)?,
            selection: derive(file.selection.as_deref(), 0.22, Color::DarkGray)?,
            cursorline: derive(file.cursorline.as_deref(), 0.07, bg)?,
            accent,
            secondary: derive(file.secondary.as_deref(), 0.75, accent)?,
            heading: derive(file.heading.as_deref(), 1.15, fg)?,
            link: derive(file.link.as_deref(), 0.85, Color::Magenta)?,
            broken: parse_or(file.broken.as_deref(), "#c0554f")?,
            code: derive(file.code.as_deref(), 0.80, Color::Cyan)?,
            tag: derive(file.tag.as_deref(), 0.70, Color::Blue)?,
            added: parse_or(file.added.as_deref(), "#5faf5f")?,
            removed: parse_or(file.removed.as_deref(), "#c0554f")?,
            modified: parse_or(file.modified.as_deref(), "#d7875f")?,
            mode_normal: match parse(file.mode_normal.as_deref())? {
                Some(c) => c,
                None => accent,
            },
            mode_insert: parse_or(file.mode_insert.as_deref(), "#5faf5f")?,
            mode_visual: parse_or(file.mode_visual.as_deref(), "#8f7fd0")?,
        }
        .with_name(file.name))
    }

    fn with_name(mut self, name: Option<String>) -> Theme {
        // The name is only ever shown, so leaking one string per theme change
        // is cheaper than threading a lifetime through every draw call.
        if let Some(name) = name {
            self.name = Box::leak(name.into_boxed_str());
        }
        self
    }

    /// Read a Ghostty theme file: `background`, `foreground` and
    /// `palette = N=#rrggbb` lines. Roles are assigned from the sixteen ANSI
    /// slots the way a terminal application would use them, preferring the
    /// bright half where a role has to stand out against the ground.
    pub fn from_ghostty(body: &str) -> Result<Theme> {
        let mut palette: BTreeMap<u8, String> = BTreeMap::new();
        let mut keys: BTreeMap<&str, String> = BTreeMap::new();
        for line in body.lines() {
            let line = line.trim();
            // A leading '#' is a comment; a '#' inside a value is a colour.
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if key == "palette" {
                if let Some((slot, colour)) = value.split_once('=') {
                    if let Ok(slot) = slot.trim().parse::<u8>() {
                        palette.insert(slot, colour.trim().to_string());
                    }
                }
            } else {
                keys.insert(
                    match key {
                        "background" => "background",
                        "foreground" => "foreground",
                        "selection-background" => "selection",
                        "cursor-color" => "cursor",
                        _ => continue,
                    },
                    value.to_string(),
                );
            }
        }
        if !keys.contains_key("background") && palette.is_empty() {
            return Err(anyhow!("not a Ghostty theme: no background or palette"));
        }
        let slot = |n: u8| palette.get(&n).cloned();
        // Bright first: the official Gotham port fills the bright slots with
        // background shades, and a theme that fixes that is exactly the one
        // worth honouring.
        let pick = |bright: u8, normal: u8| slot(bright).or_else(|| slot(normal));
        let file = ThemeFile {
            name: None,
            dark: Some(true),
            background: keys.get("background").cloned().or_else(|| slot(0)),
            surface: None,
            overlay: None,
            text: keys.get("foreground").cloned().or_else(|| slot(7)),
            muted: pick(12, 4),
            faint: slot(8),
            border: None,
            border_focus: pick(6, 6),
            selection: keys.get("selection").cloned(),
            cursorline: None,
            accent: pick(3, 3),
            secondary: pick(6, 6),
            heading: slot(15),
            link: pick(12, 4),
            broken: pick(9, 1),
            code: pick(10, 2),
            tag: pick(13, 5),
            added: pick(10, 2),
            removed: pick(9, 1),
            modified: pick(11, 3),
            mode_normal: pick(3, 3),
            mode_insert: pick(10, 2),
            mode_visual: pick(14, 6),
        };
        Theme::from_file(file)
    }

    /// Resolve `spec` — a built-in name, a theme file, or a Ghostty theme —
    /// and say where it came from. Falls back to the default rather than
    /// failing: a mistyped theme should not stop you reading your notes.
    pub fn resolve(spec: &str, vault_root: &Path) -> (Theme, String) {
        match Theme::try_resolve(spec, vault_root) {
            Ok(found) => found,
            Err(err) => {
                let mut theme = Theme::builtin(DEFAULT).unwrap_or_default();
                theme.name = "";
                (theme, format!("{spec}: {err}; using {DEFAULT}"))
            }
        }
    }

    fn try_resolve(spec: &str, vault_root: &Path) -> Result<(Theme, String)> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Ok((Theme::builtin(DEFAULT).unwrap(), DEFAULT.into()));
        }
        // An explicit path wins, so a theme can live anywhere.
        if spec.contains('/') || spec.contains('\\') {
            let path = expand(spec);
            let body = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            return Ok((parse_any(&body, &path)?, spec.to_string()));
        }
        for dir in theme_dirs(vault_root) {
            let path = dir.join(format!("{spec}.toml"));
            if let Ok(body) = std::fs::read_to_string(&path) {
                return Ok((Theme::from_toml(&body)?, format!("{}", path.display())));
            }
        }
        if let Some(theme) = Theme::builtin(spec) {
            return Ok((theme, spec.to_string()));
        }
        // Last: the terminal's own themes, so `theme = "Gotham"` finds the one
        // already on screen without anyone transcribing it. Ghostty names them
        // "Catppuccin Mocha"; nobody types that, so match loosely.
        for dir in ghostty_dirs() {
            if let Some(path) = find_loosely(&dir, spec) {
                let body = std::fs::read_to_string(&path)?;
                return Ok((
                    Theme::from_ghostty(&body)?,
                    format!("ghostty:{}", file_name(&path)),
                ));
            }
        }
        Err(anyhow!("no such theme"))
    }

    /// Every theme that can be selected, for the picker. Built-ins first, then
    /// user files, then whatever Ghostty has.
    pub fn available(vault_root: &Path) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Theme::builtin_names()
            .into_iter()
            .map(|n| (n.to_string(), "built in".to_string()))
            .collect();
        // Built-ins keep their declared order — a shortlist, not an index —
        // but anything found on disk is sorted, because a directory listing's
        // order means nothing to the person reading it.
        let mut found = Vec::new();
        for dir in theme_dirs(vault_root) {
            collect(&dir, Some("toml"), "theme file", &mut found);
        }
        let user_themes = found.len();
        for dir in ghostty_dirs() {
            collect(&dir, None, "ghostty", &mut found);
        }
        found[..user_themes].sort_by_key(|(n, _)| n.to_lowercase());
        found[user_themes..].sort_by_key(|(n, _)| n.to_lowercase());
        found.retain(|(n, _)| !out.iter().any(|(b, _)| b == n));
        out.extend(found);
        out
    }

    // ---- styles --------------------------------------------------------

    pub fn base(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    pub fn dimmed(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn faded(&self) -> Style {
        Style::default().fg(self.faint)
    }

    pub fn title(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected(&self) -> Style {
        Style::default()
            .fg(self.heading)
            .bg(self.selection)
            .add_modifier(Modifier::BOLD)
    }

    pub fn border_style(&self, focused: bool) -> Style {
        Style::default().fg(if focused {
            self.border_focus
        } else {
            self.border
        })
    }

    /// Heading colour ramps from bright to quiet as the level deepens.
    pub fn heading_style(&self, level: u8) -> Style {
        let base = Style::default().add_modifier(Modifier::BOLD);
        match level {
            1 => base.fg(self.accent),
            2 => base.fg(self.heading),
            3 => base.fg(self.fg),
            _ => base.fg(self.muted),
        }
    }
}

fn collect(dir: &Path, ext: Option<&str>, kind: &str, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        match ext {
            Some(want) if path.extension().and_then(|e| e.to_str()) != Some(want) => continue,
            _ => {}
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !out.iter().any(|(n, _)| n == stem) {
            out.push((stem.to_string(), kind.to_string()));
        }
    }
}

/// Compare names ignoring case, spaces, dashes and underscores, so
/// `catppuccin-mocha` finds `Catppuccin Mocha`.
fn loose(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn find_loosely(dir: &Path, spec: &str) -> Option<PathBuf> {
    let want = loose(spec);
    let exact = dir.join(spec);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        let stem = path.file_stem()?.to_str()?;
        (path.is_file() && loose(stem) == want).then_some(path)
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

fn parse_any(body: &str, path: &Path) -> Result<Theme> {
    if path.extension().and_then(|e| e.to_str()) == Some("toml") {
        return Theme::from_toml(body);
    }
    // A Ghostty theme has no section headers and uses `palette =` lines.
    Theme::from_ghostty(body).or_else(|_| Theme::from_toml(body))
}

fn theme_dirs(vault_root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![vault_root.join(".trafford/themes")];
    if let Some(config) = dirs::config_dir() {
        dirs.push(config.join("trafford/themes"));
    }
    dirs
}

fn ghostty_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(config) = dirs::config_dir() {
        dirs.push(config.join("ghostty/themes"));
    }
    dirs.push(PathBuf::from(
        "/Applications/Ghostty.app/Contents/Resources/ghostty/themes",
    ));
    dirs.push(PathBuf::from("/usr/share/ghostty/themes"));
    dirs
}

fn expand(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// A colour a theme stated, or `None` if it said nothing.
fn parse(value: Option<&str>) -> Result<Option<Color>> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(raw) => parse_colour(raw).map(Some),
        None => Ok(None),
    }
}

/// A colour a theme stated, or the given fallback.
fn parse_or(value: Option<&str>, fallback: &str) -> Result<Color> {
    Ok(parse(value)?.unwrap_or(parse_colour(fallback)?))
}

pub fn parse_colour(raw: &str) -> Result<Color> {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix('#') {
        let (r, g, b) = match hex.len() {
            3 => {
                let c = |i: usize| -> Result<u8> {
                    let d = u8::from_str_radix(&hex[i..i + 1], 16)?;
                    Ok(
                        d * 17, // #abc means #aabbcc
                    )
                };
                (c(0)?, c(1)?, c(2)?)
            }
            6 => (
                u8::from_str_radix(&hex[0..2], 16)?,
                u8::from_str_radix(&hex[2..4], 16)?,
                u8::from_str_radix(&hex[4..6], 16)?,
            ),
            _ => return Err(anyhow!("{raw} is not a #rgb or #rrggbb colour")),
        };
        return Ok(Color::Rgb(r, g, b));
    }
    Ok(match raw.to_lowercase().replace(['-', '_'], "").as_str() {
        "reset" | "default" | "none" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" | "white" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "brightred" | "lightred" => Color::LightRed,
        "brightgreen" | "lightgreen" => Color::LightGreen,
        "brightyellow" | "lightyellow" => Color::LightYellow,
        "brightblue" | "lightblue" => Color::LightBlue,
        "brightmagenta" | "lightmagenta" => Color::LightMagenta,
        "brightcyan" | "lightcyan" => Color::LightCyan,
        "brightwhite" => Color::White,
        other => return Err(anyhow!("{other} is not a colour")),
    })
}

/// Mix `t` of `b` into `a`. Used to derive the shades a theme leaves unstated,
/// so a minimal theme file still produces a coherent set.
fn blend(a: Color, b: Color, t: f32) -> Option<Color> {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        // Terminal-palette themes have nothing to interpolate between.
        return None;
    };
    let f = |x: u8, y: u8| ((x as f32) + ((y as f32) - (x as f32)) * t).clamp(0.0, 255.0) as u8;
    Some(Color::Rgb(f(ar, br), f(ag, bg), f(ab, bb)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The user's own Gotham, which deliberately replaces the official port's
    /// bright slots with legible tones rather than background shades.
    const GHOSTTY_GOTHAM: &str = "\
# Gotham
background = #0a0f14
foreground = #98d1ce
cursor-color = #98d1ce
selection-background = #245361
palette = 0=#0a0f14
palette = 1=#c33027
palette = 2=#26a98b
palette = 3=#edb54b
palette = 4=#195465
palette = 5=#4e5165
palette = 6=#33859d
palette = 7=#98d1ce
palette = 8=#245361
palette = 9=#d26939
palette = 10=#3fc2a0
palette = 11=#f2c96b
palette = 12=#599caa
palette = 13=#a3a6c0
palette = 14=#5fb3c9
palette = 15=#d3ebe9
";

    fn rgb(c: Color) -> (u8, u8, u8) {
        match c {
            Color::Rgb(r, g, b) => (r, g, b),
            other => panic!("expected an rgb colour, got {other:?}"),
        }
    }

    #[test]
    fn every_builtin_theme_parses() {
        for name in Theme::builtin_names() {
            let theme = Theme::builtin(name).unwrap_or_else(|| panic!("{name} failed to parse"));
            assert!(!theme.name.is_empty(), "{name} has no display name");
        }
    }

    #[test]
    fn gotham_matches_the_published_palette() {
        let t = Theme::builtin("gotham").unwrap();
        assert_eq!(rgb(t.bg), (0x0a, 0x0f, 0x14));
        assert_eq!(rgb(t.fg), (0x98, 0xd1, 0xce));
        assert_eq!(rgb(t.accent), (0xed, 0xb5, 0x4b));
        assert_eq!(rgb(t.heading), (0xd3, 0xeb, 0xe9));
    }

    #[test]
    fn colours_parse_in_every_accepted_form() {
        assert_eq!(rgb(parse_colour("#0a0f14").unwrap()), (0x0a, 0x0f, 0x14));
        // #abc is shorthand for #aabbcc.
        assert_eq!(rgb(parse_colour("#abc").unwrap()), (0xaa, 0xbb, 0xcc));
        assert_eq!(parse_colour("reset").unwrap(), Color::Reset);
        assert_eq!(parse_colour("bright-blue").unwrap(), Color::LightBlue);
        assert_eq!(parse_colour("BrightBlue").unwrap(), Color::LightBlue);
        assert!(parse_colour("#12345").is_err());
        assert!(parse_colour("chartreuse").is_err());
    }

    #[test]
    fn a_ghostty_theme_becomes_a_usable_theme() {
        let t = Theme::from_ghostty(GHOSTTY_GOTHAM).unwrap();
        assert_eq!(rgb(t.bg), (0x0a, 0x0f, 0x14), "background");
        assert_eq!(rgb(t.fg), (0x98, 0xd1, 0xce), "foreground");
        assert_eq!(rgb(t.selection), (0x24, 0x53, 0x61), "selection-background");
        assert_eq!(rgb(t.accent), (0xed, 0xb5, 0x4b), "yellow");
        assert_eq!(rgb(t.heading), (0xd3, 0xeb, 0xe9), "bright white");
        assert_eq!(rgb(t.code), (0x3f, 0xc2, 0xa0), "bright green");
    }

    /// The whole reason for preferring the bright half: a theme that fixed
    /// slots 8-15 should have that work honoured, not discarded.
    #[test]
    fn ghostty_prefers_the_bright_slots_where_a_role_must_stand_out() {
        let t = Theme::from_ghostty(GHOSTTY_GOTHAM).unwrap();
        assert_eq!(rgb(t.link), (0x59, 0x9c, 0xaa), "bright blue, not #195465");
        assert_eq!(rgb(t.removed), (0xd2, 0x69, 0x39), "bright red slot");
    }

    #[test]
    fn ghostty_comments_are_not_mistaken_for_colours() {
        let t = Theme::from_ghostty("# a comment\nbackground = #101010\n").unwrap();
        assert_eq!(rgb(t.bg), (0x10, 0x10, 0x10));
    }

    #[test]
    fn a_file_that_is_not_a_theme_is_rejected() {
        assert!(Theme::from_ghostty("hello\nworld\n").is_err());
    }

    #[test]
    fn shades_a_theme_leaves_out_are_derived_from_the_two_it_states() {
        let t = Theme::from_toml("background = \"#000000\"\ntext = \"#ffffff\"\n").unwrap();
        // Every derived shade must sit between the ground and the text.
        for (name, colour) in [
            ("surface", t.surface),
            ("faint", t.faint),
            ("muted", t.muted),
            ("selection", t.selection),
        ] {
            let (r, _, _) = rgb(colour);
            assert!(r > 0 && r < 255, "{name} was not derived, got {colour:?}");
        }
        assert!(
            rgb(t.surface).0 < rgb(t.muted).0,
            "surface must be nearer the ground"
        );
    }

    #[test]
    fn a_terminal_palette_theme_needs_no_blending() {
        let t = Theme::builtin("mono").unwrap();
        assert_eq!(t.bg, Color::Reset);
        assert_eq!(t.accent, Color::Yellow);
    }

    #[test]
    fn an_unknown_theme_falls_back_instead_of_failing() {
        let (theme, note) = Theme::resolve("no-such-theme", Path::new("/nonexistent"));
        assert!(note.contains("using"), "{note}");
        assert_eq!(rgb(theme.bg), rgb(Theme::builtin(DEFAULT).unwrap().bg));
    }

    #[test]
    fn a_named_builtin_resolves_to_itself() {
        let (theme, source) = Theme::resolve("paper", Path::new("/nonexistent"));
        assert_eq!(source, "paper");
        assert_eq!(theme.name, "Paper");
    }

    /// A role that comes out the same colour as the ground is invisible, and
    /// the derivation makes that easy to do by accident on a light theme.
    #[test]
    fn nothing_a_theme_derives_is_invisible_against_its_ground() {
        let cases = [
            // Dark and light, each stating only the two required colours.
            "background = \"#0a0f14\"\ntext = \"#98d1ce\"\n".to_string(),
            "background = \"#eff1f5\"\ntext = \"#4c4f69\"\n".to_string(),
            "background = \"#ffffff\"\ntext = \"#000000\"\n".to_string(),
        ];
        for body in cases {
            let t = Theme::from_toml(&body).unwrap();
            for (name, colour) in [
                ("text", t.fg),
                ("muted", t.muted),
                ("faint", t.faint),
                ("heading", t.heading),
                ("link", t.link),
                ("code", t.code),
                ("tag", t.tag),
                ("border_focus", t.border_focus),
            ] {
                assert_ne!(rgb(colour), rgb(t.bg), "{name} is invisible in {body:?}");
            }
            // The lifted grounds must differ from the page, or panes and
            // popups stop reading as separate surfaces.
            assert_ne!(rgb(t.surface), rgb(t.bg), "surface in {body:?}");
            assert_ne!(rgb(t.overlay), rgb(t.bg), "overlay in {body:?}");
        }
    }

    #[test]
    fn a_light_ghostty_theme_stays_light() {
        let body = "\
background = #eff1f5
foreground = #4c4f69
palette = 3=#df8e1d
palette = 7=#4c4f69
palette = 15=#5c5f77
";
        let t = Theme::from_ghostty(body).unwrap();
        let (br, _, _) = rgb(t.bg);
        let (fr, _, _) = rgb(t.fg);
        assert!(br > fr, "a light ground must stay lighter than its text");
    }

    #[test]
    fn loose_matching_ignores_case_and_separators() {
        assert_eq!(loose("Catppuccin Mocha"), "catppuccinmocha");
        assert_eq!(loose("catppuccin-mocha"), "catppuccinmocha");
        assert_eq!(loose("catppuccin_MOCHA"), "catppuccinmocha");
        assert_ne!(loose("Catppuccin Latte"), loose("Catppuccin Mocha"));
    }

    #[test]
    fn every_builtin_is_listed_as_available() {
        let found = Theme::available(Path::new("/nonexistent"));
        for name in Theme::builtin_names() {
            assert!(
                found.iter().any(|(n, _)| n == name),
                "{name} was not listed"
            );
        }
        // Built-ins lead, in the order they are declared.
        let leading: Vec<&str> = found
            .iter()
            .take(Theme::builtin_names().len())
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(leading, Theme::builtin_names());
    }

    #[test]
    fn themes_found_on_disk_are_listed_alphabetically() {
        let found = Theme::available(Path::new("/nonexistent"));
        let on_disk: Vec<String> = found
            .iter()
            .skip(Theme::builtin_names().len())
            .map(|(n, _)| n.to_lowercase())
            .collect();
        let mut sorted = on_disk.clone();
        sorted.sort();
        assert_eq!(on_disk, sorted, "themes on disk should be in name order");
    }
}
