//! Nexonic AI desktop core: typed commands the interface calls over Tauri IPC.
//!
//! Everything returned here is real state: detected hardware, the live
//! permission set and the encrypted SQLite memory store. Nothing is hardcoded or
//! simulated.

mod keychain;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;

use nexonic_memory_engine::persist::{generate_key, PersistentMemory};
use nexonic_memory_engine::{AgentId, Domain, Kind, Memory, MemoryId, NewMemory, Reader};
use nexonic_model_router::{
    fetch_openrouter_catalogue, ChatMessage, CompletionResult, ModelEntry, ModelRouter,
    OpenRouterHttpTransport, RouterConfig,
};
use nexonic_permissions::{Actor, Capability, Decision, Permissions, Scope};
use serde::Serialize;
use tauri::{Manager, State};

/// Shared application state behind the IPC boundary.
pub struct AppState {
    permissions: Mutex<Permissions>,
    memory: Mutex<PersistentMemory>,
    router: Mutex<ModelRouter>,
    persistent: bool,
    /// Why the encrypted on-disk store could not be opened, if it failed.
    /// The app then runs on a temporary in-memory store and says so.
    storage_error: Option<String>,
}

fn router_from_memory(mem: &PersistentMemory) -> ModelRouter {
    let allow_paid = mem
        .get_value("router.allow_paid")
        .ok()
        .flatten()
        .is_some_and(|v| v == "true");
    let preferred_model = mem
        .get_value("router.preferred_model")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty());
    ModelRouter::new(RouterConfig {
        allow_paid,
        preferred_model,
        ..RouterConfig::default()
    })
}

impl AppState {
    pub fn open_persistent(path: &Path, key: &[u8; 32]) -> Result<Self, String> {
        let mem = PersistentMemory::open(path, key).map_err(|e| e.to_string())?;
        let router = router_from_memory(&mem);
        Ok(Self {
            permissions: Mutex::new(Permissions::new()),
            memory: Mutex::new(mem),
            router: Mutex::new(router),
            persistent: true,
            storage_error: None,
        })
    }

    pub fn in_memory_for_tests() -> Result<Self, String> {
        let key = generate_key();
        let mem = PersistentMemory::open_in_memory(&key).map_err(|e| e.to_string())?;
        Ok(Self {
            permissions: Mutex::new(Permissions::new()),
            memory: Mutex::new(mem),
            router: Mutex::new(ModelRouter::default()),
            persistent: false,
            storage_error: None,
        })
    }

