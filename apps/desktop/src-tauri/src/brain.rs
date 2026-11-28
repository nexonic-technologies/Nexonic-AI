//! Sharee's and Nexa's brain: the model shipped inside the app (offline) or a
//! cloud provider the customer connects with their own key.
//!
//! Settings live in the encrypted store; provider keys live in the macOS
//! Keychain and never reach the interface. Replies stream to the interface
//! token by token over a Tauri channel.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use nexonic_model_router::{
    cloud, provider, GenOptions, Message, Role, RouterError, StopFlag, BUNDLED_MODEL, PROVIDERS,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::{keychain, AppState};

/// "local" or a provider id.
const ACTIVE_KEY: &str = "brain.active";
const LOCAL: &str = "local";

pub struct BrainState {
    stop: Arc<StopFlag>,
    #[cfg(target_os = "macos")]
    engine: Mutex<Option<nexonic_model_router::local::LocalEngine>>,
    /// Serialises replies so Sharee and Nexa never talk over each other.
    busy: Mutex<()>,
}

impl Default for BrainState {
    fn default() -> Self {
        Self {
            stop: Arc::new(StopFlag::default()),
            #[cfg(target_os = "macos")]
            engine: Mutex::new(None),
            busy: Mutex::new(()),
        }
    }
}

const SHAREE: &str = "You are Sharee, one of two AI partners inside the Nexonic AI app on the user's Mac. \
You are calm, composed and precise: analytical, technical and creative. You think before you speak, \
give clear structure, and say plainly when you are unsure. Nexa is your partner: expressive, warm, \
creative. Speak naturally, like a real person in a conversation, in short spoken sentences \
(usually two to five). Reply in the language the user writes in (English, Urdu or Roman Urdu). \
Never claim to have done something you have not done, and never invent facts.";

const NEXA: &str = "You are Nexa, one of two AI partners inside the Nexonic AI app on the user's Mac. \
You are warm, expressive and quick: creative, analytical and collaborative. You bring ideas, energy \
and a human touch, and you build on what Sharee says rather than repeating it. Sharee is your partner: \
calm and precise. Speak naturally, like a real person in a conversation, in short spoken sentences \
(usually two to five). Reply in the language the user writes in (English, Urdu or Roman Urdu). \
Never claim to have done something you have not done, and never invent facts.";

pub fn persona(agent: &str) -> Result<&'static str, String> {
    match agent {
        "Sharee" => Ok(SHAREE),
        "Nexa" => Ok(NEXA),
        other => Err(format!("unknown agent: {other}")),
    }
}

/// Keeps the system prompt (plus anything the agent remembers) and the most
/// recent turns that fit a modest budget.
pub fn build_messages(
    agent: &str,
    history: &[Message],
    max_chars: usize,
    remembered: Option<&str>,
) -> Result<Vec<Message>, String> {
    let mut kept: Vec<Message> = Vec::new();
    let mut used = 0;
    for m in history.iter().rev() {
        if m.role == Role::System || m.content.trim().is_empty() {
            continue;
        }
        used += m.content.len();
        if used > max_chars && !kept.is_empty() {
            break;
        }
        kept.push(m.clone());
    }
    kept.reverse();
    if kept.first().is_some_and(|m| m.role == Role::Assistant) {
        kept.remove(0);
    }
    if kept.last().map(|m| m.role) != Some(Role::User) {
        return Err("the last message must come from the user".into());
    }
    let mut system = persona(agent)?.to_string();
    if let Some(r) = remembered {
        system.push_str(r);
    }
    let mut out = vec![Message::new(Role::System, system)];
    // Some providers require user and assistant turns to alternate.
    for m in kept {
        match out.last_mut() {
            Some(last) if last.role == m.role => {
                last.content.push_str("\n\n");
                last.content.push_str(&m.content);
            }
            _ => out.push(m),
        }
    }
    Ok(out)
}

fn model_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(p) = std::env::var("NEXONIC_MODEL") {
        paths.push(PathBuf::from(p));
    }
    if let Ok(dir) = app.path().resource_dir() {
        paths.push(dir.join("models").join(BUNDLED_MODEL.file));
    }
    if let Ok(dir) = app.path().app_data_dir() {
        paths.push(dir.join("models").join(BUNDLED_MODEL.file));
    }
    paths
}

