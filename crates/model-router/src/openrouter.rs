//! OpenRouter provider catalogue and free-only router for Nexonic AI (M17).
//!
//! Enforces strict cost control by defaulting to free-tier models only
//! (`allow_paid == false`), discovers available models dynamically from the
//! OpenRouter catalogue, and tracks per-model cooldowns on rate limits (`429`)
//! or transient provider unavailability (`5xx`).

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
pub const OPENROUTER_CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const DEFAULT_COOLDOWN_SECS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterError {
    MissingApiKey,
    NoEligibleModels,
    RateLimited {
        model: String,
        message: String,
    },
    ProviderUnavailable {
        model: String,
        status: u16,
        message: String,
    },
    InvalidResponse(String),
    Transport(String),
}

impl fmt::Display for RouterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => write!(f, "OpenRouter API key is not configured"),
            Self::NoEligibleModels => write!(
                f,
                "no eligible models available (all models are either paid with allow_paid=false or on cooldown)"
            ),
            Self::RateLimited { model, message } => {
                write!(f, "rate limited on {model}: {message}")
            }
            Self::ProviderUnavailable { model, status, message } => {
                write!(f, "model {model} unavailable (HTTP {status}): {message}")
            }
            Self::InvalidResponse(msg) => write!(f, "invalid provider response: {msg}"),
            Self::Transport(msg) => write!(f, "network transport error: {msg}"),
        }
    }
}

impl std::error::Error for RouterError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub is_free: bool,
    pub context_length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResult {
    pub model: String,
    pub content: String,
    pub attempts: usize,
}

#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub allow_paid: bool,
    pub preferred_model: Option<String>,
    pub cooldown_secs: u64,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            allow_paid: false,
            preferred_model: None,
            cooldown_secs: DEFAULT_COOLDOWN_SECS,
        }
    }
}

/// Transport abstraction so routing, free-only gating, and cooldown fallback
/// are verified deterministically without network access in unit tests.
pub trait ChatTransport {
    fn complete(
        &self,
        api_key: &str,
        model_id: &str,
        messages: &[ChatMessage],
    ) -> Result<String, RouterError>;
}

/// Curated fallback list of free OpenRouter model IDs used when a live
/// catalogue refresh has not yet succeeded. Only `:free` entries are included.
pub fn builtin_free_models() -> Vec<ModelEntry> {
    [
        (
            "meta-llama/llama-3.3-70b-instruct:free",
            "Llama 3.3 70B Instruct (free)",
            131_072,
        ),
        (
            "qwen/qwen-2.5-72b-instruct:free",
            "Qwen 2.5 72B Instruct (free)",
            32_768,
        ),
        (
            "mistralai/mistral-small-3.1-24b-instruct:free",
            "Mistral Small 3.1 24B (free)",
            96_000,
        ),
        (
            "google/gemma-3-27b-it:free",
            "Gemma 3 27B IT (free)",
            96_000,
        ),
    ]
    .into_iter()
    .map(|(id, name, context_length)| ModelEntry {
        id: id.to_string(),
        name: name.to_string(),
        is_free: true,
        context_length,
    })
    .collect()
}

fn price_is_zero(raw: Option<&serde_json::Value>) -> bool {
    let Some(val) = raw else {
        return false;
    };
    if let Some(s) = val.as_str() {
        let trimmed = s.trim();
        return !trimmed.is_empty() && trimmed.parse::<f64>().is_ok_and(|n| n == 0.0);
    }
    val.as_f64().is_some_and(|n| n == 0.0)
}