    /// Opens the encrypted store; if the Keychain or the database refuses
    /// (denied Keychain prompt, missing key for an existing database, …) the
    /// app still starts on a temporary in-memory store instead of crashing.
    /// The existing database file is never modified or replaced.
    pub fn open_or_fallback(path: &Path) -> Self {
        let opened =
            keychain::load_or_create_key().and_then(|key| Self::open_persistent(path, &key));
        match opened {
            Ok(st) => st,
            Err(err) => {
                eprintln!("[nexonic] encrypted memory unavailable, using a temporary store: {err}");
                let key = generate_key();
                let mem = PersistentMemory::open_in_memory(&key)
                    .expect("in-memory store cannot fail to open");
                Self {
                    permissions: Mutex::new(Permissions::new()),
                    memory: Mutex::new(mem),
                    router: Mutex::new(ModelRouter::default()),
                    persistent: false,
                    storage_error: Some(err),
                }
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    /// e.g. "Apple M5" as reported by the CPU brand string.
    pub chip: String,
    pub cpu_cores: usize,
    pub memory_bytes: u64,
    pub os_name: String,
    pub os_version: String,
    pub arch: String,
    pub app_version: String,
}

pub fn detect_system() -> SystemInfo {
    let (chip, cpu_cores, memory_bytes) = match sysinfo::System::new() {
        Ok(mut sys) => {
            sys.refresh_cpu_all();
            sys.refresh_memory();
            let chip = sys
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_string())
                .filter(|b| !b.is_empty())
                .unwrap_or_else(|| "Unknown processor".into());
            (chip, sys.cpus().len(), sys.total_memory())
        }
        Err(_) => ("Unknown processor".into(), 0, 0),
    };
    SystemInfo {
        chip,
        cpu_cores,
        memory_bytes,
        os_name: sysinfo::System::name().unwrap_or_else(|_| "Unknown OS".into()),
        os_version: sysinfo::System::os_version().unwrap_or_default(),
        arch: std::env::consts::ARCH.into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionView {
    pub capability: String,
    pub granted: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsView {
    pub stopped: bool,
    pub items: Vec<PermissionView>,
}

fn cap_name(c: Capability) -> String {
    format!("{c:?}")
}

fn parse_cap(name: &str) -> Result<Capability, String> {
    Capability::ALL
        .into_iter()
        .find(|c| cap_name(*c) == name)
        .ok_or_else(|| format!("unknown capability: {name}"))
}

fn view(p: &mut Permissions) -> PermissionsView {
    let items = Capability::ALL
        .into_iter()
        .map(|c| PermissionView {
            capability: cap_name(c),
            granted: p.check(&Actor::System, c, None) == Decision::Allowed,
        })
        .collect();
    PermissionsView {
        stopped: p.is_stopped(),
        items,
    }
}

#[tauri::command]
fn system_info() -> SystemInfo {
    detect_system()
}

#[tauri::command]
fn permissions(state: State<'_, AppState>) -> Result<PermissionsView, String> {
    let mut p = state.permissions.lock().map_err(|e| e.to_string())?;
    Ok(view(&mut p))
}

/// Called only from the Privacy & Permissions UI, i.e. by the user.
#[tauri::command]
fn set_permission(
    state: State<'_, AppState>,
    capability: String,
    granted: bool,
) -> Result<PermissionsView, String> {
    let cap = parse_cap(&capability)?;
    let mut p = state.permissions.lock().map_err(|e| e.to_string())?;
    if granted {
        p.grant(&Actor::User, cap, Scope::Any)
            .map_err(|e| e.to_string())?;
    } else {
        p.revoke(&Actor::User, cap).map_err(|e| e.to_string())?;
    }
    Ok(view(&mut p))
}

#[tauri::command]
fn emergency_stop(state: State<'_, AppState>) -> Result<PermissionsView, String> {
    let mut p = state.permissions.lock().map_err(|e| e.to_string())?;
    p.emergency_stop();
    Ok(view(&mut p))
}

#[tauri::command]
fn resume(state: State<'_, AppState>) -> Result<PermissionsView, String> {
    let mut p = state.permissions.lock().map_err(|e| e.to_string())?;
    p.resume(&Actor::User).map_err(|e| e.to_string())?;
    Ok(view(&mut p))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
    pub user_visible: usize,
    pub encrypted: bool,
    pub persistent: bool,
    pub storage_error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryView {
    pub id: u64,
    pub domain: String,
    pub kind: String,
    pub text: String,
    pub source: String,
    pub confidence: f32,
    pub never_share_with: Vec<String>,
    pub created_at: u64,
}

fn agent_label(a: AgentId) -> &'static str {
    match a {
        AgentId::Sharee => "sharee",
        AgentId::Nexa => "nexa",
    }
}

fn domain_label(d: Domain) -> &'static str {
    match d {
        Domain::Private(AgentId::Sharee) => "sharee",
        Domain::Private(AgentId::Nexa) => "nexa",
        Domain::Collective => "collective",
    }
}

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Episodic => "episodic",
        Kind::Semantic => "semantic",
        Kind::Procedural => "procedural",
        Kind::Preference => "preference",
        Kind::Prospective => "prospective",
        Kind::Reflective => "reflective",
    }
}

fn memory_to_view(m: &Memory) -> MemoryView {
    MemoryView {
        id: m.id.0,
        domain: domain_label(m.domain).into(),
        kind: kind_label(m.kind).into(),
        text: m.text.clone(),
        source: m.source.clone(),
        confidence: m.confidence,
        never_share_with: m
            .never_share_with
            .iter()
            .map(|a| agent_label(*a).into())
            .collect(),
        created_at: m.created_at,
    }
}

fn parse_domain_arg(raw: Option<&str>) -> Result<Domain, String> {
    match raw.unwrap_or("collective") {
        "collective" => Ok(Domain::Collective),
        "sharee" => Ok(Domain::Private(AgentId::Sharee)),
        "nexa" => Ok(Domain::Private(AgentId::Nexa)),
        other => Err(format!("unknown memory domain: {other}")),
    }
}

fn parse_kind_arg(raw: Option<&str>) -> Result<Kind, String> {
    match raw.unwrap_or("semantic") {
        "episodic" => Ok(Kind::Episodic),
        "semantic" => Ok(Kind::Semantic),
        "procedural" => Ok(Kind::Procedural),
        "preference" => Ok(Kind::Preference),
        "prospective" => Ok(Kind::Prospective),
        "reflective" => Ok(Kind::Reflective),
        other => Err(format!("unknown memory kind: {other}")),
    }
}

fn parse_agent_arg(raw: &str) -> Result<AgentId, String> {
    match raw {
        "sharee" => Ok(AgentId::Sharee),
        "nexa" => Ok(AgentId::Nexa),
        other => Err(format!("unknown agent: {other}")),
    }
}

#[tauri::command]
fn memory_summary(state: State<'_, AppState>) -> Result<MemorySummary, String> {
    let m = state.memory.lock().map_err(|e| e.to_string())?;
    Ok(MemorySummary {
        user_visible: m.context_for(Reader::User).len(),
        encrypted: true,
        persistent: state.persistent,
        storage_error: state.storage_error.clone(),
    })
}

#[tauri::command]
fn memories_recent(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<MemoryView>, String> {
    let m = state.memory.lock().map_err(|e| e.to_string())?;
    let cap = limit.unwrap_or(20).clamp(1, 200);
    Ok(m.recent(Reader::User, cap)
        .into_iter()
        .map(memory_to_view)
        .collect())
}

#[tauri::command]
fn remember_note(
    state: State<'_, AppState>,
    text: String,
    domain: Option<String>,
    kind: Option<String>,
    never_share_with: Option<Vec<String>>,
) -> Result<MemoryView, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("memory text must not be empty".into());
    }
    let domain = parse_domain_arg(domain.as_deref())?;
    let kind = parse_kind_arg(kind.as_deref())?;
    let never: BTreeSet<AgentId> = never_share_with
        .unwrap_or_default()
        .iter()
        .map(|s| parse_agent_arg(s))
        .collect::<Result<_, _>>()?;

    let mut m = state.memory.lock().map_err(|e| e.to_string())?;
    let id = m
        .remember(NewMemory {
            domain,
            kind,
            text: trimmed.to_string(),
            source: "user".into(),
            confidence: 1.0,
            never_share_with: never,
        })
        .map_err(|e| e.to_string())?;
    let stored = m.get(Reader::User, id).map_err(|e| e.to_string())?;
    Ok(memory_to_view(stored))
}

#[tauri::command]
fn forget_memory(state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let mut m = state.memory.lock().map_err(|e| e.to_string())?;
    m.forget(Reader::User, MemoryId(id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn kv_get(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    if key.trim().is_empty() {
        return Err("kv key must not be empty".into());
    }
    let m = state.memory.lock().map_err(|e| e.to_string())?;
    m.get_value(&key).map_err(|e| e.to_string())
}

#[tauri::command]
fn kv_set(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("kv key must not be empty".into());
    }
    let mut m = state.memory.lock().map_err(|e| e.to_string())?;
    m.put(&key, &value).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: String,
    pub website_url: String,
    pub message: String,
}

/// True only when `latest` is a strictly higher version than `current`
/// (numeric major.minor.patch; pre-release suffixes are ignored).
fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

fn parse_release_json(body: &str, current: &str) -> UpdateStatus {
    let release_default = "https://github.com/nexonic-technologies/Nexonic-AI/releases".to_string();
    let website = "https://nexonictechnologies.com/".to_string();
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    if let Ok(v) = parsed {
        if let Some(tag) = v.get("tag_name").and_then(|t| t.as_str()) {
            let latest = tag.trim_start_matches('v').to_string();
            let url = v
                .get("html_url")
                .and_then(|u| u.as_str())
                .unwrap_or(&release_default)
                .to_string();
            let newer = is_newer(&latest, current);
            let message = if newer {
                format!("Update available: v{latest} (current v{current})")
            } else {
                format!("You are up to date (v{current})")
            };
            return UpdateStatus {
                current_version: current.to_string(),
                latest_version: Some(latest),
                update_available: newer,
                release_url: url,
                website_url: website,
                message,
            };
        }
    }
    UpdateStatus {
        current_version: current.to_string(),
        latest_version: None,
        update_available: false,
        release_url: release_default,
        website_url: website,
        message: format!("Running v{current} (pre-release development build)"),
    }
}

/// Runs on a worker thread so the window never freezes while GitHub answers.
#[tauri::command]
async fn check_updates() -> UpdateStatus {
    tauri::async_runtime::spawn_blocking(fetch_update_status)
        .await
        .unwrap_or_else(|_| parse_release_json("{}", env!("CARGO_PKG_VERSION")))
}

fn fetch_update_status() -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION");
    let out = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "5",
            "-H",
            "Accept: application/vnd.github+json",
            "https://api.github.com/repos/nexonic-technologies/Nexonic-AI/releases/latest",
        ])
        .output();
    match out {
        Ok(res) if res.status.success() => {
            let body = String::from_utf8_lossy(&res.stdout);
            parse_release_json(&body, current)
        }
        _ => parse_release_json("{}", current),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub provider: String,
    pub has_key: bool,
    pub key_hint: Option<String>,
    pub allow_paid: bool,
    pub preferred_model: Option<String>,
    pub free_count: usize,
    pub total_count: usize,
    pub cooldown_count: usize,
    pub models: Vec<ModelEntry>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn build_provider_status(state: &AppState) -> Result<ProviderStatus, String> {
    let key = keychain::load_provider_key("openrouter")?;
    let now = now_secs();
    let mut r = state.router.lock().map_err(|e| e.to_string())?;
    r.clear_expired_cooldowns(now);
    let total_count = r.catalogue().len();
    let free_count = r.catalogue().iter().filter(|m| m.is_free).count();
    let models: Vec<ModelEntry> = r
        .visible_catalogue()
        .into_iter()
        .take(40)
        .cloned()
        .collect();
    Ok(ProviderStatus {
        provider: "openrouter".into(),
        has_key: key.is_some(),
        key_hint: key.as_deref().map(|s| keychain::mask_key(s)),
        allow_paid: r.config().allow_paid,
        preferred_model: r.config().preferred_model.clone(),
        free_count,
        total_count,
        cooldown_count: r.cooldown_count(now),
        models,
    })
}

#[tauri::command]
fn provider_status(state: State<'_, AppState>) -> Result<ProviderStatus, String> {
    build_provider_status(&state)
}

#[tauri::command]
fn save_provider_key(
    state: State<'_, AppState>,
    api_key: String,
) -> Result<ProviderStatus, String> {
    keychain::save_provider_key("openrouter", &api_key)?;
    build_provider_status(&state)
}

#[tauri::command]
fn clear_provider_key(state: State<'_, AppState>) -> Result<ProviderStatus, String> {
    keychain::delete_provider_key("openrouter")?;
    build_provider_status(&state)
}

#[tauri::command]
fn set_router_policy(
    state: State<'_, AppState>,
    allow_paid: bool,
    preferred_model: Option<String>,
) -> Result<ProviderStatus, String> {
    {
        let mut r = state.router.lock().map_err(|e| e.to_string())?;
        r.set_policy(allow_paid, preferred_model.clone());
    }
    {
        let mut m = state.memory.lock().map_err(|e| e.to_string())?;
        m.put(
            "router.allow_paid",
            if allow_paid { "true" } else { "false" },
        )
        .map_err(|e| e.to_string())?;
        m.put(
            "router.preferred_model",
            preferred_model.as_deref().unwrap_or("").trim(),
        )
        .map_err(|e| e.to_string())?;
    }
    build_provider_status(&state)
}

#[tauri::command]
async fn refresh_models(state: State<'_, AppState>) -> Result<ProviderStatus, String> {
    let key = keychain::load_provider_key("openrouter")?;
    let models = tauri::async_runtime::spawn_blocking(move || {
        fetch_openrouter_catalogue(key.as_deref().map(|s| s.as_str()))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    {
        let mut r = state.router.lock().map_err(|e| e.to_string())?;
        r.update_catalogue(models);
    }
    build_provider_status(&state)
}

#[tauri::command]
async fn test_completion(
    state: State<'_, AppState>,
    prompt: String,
) -> Result<CompletionResult, String> {
    let Some(api_key) = keychain::load_provider_key("openrouter")? else {
        return Err("Save an OpenRouter API key in Intelligence → Providers first.".into());
    };
    let clean_prompt = prompt.trim().to_string();
    if clean_prompt.is_empty() {
        return Err("Prompt must not be empty.".into());
    }
    let now = now_secs();
    let mut router_snapshot = state.router.lock().map_err(|e| e.to_string())?.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let msgs = vec![ChatMessage {
            role: "user".into(),
            content: clean_prompt,
        }];
        let out = router_snapshot.complete_chat(&api_key, &msgs, now, &OpenRouterHttpTransport);
        (router_snapshot, out)
    })
    .await
    .map_err(|e| e.to_string())?;
    *state.router.lock().map_err(|e| e.to_string())? = res.0;
    res.1.map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("failed to locate app data directory: {e}"))?;
            std::fs::create_dir_all(&dir)?;
            app.manage(AppState::open_or_fallback(&dir.join("memory.sqlite")));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            system_info,
            permissions,
            set_permission,
            emergency_stop,
            resume,
            memory_summary,
            memories_recent,
            remember_note,
            forget_memory,
            kv_get,
            kv_set,
            check_updates,
            provider_status,
            save_provider_key,
            clear_provider_key,
            set_router_policy,
            refresh_models,
            test_completion
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nexonic AI");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_info_is_detected_not_hardcoded() {
        let s = detect_system();
        assert!(s.cpu_cores > 0);
        assert!(s.memory_bytes > 0);
        assert_eq!(s.arch, std::env::consts::ARCH);
    }

