//! EXP-229: on-disk registry of the `coding_sessions` rows THIS install
//! launched, so a relaunched IDE can end the orphans a crash / forced logout /
//! failed quit-time sweep left `running` on the server.
//!
//! Why it exists: [`crate::coding_flow::LocalSessions`] is in-memory only —
//! a SIGKILL, a panic, or the EXP-229 deploy-logout (token deleted, so the
//! quit-time `codingSessions.end` could never authenticate) strands rows in
//! `running`, and every client's start-gate then blocks the issue as "coding
//! now" until the server's 2h staleness sweep. The registry closes that
//! window: entries are recorded on [`LocalSessions::insert`], removed on
//! [`LocalSessions::remove`], and whatever survives is reconciled (ended
//! best-effort, idempotent server-side) the next time the account connects
//! ([`reconcile_stale_sessions`], wired into `session::connect_account`).
//!
//! Deliberately NOT touched by the quit hook: entries surviving a clean quit
//! are re-ended as no-ops by the next launch's reconcile — which doubles as
//! the retry for ends the 2s quit drain lost to a dead network.
//!
//! Safety: the reconcile only ends ids NOT currently in `LocalSessions`, and
//! `KillWatch` only watches locally launched (i.e. `LocalSessions`-tracked)
//! sessions — so a reconcile-driven `ended` flip can never kill live local
//! work, here or on another device (each device only watches its own ids). A
//! swept-then-resurrected row now owned by a teammate makes our `end` 403 —
//! entry dropped, nobody killed. Everything is best-effort with the server
//! sweep as the backstop: file errors are logged and swallowed.
//!
//! File format: `{data_dir}/coding-session-registry.json`, a JSON array of
//! `{"id", "accountId"}` — one file for all accounts (precedent:
//! `accounts.json`), filtered per account at reconcile time.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gpui::App;

use crate::coding_flow::LocalSessions;
use crate::session::AuthContext;

/// One recorded session: the synced `coding_sessions` row id plus the local
/// account that started it (only that account's token can end it).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    id: String,
    account_id: String,
}

/// Serializes every read-modify-write across threads (foreground
/// insert/remove vs the reconcile and sign-out background threads).
/// Poison-tolerant like `coding_flow::PENDING_ENDS` — a panicked writer must
/// not wedge session bookkeeping forever.
static LOCK: Mutex<()> = Mutex::new(());

fn registry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("coding-session-registry.json")
}