/// Parses OpenRouter `/api/v1/models` JSON into normalized [`ModelEntry`] records.
pub fn parse_openrouter_catalogue(json: &str) -> Result<Vec<ModelEntry>, RouterError> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| RouterError::InvalidResponse(e.to_string()))?;
    let data = root
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| RouterError::InvalidResponse("missing `data` array".into()))?;

    let mut models = Vec::with_capacity(data.len());
    for item in data {
        let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string();
        let pricing = item.get("pricing");
        let prompt_free = pricing.is_some_and(|p| price_is_zero(p.get("prompt")));
        let completion_free = pricing.is_some_and(|p| price_is_zero(p.get("completion")));
        let is_free = id.ends_with(":free") || (prompt_free && completion_free);
        let context_length = item
            .get("context_length")
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(8_192);

        models.push(ModelEntry {
            id: id.to_string(),
            name,
            is_free,
            context_length,
        });
    }

    // Put free models first, then larger context windows.
    models.sort_by(|a, b| {
        b.is_free
            .cmp(&a.is_free)
            .then_with(|| b.context_length.cmp(&a.context_length))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(models)
}

/// Parses OpenRouter `/api/v1/chat/completions` JSON response content.
pub fn parse_completion_response(json: &str) -> Result<String, RouterError> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| RouterError::InvalidResponse(e.to_string()))?;
    if let Some(err) = root.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown provider error")
            .to_string();
        let code = err.get("code").and_then(|v| v.as_u64()).unwrap_or(500) as u16;
        if code == 429 {
            return Err(RouterError::RateLimited {
                model: String::new(),
                message: msg,
            });
        }
        return Err(RouterError::ProviderUnavailable {
            model: String::new(),
            status: code,
            message: msg,
        });
    }
    root.get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RouterError::InvalidResponse("missing choices[0].message.content".into()))
}

#[derive(Debug, Clone)]
pub struct ModelRouter {
    config: RouterConfig,
    catalogue: Vec<ModelEntry>,
    cooldowns: HashMap<String, u64>,
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new(RouterConfig::default())
    }
}

impl ModelRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self {
            config,
            catalogue: builtin_free_models(),
            cooldowns: HashMap::new(),
        }
    }

    pub fn config(&self) -> &RouterConfig {
        &self.config
    }

    pub fn set_policy(&mut self, allow_paid: bool, preferred_model: Option<String>) {
        self.config.allow_paid = allow_paid;
        self.config.preferred_model = preferred_model
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
    }

    pub fn update_catalogue(&mut self, models: Vec<ModelEntry>) {
        if !models.is_empty() {
            self.catalogue = models;
        }
    }

    pub fn catalogue(&self) -> &[ModelEntry] {
        &self.catalogue
    }

    pub fn visible_catalogue(&self) -> Vec<&ModelEntry> {
        self.catalogue
            .iter()
            .filter(|m| self.config.allow_paid || m.is_free)
            .collect()
    }

    pub fn is_on_cooldown(&self, model_id: &str, now_epoch_secs: u64) -> bool {
        self.cooldowns
            .get(model_id)
            .is_some_and(|until| now_epoch_secs < *until)
    }

    pub fn cooldown_count(&self, now_epoch_secs: u64) -> usize {
        self.cooldowns
            .values()
            .filter(|until| now_epoch_secs < **until)
            .count()
    }

    pub fn mark_cooldown(&mut self, model_id: &str, now_epoch_secs: u64) {
        let until = now_epoch_secs.saturating_add(self.config.cooldown_secs);
        self.cooldowns.insert(model_id.to_string(), until);
    }

    pub fn clear_expired_cooldowns(&mut self, now_epoch_secs: u64) {
        self.cooldowns.retain(|_, until| now_epoch_secs < *until);
    }

    /// Returns eligible models in routing priority order:
    /// `preferred_model` first (if eligible & off cooldown), then remaining
    /// free/opted-in models off cooldown.
    pub fn eligible_models(&self, now_epoch_secs: u64) -> Vec<&ModelEntry> {
        let mut out: Vec<&ModelEntry> = self
            .catalogue
            .iter()
            .filter(|m| {
                (self.config.allow_paid || m.is_free) && !self.is_on_cooldown(&m.id, now_epoch_secs)
            })
            .collect();

        if let Some(pref) = &self.config.preferred_model {
            if let Some(pos) = out.iter().position(|m| &m.id == pref) {
                let entry = out.remove(pos);
                out.insert(0, entry);
            }
        }
        out
    }

    /// Sends a completion request across eligible models, placing rate-limited
    /// or unavailable models onto cooldown and falling back automatically.
    pub fn complete_chat(
        &mut self,
        api_key: &str,
        messages: &[ChatMessage],
        now_epoch_secs: u64,
        transport: &dyn ChatTransport,
    ) -> Result<CompletionResult, RouterError> {
        let key = Zeroizing::new(api_key.trim().to_string());
        if key.is_empty() {
            return Err(RouterError::MissingApiKey);
        }
        self.clear_expired_cooldowns(now_epoch_secs);
        let candidate_ids: Vec<String> = self
            .eligible_models(now_epoch_secs)
            .into_iter()
            .map(|m| m.id.clone())
            .collect();

        if candidate_ids.is_empty() {
            return Err(RouterError::NoEligibleModels);
        }

        let mut last_err = RouterError::NoEligibleModels;
        for (idx, model_id) in candidate_ids.iter().enumerate() {
            match transport.complete(&key, model_id, messages) {
                Ok(content) => {
                    return Ok(CompletionResult {
                        model: model_id.clone(),
                        content,
                        attempts: idx + 1,
                    });
                }
                Err(RouterError::RateLimited { message, .. }) => {
                    self.mark_cooldown(model_id, now_epoch_secs);
                    last_err = RouterError::RateLimited {
                        model: model_id.clone(),
                        message,
                    };
                }
                Err(RouterError::ProviderUnavailable {
                    status, message, ..
                }) => {
                    self.mark_cooldown(model_id, now_epoch_secs);
                    last_err = RouterError::ProviderUnavailable {
                        model: model_id.clone(),
                        status,
                        message,
                    };
                }
                Err(other) => return Err(other),
            }
        }
        Err(last_err)
    }
}

