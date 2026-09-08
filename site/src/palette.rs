//! The site's colours, generated from the app's themes.
//!
//! Typed into a stylesheet by hand, the site's blue and the app's blue drift
//! the first time a theme is adjusted — and since the screenshots on the page
//! come out of the real binary, the drift shows up as a screenshot that looks
//! subtly broken rather than as a colour that is wrong. So the stylesheet
//! refers only to `var(--…)` and this file writes the values.
//!
//! One thing is not a straight copy. A terminal theme is designed against a
//! terminal's font rendering, and some roles — `faint`, `border` — are
//! deliberately at the edge of legible there. The same value in a browser can
//! be genuinely unreadable, so a role used for *text* is lifted until it meets
//! WCAG AA against the ground it sits on. `every_text_role_meets_aa` asserts
//! the output rather than the intent.

use std::fmt::Write as _;

use anyhow::{anyhow, Context, Result};
use trafford::ui::theme::{Color, Theme};

/// Contrast required of anything a reader is expected to read.
const AA: f64 = 4.5;
/// Contrast required of borders and rules — shapes, not words.
const AA_LARGE: f64 = 3.0;

/// Every built-in theme, as CSS custom properties.
///
/// The first dark theme is the default, the first light one is what a reader
/// whose system says so gets, and every theme also has a `[data-theme]` block
/// so the switcher can pick one explicitly.
pub fn stylesheet() -> Result<String> {
    let themes = translatable()?;
    let dark = themes
        .iter()
        .find(|(_, t)| t.dark)
        .ok_or_else(|| anyhow!("no dark theme to default to"))?;
    let default_vars = format!(
        "{}{}",
        vars(&dark.1).with_context(|| format!("theme {}", dark.0))?,
        terminal_vars(&dark.1)?
    );
    let light = themes.iter().find(|(_, t)| !t.dark);

    let mut css = String::new();
    css.push_str("/* Generated from trafford's built-in themes. Do not edit. */\n");
    let _ = writeln!(css, ":root {{\n{}}}\n", default_vars);

    if let Some((_, light)) = light {
        // Only when the reader has not chosen: an explicit `[data-theme]`
        // below wins, which is what makes the switcher stick.
        let _ = writeln!(
            css,
            "@media (prefers-color-scheme: light) {{\n  :root:not([data-theme]) {{\n{}  }}\n}}\n",
            indent(&format!("{}{}", vars(light)?, terminal_vars(light)?))
        );
    }
    for (name, theme) in &themes {
        let _ = writeln!(
            css,
            "[data-theme=\"{name}\"] {{\n{}{}}}\n",
            vars(theme).with_context(|| format!("theme {name}"))?,
            terminal_vars(theme).with_context(|| format!("theme {name}"))?
        );
    }
    Ok(css)
}

/// Every translatable theme's roles, as JSON.
///
/// `tools/shots.py` reads this rather than parsing `themes/*.toml`: a theme
/// file may leave a role unstated and have it derived, and a second reader of
/// the format would get a different answer from the program. One table, from
/// the same `Theme` the app draws with.
pub fn as_json() -> Result<String> {
    let mut out = String::from("{\n  \"roles\": [");
    for (i, role) in ROLES.iter().enumerate() {
        let _ = write!(out, "{}\"{role}\"", if i == 0 { "" } else { ", " });
    }
    out.push_str("],\n  \"themes\": {\n");
    let themes = translatable()?;
    for (n, (name, theme)) in themes.iter().enumerate() {
        let colours = role_colours(theme).with_context(|| format!("theme {name}"))?;
        let _ = write!(out, "    \"{name}\": {{");
        for (i, (role, colour)) in ROLES.iter().zip(&colours).enumerate() {
            let _ = write!(
                out,
                "{}\"{role}\": \"{colour}\"",
                if i == 0 { "" } else { ", " }
            );
        }
        let _ = writeln!(out, "}}{}", if n + 1 == themes.len() { "" } else { "," });
    }
    out.push_str("  }\n}\n");
    Ok(out)
}

