use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::sync::mpsc::Sender;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    fn wire(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

/// Events streamed back from the worker thread to the UI loop.
#[derive(Debug, Clone)]
pub enum Event {
    /// A chunk of assistant text.
    Delta(String),
    /// The turn finished cleanly.
    Done,
    Error(String),
}

/// A note excerpt handed to the model as grounding context.
#[derive(Debug, Clone)]
pub struct ContextNote {
    pub id: String,
    pub title: String,
    pub body: String,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    max_tokens: u32,
    stream: bool,
    system: String,
    messages: Vec<WireMessage<'a>>,
}

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    kind: String,
    delta: Option<Delta>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct Delta {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

pub fn api_key() -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
}

/// Build the system prompt: who the model is and which notes it may lean on.
pub fn system_prompt(vault_name: &str, context: &[ContextNote]) -> String {
    let mut s = String::new();
    s.push_str(
        "You are the assistant embedded in trafford, a terminal knowledge base. \
         The user keeps a vault of interlinked markdown notes and is asking about them.\n\n\
         Answer from the notes below when they are relevant, and say plainly when they \
         do not cover the question rather than inventing detail. Cite notes by their \
         wikilink form, for example [[Note Title]], so the user can jump straight to them. \
         Keep answers tight and use markdown. When asked to draft or rewrite a note, \
         return only the note body so it can be inserted directly.\n\n",
    );
    s.push_str(&format!("Vault: {vault_name}\n"));
    if context.is_empty() {
        s.push_str("\nNo notes were retrieved for this question.\n");
        return s;
    }
    s.push_str("\n--- Retrieved notes ---\n");
    for note in context {
        s.push_str(&format!("\n## {} ({})\n", note.title, note.id));
        s.push_str(&note.body);
        s.push('\n');
    }
    s.push_str("\n--- End of retrieved notes ---\n");
    s
}

/// Truncate a note body so a handful of notes still fit comfortably in context.
pub fn excerpt(body: &str, max_chars: usize) -> String {
    if body.chars().count() <= max_chars {
        return body.to_string();
    }
    let cut: String = body.chars().take(max_chars).collect();
    // Prefer breaking at a paragraph boundary so excerpts read cleanly.
    match cut.rfind("\n\n") {
        Some(i) if i > max_chars / 2 => format!("{}\n\n[...truncated]", &cut[..i]),
        _ => format!("{cut}\n\n[...truncated]"),
    }
}

/// Send a turn to the API on a background thread, streaming deltas to `tx`.
/// Returns immediately; the UI keeps redrawing while tokens arrive.
pub fn spawn(
    model: String,
    system: String,
    history: Vec<Message>,
    tx: Sender<Event>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        if let Err(err) = stream(&model, &system, &history, &tx) {
            let _ = tx.send(Event::Error(err.to_string()));
            return;
        }
        let _ = tx.send(Event::Done);
    })
}

fn stream(model: &str, system: &str, history: &[Message], tx: &Sender<Event>) -> Result<()> {
    let Some(key) = api_key() else {
        bail!("ANTHROPIC_API_KEY is not set");
    };
    let messages: Vec<WireMessage> = history
        .iter()
        .map(|m| WireMessage {
            role: m.role.wire(),
            content: &m.text,
        })
        .collect();
    if messages.is_empty() {
        bail!("nothing to send");
    }

    let body = Request {
        model,
        max_tokens: 4096,
        stream: true,
        system: system.to_string(),
        messages,
    };

    let response = ureq::post(API_URL)
        .set("x-api-key", &key)
        .set("anthropic-version", API_VERSION)
        .set("content-type", "application/json")
        .send_json(serde_json::to_value(&body)?);

    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            bail!("{code}: {}", extract_api_error(&text));
        }
        Err(e) => bail!(e.to_string()),
    };

    let reader = BufReader::new(response.into_reader());
    for line in reader.lines() {
        let line = line?;
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        let Ok(event) = serde_json::from_str::<StreamEvent>(payload) else {
            continue;
        };
        match event.kind.as_str() {
            "content_block_delta" => {
                if let Some(text) = event.delta.and_then(|d| d.text) {
                    if tx.send(Event::Delta(text)).is_err() {
                        // The UI dropped the receiver; stop reading.
                        return Ok(());
                    }
                }
            }
            "error" => {
                let msg = event
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "stream error".into());
                bail!(msg);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Pull a human-readable message out of an API error body.
pub fn extract_api_error(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                "request failed".into()
            } else {
                trimmed.chars().take(200).collect()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_embeds_retrieved_notes() {
        let ctx = vec![ContextNote {
            id: "a.md".into(),
            title: "Alpha".into(),
            body: "body text".into(),
        }];
        let prompt = system_prompt("myvault", &ctx);
        assert!(prompt.contains("myvault"));
        assert!(prompt.contains("## Alpha (a.md)"));
        assert!(prompt.contains("body text"));
    }

    #[test]
    fn system_prompt_says_so_when_nothing_matched() {
        let prompt = system_prompt("v", &[]);
        assert!(prompt.contains("No notes were retrieved"));
    }

    #[test]
    fn excerpt_passes_short_bodies_through_untouched() {
        assert_eq!(excerpt("short", 100), "short");
    }

    #[test]
    fn excerpt_prefers_a_paragraph_boundary() {
        let body = format!("{}\n\n{}", "a".repeat(60), "b".repeat(60));
        let out = excerpt(&body, 100);
        assert!(out.ends_with("[...truncated]"));
        assert!(!out.contains("bbbb"));
    }

    #[test]
    fn excerpt_hard_cuts_when_there_is_no_boundary() {
        let out = excerpt(&"x".repeat(500), 50);
        assert!(out.starts_with(&"x".repeat(50)));
        assert!(out.ends_with("[...truncated]"));
    }

    #[test]
    fn api_errors_are_unwrapped_from_json() {
        let body =
            r#"{"type":"error","error":{"type":"not_found_error","message":"model not found"}}"#;
        assert_eq!(extract_api_error(body), "model not found");
    }

    #[test]
    fn non_json_error_bodies_fall_back_to_raw_text() {
        assert_eq!(extract_api_error("  gateway timeout "), "gateway timeout");
        assert_eq!(extract_api_error(""), "request failed");
    }

    #[test]
    fn stream_events_deserialize() {
        let payload = r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}"#;
        let ev: StreamEvent = serde_json::from_str(payload).unwrap();
        assert_eq!(ev.kind, "content_block_delta");
        assert_eq!(ev.delta.unwrap().text.unwrap(), "hi");
    }
}
