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
    let default_vars = vars(&dark.1).with_context(|| format!("theme {}", dark.0))?;
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
            indent(&vars(light)?)
        );
    }
    for (name, theme) in &themes {
        let _ = writeln!(
            css,
            "[data-theme=\"{name}\"] {{\n{}}}\n",
            vars(theme).with_context(|| format!("theme {name}"))?
        );
    }
    Ok(css)
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

/// The custom properties for one theme.
fn vars(t: &Theme) -> Result<String> {
    let bg = rgb(t.bg)?;
    let surface = rgb(t.surface)?;
    // Text roles sit on the page ground; `code` sits on the surface panels, so
    // that is what it has to be legible against.
    let pairs: [(&str, (u8, u8, u8)); 20] = [
        ("bg", bg),
        ("surface", surface),
        ("overlay", rgb(t.overlay)?),
        ("fg", lift(rgb(t.fg)?, bg, AA)),
        ("muted", lift(rgb(t.muted)?, bg, AA)),
        ("faint", lift(rgb(t.faint)?, bg, AA_LARGE)),
        ("border", rgb(t.border)?),
        ("border-focus", rgb(t.border_focus)?),
        ("selection", rgb(t.selection)?),
        ("cursorline", rgb(t.cursorline)?),
        ("accent", lift(rgb(t.accent)?, bg, AA_LARGE)),
        ("secondary", lift(rgb(t.secondary)?, bg, AA_LARGE)),
        ("heading", lift(rgb(t.heading)?, bg, AA)),
        ("link", lift(rgb(t.link)?, bg, AA)),
        ("broken", lift(rgb(t.broken)?, bg, AA)),
        ("code", lift(rgb(t.code)?, surface, AA)),
        ("tag", lift(rgb(t.tag)?, bg, AA)),
        ("added", lift(rgb(t.added)?, bg, AA_LARGE)),
        ("removed", lift(rgb(t.removed)?, bg, AA_LARGE)),
        ("modified", lift(rgb(t.modified)?, bg, AA_LARGE)),
    ];
    let mut out = String::new();
    for (name, colour) in pairs {
        let _ = writeln!(out, "  --{name}: {};", hex(colour));
    }
    let _ = writeln!(
        out,
        "  color-scheme: {};",
        if t.dark { "dark" } else { "light" }
    );
    Ok(out)
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
                ("faint", t.faint, bg, AA_LARGE),
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
}