/// The built-in themes that have colours of their own.
///
/// `mono` is deliberately not one of them. It sets its background, foreground
/// and half its roles to `reset`, which means "whatever the terminal already
/// is" — a real and useful thing to ask for in a terminal, and a thing a
/// browser cannot answer. Rather than inventing a palette on its behalf and
/// calling the result `mono`, the site does not offer it.
pub fn translatable() -> Result<Vec<(String, Theme)>> {
    Ok(builtins()?
        .into_iter()
        .filter(|(_, t)| rgb(t.bg).is_ok() && rgb(t.fg).is_ok())
        .collect())
}

/// The built-in themes, in the order the app lists them.
pub fn builtins() -> Result<Vec<(String, Theme)>> {
    Theme::builtin_names()
        .into_iter()
        .map(|name| {
            let theme = Theme::builtin(name)
                .ok_or_else(|| anyhow!("built-in theme {name} does not parse"))?;
            Ok((name.to_string(), theme))
        })
        .collect()
}

/// Every role a theme names, as the recordings refer to them.
///
/// The order is the theme file's. `shots.py` records through a theme whose
/// colours are sentinels — one unique value per entry here — so a captured
/// cell maps back to exactly one role. Reversing a real palette instead is
/// ambiguous: gotham draws `faint` and `selection` in the same hex, and they
/// are nowhere near each other in paper.
pub const ROLES: [&str; 23] = [
    "bg",
    "surface",
    "overlay",
    "fg",
    "muted",
    "faint",
    "border",
    "border-focus",
    "selection",
    "cursorline",
    "accent",
    "secondary",
    "heading",
    "link",
    "broken",
    "code",
    "tag",
    "added",
    "removed",
    "modified",
    "mode-normal",
    "mode-insert",
    "mode-visual",
];

/// One theme's roles, in `ROLES` order, as `rrggbb` with no leading `#`.
pub fn role_colours(t: &Theme) -> Result<Vec<String>> {
    let colours = [
        t.bg,
        t.surface,
        t.overlay,
        t.fg,
        t.muted,
        t.faint,
        t.border,
        t.border_focus,
        t.selection,
        t.cursorline,
        t.accent,
        t.secondary,
        t.heading,
        t.link,
        t.broken,
        t.code,
        t.tag,
        t.added,
        t.removed,
        t.modified,
        t.mode_normal,
        t.mode_insert,
        t.mode_visual,
    ];
    colours
        .into_iter()
        .map(|c| Ok(hex(rgb(c)?).trim_start_matches('#').to_string()))
        .collect()
}

/// The terminal's own colours, for the recordings.
///
/// Deliberately not run through `lift`: a recording is a picture of a terminal,
/// drawn in the terminal's colours on the terminal's own ground, and raising
/// one of them for contrast against the *page* would be showing the reader
/// something the program does not draw. `tools/look.py` refuses to
/// contrast-check inside a recording for the same reason.
fn terminal_vars(t: &Theme) -> Result<String> {
    let mut out = String::new();
    for (role, colour) in ROLES.iter().zip(role_colours(t)?) {
        let _ = writeln!(out, "  --term-{role}: #{colour};");
    }
    Ok(out)
}

/// The custom properties for one theme.
fn vars(t: &Theme) -> Result<String> {
    let mut out = String::new();
    for (name, colour) in page_roles(t)? {
        let _ = writeln!(out, "  --{name}: {};", hex(colour));
    }
    let _ = writeln!(
        out,
        "  color-scheme: {};",
        if t.dark { "dark" } else { "light" }
    );
    Ok(out)
}

