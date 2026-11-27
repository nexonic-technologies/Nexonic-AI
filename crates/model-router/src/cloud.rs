//! Cloud providers: OpenAI-compatible chat completions (OpenRouter, OpenAI,
//! DeepSeek) and Anthropic messages, both streamed as server-sent events.

use crate::{GenOptions, Message, Provider, Role, RouterError, StopFlag, Wire};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

/// Request body for a provider. Kept separate from the network call so it can be tested.
pub fn request_body(p: &Provider, model: &str, messages: &[Message], opts: GenOptions) -> Value {
    match p.wire {
        Wire::OpenAiCompatible => {
            let msgs: Vec<Value> = messages
                .iter()
                .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
                .collect();
            let mut body = json!({
                "model": model,
                "messages": msgs,
                "stream": true,
                "temperature": opts.temperature,
            });
            // OpenAI's current models take max_completion_tokens; the others max_tokens.
            let key = if p.id == "openai" {
                "max_completion_tokens"
            } else {
                "max_tokens"
            };
            body[key] = json!(opts.max_tokens);
            body
        }
        Wire::Anthropic => {
            let system: Vec<&str> = messages
                .iter()
                .filter(|m| m.role == Role::System)
                .map(|m| m.content.as_str())
                .collect();
            let msgs: Vec<Value> = messages
                .iter()
                .filter(|m| m.role != Role::System)
                .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
                .collect();
            let mut body = json!({
                "model": model,
                "max_tokens": opts.max_tokens,
                "messages": msgs,
                "stream": true,
                "temperature": opts.temperature,
            });
            if !system.is_empty() {
                body["system"] = json!(system.join("\n\n"));
            }
            body
        }
    }
}

/// Text carried by one SSE `data:` payload, if any.
pub fn delta_text(wire: Wire, data: &str) -> Option<String> {
    let v: Value = serde_json::from_str(data).ok()?;
    let text = match wire {
        Wire::OpenAiCompatible => v.pointer("/choices/0/delta/content")?.as_str()?,
        Wire::Anthropic => {
            if v.get("type")?.as_str()? != "content_block_delta" {
                return None;
            }
            v.pointer("/delta/text")?.as_str()?
        }
    };
    (!text.is_empty()).then(|| text.to_string())
}

/// The provider's own error message from an error body or SSE error event.
pub fn error_message(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    v.pointer("/error/message")
        .or_else(|| v.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Streams a reply from a cloud provider. Returns the full text.
pub fn chat(
    p: &Provider,
    key: &str,
    model: &str,
    messages: &[Message],
    opts: GenOptions,
    stop: &StopFlag,
    on_token: &mut dyn FnMut(&str),
) -> Result<String, RouterError> {
    if key.trim().is_empty() {
        return Err(RouterError::MissingKey(p.name.to_string()));
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| RouterError::Provider(format!("Could not start the network client: {e}")))?;

    let body = request_body(p, model, messages, opts);
    let req = match p.wire {
        Wire::OpenAiCompatible => client
            .post(format!("{}/chat/completions", p.base_url))
            .bearer_auth(key.trim())
            .header("HTTP-Referer", "https://nexonictechnologies.com")
            .header("X-Title", "Nexonic AI"),
        Wire::Anthropic => client
            .post(format!("{}/v1/messages", p.base_url))
            .header("x-api-key", key.trim())
            .header("anthropic-version", "2023-06-01"),
    };
    let resp = req
        .header("Accept", "text/event-stream")
        .json(&body)
        .send()
        .map_err(|e| RouterError::Provider(format!("{} could not be reached: {e}", p.name)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        let why = error_message(&text).unwrap_or_else(|| text.chars().take(240).collect());
        return Err(RouterError::Provider(format!(
            "{} answered {status}: {why}",
            p.name
        )));
    }

    let mut full = String::new();
    for line in BufReader::new(resp).lines() {
        if stop.is_stopped() {
            return Err(RouterError::Cancelled);
        }
        let line =
            line.map_err(|e| RouterError::Provider(format!("{} stream broke: {e}", p.name)))?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        if let Some(t) = delta_text(p.wire, data) {
            full.push_str(&t);
            on_token(&t);
        } else if data.contains("\"error\"") {
            if let Some(why) = error_message(data) {
                return Err(RouterError::Provider(format!("{}: {why}", p.name)));
            }
        }
    }
    if full.trim().is_empty() {
        return Err(RouterError::Provider(format!(
            "{} returned an empty reply.",
            p.name
        )));
    }
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider;

    fn msgs() -> Vec<Message> {
        vec![
            Message::new(Role::System, "You are Sharee."),
            Message::new(Role::User, "Hello"),
        ]
    }

    #[test]
    fn openai_compatible_body_keeps_system_message() {
        let p = provider("openrouter").unwrap();
        let b = request_body(p, "openrouter/free", &msgs(), GenOptions::default());
        assert_eq!(b["model"], "openrouter/free");
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["max_tokens"], 700);
        assert_eq!(b["stream"], true);
    }

    #[test]
    fn openai_uses_max_completion_tokens() {
        let p = provider("openai").unwrap();
        let b = request_body(p, "gpt-6-luna", &msgs(), GenOptions::default());
        assert!(b.get("max_tokens").is_none());
        assert_eq!(b["max_completion_tokens"], 700);
    }

    #[test]
    fn anthropic_body_moves_system_to_top_level() {
        let p = provider("anthropic").unwrap();
        let b = request_body(p, "claude-haiku-5-5", &msgs(), GenOptions::default());
        assert_eq!(b["system"], "You are Sharee.");
        assert_eq!(b["messages"].as_array().unwrap().len(), 1);
        assert_eq!(b["messages"][0]["role"], "user");
    }

    #[test]
    fn parses_stream_deltas() {
        assert_eq!(
            delta_text(
                Wire::OpenAiCompatible,
                r#"{"choices":[{"delta":{"content":"Hi"}}]}"#
            ),
            Some("Hi".into())
        );
        assert_eq!(
            delta_text(Wire::OpenAiCompatible, r#"{"choices":[{"delta":{}}]}"#),
            None
        );
        assert_eq!(
            delta_text(
                Wire::Anthropic,
                r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Yo"}}"#
            ),
            Some("Yo".into())
        );
        assert_eq!(
            delta_text(Wire::Anthropic, r#"{"type":"message_start"}"#),
            None
        );
    }

    #[test]
    fn reads_provider_error_messages() {
        assert_eq!(
            error_message(r#"{"error":{"message":"Invalid API key"}}"#),
            Some("Invalid API key".into())
        );
        assert_eq!(error_message("not json"), None);
    }

    #[test]
    fn empty_key_is_refused_without_network() {
        let p = provider("openrouter").unwrap();
        let err = chat(
            p,
            "  ",
            "m",
            &msgs(),
            GenOptions::default(),
            &StopFlag::default(),
            &mut |_| {},
        );
        assert_eq!(err, Err(RouterError::MissingKey("OpenRouter".into())));
    }
}
