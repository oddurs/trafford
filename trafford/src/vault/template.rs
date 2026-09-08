//! Expanding a template when a note is made from one.
//!
//! Not Templater — a handful of its expressions, chosen by reading the five
//! templates this vault actually has. Everything else is left exactly as
//! written, so a template using more than this degrades to what it did before
//! rather than losing text.
//!
//! The whole inventory, counted from `_templates/`:
//!
//! ```text
//!   5  <% tp.date.now("YYYY-MM-DDTHH:mm:ssZ") %>
//!   4  <% tp.file.cursor(0) %>
//!   3  <% tp.file.title %>
//!   4  <% tp.date.now("YYYY-MM-DD", ±n) %>
//!   1  <% tp.date.now("ww, YYYY") %>
//!   1  <% tp.date.now("dddd, MMMM D, YYYY") %>
//! ```

use chrono::{DateTime, Duration, Local};

/// A template with its expressions filled in.
pub struct Expanded {
    pub text: String,
    /// Where `tp.file.cursor()` asked the cursor to go, as a line index.
    pub cursor: Option<usize>,
}

/// Fill in what this understands and leave the rest alone.
pub fn expand(template: &str, title: &str, now: DateTime<Local>) -> Expanded {
    let mut out = String::with_capacity(template.len());
    let mut cursor_at = None;
    let mut rest = template;

    while let Some(start) = rest.find("<%") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("%>") else {
            // Never closed. That is not an expression, it is text.
            out.push_str(&rest[start..]);
            return finish(out, cursor_at);
        };
        let body = after[..end].trim();
        match evaluate(body, title, now) {
            Some(Value::Text(text)) => out.push_str(&text),
            Some(Value::Cursor) => {
                if cursor_at.is_none() {
                    cursor_at = Some(out.matches('\n').count());
                }
            }
            // Not understood: put it back exactly as it was. Blanking it would
            // lose whatever the reader meant by it.
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    finish(out, cursor_at)
}

fn finish(text: String, cursor: Option<usize>) -> Expanded {
    Expanded { text, cursor }
}

enum Value {
    Text(String),
    Cursor,
}

fn evaluate(body: &str, title: &str, now: DateTime<Local>) -> Option<Value> {
    if body == "tp.file.title" {
        return Some(Value::Text(title.to_string()));
    }
    if body.starts_with("tp.file.cursor") {
        return Some(Value::Cursor);
    }
    let args = body.strip_prefix("tp.date.now")?.trim();
    let args = args.strip_prefix('(')?.strip_suffix(')')?;
    let (format, offset) = split_args(args)?;
    let when = now + Duration::days(offset);
    Some(Value::Text(
        when.format(&moment_to_strftime(&format)).to_string(),
    ))
}

/// `"YYYY-MM-DD", -1` into its two parts.
fn split_args(args: &str) -> Option<(String, i64)> {
    let args = args.trim();
    let quote = args.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &args[1..];
    let close = rest.find(quote)?;
    let format = rest[..close].to_string();
    let tail = rest[close + 1..].trim().trim_start_matches(',').trim();
    let offset = if tail.is_empty() {
        0
    } else {
        tail.parse().ok()?
    };
    Some((format, offset))
}

/// Templater's dates are moment.js, and chrono's are strftime.
///
/// Longest token first, or `YYYY` is read as two `YY`s and `MMMM` as two
/// `MM`s. Anything unrecognised is copied through, which is what makes a
/// separator like `-` or `, ` survive.
pub fn moment_to_strftime(format: &str) -> String {
    const TOKENS: &[(&str, &str)] = &[
        ("YYYY", "%Y"),
        ("MMMM", "%B"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("MMM", "%b"),
        ("YY", "%y"),
        ("MM", "%m"),
        ("DD", "%d"),
        ("HH", "%H"),
        ("hh", "%I"),
        ("mm", "%M"),
        ("ss", "%S"),
        ("ww", "%V"),
        ("A", "%p"),
        ("D", "%-d"),
        ("M", "%-m"),
        ("Z", "%:z"),
    ];
    let mut out = String::with_capacity(format.len() * 2);
    let bytes = format.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Anything in brackets is literal, which is how `YYYY-[W]ww` keeps
        // its W.
        if bytes[i] == b'[' {
            if let Some(close) = format[i..].find(']') {
                out.push_str(&format[i + 1..i + close]);
                i += close + 1;
                continue;
            }
        }
        let matched = TOKENS
            .iter()
            .find(|(m, _)| format[i..].starts_with(m))
            .map(|(m, s)| (m.len(), *s));
        match matched {
            Some((len, strf)) => {
                out.push_str(strf);
                i += len;
            }
            None => {
                // A literal `%` would be read as a strftime escape.
                if bytes[i] == b'%' {
                    out.push('%');
                }
                out.push(format[i..].chars().next().unwrap());
                i += format[i..].chars().next().unwrap().len_utf8();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn when() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 3, 17, 9, 30, 0).unwrap()
    }

    fn ex(t: &str) -> String {
        expand(t, "My Note", when()).text
    }

    #[test]
    fn the_expressions_this_vault_uses_all_expand() {
        assert_eq!(ex(r#"<% tp.date.now("YYYY-MM-DD") %>"#), "2026-03-17");
        assert_eq!(
            ex(r#"<% tp.date.now("dddd, MMMM D, YYYY") %>"#),
            "Tuesday, March 17, 2026"
        );
        assert_eq!(ex("<% tp.file.title %>"), "My Note");
    }

    #[test]
    fn a_day_offset_moves_the_date() {
        assert_eq!(ex(r#"<% tp.date.now("YYYY-MM-DD", -1) %>"#), "2026-03-16");
        assert_eq!(ex(r#"<% tp.date.now("YYYY-MM-DD", 1) %>"#), "2026-03-18");
        assert_eq!(ex(r#"<% tp.date.now("YYYY-MM-DD", -7) %>"#), "2026-03-10");
    }

    #[test]
    fn the_prev_and_next_links_a_daily_note_writes_come_out_right() {
        // The reason the format has to be exact: get it wrong and every day
        // links to a note that does not exist.
        let line = r#"<< [[<% tp.date.now("YYYY-MM-DD", -1) %>]] | [[<% tp.date.now("YYYY-MM-DD", 1) %>]] >>"#;
        assert_eq!(ex(line), "<< [[2026-03-16]] | [[2026-03-18]] >>");
    }

    #[test]
    fn a_bracketed_literal_survives() {
        // `YYYY-[W]ww` is how a week note is named.
        assert_eq!(ex(r#"<% tp.date.now("YYYY-[W]ww") %>"#), "2026-W12");
    }

    #[test]
    fn the_cursor_marker_expands_to_nothing_and_says_where_it_was() {
        let out = expand(
            "# Title\n\n## Notes\n<% tp.file.cursor(0) %>\n",
            "T",
            when(),
        );
        assert_eq!(out.text, "# Title\n\n## Notes\n\n");
        assert_eq!(out.cursor, Some(3), "the line it was on");
    }

    #[test]
    fn an_expression_this_does_not_know_is_left_exactly_as_written() {
        // Losing text is worse than not expanding it.
        let t = "<% tp.system.prompt(\"name\") %> and <% tp.file.folder() %>";
        assert_eq!(ex(t), t);
    }

    #[test]
    fn an_unclosed_expression_is_text() {
        assert_eq!(
            ex("a <% tp.date.now(\"YYYY\") and on"),
            "a <% tp.date.now(\"YYYY\") and on"
        );
    }

    #[test]
    fn a_note_with_no_expressions_is_untouched() {
        let t = "# Plain\n\nJust words, 50% of them.\n";
        assert_eq!(ex(t), t);
    }

    #[test]
    fn moment_tokens_map_longest_first() {
        assert_eq!(moment_to_strftime("YYYY-MM-DD"), "%Y-%m-%d");
        assert_eq!(moment_to_strftime("MMMM"), "%B");
        assert_eq!(moment_to_strftime("dddd, MMMM D, YYYY"), "%A, %B %-d, %Y");
        assert_eq!(
            moment_to_strftime("YYYY-MM-DDTHH:mm:ss"),
            "%Y-%m-%dT%H:%M:%S"
        );
    }
}