/// One role and the colour it resolved to.
type Resolved = (&'static str, (u8, u8, u8));

/// Every page role, resolved — the one place any of them is decided.
///
/// `swatches()` reads this rather than working the colours out again. It did
/// work them out again, and the day `accent-text` started lifting against the
/// panel as well as the page, the specimen went on showing the old number:
/// a page whose whole purpose is that it cannot disagree with the site,
/// disagreeing with the site.
fn page_roles(t: &Theme) -> Result<Vec<Resolved>> {
    let bg = rgb(t.bg)?;
    let surface = rgb(t.surface)?;
    // Text roles sit on the page ground; `code` sits on the surface panels, so
    // that is what it has to be legible against.
    let pairs: [Resolved; 23] = [
        ("bg", bg),
        ("surface", surface),
        ("overlay", rgb(t.overlay)?),
        ("fg", lift(rgb(t.fg)?, bg, AA)),
        ("muted", lift(rgb(t.muted)?, bg, AA)),
        // `faint` is a border colour in the terminal and a *text* colour on the
        // site — every use of it here is a label. It has to clear AA.
        ("faint", lift(rgb(t.faint)?, bg, AA)),
        ("border", rgb(t.border)?),
        ("border-focus", rgb(t.border_focus)?),
        ("selection", rgb(t.selection)?),
        ("cursorline", rgb(t.cursorline)?),
        // The accent is a bar, a marker and a mark: a shape, which needs 3:1.
        ("accent", lift(rgb(t.accent)?, bg, AA_LARGE)),
        // The same colour used for *words* needs 4.5:1, and in the light theme
        // the two are far enough apart to matter. One role each rather than a
        // compromise that is wrong for both.
        ("accent-text", legible(rgb(t.accent)?, bg, surface)),
        ("secondary", lift(rgb(t.secondary)?, bg, AA_LARGE)),
        ("heading", lift(rgb(t.heading)?, bg, AA)),
        ("link", lift(rgb(t.link)?, bg, AA)),
        ("broken", lift(rgb(t.broken)?, bg, AA)),
        ("code", lift(rgb(t.code)?, surface, AA)),
        ("tag", lift(rgb(t.tag)?, bg, AA)),
        ("added", lift(rgb(t.added)?, bg, AA_LARGE)),
        ("removed", lift(rgb(t.removed)?, bg, AA_LARGE)),
        ("modified", lift(rgb(t.modified)?, bg, AA_LARGE)),
        // A callout titles itself in the colour of its kind, which makes these
        // three words rather than the bars they are beside.
        ("added-text", legible(rgb(t.added)?, bg, surface)),
        ("modified-text", legible(rgb(t.modified)?, bg, surface)),
    ];
    Ok(pairs.to_vec())
}

/// A colour that carries as words on either ground the page has.
///
/// Lifting against `bg` alone is not enough: a callout sits on `surface`, and
/// in paper that is the *darker* of the two, so a colour that just clears AA
/// on the page ground misses on the panel. Every one of the three callout
/// titles did — the worst at 3.88:1 — and only in the light theme, which is
/// the one nobody was looking at.
fn legible(colour: (u8, u8, u8), bg: (u8, u8, u8), surface: (u8, u8, u8)) -> (u8, u8, u8) {
    lift(lift(colour, bg, AA), surface, AA)
}

fn indent(block: &str) -> String {
    block
        .lines()
        .map(|l| format!("  {l}\n"))
        .collect::<String>()
}

/// One theme colour as `#rrggbb`, for the places outside the stylesheet that
/// need a literal — the favicon, mostly.
pub fn hex_of(c: Color) -> Result<String> {
    Ok(hex(rgb(c)?))
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A ratatui colour as RGB.
///
/// Anything that has no fixed value — `Reset`, which means "whatever the
/// terminal was" — is an error rather than a guess: inventing a colour here
/// would produce a palette nobody chose and no test would notice.
fn rgb(c: Color) -> Result<(u8, u8, u8)> {
    Ok(match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(i) => indexed(i),
        Color::Black => indexed(0),
        Color::Red => indexed(1),
        Color::Green => indexed(2),
        Color::Yellow => indexed(3),
        Color::Blue => indexed(4),
        Color::Magenta => indexed(5),
        Color::Cyan => indexed(6),
        Color::Gray => indexed(7),
        Color::DarkGray => indexed(8),
        Color::LightRed => indexed(9),
        Color::LightGreen => indexed(10),
        Color::LightYellow => indexed(11),
        Color::LightBlue => indexed(12),
        Color::LightMagenta => indexed(13),
        Color::LightCyan => indexed(14),
        Color::White => indexed(15),
        Color::Reset => {
            return Err(anyhow!(
                "a theme role is `reset`, which has no colour outside a terminal"
            ))
        }
    })
}

/// The xterm-256 palette: sixteen named colours, a 6×6×6 cube, then greys.
fn indexed(i: u8) -> (u8, u8, u8) {
    const BASE: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00),
        (0x80, 0x00, 0x00),
        (0x00, 0x80, 0x00),
        (0x80, 0x80, 0x00),
        (0x00, 0x00, 0x80),
        (0x80, 0x00, 0x80),
        (0x00, 0x80, 0x80),
        (0xc0, 0xc0, 0xc0),
        (0x80, 0x80, 0x80),
        (0xff, 0x00, 0x00),
        (0x00, 0xff, 0x00),
        (0xff, 0xff, 0x00),
        (0x00, 0x00, 0xff),
        (0xff, 0x00, 0xff),
        (0x00, 0xff, 0xff),
        (0xff, 0xff, 0xff),
    ];
    match i {
        0..=15 => BASE[i as usize],
        16..=231 => {
            let n = i - 16;
            let step = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            (step(n / 36), step((n / 6) % 6), step(n % 6))
        }
        _ => {
            let v = 8 + 10 * (i - 232);
            (v, v, v)
        }
    }
}

