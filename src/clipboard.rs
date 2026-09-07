//! Putting text on the clipboard.
//!
//! Two routes, tried in order:
//!
//! 1. **OSC 52** — an escape sequence asking the *terminal* to copy. It works
//!    through ssh and tmux, where the machine running trafford has no
//!    pasteboard of its own, and it is the reason a TUI can beat a GUI here.
//!    Not every terminal honours it, and none of them answer, so success
//!    cannot be observed.
//! 2. **A local pasteboard command** — `pbcopy`, `wl-copy`, `xclip`. These do
//!    report failure, so they are what a confident message is based on.
//!
//! Both are attempted: the local command confirms, and OSC 52 covers the case
//! where trafford is running somewhere else.

use anyhow::{anyhow, Result};
use base64::Engine;
use std::io::Write;
use std::process::{Command, Stdio};

/// Terminals cap how much they will accept in one escape sequence. Beyond
/// this, only the local pasteboard is used — a truncated clipboard is worse
/// than one that reports it could not help.
const OSC52_LIMIT: usize = 100_000;

/// What happened, so the status line can say something true rather than
/// claiming success it cannot know about.
#[derive(Debug, PartialEq, Eq)]
pub enum Copied {
    /// A local pasteboard took it, and said so.
    Local,
    /// Only the terminal was asked. It does not answer, so this is a hope.
    TerminalAsked,
}

pub fn copy(text: &str) -> Result<Copied> {
    if text.is_empty() {
        return Err(anyhow!("nothing to copy"));
    }
    // Ask the terminal first: over ssh it is the only one that can help, and
    // it costs a write either way.
    let asked = write_osc52(text).is_ok();
    match local_copy(text) {
        Ok(()) => Ok(Copied::Local),
        Err(_) if asked => Ok(Copied::TerminalAsked),
        Err(err) => Err(err),
    }
}

/// The OSC 52 sequence: `ESC ] 52 ; c ; <base64> BEL`.
///
/// Wrapped for tmux when running inside it, which otherwise swallows the
/// sequence rather than passing it to the terminal underneath.
pub fn osc52(text: &str, in_tmux: bool) -> Option<String> {
    if text.len() > OSC52_LIMIT {
        return None;
    }
    let payload = base64::engine::general_purpose::STANDARD.encode(text);
    let inner = format!("\x1b]52;c;{payload}\x07");
    Some(if in_tmux {
        // tmux passes a sequence through only when it is asked to.
        format!("\x1bPtmux;{}\x1b\\", inner.replace('\x1b', "\x1b\x1b"))
    } else {
        inner
    })
}

fn write_osc52(text: &str) -> Result<()> {
    let in_tmux = std::env::var("TMUX").is_ok();
    let sequence = osc52(text, in_tmux).ok_or_else(|| anyhow!("too long for the terminal"))?;
    let mut out = std::io::stdout();
    out.write_all(sequence.as_bytes())?;
    out.flush()?;
    Ok(())
}

/// The pasteboard commands worth trying, in order of how likely they are to be
/// the right one for the platform.
fn candidates() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("pbcopy", vec![]),
        ("wl-copy", vec![]),
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["--clipboard", "--input"]),
    ]
}

fn local_copy(text: &str) -> Result<()> {
    let mut last = anyhow!("no clipboard command found");
    for (program, args) in candidates() {
        match run(program, &args, text) {
            Ok(()) => return Ok(()),
            Err(err) => last = err,
        }
    }
    Err(last)
}

fn run(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow!("{program} took no input"))?
        .write_all(text.as_bytes())?;
    if child.wait()?.success() {
        Ok(())
    } else {
        Err(anyhow!("{program} failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(sequence: &str) -> String {
        let start = sequence.find("52;c;").unwrap() + 5;
        let end = sequence[start..].find('\x07').unwrap() + start;
        let payload = &sequence[start..end];
        String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(payload)
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn the_sequence_carries_the_text_base64_encoded() {
        let s = osc52("hello", false).unwrap();
        assert!(s.starts_with("\x1b]52;c;"), "{s:?}");
        assert!(s.ends_with('\x07'));
        assert_eq!(decode(&s), "hello");
    }

    #[test]
    fn non_ascii_survives_the_round_trip() {
        let text = "日本語 and [[a link]] ✨";
        assert_eq!(decode(&osc52(text, false).unwrap()), text);
    }

    /// tmux drops an escape sequence it was not told to pass through, so the
    /// wrapper is the difference between copy working inside tmux and not.
    #[test]
    fn inside_tmux_the_sequence_is_wrapped_for_passthrough() {
        let s = osc52("hello", true).unwrap();
        assert!(s.starts_with("\x1bPtmux;"), "{s:?}");
        assert!(s.ends_with("\x1b\\"), "{s:?}");
        // The inner escape must be doubled, or tmux eats it.
        assert!(s.contains("\x1b\x1b]52;c;"), "{s:?}");
    }

    #[test]
    fn text_too_long_for_a_terminal_is_refused_rather_than_truncated() {
        let huge = "x".repeat(OSC52_LIMIT + 1);
        assert!(osc52(&huge, false).is_none());
        // Just under the limit still works.
        assert!(osc52(&"x".repeat(OSC52_LIMIT - 1), false).is_some());
    }

    #[test]
    fn copying_nothing_is_an_error_not_a_silent_success() {
        assert!(copy("").is_err());
    }
}