    #[test]
    fn capability_names_round_trip() {
        for c in Capability::ALL {
            assert_eq!(parse_cap(&cap_name(c)).unwrap(), c);
        }
        assert!(parse_cap("Nope").is_err());
    }

    #[test]
    fn permissions_view_starts_all_off() {
        let mut p = Permissions::new();
        let v = view(&mut p);
        assert!(!v.stopped);
        assert!(v.items.iter().all(|i| !i.granted));
        assert_eq!(v.items.len(), Capability::ALL.len());
    }

    #[test]
    fn persistent_state_survives_reopen_with_isolation() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "nexonic-app-mem-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let key = generate_key();
        let id = {
            let st = AppState::open_persistent(&path, &key).unwrap();
            assert!(st.persistent);
            let mut m = st.memory.lock().unwrap();
            m.put("focus", r#"[{"id":"1","text":"Review M20","done":false}]"#)
                .unwrap();
            m.remember(NewMemory {
                domain: Domain::Private(AgentId::Nexa),
                kind: Kind::Preference,
                text: "Prefer dark workspace".into(),
                source: "user".into(),
                confidence: 1.0,
                never_share_with: [AgentId::Sharee].into_iter().collect(),
            })
            .unwrap()
        };

        let reopened = AppState::open_persistent(&path, &key).unwrap();
        let m = reopened.memory.lock().unwrap();
        assert_eq!(m.context_for(Reader::User).len(), 1);
        assert!(m.get(Reader::Agent(AgentId::Sharee), id).is_err());
        assert!(m
            .get_value("focus")
            .unwrap()
            .unwrap()
            .contains("Review M20"));
        drop(m);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn memory_helpers_parse_and_forget() {
        let st = AppState::in_memory_for_tests().unwrap();
        let mut m = st.memory.lock().unwrap();
        let id = m
            .remember(NewMemory {
                domain: parse_domain_arg(Some("sharee")).unwrap(),
                kind: parse_kind_arg(Some("semantic")).unwrap(),
                text: "User works on Apple Silicon".into(),
                source: "user".into(),
                confidence: 1.0,
                never_share_with: BTreeSet::new(),
            })
            .unwrap();
        let recent: Vec<MemoryView> = m
            .recent(Reader::User, 10)
            .into_iter()
            .map(memory_to_view)
            .collect();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].domain, "sharee");
        m.forget(Reader::User, id).unwrap();
        assert!(m.recent(Reader::User, 10).is_empty());
    }

    #[test]
    fn release_update_parser_handles_tag_and_prerelease() {
        let up = parse_release_json(
            r#"{"tag_name":"v0.2.0","html_url":"https://github.com/nexonic-technologies/Nexonic-AI/releases/tag/v0.2.0"}"#,
            "0.1.0",
        );
        assert!(up.update_available);
        assert_eq!(up.latest_version.as_deref(), Some("0.2.0"));

        let older = parse_release_json(r#"{"tag_name":"v0.0.9"}"#, "0.1.0");
        assert!(!older.update_available, "an older release is not an update");
        let same = parse_release_json(r#"{"tag_name":"v0.1.0"}"#, "0.1.0");
        assert!(!same.update_available);
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("1.0.0-beta", "1.0.0"));

        let dev = parse_release_json("{}", "0.1.0");
        assert!(!dev.update_available);
        assert!(dev.latest_version.is_none());
    }

    #[test]
    fn router_policy_defaults_free_and_restores_from_memory() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "nexonic-router-mem-{}-{}.sqlite",
            std::process::id(),
            now_secs()
        ));
        let key = generate_key();
        {
            let st = AppState::open_persistent(&path, &key).unwrap();
            let status = build_provider_status(&st).unwrap();
            assert!(!status.allow_paid);
            assert!(status.free_count > 0);
            let mut m = st.memory.lock().unwrap();
            m.put("router.allow_paid", "true").unwrap();
            m.put("router.preferred_model", "google/gemma-3-27b-it:free")
                .unwrap();
        }
        let reopened = AppState::open_persistent(&path, &key).unwrap();
        let status2 = build_provider_status(&reopened).unwrap();
        assert!(status2.allow_paid);
        assert_eq!(
            status2.preferred_model.as_deref(),
            Some("google/gemma-3-27b-it:free")
        );
        let _ = std::fs::remove_file(&path);
    }
}