/// WCAG relative luminance.
fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let channel = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// The WCAG contrast ratio between two colours, 1.0 to 21.0.
pub fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Move `fg` away from `bg` until it meets `wanted`, or as far as it can go.
///
/// Deliberately not an error. A vault's theme is the reader's choice and a
/// terminal is entitled to a `faint` that a browser cannot render legibly;
/// refusing to build would make the app's themes answerable to the website,
/// which is backwards. Nudging is visible in the generated CSS and asserted in
/// a test, so it is a decision rather than an accident.
fn lift(fg: (u8, u8, u8), bg: (u8, u8, u8), wanted: f64) -> (u8, u8, u8) {
    if contrast(fg, bg) >= wanted {
        return fg;
    }
    // Toward white on a dark ground, toward black on a light one. Anything
    // else would move the colour through the background it is trying to
    // escape and get worse before it got better.
    let target: (u8, u8, u8) = if luminance(bg) < 0.5 {
        (255, 255, 255)
    } else {
        (0, 0, 0)
    };
    let mut best = fg;
    for step in 1..=64 {
        let t = step as f64 / 64.0;
        let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
        best = (
            mix(fg.0, target.0),
            mix(fg.1, target.1),
            mix(fg.2, target.2),
        );
        if contrast(best, bg) >= wanted {
            break;
        }
    }
    best
}

/// The palette, drawn, with the contrast every role actually achieves.
///
/// A hex value tells a reader nothing and a ratio tells them everything, which
/// is the same argument `CLAUDE.md` makes about the terminal: look at colour,
/// do not reason about it. The swatches take the current theme through
/// `var(--…)`, so switching the theme in the header re-measures nothing and
/// re-draws everything.
pub fn swatches() -> Result<String> {
    let roles: [(&str, &str, bool); 12] = [
        ("bg", "the page", false),
        ("surface", "a card, a code block", false),
        ("fg", "body text", true),
        ("muted", "secondary text", true),
        ("faint", "labels and captions", true),
        ("heading", "headings", true),
        ("link", "a link", true),
        ("accent", "a bar, a marker, the mark", false),
        ("accent-text", "the same colour, used for words", true),
        ("code", "inline code", true),
        ("tag", "a #tag", true),
        ("border", "every rule on the page", false),
    ];
    let (name, theme) = translatable()?
        .into_iter()
        .find(|(_, t)| t.dark)
        .ok_or_else(|| anyhow!("no dark theme to describe"))?;
    let bg = rgb(theme.bg)?;
    let resolved = page_roles(&theme)?;

    let mut out = format!(
        "<h2 id=\"colour\"><a class=\"anchor\" href=\"#colour\">Colour</a></h2>\n         <p>Generated from the application's own theme files — the ratios below are          measured against <code>--bg</code> and are what the {name} theme achieves.          A role used for words is lifted until it clears 4.5:1; a role used for a          shape needs 3:1, which is why <code>--accent</code> and          <code>--accent-text</code> are two roles rather than a compromise.</p>\n         <div class=\"swatches\">\n"
    );
    for (role, use_for, is_text) in roles {
        let colour = resolved
            .iter()
            .find(|(name, _)| *name == role)
            .map(|(_, c)| *c)
            .ok_or_else(|| anyhow!("the specimen names --{role}, which is not a role"))?;
        let ratio = contrast(colour, bg);
        let against = if is_text {
            format!("{ratio:.1}:1")
        } else {
            String::from("—")
        };
        let _ = writeln!(
            out,
            "<div class=\"swatch\">\
             <span class=\"chip\" style=\"background: var(--{role})\"></span>\
             <code>--{role}</code><span class=\"swatch-use\">{use_for}</span>\
             <span class=\"swatch-ratio\">{against}</span></div>"
        );
    }
    out.push_str("</div>\n");
    out.push_str(&terminal_swatches()?);
    Ok(out)
}