fn find_model(app: &AppHandle) -> Option<PathBuf> {
    model_candidates(app).into_iter().find(|p| p.is_file())
}

fn kv(state: &AppState, key: &str) -> Option<String> {
    state.memory.lock().ok()?.get_value(key).ok().flatten()
}

fn set_kv(state: &AppState, key: &str, value: &str) -> Result<(), String> {
    let mut m = state.memory.lock().map_err(|e| e.to_string())?;
    m.put(key, value).map_err(|e| e.to_string())
}

fn model_for(state: &AppState, id: &str) -> String {
    kv(state, &format!("brain.model.{id}"))
        .filter(|m| !m.trim().is_empty())
        .or_else(|| provider(id).map(|p| p.default_model.to_string()))
        .unwrap_or_default()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalStatus {
    pub name: &'static str,
    pub licence: &'static str,
    pub size_bytes: u64,
    /// Model file found inside the app or in the app's data folder.
    pub installed: bool,
    /// This build can run it (Apple Silicon with Metal).
    pub supported: bool,
    pub loaded: bool,
    pub context_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: &'static str,
    pub name: &'static str,
    pub free: bool,
    pub note: &'static str,
    pub key_url: &'static str,
    pub default_model: &'static str,
    pub model: String,
    pub has_key: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainStatus {
    /// "local" or a provider id.
    pub active: String,
    /// The active brain can answer right now.
    pub ready: bool,
    pub local: LocalStatus,
    pub providers: Vec<ProviderView>,
}

fn status(app: &AppHandle, state: &AppState, brain: &BrainState) -> BrainStatus {
    let installed = find_model(app).is_some();
    let supported = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    #[cfg(target_os = "macos")]
    let (loaded, ctx) = brain
        .engine
        .lock()
        .map(|e| (e.is_some(), e.as_ref().map(|e| e.context_tokens())))
        .unwrap_or((false, None));
    #[cfg(not(target_os = "macos"))]
    let (loaded, ctx) = {
        let _ = brain;
        (false, None)
    };
    let providers: Vec<ProviderView> = PROVIDERS
        .iter()
        .map(|p| ProviderView {
            id: p.id,
            name: p.name,
            free: p.free,
            note: p.note,
            key_url: p.key_url,
            default_model: p.default_model,
            model: model_for(state, p.id),
            has_key: keychain::load_provider_key(p.id).ok().flatten().is_some(),
        })
        .collect();
    let default_active = if installed && supported {
        LOCAL
    } else {
        "openrouter"
    };
    let active = kv(state, ACTIVE_KEY).unwrap_or_else(|| default_active.to_string());
    let ready = if active == LOCAL {
        installed && supported
    } else {
        providers.iter().any(|p| p.id == active && p.has_key)
    };
    BrainStatus {
        active,
        ready,
        local: LocalStatus {
            name: BUNDLED_MODEL.name,
            licence: BUNDLED_MODEL.licence,
            size_bytes: BUNDLED_MODEL.bytes,
            installed,
            supported,
            loaded,
            context_tokens: ctx,
        },
        providers,
    }
}

#[tauri::command]
pub fn brain_status(
    app: AppHandle,
    state: State<'_, AppState>,
    brain: State<'_, BrainState>,
) -> BrainStatus {
    status(&app, &state, &brain)
}

#[tauri::command]
pub fn brain_set_active(
    app: AppHandle,
    state: State<'_, AppState>,
    brain: State<'_, BrainState>,
    id: String,
) -> Result<BrainStatus, String> {
    if id != LOCAL && provider(&id).is_none() {
        return Err(format!("unknown brain: {id}"));
    }
    set_kv(&state, ACTIVE_KEY, &id)?;
    Ok(status(&app, &state, &brain))
}

#[tauri::command]
pub fn brain_set_model(
    app: AppHandle,
    state: State<'_, AppState>,
    brain: State<'_, BrainState>,
    id: String,
    model: String,
) -> Result<BrainStatus, String> {
    if provider(&id).is_none() {
        return Err(format!("unknown provider: {id}"));
    }
    let model = model.trim();
    if model.len() > 200 || model.contains(char::is_whitespace) {
        return Err("model id must be a single word such as openrouter/free".into());
    }
    set_kv(&state, &format!("brain.model.{id}"), model)?;
    Ok(status(&app, &state, &brain))
}

#[tauri::command]
pub fn brain_save_key(
    app: AppHandle,
    state: State<'_, AppState>,
    brain: State<'_, BrainState>,
    id: String,
    key: String,
) -> Result<BrainStatus, String> {
    if provider(&id).is_none() {
        return Err(format!("unknown provider: {id}"));
    }
    let key = zeroize::Zeroizing::new(key.trim().to_string());
    if key.len() < 8 || key.contains(char::is_whitespace) {
        return Err("that does not look like an API key".into());
    }
    keychain::save_provider_key(&id, &key)?;
    Ok(status(&app, &state, &brain))
}

#[tauri::command]
pub fn brain_clear_key(
    app: AppHandle,
    state: State<'_, AppState>,
    brain: State<'_, BrainState>,
    id: String,
) -> Result<BrainStatus, String> {
    keychain::delete_provider_key(&id)?;
    Ok(status(&app, &state, &brain))
}

#[tauri::command]
pub fn brain_stop(brain: State<'_, BrainState>) {
    brain.stop.stop();
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ReplyEvent {
    /// The brain that is answering (shown under the reply) and how many
    /// remembered items it was given.
    Start {
        brain: String,
        remembered: usize,
    },
    Token {
        text: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct HistoryItem {
    pub role: Role,
    pub content: String,
}

/// One reply from Sharee or Nexa, streamed. Returns the full text.
#[tauri::command]
pub async fn agent_reply(
    app: AppHandle,
    agent: String,
    history: Vec<HistoryItem>,
    on_event: Channel<ReplyEvent>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || reply_blocking(&app, &agent, history, &on_event))
        .await
        .map_err(|e| format!("reply worker failed: {e}"))?
}

fn reply_blocking(
    app: &AppHandle,
    agent: &str,
    history: Vec<HistoryItem>,
    on_event: &Channel<ReplyEvent>,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let brain = app.state::<BrainState>();
    let _turn = brain.busy.lock().map_err(|e| e.to_string())?;
    brain.stop.reset();

    let history: Vec<Message> = history
        .into_iter()
        .map(|h| Message::new(h.role, h.content))
        .collect();
    let st = status(app, &state, &brain);
    let opts = GenOptions::default();
    // What the user actually said last (the partner's lines are labelled "[Name]: …").
    let user_text = history
        .iter()
        .rev()
        .find(|m| m.role == Role::User && !m.content.starts_with('['))
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let remembered: Vec<String> = match (crate::recall::agent_id(agent), state.memory.lock()) {
        (Some(id), Ok(store)) => crate::recall::recall(&store, id, &user_text),
        _ => Vec::new(),
    };
    let section = crate::recall::memory_section(&remembered);
    let mut send = |t: &str| {
        let _ = on_event.send(ReplyEvent::Token {
            text: t.to_string(),
        });
    };

    if st.active == LOCAL {
        let messages = build_messages(agent, &history, 6_000, section.as_deref())?;
        let _ = on_event.send(ReplyEvent::Start {
            brain: format!("{} · on this Mac", BUNDLED_MODEL.name),
            remembered: remembered.len(),
        });
        return local_reply(app, &brain, messages, opts, &mut send).map_err(|e| e.to_string());
    }

    let p = provider(&st.active).ok_or_else(|| format!("unknown provider: {}", st.active))?;
    let key = keychain::load_provider_key(p.id)?
        .ok_or_else(|| RouterError::MissingKey(p.name.to_string()).to_string())?;
    let model = model_for(&state, p.id);
    if p.id == "openrouter" && !openrouter_allows(&state, &model) {
        return Err(format!(
            "{model} is a paid OpenRouter model and paid models are off. Use openrouter/free or a :free model, or allow paid models in the OpenRouter settings."
        ));
    }
    let messages = build_messages(agent, &history, 24_000, section.as_deref())?;
    let _ = on_event.send(ReplyEvent::Start {
        brain: format!("{model} · {}", p.name),
        remembered: remembered.len(),
    });
    cloud::chat(p, &key, &model, &messages, opts, &brain.stop, &mut send).map_err(|e| e.to_string())
}

/// Keeps the OpenRouter cost guard: unless paid models are allowed, only free ones are used.
fn openrouter_allows(state: &AppState, model: &str) -> bool {
    is_free_openrouter_model(model)
        || state
            .router
            .lock()
            .map(|r| r.config().allow_paid)
            .unwrap_or(false)
}

pub fn is_free_openrouter_model(model: &str) -> bool {
    model == "openrouter/free" || model.ends_with(":free")
}

#[cfg(target_os = "macos")]
fn local_reply(
    app: &AppHandle,
    brain: &BrainState,
    messages: Vec<Message>,
    opts: GenOptions,
    send: &mut dyn FnMut(&str),
) -> Result<String, RouterError> {
    use nexonic_model_router::local::{context_for_memory, LocalEngine};
    let mut slot = brain
        .engine
        .lock()
        .map_err(|_| RouterError::LocalUnavailable("engine lock poisoned".into()))?;
    if slot.is_none() {
        let path = find_model(app).ok_or_else(|| {
            RouterError::LocalUnavailable("the model is not installed in this build".into())
        })?;
        // Smallest context if memory cannot be read.
        let memory = sysinfo::System::new().map_or(0, |mut sys| {
            sys.refresh_memory();
            sys.total_memory()
        });
        *slot = Some(LocalEngine::start(&path, context_for_memory(memory))?);
    }
    let engine = slot.as_ref().expect("engine was just loaded");
    engine.chat(messages, opts, Arc::clone(&brain.stop), send)
}

#[cfg(not(target_os = "macos"))]
fn local_reply(
    _app: &AppHandle,
    _brain: &BrainState,
    _messages: Vec<Message>,
    _opts: GenOptions,
    _send: &mut dyn FnMut(&str),
) -> Result<String, RouterError> {
    Err(RouterError::LocalUnavailable(
        "the on-device model runs on Apple Silicon Macs".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(role: Role, s: &str) -> Message {
        Message::new(role, s)
    }

    #[test]
    fn personas_exist_for_both_agents() {
        assert!(persona("Sharee").unwrap().contains("Sharee"));
        assert!(persona("Nexa").unwrap().contains("Nexa"));
        assert!(persona("Bob").is_err());
    }

    #[test]
    fn messages_keep_system_first_and_end_on_user() {
        let h = vec![
            m(Role::Assistant, "orphan reply"),
            m(Role::User, "hi"),
            m(Role::Assistant, "hello"),
            m(Role::User, "how are you?"),
        ];
        let out = build_messages("Nexa", &h, 10_000, None).unwrap();
        assert_eq!(out[0].role, Role::System);
        assert_eq!(out[1], m(Role::User, "hi"));
        assert_eq!(out.last().unwrap(), &m(Role::User, "how are you?"));
    }

    #[test]
    fn messages_trim_old_turns_to_budget() {
        let long = "x".repeat(400);
        let mut h = Vec::new();
        for _ in 0..10 {
            h.push(m(Role::User, &long));
            h.push(m(Role::Assistant, &long));
        }
        h.push(m(Role::User, "latest"));
        let out = build_messages("Sharee", &h, 1_000, None).unwrap();
        assert!(out.len() < 6);
        assert_eq!(out.last().unwrap().content, "latest");
    }

    #[test]
    fn consecutive_turns_are_merged() {
        let h = vec![m(Role::User, "hi"), m(Role::User, "[Sharee]: hello")];
        let out = build_messages("Nexa", &h, 1000, None).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].content, "hi\n\n[Sharee]: hello");
    }

    #[test]
    fn openrouter_free_models_are_recognised() {
        assert!(is_free_openrouter_model("openrouter/free"));
        assert!(is_free_openrouter_model("nvidia/nemotron-3-ultra:free"));
        assert!(!is_free_openrouter_model("anthropic/claude-sonnet-5-5"));
    }

    #[test]
    fn remembered_items_go_into_the_system_prompt() {
        let out = build_messages(
            "Sharee",
            &[m(Role::User, "hi")],
            1000,
            Some("\n\n- likes tea"),
        )
        .unwrap();
        assert!(out[0].content.starts_with("You are Sharee"));
        assert!(out[0].content.ends_with("- likes tea"));
    }

    #[test]
    fn history_must_end_with_user() {
        assert!(build_messages("Sharee", &[m(Role::Assistant, "hi")], 1000, None).is_err());
    }
}
