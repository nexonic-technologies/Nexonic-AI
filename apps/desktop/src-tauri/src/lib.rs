//! Nexonic AI desktop core: typed commands the interface calls over Tauri IPC.
//!
//! Everything returned here is real state: detected hardware, the live
//! permission set and the memory store. Nothing is hardcoded or simulated.

use std::sync::Mutex;

use nexonic_memory_engine::{MemoryStore, Reader};
use nexonic_permissions::{Actor, Capability, Decision, Permissions, Scope};
use serde::Serialize;
use tauri::State;

/// Shared application state behind the IPC boundary.
pub struct AppState {
    permissions: Mutex<Permissions>,
    memory: Mutex<MemoryStore>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            permissions: Mutex::new(Permissions::new()),
            memory: Mutex::new(MemoryStore::new()),
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
}

#[tauri::command]
fn memory_summary(state: State<'_, AppState>) -> Result<MemorySummary, String> {
    let m = state.memory.lock().map_err(|e| e.to_string())?;
    Ok(MemorySummary {
        user_visible: m.context_for(Reader::User).len(),
    })
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            system_info,
            permissions,
            set_permission,
            emergency_stop,
            resume,
            memory_summary
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
}
