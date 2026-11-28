//! Model routing for Sharee and Nexa.
//!
//! Two kinds of brain:
//! - **On this Mac**: a small open model shipped inside the app and run by
//!   llama.cpp (Metal on Apple Silicon). Works offline, nothing leaves the Mac.
//! - **Cloud**: OpenRouter (free models with a free key) or the customer's own
//!   key for OpenAI, Anthropic or DeepSeek. The [`openrouter`] module keeps the
//!   free-only catalogue, cooldowns and fallback for OpenRouter.
//!
//! Every reply is streamed token by token through a callback. Nothing here
//! invents text: if no brain is available the caller gets an error and the UI
//! says so.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

pub mod cloud;
#[cfg(feature = "local")]
pub mod embed;
#[cfg(feature = "local")]
pub mod local;
pub mod openrouter;

// The OpenRouter catalogue router (free-only default, cooldown fallback, test completion).
pub use openrouter::{
    fetch_openrouter_catalogue, ChatMessage, CompletionResult, ModelEntry, ModelRouter,
    OpenRouterHttpTransport, RouterConfig,
};

/// The model shipped inside the app.
pub const BUNDLED_MODEL: BundledModel = BundledModel {
    name: "Qwen3.5 2B",
    file: "qwen3.5-2b-q4_k_m.gguf",
    licence: "Apache-2.0",
    bytes: 1_280_835_840,
    sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
    source: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf",
};

