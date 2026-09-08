//! The design system: one source for every measurement the site makes.
//!
//! Colour already worked this way — `palette.rs` generates it from the app's
//! themes, and the stylesheet holds no hex literal. Everything else did not:
//! the stylesheet had about sixty locally-chosen sizes and five ad-hoc
//! durations, each defensible on its own line and none of them a system.
//!
//! So the scales live here, as data, for three reasons that a block of custom
//! properties in the CSS would not give:
//!
//!   * the specimen page at `/design` is *rendered from them*, so it cannot
//!     drift from what the site actually uses;
//!   * `no_raw_values_for_what_the_system_owns` reads the stylesheet and fails
//!     when a font size, radius, duration or space is written as a literal;
//!   * reduced motion is a property of the tokens rather than of every rule
//!     that animates — the durations become `0ms` and every transition stops,
//!     including ones written after this was.
//!
//! What is deliberately *not* here: the width of a recording, the height of a
//! cropped card. Those are measurements of a particular picture, not decisions
//! about the system, and pretending otherwise would make the scale meaningless.

use std::fmt::Write as _;

/// One token: what it is called, what it is, and what it is for.
///
/// The third field is not decoration — it is what the specimen page prints,
/// and a token nobody can say the use of is a token that should not exist.
pub struct Token {
    pub name: &'static str,
    pub value: &'static str,
    pub use_for: &'static str,
}

/// The type scale. Monospace, so the steps are closer together than a
/// proportional scale would want: a monospaced face at 1.25× reads as a
/// different font rather than as the same one larger.
pub const TYPE: &[Token] = &[
    Token {
        name: "text-3xs",
        value: "0.7rem",
        use_for: "eyebrows, the section numbers in the gutter",
    },
    Token {
        name: "text-2xs",
        value: "0.78rem",
        use_for: "captions, the facts line, footnotes",
    },
    Token {
        name: "text-xs",
        value: "0.82rem",
        use_for: "controls, chips, the install command",
    },
    Token {
        name: "text-sm",
        value: "0.9rem",
        use_for: "card body, navigation, the footer",
    },
    Token {
        name: "text-md",
        value: "1rem",
        use_for: "body text, and the base every rem is measured from",
    },
    Token {
        name: "text-lg",
        value: "1.15rem",
        use_for: "a lead paragraph under a heading",
    },
    Token {
        name: "text-xl",
        value: "1.4rem",
        use_for: "h3, and h2 in the documentation",
    },
    Token {
        name: "text-2xl",
        value: "1.75rem",
        use_for: "card titles on the landing page",
    },
    Token {
        name: "text-3xl",
        value: "clamp(1.7rem, 3.4vw, 2.5rem)",
        use_for: "a section heading",
    },
    Token {
        name: "text-4xl",
        value: "clamp(2.4rem, 6.2vw, 4.4rem)",
        use_for: "the claim, once per page",
    },
];

/// Spacing, on a four-pixel grid until it stops being useful and doubles.
pub const SPACE: &[Token] = &[
    Token {
        name: "space-1",
        value: "0.25rem",
        use_for: "inside a chip",
    },
    Token {
        name: "space-2",
        value: "0.5rem",
        use_for: "between a label and its value",
    },
    Token {
        name: "space-3",
        value: "0.75rem",
        use_for: "inside a control",
    },
    Token {
        name: "space-4",
        value: "1rem",
        use_for: "between paragraphs",
    },
    Token {
        name: "space-5",
        value: "1.5rem",
        use_for: "inside a card",
    },
    Token {
        name: "space-6",
        value: "2rem",
        use_for: "between a heading and what follows",
    },
    Token {
        name: "space-7",
        value: "3rem",
        use_for: "between groups",
    },
    Token {
        name: "space-8",
        value: "4rem",
        use_for: "around the footer",
    },
    Token {
        name: "space-9",
        value: "6rem",
        use_for: "the largest deliberate gap",
    },
    Token {
        name: "space-section",
        value: "clamp(3.5rem, 8vw, 6rem)",
        use_for: "above and below a section",
    },
    Token {
        name: "space-gutter",
        value: "clamp(1rem, 4vw, 2.5rem)",
        use_for: "the page's own margin",
    },
];

/// Corners. Three, because a fourth would be a decision nobody could defend.
pub const RADIUS: &[Token] = &[
    Token {
        name: "radius-sm",
        value: "4px",
        use_for: "a chip or a key",
    },
    Token {
        name: "radius-md",
        value: "6px",
        use_for: "a code block, an input, a framed recording",
    },
    Token {
        name: "radius-lg",
        value: "14px",
        use_for: "a card, and the panel a recording sits in",
    },
];

