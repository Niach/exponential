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
    /// The LIVE owner: the pid of the process whose child is still running
    /// this session. Absent means nobody owns it — a pre-pid file, or (EXP-641)
    /// an entry [`mark_ended`] kept only because its `end` never resolved.
    /// Deliberately encoded by CLEARING the pid rather than by a new `ended`
    /// key: the desktop app shares this file byte-for-byte and would silently
    /// drop a field its own struct does not know the next time it rewrites,
    /// resurrecting the entry as "live". An absent pid round-trips through
    /// both readers and already means "reconcile me" on either side.
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

/// EXP-641: the session is over locally but its `codingSessions.end` did not
/// resolve (426 gate, dead network, 5xx), so the entry stays for the next
/// launch's reconcile — with its pid cleared, because no process runs it any
/// more. Without this an ended-but-retained row still counted as a live
/// session in [`sessions_owned_by`], and `exponential update` refused to
/// restart an idle daemon. Unknown ids and already-cleared entries are
/// no-ops (no rewrite).
pub fn mark_ended(data_dir: &Path, session_id: &str) {
    let _guard = locked();
    let mut entries = load(data_dir);
    let mut changed = false;
    for entry in entries.iter_mut() {
        if entry.id == session_id && entry.pid.is_some() {
            entry.pid = None;
            changed = true;
        }
    }
    if changed {
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
            // No owner: a pre-pid entry (desktop parity) or one whose
            // session already ended ([`mark_ended`]) — reconcile it.
            None => true,
        })
        .map(|entry| entry.id)
        .collect()
}

/// Live sessions a daemon at `pid` still owns (EXP-641: `exponential update`
/// must not restart a daemon out from under a running agent). Entries kept
/// only for reconcile carry no pid ([`mark_ended`]) and are NOT live.
pub fn sessions_owned_by(data_dir: &Path, pid: u32) -> usize {
    let _guard = locked();
    load(data_dir)
        .iter()
        .filter(|entry| entry.pid == Some(pid))
        .count()
}