/// Load tolerantly: missing file = first run = empty; corrupt JSON = warn and
/// treat as empty (the next write rewrites it cleanly).
fn load(data_dir: &Path) -> Vec<RegistryEntry> {
    let path = registry_path(data_dir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str(&raw) {
        Ok(entries) => entries,
        Err(err) => {
            log::warn!(
                "[session-registry] corrupt {} ({err}) — treating as empty",
                path.display()
            );
            Vec::new()
        }
    }
}

/// Persist via tmp-file + rename so a torn write can never leave corrupt JSON
/// behind (which would silently drop orphans). Best-effort: errors are logged
/// and swallowed — the server staleness sweep remains the backstop.
fn save(data_dir: &Path, entries: &[RegistryEntry]) {
    let path = registry_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let result = fs::create_dir_all(data_dir)
        .map_err(|err| err.to_string())
        .and_then(|_| serde_json::to_string_pretty(entries).map_err(|err| err.to_string()))
        .and_then(|json| fs::write(&tmp, json).map_err(|err| err.to_string()))
        .and_then(|_| fs::rename(&tmp, &path).map_err(|err| err.to_string()));
    if let Err(err) = result {
        log::warn!("[session-registry] could not persist {}: {err}", path.display());
    }
}

/// Record a freshly launched session (dedupes on id — re-recording is a
/// no-op). Called from [`LocalSessions::insert`].
pub(crate) fn record(data_dir: &Path, session_id: &str, account_id: &str) {
    let _guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut entries = load(data_dir);
    if entries.iter().any(|entry| entry.id == session_id) {
        return;
    }
    entries.push(RegistryEntry {
        id: session_id.to_string(),
        account_id: account_id.to_string(),
    });
    save(data_dir, &entries);
}

/// Drop a session (normal end paths + resolved reconcile/sign-out ends).
/// Unknown ids are a no-op.
pub(crate) fn remove(data_dir: &Path, session_id: &str) {
    let _guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut entries = load(data_dir);
    let before = entries.len();
    entries.retain(|entry| entry.id != session_id);
    if entries.len() != before {
        save(data_dir, &entries);
    }
}

/// The recorded session ids belonging to `account_id`.
pub(crate) fn entries_for_account(data_dir: &Path, account_id: &str) -> Vec<String> {
    let _guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    load(data_dir)
        .into_iter()
        .filter(|entry| entry.account_id == account_id)
        .map(|entry| entry.id)
        .collect()
}

/// Whether an end attempt's outcome RESOLVES the registry entry (drop it) or
/// should leave it for the next reconcile. `Ok` and any 4xx resolve (404 =
/// row already swept, 403 = not ours anymore after an EXP-105 resurrection —
/// our end can never succeed, stop retrying); transport / 401 / 5xx
/// (mid-deploy) keep the entry for the next sign-in.
pub(crate) fn end_outcome_resolves(
    result: &Result<api::coding_sessions::CodingSession, api::ApiError>,
) -> bool {
    match result {
        Ok(_) => true,
        Err(api::ApiError::Http { status, .. }) => (400..500).contains(status),
        Err(_) => false,
    }
}

/// EXP-229 startup/login reconcile: end (best-effort, idempotent) every
/// registry entry for `account` that is NOT a live [`LocalSessions`] entry —
/// a same-process re-login keeps its running children, while a cold start
/// (empty `LocalSessions`) treats every recorded id as an orphan. Runs on a
/// fire-and-forget plain thread (`spawn_tracked_end` style); the token
/// provider is call-time, so it survives a token refresh mid-loop.
pub(crate) fn reconcile_stale_sessions(account: &api::Account, cx: &mut App) {
    let Some(auth) = cx.try_global::<AuthContext>() else {
        return;
    };
    let data_dir = auth.data_dir.clone();
    let live_ids: Vec<String> = LocalSessions::global_ref(cx)
        .map(|sessions| sessions.read(cx).session_ids())
        .unwrap_or_default();
    let stale: Vec<String> = entries_for_account(&data_dir, &account.id)
        .into_iter()
        .filter(|id| !live_ids.contains(id))
        .collect();
    if stale.is_empty() {
        return;
    }
    let provider: std::sync::Arc<dyn api::TokenProvider> =
        auth.auth.token_provider(&account.id);
    let trpc = api::TrpcClient::new(&account.instance_url, provider);
    std::thread::spawn(move || {
        for id in stale {
            let result = api::coding_sessions::end(&trpc, &id);
            match &result {
                Ok(_) => log::info!("[session-registry] ended stale coding session {id}"),
                Err(err) => log::warn!(
                    "[session-registry] could not end stale coding session {id}: {err}"
                ),
            }
            if end_outcome_resolves(&result) {
                remove(&data_dir, &id);
            }
        }
    });
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
            let path = std::env::temp_dir().join(format!(
                "exp-session-registry-{tag}-{}-{n}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            TempDir { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn missing_file_reads_empty() {
        let dir = TempDir::new("missing");
        assert!(entries_for_account(&dir.path, "acct-1").is_empty());
    }

    #[test]
    fn record_roundtrips_and_remove_drops() {
        let dir = TempDir::new("roundtrip");
        record(&dir.path, "sess-1", "acct-1");
        record(&dir.path, "sess-2", "acct-1");
        // Re-recording dedupes.
        record(&dir.path, "sess-1", "acct-1");
        assert_eq!(
            entries_for_account(&dir.path, "acct-1"),
            vec!["sess-1".to_string(), "sess-2".to_string()]
        );

        remove(&dir.path, "sess-1");
        assert_eq!(
            entries_for_account(&dir.path, "acct-1"),
            vec!["sess-2".to_string()]
        );
        // Unknown-id remove is a no-op.
        remove(&dir.path, "sess-unknown");
        assert_eq!(
            entries_for_account(&dir.path, "acct-1"),
            vec!["sess-2".to_string()]
        );
    }

    #[test]
    fn entries_filter_by_account() {
        let dir = TempDir::new("accounts");
        record(&dir.path, "sess-a", "acct-1");
        record(&dir.path, "sess-b", "acct-2");
        assert_eq!(
            entries_for_account(&dir.path, "acct-1"),
            vec!["sess-a".to_string()]
        );
        assert_eq!(
            entries_for_account(&dir.path, "acct-2"),
            vec!["sess-b".to_string()]
        );
    }

    #[test]
    fn corrupt_file_reads_empty_and_next_record_heals_it() {
        let dir = TempDir::new("corrupt");
        fs::write(registry_path(&dir.path), "{not json").unwrap();
        assert!(entries_for_account(&dir.path, "acct-1").is_empty());

        record(&dir.path, "sess-1", "acct-1");
        assert_eq!(
            entries_for_account(&dir.path, "acct-1"),
            vec!["sess-1".to_string()]
        );
        // The file is valid JSON again.
        let raw = fs::read_to_string(registry_path(&dir.path)).unwrap();
        serde_json::from_str::<Vec<RegistryEntry>>(&raw).unwrap();
    }

    #[test]
    fn concurrent_records_lose_no_updates() {
        let dir = TempDir::new("concurrent");
        let handles: Vec<_> = (0..8)
            .map(|n| {
                let path = dir.path.clone();
                std::thread::spawn(move || {
                    record(&path, &format!("sess-{n}"), "acct-1");
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let mut ids = entries_for_account(&dir.path, "acct-1");
        ids.sort();
        let expected: Vec<String> = (0..8).map(|n| format!("sess-{n}")).collect();
        assert_eq!(ids, expected);
    }

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
        assert!(!end_outcome_resolves(&Err(api::ApiError::Transport(
            "refused".into()
        ))));
        assert!(!end_outcome_resolves(&Err(api::ApiError::Unauthorized)));
    }
}