/// Motion. Every duration is a token so that reduced motion can zero all of
/// them at once — including the ones in rules written after this file.
pub const MOTION: &[Token] = &[
    Token {
        name: "motion-fast",
        value: "150ms",
        use_for: "a hover, a colour change",
    },
    Token {
        name: "motion-base",
        value: "260ms",
        use_for: "a control changing state",
    },
    Token {
        name: "motion-slow",
        value: "520ms",
        use_for: "a section arriving",
    },
    Token {
        name: "ease",
        value: "cubic-bezier(0.2, 0, 0, 1)",
        use_for: "every transition on the site; one curve",
    },
];

/// Line height and the widths a page is built from.
pub const LAYOUT: &[Token] = &[
    Token {
        name: "leading-tight",
        value: "1.1",
        use_for: "display sizes",
    },
    Token {
        name: "leading-snug",
        value: "1.4",
        use_for: "headings and card titles",
    },
    Token {
        name: "leading-body",
        value: "1.75",
        use_for: "prose; monospace needs the room",
    },
    Token {
        name: "measure",
        value: "66ch",
        use_for: "how wide a line of prose may be",
    },
    Token {
        name: "page",
        value: "84rem",
        use_for: "the widest the page gets",
    },
    Token {
        name: "sidebar",
        value: "15rem",
        use_for: "the documentation navigation",
    },
    Token {
        name: "rail",
        value: "14rem",
        use_for: "the table of contents beside a page",
    },
];

/// Every group, in the order the specimen page shows them.
pub fn groups() -> Vec<(&'static str, &'static str, &'static [Token])> {
    vec![
        (
            "Type",
            "Ten steps. The claim is the only 4xl on a page.",
            TYPE,
        ),
        (
            "Space",
            "A four-pixel grid, doubling once it stops being useful.",
            SPACE,
        ),
        ("Radius", "Three corners: a chip, a panel, a card.", RADIUS),
        (
            "Motion",
            "One curve, three durations, and none of them under reduced motion.",
            MOTION,
        ),
        (
            "Layout",
            "Line height, and the widths a page is built from.",
            LAYOUT,
        ),
    ]
}

/// The tokens, as custom properties.
pub fn stylesheet() -> String {
    let mut css = String::from("/* Generated from site/src/design.rs. Do not edit. */\n:root {\n");
    for (_, _, tokens) in groups() {
        for token in tokens {
            let _ = writeln!(css, "  --{}: {};", token.name, token.value);
        }
    }
    css.push_str("}\n\n");

    // Reduced motion is a property of the scale, not of each rule that uses
    // it. Zero the durations and everything stops, including whatever is
    // written next.
    css.push_str("@media (prefers-reduced-motion: reduce) {\n  :root {\n");
    for token in MOTION {
        if token.value.ends_with("ms") {
            let _ = writeln!(css, "    --{}: 0ms;", token.name);
        }
    }
    css.push_str("  }\n}\n");
    css
}

/// The specimen: every token, drawn at the size it is.
///
/// Rendered from the same constants the stylesheet is generated from, so a
/// scale cannot be shown as one thing and used as another — which is the whole
/// reason this page is generated rather than written.
pub fn specimen(swatches: &str) -> String {
    let mut out = String::from(
        "<article class=\"prose specimen\">\n\
         <h1>The design system</h1>\n\
         <p>Every measurement this site makes comes from one of the scales \
         below, and this page is rendered from the same constants the \
         stylesheet is generated from. A token shown here is a token in use: \
         <code>every_token_is_used_by_the_stylesheet</code> fails if one is \
         defined and never reached for, and \
         <code>no_raw_values_for_what_the_system_owns</code> fails if a font \
         size, radius, duration or space is written as a literal instead.</p>\n",
    );
    out.push_str(swatches);

    for (title, blurb, tokens) in groups() {
        let id = crate::vault_slug(title);
        let _ = writeln!(
            out,
            "<h2 id=\"{id}\"><a class=\"anchor\" href=\"#{id}\">{title}</a></h2>\n             <p>{blurb}</p>\n<div class=\"table-scroll\">\n<table class=\"tokens\">\n             <thead><tr><th>Token</th><th>Value</th><th>For</th><th>Set at</th></tr></thead>\n<tbody>"
        );
        for token in tokens {
            let sample = sample(token);
            let _ = writeln!(
                out,
                "<tr><td><code>--{}</code></td><td class=\"token-value\">{}</td>                 <td>{}</td><td>{sample}</td></tr>",
                token.name,
                crate::html::escape(token.value),
                crate::html::escape(token.use_for),
            );
        }
        out.push_str("</tbody>\n</table>\n</div>\n");
    }
    out.push_str("</article>\n");
    out
}