/// Fetches the live OpenRouter catalogue over HTTPS using `curl`.
pub fn fetch_openrouter_catalogue(api_key: Option<&str>) -> Result<Vec<ModelEntry>, RouterError> {
    let mut cmd = std::process::Command::new("curl");
    cmd.args([
        "-fsSL",
        "--max-time",
        "10",
        "-H",
        "Accept: application/json",
    ]);
    let auth_header = api_key
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|k| Zeroizing::new(format!("Authorization: Bearer {k}")));
    if let Some(ref h) = auth_header {
        cmd.args(["-H", h.as_str()]);
    }
    cmd.arg(OPENROUTER_MODELS_URL);

    let out = cmd
        .output()
        .map_err(|e| RouterError::Transport(e.to_string()))?;
    if !out.status.success() {
        return Err(RouterError::Transport(format!(
            "catalogue request exited with {}",
            out.status
        )));
    }
    let body = String::from_utf8_lossy(&out.stdout);
    parse_openrouter_catalogue(&body)
}

/// Production OpenRouter HTTPS transport using `curl` so the workspace avoids
/// extra TLS crate dependencies while keeping secrets out of error strings.
pub struct OpenRouterHttpTransport;

impl ChatTransport for OpenRouterHttpTransport {
    fn complete(
        &self,
        api_key: &str,
        model_id: &str,
        messages: &[ChatMessage],
    ) -> Result<String, RouterError> {
        let payload = serde_json::json!({
            "model": model_id,
            "messages": messages,
        });
        let body_str = serde_json::to_string(&payload)
            .map_err(|e| RouterError::InvalidResponse(e.to_string()))?;
        let auth = Zeroizing::new(format!("Authorization: Bearer {api_key}"));

        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "25",
                "-w",
                "\n%{http_code}",
                "-H",
                "Content-Type: application/json",
                "-H",
                "HTTP-Referer: https://nexonictechnologies.com",
                "-H",
                "X-Title: Nexonic AI",
                "-H",
                auth.as_str(),
                "-d",
                &body_str,
                OPENROUTER_CHAT_URL,
            ])
            .output()
            .map_err(|e| RouterError::Transport(e.to_string()))?;

        if !out.status.success() {
            return Err(RouterError::Transport(format!(
                "request failed with exit status {}",
                out.status
            )));
        }

        let raw = String::from_utf8_lossy(&out.stdout);
        let (json_part, status_str) = raw.rsplit_once('\n').unwrap_or((raw.as_ref(), "200"));
        let status: u16 = status_str.trim().parse().unwrap_or(500);

        if status == 429 {
            return Err(RouterError::RateLimited {
                model: model_id.to_string(),
                message: "rate limit exceeded".into(),
            });
        }
        if status >= 500 || status == 404 || status == 408 {
            return Err(RouterError::ProviderUnavailable {
                model: model_id.to_string(),
                status,
                message: json_part.chars().take(160).collect(),
            });
        }
        if status >= 400 {
            return Err(RouterError::InvalidResponse(format!(
                "HTTP {status} from OpenRouter"
            )));
        }

        parse_completion_response(json_part).map_err(|e| match e {
            RouterError::RateLimited { message, .. } => RouterError::RateLimited {
                model: model_id.to_string(),
                message,
            },
            RouterError::ProviderUnavailable {
                status, message, ..
            } => RouterError::ProviderUnavailable {
                model: model_id.to_string(),
                status,
                message,
            },
            other => other,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct FakeTransport {
        responses: HashMap<String, Result<String, RouterError>>,
        seen: RefCell<Vec<String>>,
    }

    impl FakeTransport {
        fn new(responses: Vec<(&str, Result<String, RouterError>)>) -> Self {
            Self {
                responses: responses
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl ChatTransport for FakeTransport {
        fn complete(
            &self,
            _api_key: &str,
            model_id: &str,
            _messages: &[ChatMessage],
        ) -> Result<String, RouterError> {
            self.seen.borrow_mut().push(model_id.to_string());
            self.responses.get(model_id).cloned().unwrap_or_else(|| {
                Err(RouterError::ProviderUnavailable {
                    model: model_id.to_string(),
                    status: 503,
                    message: "not configured".into(),
                })
            })
        }
    }

    #[test]
    fn free_only_default_blocks_paid_models_until_opted_in() {
        let json = r#"{
          "data": [
            {"id": "openai/gpt-5", "name": "GPT-5", "context_length": 200000, "pricing": {"prompt": "0.00001", "completion": "0.00003"}},
            {"id": "meta-llama/llama-3.3-70b-instruct:free", "name": "Llama 3.3 Free", "context_length": 131072, "pricing": {"prompt": "0", "completion": "0"}}
          ]
        }"#;
        let models = parse_openrouter_catalogue(json).unwrap();
        let mut router = ModelRouter::default();
        router.update_catalogue(models);

        let eligible: Vec<&str> = router
            .eligible_models(1000)
            .into_iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(eligible, vec!["meta-llama/llama-3.3-70b-instruct:free"]);

        router.set_policy(true, Some("openai/gpt-5".into()));
        let after_opt_in: Vec<&str> = router
            .eligible_models(1000)
            .into_iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(
            after_opt_in,
            vec!["openai/gpt-5", "meta-llama/llama-3.3-70b-instruct:free"]
        );
    }

    #[test]
    fn rate_limit_puts_model_on_cooldown_and_falls_back_cleanly() {
        let mut router = ModelRouter::default();
        router.update_catalogue(vec![
            ModelEntry {
                id: "free/model-a:free".into(),
                name: "A".into(),
                is_free: true,
                context_length: 8192,
            },
            ModelEntry {
                id: "free/model-b:free".into(),
                name: "B".into(),
                is_free: true,
                context_length: 8192,
            },
        ]);
        router.set_policy(false, Some("free/model-a:free".into()));

        let transport = FakeTransport::new(vec![
            (
                "free/model-a:free",
                Err(RouterError::RateLimited {
                    model: "free/model-a:free".into(),
                    message: "429".into(),
                }),
            ),
            ("free/model-b:free", Ok("Hello from B".into())),
        ]);

        let msgs = vec![ChatMessage {
            role: "user".into(),
            content: "Hi".into(),
        }];
        let res = router
            .complete_chat("sk-or-test", &msgs, 1_000, &transport)
            .unwrap();
        assert_eq!(res.model, "free/model-b:free");
        assert_eq!(res.content, "Hello from B");
        assert_eq!(res.attempts, 2);
        assert!(router.is_on_cooldown("free/model-a:free", 1_050));

        // Second request skips cooled-down A immediately until cooldown expires.
        let res2 = router
            .complete_chat("sk-or-test", &msgs, 1_050, &transport)
            .unwrap();
        assert_eq!(res2.model, "free/model-b:free");
        assert_eq!(res2.attempts, 1);
        assert!(!router.is_on_cooldown("free/model-a:free", 1_000 + DEFAULT_COOLDOWN_SECS));
    }

    #[test]
    fn missing_key_is_rejected_without_fake_reply() {
        let mut router = ModelRouter::default();
        let transport = FakeTransport::new(vec![]);
        let msgs = vec![ChatMessage {
            role: "user".into(),
            content: "Hi".into(),
        }];
        let err = router
            .complete_chat("   ", &msgs, 100, &transport)
            .unwrap_err();
        assert_eq!(err, RouterError::MissingApiKey);
        assert!(transport.seen.borrow().is_empty());
    }
}
