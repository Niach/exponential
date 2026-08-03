//! On-disk record of the `coding_sessions` rows THIS install launched —
//! byte-format parity with the desktop's `ui/src/session_registry.rs`
//! (`{data_dir}/coding-session-registry.json`, entries `{id, accountId,
//! pid}`), so a machine running both the desktop app and the CLI daemon
//! shares ONE registry and each process's reconcile skips rows owned by a
//! LIVE sibling (the EXP-295 pid guard). Everything here is best-effort —
//! the server's 2h staleness sweep remains the backstop.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Serializes every load-modify-save (desktop parity — its registry takes
/// the same in-process lock). The CLI genuinely needs it: `record` runs on
/// per-remote-start threads while supervisors `remove` concurrently, and an
/// unguarded interleave loses a live session's entry (which a later crash
/// then never reconciles). Poison-tolerant like everything else here.
static LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    id: String,
    account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
}

fn registry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("coding-session-registry.json")
}

fn load(data_dir: &Path) -> Vec<RegistryEntry> {
    let raw = match std::fs::read_to_string(registry_path(data_dir)) {
        Ok(raw) => raw,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&raw).unwrap_or_else(|err| {
        log::warn!("session registry unreadable ({err}); starting empty");
        Vec::new()
    })
}

fn save(data_dir: &Path, entries: &[RegistryEntry]) {
    let path = registry_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let Ok(json) = serde_json::to_string_pretty(entries) else {
        return;
    };
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

pub fn record(data_dir: &Path, session_id: &str, account_id: &str) {
    let _guard = locked();
    let mut entries = load(data_dir);
    if entries.iter().any(|entry| entry.id == session_id) {
        return;
    }
    entries.push(RegistryEntry {
        id: session_id.to_string(),
        account_id: account_id.to_string(),
        pid: Some(std::process::id()),
    });
    save(data_dir, &entries);
}

pub fn remove(data_dir: &Path, session_id: &str) {
    let _guard = locked();
    let mut entries = load(data_dir);
    let before = entries.len();
    entries.retain(|entry| entry.id != session_id);
    if entries.len() != before {
        save(data_dir, &entries);
    }
}

/// `kill(pid, 0)` liveness: alive on success or EPERM; gone on ESRCH.
fn process_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Stale rows this account left behind: recorded entries not owned by a
/// LIVE sibling process (desktop app or another daemon on this machine).
/// The caller ends each id (`codingSessions.end`, idempotent) and calls
/// [`remove`] on 2xx/4xx outcomes — 4xx means swept/foreign, stop retrying.
pub fn stale_ids(data_dir: &Path, account_id: &str) -> Vec<String> {
    let _guard = locked();
    let own_pid = std::process::id();
    load(data_dir)
        .into_iter()
        .filter(|entry| entry.account_id == account_id)
        .filter(|entry| match entry.pid {
            // A live process that is not us owns this row — leave it alone.
            Some(pid) => pid == own_pid || !process_alive(pid),
            // Pre-pid entry: reconcile unconditionally (desktop parity).
            None => true,
        })
        .map(|entry| entry.id)
        .collect()
}

/// Whether an end outcome RESOLVES the entry (desktop parity): success and
/// any 4xx (404 = swept, 403 = resurrected under another owner) drop it;
/// transport/401/5xx keep it for the next start's reconcile.
pub fn end_outcome_resolves(result: &Result<api::coding_sessions::CodingSession, api::ApiError>) -> bool {
    match result {
        Ok(_) => true,
        Err(api::ApiError::Http { status, .. }) => (400..500).contains(status),
        Err(_) => false,
    }
}