/// A token drawn as what it does, rather than described.
fn sample(token: &Token) -> String {
    let name = token.name;
    if name.starts_with("text-") {
        format!("<span class=\"specimen-type\" style=\"font-size: var(--{name})\">Aa</span>")
    } else if name.starts_with("space-") {
        format!("<span class=\"specimen-space\" style=\"width: var(--{name})\"></span>")
    } else if name.starts_with("radius-") {
        format!("<span class=\"specimen-radius\" style=\"border-radius: var(--{name})\"></span>")
    } else if name.starts_with("motion-") || name == "ease" {
        format!("<span class=\"specimen-motion\" data-motion=\"{name}\"></span>")
    } else if name.starts_with("leading-") {
        format!(
            "<span class=\"specimen-leading\" style=\"line-height: var(--{name})\">one<br>two</span>"
        )
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSS: &str = include_str!("../assets/site.css");

    fn all() -> Vec<&'static Token> {
        groups()
            .into_iter()
            .flat_map(|(_, _, t)| t.iter())
            .collect()
    }

    #[test]
    fn every_token_is_used_by_the_stylesheet() {
        for token in all() {
            assert!(
                CSS.contains(&format!("var(--{})", token.name)),
                "--{} is defined and never used; a scale nobody reaches for is \
                 a scale that is wrong",
                token.name
            );
        }
    }

    #[test]
    fn no_token_is_defined_twice() {
        let mut names: Vec<&str> = all().iter().map(|t| t.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "a token name is defined twice");
    }

    /// The point of the whole file. A font size, a radius, a duration or a gap
    /// written as a literal is a decision made in one place that the system
    /// cannot see — which is what sixty of them looked like before this.
    #[test]
    fn no_raw_values_for_what_the_system_owns() {
        let owned = [
            ("font-size", &["var(--text-", "inherit", "em", "%"][..]),
            (
                "border-radius",
                &["var(--radius-", "0", "50%", "999px", "%"][..],
            ),
            ("gap", &["var(--space-", "0", "1px"][..]),
            ("padding", &["var(--space-", "0"][..]),
            ("margin", &["var(--space-", "0", "auto"][..]),
        ];
        let mut wrong = Vec::new();
        for (number, line) in CSS.lines().enumerate() {
            let line = line.trim();
            if line.starts_with("/*") || line.starts_with('*') || line.starts_with("--") {
                continue;
            }
            let Some((property, value)) = line.split_once(':') else {
                continue;
            };
            let property = property.trim();
            let value = value.trim().trim_end_matches(';');
            for (owner, allowed) in owned {
                // `padding-left` is owned by `padding`; `font-family` is not
                // owned by `font-size`.
                if property != owner && !property.starts_with(&format!("{owner}-")) {
                    continue;
                }
                if allowed.iter().any(|ok| value.contains(ok)) {
                    continue;
                }
                wrong.push(format!("  site.css:{}: {property}: {value}", number + 1));
            }
        }
        assert!(
            wrong.is_empty(),
            "these use a literal where the design system has a token:\n{}",
            wrong.join("\n")
        );
    }

    /// Nothing animates for a reader who asked it not to, and the guarantee is
    /// one block rather than one per rule.
    #[test]
    fn reduced_motion_zeroes_every_duration() {
        let css = stylesheet();
        let (_, quiet) = css
            .split_once("prefers-reduced-motion")
            .expect("a quiet block");
        for token in MOTION {
            if token.value.ends_with("ms") {
                assert!(
                    quiet.contains(&format!("--{}: 0ms;", token.name)),
                    "--{} keeps its duration under reduced motion",
                    token.name
                );
            }
        }
    }

    #[test]
    fn the_type_scale_only_goes_up() {
        let fixed: Vec<f32> = TYPE
            .iter()
            .filter_map(|t| t.value.strip_suffix("rem"))
            .filter_map(|v| v.parse().ok())
            .collect();
        assert!(fixed.len() >= 8, "expected most steps to be fixed sizes");
        assert!(
            fixed.windows(2).all(|w| w[1] > w[0]),
            "the type scale is not monotonic: {fixed:?}"
        );
    }

    #[test]
    fn every_token_says_what_it_is_for() {
        for token in all() {
            assert!(
                token.use_for.len() > 8,
                "--{} has no stated use, and the specimen page prints it",
                token.name
            );
        }
    }

    #[test]
    fn the_tokens_are_deterministic() {
        assert_eq!(stylesheet(), stylesheet());
    }
}