/// The embedding model shipped inside the app for memory recall by meaning.
/// Converted to GGUF (Q8_0) at build time from the official files at a pinned
/// revision with llama.cpp's own converter (see .github/workflows/desktop.yml).
pub const EMBED_MODEL: EmbedModel = EmbedModel {
    name: "multilingual-E5-small",
    file: "multilingual-e5-small-q8_0.gguf",
    licence: "MIT",
    repo: "intfloat/multilingual-e5-small",
    revision: "fd1525a9fd15316a2d503bf26ab031a61d056e98",
    weights_sha256: "1a55775f53449dac10a2bcbc312469fac40b96d53198c407081a831f81c98477",
    bytes: 132_440_512,
    sha256: "64e9d486b70249537ee391c10bed279e975284006fd574bb0aadd01392cf945f",
};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct EmbedModel {
    pub name: &'static str,
    pub file: &'static str,
    pub licence: &'static str,
    pub repo: &'static str,
    pub revision: &'static str,
    /// SHA-256 of the official model.safetensors at `revision`.
    pub weights_sha256: &'static str,
    /// Size and SHA-256 of the GGUF the build produces (checked in CI).
    pub bytes: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct BundledModel {
    pub name: &'static str,
    pub file: &'static str,
    pub licence: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub source: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Wire {
    /// `POST {base}/chat/completions` with `Authorization: Bearer`.
    OpenAiCompatible,
    /// `POST {base}/v1/messages` with `x-api-key`.
    Anthropic,
}

/// A cloud provider the customer can connect with a key.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub wire: Wire,
    pub base_url: &'static str,
    pub default_model: &'static str,
    /// Where the customer creates a key.
    pub key_url: &'static str,
    /// Has free models with a free key.
    pub free: bool,
    pub note: &'static str,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "openrouter",
        name: "OpenRouter",
        wire: Wire::OpenAiCompatible,
        base_url: "https://openrouter.ai/api/v1",
        default_model: "openrouter/free",
        key_url: "https://openrouter.ai/keys",
        free: true,
        note: "Free key. openrouter/free picks a free model (NVIDIA Nemotron, DeepSeek, Qwen and others); any OpenRouter model id works.",
    },
    Provider {
        id: "openai",
        name: "OpenAI (GPT)",
        wire: Wire::OpenAiCompatible,
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-6-luna",
        key_url: "https://platform.openai.com/api-keys",
        free: false,
        note: "Paid key from your OpenAI account.",
    },
    Provider {
        id: "anthropic",
        name: "Anthropic (Claude)",
        wire: Wire::Anthropic,
        base_url: "https://api.anthropic.com",
        default_model: "claude-haiku-5-5",
        key_url: "https://console.anthropic.com/settings/keys",
        free: false,
        note: "Paid key from your Anthropic account.",
    },
    Provider {
        id: "deepseek",
        name: "DeepSeek",
        wire: Wire::OpenAiCompatible,
        base_url: "https://api.deepseek.com",
        default_model: "deepseek-flash",
        key_url: "https://platform.deepseek.com/api_keys",
        free: false,
        note: "Paid key from your DeepSeek account.",
    },
];

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenOptions {
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for GenOptions {
    fn default() -> Self {
        Self {
            max_tokens: 700,
            temperature: 0.7,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterError {
    /// No key saved for the chosen provider.
    MissingKey(String),
    /// The on-device model is not installed or could not load.
    LocalUnavailable(String),
    /// Network or provider error, with the provider's own message when it gave one.
    Provider(String),
    Cancelled,
}

impl std::fmt::Display for RouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouterError::MissingKey(p) => write!(f, "No API key saved for {p}."),
            RouterError::LocalUnavailable(why) => write!(f, "On-device model unavailable: {why}"),
            RouterError::Provider(why) => write!(f, "{why}"),
            RouterError::Cancelled => write!(f, "Stopped."),
        }
    }
}

impl std::error::Error for RouterError {}

/// Shared stop flag for a running reply.
#[derive(Debug, Default)]
pub struct StopFlag(AtomicBool);

impl StopFlag {
    pub fn stop(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// ChatML prompt for Qwen-family models. Thinking is switched off with an
/// empty think block so replies start at once (the persona prompt asks for
/// short, spoken answers).
pub fn chatml_prompt(messages: &[Message]) -> String {
    let mut out = String::new();
    for m in messages {
        out.push_str("<|im_start|>");
        out.push_str(m.role.as_str());
        out.push('\n');
        out.push_str(m.content.trim());
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>assistant\n<think>\n\n</think>\n\n");
    out
}

/// Joins streamed bytes into valid UTF-8 text, holding back an incomplete
/// multi-byte character until its remaining bytes arrive.
#[derive(Debug, Default)]
pub struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        match std::str::from_utf8(&self.pending) {
            Ok(s) => {
                let out = s.to_string();
                self.pending.clear();
                out
            }
            Err(e) => {
                let good = e.valid_up_to();
                if e.error_len().is_some() {
                    // Invalid sequence: keep what decodes, replace the rest.
                    let out = String::from_utf8_lossy(&self.pending).into_owned();
                    self.pending.clear();
                    return out;
                }
                let out = String::from_utf8_lossy(&self.pending[..good]).into_owned();
                self.pending.drain(..good);
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_have_unique_ids_and_https() {
        let mut ids: Vec<_> = PROVIDERS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PROVIDERS.len());
        assert!(PROVIDERS.iter().all(|p| p.base_url.starts_with("https://")));
        assert!(provider("openrouter").is_some_and(|p| p.free));
        assert!(provider("nope").is_none());
    }

    #[test]
    fn chatml_switches_thinking_off() {
        let p = chatml_prompt(&[
            Message::new(Role::System, "You are Nexa."),
            Message::new(Role::User, " hi "),
        ]);
        assert!(p.starts_with("<|im_start|>system\nYou are Nexa.<|im_end|>\n"));
        assert!(p.contains("<|im_start|>user\nhi<|im_end|>\n"));
        assert!(p.ends_with("<|im_start|>assistant\n<think>\n\n</think>\n\n"));
    }

    #[test]
    fn utf8_stream_holds_split_characters() {
        let mut s = Utf8Stream::default();
        let bytes = "سلام".as_bytes();
        let mut out = String::new();
        for b in bytes {
            out.push_str(&s.push(&[*b]));
        }
        assert_eq!(out, "سلام");
    }

    #[test]
    fn bundled_model_pin_is_well_formed() {
        assert_eq!(BUNDLED_MODEL.sha256.len(), 64);
        assert!(BUNDLED_MODEL.source.starts_with("https://huggingface.co/"));
        const { assert!(BUNDLED_MODEL.bytes > 1_000_000_000) };
    }
}