/// Whether an end outcome RESOLVES the entry (desktop parity): success and
/// any 4xx (404 = swept, 403 = resurrected under another owner) drop it;
/// transport/401/5xx — and the 426 min-version gate, `UpgradeRequired`, which
/// rejects the CALL while the row stays `running` — keep it for the next
/// start's reconcile.
pub fn end_outcome_resolves(result: &Result<api::coding_sessions::CodingSession, api::ApiError>) -> bool {
    match result {
        Ok(_) => true,
        Err(api::ApiError::Http { status, .. }) => (400..500).contains(status),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("exp-cli-registry-{tag}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).expect("temp dir");
            TempDir { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn record_roundtrips_and_remove_drops() {
        let dir = TempDir::new("roundtrip");
        record(&dir.path, "sess-1", "acct-1");
        record(&dir.path, "sess-2", "acct-1");
        record(&dir.path, "sess-1", "acct-1"); // dedupes on id
        record(&dir.path, "sess-other", "acct-2");
        assert_eq!(stale_ids(&dir.path, "acct-1"), vec!["sess-1", "sess-2"]);

        remove(&dir.path, "sess-1");
        remove(&dir.path, "sess-unknown"); // no-op
        assert_eq!(stale_ids(&dir.path, "acct-1"), vec!["sess-2"]);
        assert_eq!(stale_ids(&dir.path, "acct-2"), vec!["sess-other"]);
    }

    /// EXP-641 regression: a session whose `end` never resolved (the 426
    /// gate) stays recorded for the next reconcile, but it is NOT live — the
    /// daemon it ran under is idle and `exponential update` must restart it.
    #[test]
    fn an_ended_entry_is_kept_for_reconcile_but_stops_counting_as_live() {
        let dir = TempDir::new("ended");
        let own = std::process::id();
        record(&dir.path, "sess-1", "acct-1");
        record(&dir.path, "sess-2", "acct-1");
        assert_eq!(sessions_owned_by(&dir.path, own), 2);

        mark_ended(&dir.path, "sess-1");
        assert_eq!(sessions_owned_by(&dir.path, own), 1);
        // Still on file, and still reconciled at the next launch.
        assert_eq!(stale_ids(&dir.path, "acct-1"), vec!["sess-1", "sess-2"]);

        mark_ended(&dir.path, "sess-2");
        assert_eq!(sessions_owned_by(&dir.path, own), 0);
        assert_eq!(stale_ids(&dir.path, "acct-1"), vec!["sess-1", "sess-2"]);

        // The reconcile's resolved end is what finally drops it.
        remove(&dir.path, "sess-1");
        remove(&dir.path, "sess-2");
        assert!(stale_ids(&dir.path, "acct-1").is_empty());
    }

    /// Clearing the pid must not touch anyone else's entry, and re-clearing
    /// (or clearing an unknown id) must not rewrite the file.
    #[test]
    fn mark_ended_is_targeted_and_idempotent() {
        let dir = TempDir::new("ended-idempotent");
        record(&dir.path, "sess-1", "acct-1");
        record(&dir.path, "sess-2", "acct-1");
        mark_ended(&dir.path, "sess-1");

        let after = std::fs::read_to_string(registry_path(&dir.path)).unwrap();
        mark_ended(&dir.path, "sess-1"); // already cleared
        mark_ended(&dir.path, "sess-unknown"); // never recorded
        assert_eq!(std::fs::read_to_string(registry_path(&dir.path)).unwrap(), after);
        // The sibling keeps its owner.
        assert_eq!(sessions_owned_by(&dir.path, std::process::id()), 1);
    }

    /// The on-disk format stays what the desktop reads and writes: an ended
    /// entry is simply one WITHOUT a `pid` key, exactly like a pre-pid file
    /// — so a desktop rewrite cannot resurrect it as live.
    #[test]
    fn ended_entries_serialize_as_pid_less_and_legacy_files_still_load() {
        let dir = TempDir::new("format");
        record(&dir.path, "sess-1", "acct-1");
        mark_ended(&dir.path, "sess-1");
        let raw = std::fs::read_to_string(registry_path(&dir.path)).unwrap();
        assert!(!raw.contains("pid"), "ended entry must drop the pid key: {raw}");

        std::fs::write(
            registry_path(&dir.path),
            r#"[{"id":"sess-old","accountId":"acct-1"}]"#,
        )
        .unwrap();
        assert_eq!(sessions_owned_by(&dir.path, std::process::id()), 0);
        assert_eq!(stale_ids(&dir.path, "acct-1"), vec!["sess-old"]);
    }

    #[test]
    fn corrupt_file_reads_empty_and_the_next_record_heals_it() {
        let dir = TempDir::new("corrupt");
        std::fs::write(registry_path(&dir.path), "{not json").unwrap();
        assert!(stale_ids(&dir.path, "acct-1").is_empty());
        record(&dir.path, "sess-1", "acct-1");
        assert_eq!(stale_ids(&dir.path, "acct-1"), vec!["sess-1"]);
    }

    #[cfg(unix)]
    #[test]
    fn process_liveness_probe() {
        assert!(process_alive(std::process::id()));
        // 0 is our own process group and anything past pid_t's positive
        // range would wrap into a group probe — neither owns a session.
        assert!(!process_alive(0));
        assert!(!process_alive(u32::MAX - 1));
    }

    /// Desktop parity (`ui/src/session_registry.rs`): only 2xx and 4xx
    /// resolve; transport / 401 / 5xx / the 426 gate keep the entry.
    #[test]
    fn end_outcome_resolution_matrix() {
        let session: api::coding_sessions::CodingSession =
            serde_json::from_str(r#"{"id":"sess-1"}"#).unwrap();
        assert!(end_outcome_resolves(&Ok(session)));
        for status in [400u16, 403, 404] {
            assert!(end_outcome_resolves(&Err(api::ApiError::Http {
                status,
                message: String::new(),
            })));
        }
        assert!(!end_outcome_resolves(&Err(api::ApiError::Http {
            status: 500,
            message: String::new(),
        })));
        assert!(!end_outcome_resolves(&Err(api::ApiError::transport("refused"))));
        assert!(!end_outcome_resolves(&Err(api::ApiError::Unauthorized)));
        assert!(!end_outcome_resolves(&Err(api::ApiError::UpgradeRequired)));
    }
}