/// The terminal's own palette, which is a second set and not a variant of the
/// first.
///
/// These are what the recordings are painted in, and they are deliberately
/// *not* lifted: a recording is a picture of a terminal on the terminal's own
/// ground, and raising one of its colours for contrast against the page would
/// be showing a reader something the program does not draw.
fn terminal_swatches() -> Result<String> {
    let mut out = String::from(
        "<h2 id=\"terminal\"><a class=\"anchor\" href=\"#terminal\">The terminal palette</a></h2>\n\
         <p>A recording carries the <em>role</em> that drew each cell rather than a \
         colour, so it is painted in whichever theme you are reading in — switch \
         themes and every recording on this site follows. These are the values it \
         resolves through, unlifted, because a picture of a terminal should show \
         what the terminal shows.</p>\n",
    );
    for (name, theme) in translatable()? {
        let colours = role_colours(&theme)?;
        let _ = writeln!(out, "<h3>{name}</h3>\n<div class=\"swatches\">");
        for (role, colour) in ROLES.iter().zip(&colours) {
            let _ = writeln!(
                out,
                "<div class=\"swatch\">\
                 <span class=\"chip\" style=\"background: #{colour}\"></span>\
                 <code>--term-{role}</code>\
                 <span class=\"swatch-ratio\">#{colour}</span></div>"
            );
        }
        out.push_str("</div>\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every role a reader reads words in clears AA against the ground it is
    /// drawn on — in every built-in theme, not just the default one.
    #[test]
    fn every_text_role_meets_aa() {
        for (name, t) in translatable().unwrap() {
            let bg = rgb(t.bg).unwrap();
            let surface = rgb(t.surface).unwrap();
            for (role, colour, ground, wanted) in [
                ("fg", t.fg, bg, AA),
                ("muted", t.muted, bg, AA),
                ("heading", t.heading, bg, AA),
                ("link", t.link, bg, AA),
                ("broken", t.broken, bg, AA),
                ("tag", t.tag, bg, AA),
                ("code", t.code, surface, AA),
                ("faint", t.faint, bg, AA),
                ("accent-text", t.accent, bg, AA),
                ("accent", t.accent, bg, AA_LARGE),
            ] {
                let lifted = lift(rgb(colour).unwrap(), ground, wanted);
                let got = contrast(lifted, ground);
                assert!(
                    got >= wanted - 0.01,
                    "{name}: {role} is {got:.2}:1 against its ground, wanted {wanted}"
                );
            }
        }
    }

    /// Known values, so a rewrite of the luminance maths is caught.
    #[test]
    fn contrast_matches_the_published_ratios() {
        let white = (255, 255, 255);
        let black = (0, 0, 0);
        assert!((contrast(white, black) - 21.0).abs() < 0.01);
        assert!((contrast(white, white) - 1.0).abs() < 0.01);
        // #767676 on white is the canonical "just passes AA" grey.
        assert!(contrast((0x76, 0x76, 0x76), white) >= 4.5);
        assert!(contrast((0x77, 0x77, 0x77), white) < 4.5);
    }

    /// A colour that already passes is left exactly as the theme wrote it —
    /// the generator is not allowed to redecorate.
    #[test]
    fn a_role_that_already_passes_is_untouched() {
        let bg = (0x10, 0x10, 0x14);
        let fg = (0xd0, 0xd0, 0xd0);
        assert_eq!(lift(fg, bg, AA), fg);
    }

    #[test]
    fn the_stylesheet_has_a_block_for_every_theme() {
        let css = stylesheet().unwrap();
        for (name, _) in translatable().unwrap() {
            assert!(
                css.contains(&format!("[data-theme=\"{name}\"]")),
                "no block for {name}"
            );
        }
        assert!(css.contains(":root {"), "no default block");
        assert!(css.contains("prefers-color-scheme: light"), "no light case");
    }

    /// Same input, same output. The build's determinism is only as good as its
    /// least deterministic step, and a HashMap iteration here would be one.
    #[test]
    fn the_stylesheet_is_deterministic() {
        assert_eq!(stylesheet().unwrap(), stylesheet().unwrap());
    }

    #[test]
    fn the_xterm_cube_matches_the_standard() {
        assert_eq!(indexed(16), (0, 0, 0));
        assert_eq!(indexed(231), (255, 255, 255));
        assert_eq!(indexed(196), (255, 0, 0));
        assert_eq!(indexed(232), (8, 8, 8));
    }

    /// Every role a recording names is one the stylesheet defines.
    ///
    /// The recordings carry roles rather than colours so the reader's theme
    /// reaches them, which couples three things that live apart: `ROLES`
    /// here, the theme `shots.py` records through, and the `var(--term-…)`
    /// the player asks for. Rename a role and every recording silently draws
    /// in the fallback colour — the failure looks like nothing at all.
    #[test]
    fn every_role_the_recordings_name_is_one_the_stylesheet_defines() {
        let img = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("docs/img");
        let css = stylesheet().unwrap();

        let mut seen = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(&img).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let body = std::fs::read_to_string(&path).unwrap();
            if name.ends_with(".cast.json") {
                // `"styles":[["accent","bg",false,…],…]` — the first two of
                // each entry. A literal recording carries hex, and is skipped.
                for role in body
                    .split('"')
                    .filter(|s| s.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
                    .filter(|s| !s.is_empty())
                {
                    if ROLES.contains(&role) {
                        seen.insert(role.to_string());
                    }
                }
            } else if name.ends_with(".svg") && body.contains("shot-palette") {
                for capture in body.split("var(--term-").skip(1) {
                    let role = capture.split(',').next().unwrap_or("");
                    seen.insert(role.to_string());
                }
            }
        }

        assert!(
            !seen.is_empty(),
            "no recording named a role — has shots.py stopped recording through the sentinel theme?"
        );
        for role in &seen {
            assert!(
                ROLES.contains(&role.as_str()),
                "a recording draws with `{role}`, which is not a role"
            );
            assert!(
                css.contains(&format!("--term-{role}:")),
                "recordings draw with `{role}` and the stylesheet never defines --term-{role}"
            );
        }
    }

    /// The player asks for the roles rather than for colours.
    #[test]
    fn the_player_paints_through_the_terminal_roles() {
        let js = include_str!("../assets/site.js");
        assert!(
            js.contains("var(--term-"),
            "site.js no longer paints a recording through the theme"
        );
    }

    /// Every theme answers for every role, so no recording falls back.
    #[test]
    fn every_theme_defines_every_terminal_role() {
        let css = stylesheet().unwrap();
        for (name, _) in translatable().unwrap() {
            let block = css
                .split(&format!("[data-theme=\"{name}\"] {{"))
                .nth(1)
                .unwrap_or_else(|| panic!("no block for theme {name}"));
            let block = block.split('}').next().unwrap();
            for role in ROLES {
                assert!(
                    block.contains(&format!("--term-{role}:")),
                    "theme {name} does not define --term-{role}"
                );
            }
        }
    }
}
