//! EXP-229: on-disk registry of the `coding_sessions` rows THIS install
//! launched, so a relaunched IDE can end the orphans a crash / forced logout /
//! failed quit-time sweep left `running` on the server.
//!
//! Why it exists: [`crate::coding_flow::LocalSessions`] is in-memory only —
//! a SIGKILL, a panic, or the EXP-229 deploy-logout (token deleted, so the
//! quit-time `codingSessions.end` could never authenticate) strands rows in
//! `running`, and every client's start-gate then blocks the issue as "coding
//! now" until the server's 2h staleness sweep. The registry closes that
//! window: entries are recorded on [`LocalSessions::insert`], removed when
//! the row's `codingSessions.end` RESOLVES (EXP-640 — the coding crate's
//! [`coding::SessionEndObserver`], installed by [`install_end_observer`],
//! reports every end's outcome), and whatever survives is reconciled (ended
//! best-effort, idempotent server-side) the next time the account connects
//! ([`reconcile_stale_sessions`], wired into `session::connect_account`).
//!
//! EXP-640: removal used to ride `LocalSessions::remove`, i.e. the local
//! session going away — regardless of whether its end had landed. A deploy
//! that bumps the min client version 426-gates EVERY call from the build
//! still running, so the ends fired while the app sat on the blocking
//! "Update required" screen all failed, the entries were dropped anyway, and
//! the relaunched build had nothing to reconcile: the rows stayed `running`
//! (phantom "coding now" badges) until the server's 2h sweep. Keying removal
//! on the resolved outcome makes the next launch's reconcile the retry.
//!
//! Deliberately NOT touched by the quit hook: entries surviving a quit whose
//! ends never landed (dead network, the 2s drain) are re-ended by the next
//! launch's reconcile.
//!
//! Safety: the reconcile only ends ids NOT currently in `LocalSessions`, and
//! `KillWatch` only watches locally launched (i.e. `LocalSessions`-tracked)
//! sessions — so a reconcile-driven `ended` flip can never kill live local
//! work IN THIS PROCESS. A swept-then-resurrected row now owned by a teammate
//! makes our `end` 403 — entry dropped, nobody killed. Everything is
//! best-effort with the server sweep as the backstop: file errors are logged
//! and swallowed.
//!
//! EXP-295 — that in-process reasoning had a hole ACROSS processes on ONE
//! machine. `LocalSessions` is per-process, so a second instance sees no live
//! sessions at all, while the `end` it issues is global: a cold start off a
//! COPIED data dir (an agent testing the app under an isolated
//! `XDG_DATA_HOME`, a staging build, a restored backup — the single-instance
//! socket lives in the data dir, so a copy never collides) inherited the live
//! instance's registry entries, called them orphans, and ended them. The real
//! instance then saw its own rows flip to `ended`, which IS its kill-switch
//! (`sync::kill_watch`), and tore down live agents mid-run.
//!
//! The fix is [`entry_is_owned_by_live_instance`]: entries record the PID that
//! launched them, and the reconcile skips any whose PID is another process
//! still alive on this machine. Deliberately asymmetric — a false "alive"
//! (recycled PID, a data dir carried to another machine) only defers the
//! orphan to the server's staleness sweep, i.e. degrades to pre-EXP-229
//! behavior, while a false "dead" kills a running agent.
//!
//! File format: `{data_dir}/coding-session-registry.json`, a JSON array of
//! `{"id", "accountId", "pid"}` — one file for all accounts (precedent:
//! `accounts.json`), filtered per account at reconcile time. `pid` is absent
//! in files written before EXP-295 and reconciles unconditionally, as before.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui::App;

use crate::coding_flow::LocalSessions;
use crate::session::AuthContext;

/// One recorded session: the synced `coding_sessions` row id, the local
/// account that started it (only that account's token can end it), and the
/// PID of the instance that launched it (EXP-295 — the cross-instance
/// ownership signal; `None` in pre-EXP-295 files).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    id: String,
    account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
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
        pid: Some(std::process::id()),
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

/// EXP-640: the outcome of one `codingSessions.end` for a recorded session.
/// Resolved (2xx, or a 4xx meaning swept/foreign) drops the entry; anything
/// else (transport, 5xx, 401, and the 426 min-version gate —
/// `ApiError::UpgradeRequired`) keeps it for the next launch's reconcile.
fn on_end_outcome(
    data_dir: &Path,
    session_id: &str,
    result: &Result<api::coding_sessions::CodingSession, api::ApiError>,
) {
    if end_outcome_resolves(result) {
        remove(data_dir, session_id);
    } else if let Err(err) = result {
        log::info!(
            "[session-registry] end of coding session {session_id} did not resolve ({err}) — kept for the next launch's reconcile"
        );
    }
}

/// Install the process-wide [`coding::SessionEndObserver`] that keeps the
/// registry in step with the ends the coding crate and its host issue. Call
/// once at bootstrap, before any session can be launched.
pub fn install_end_observer(data_dir: PathBuf) {
    coding::set_session_end_observer(Arc::new(move |session_id, result| {
        on_end_outcome(&data_dir, session_id, result);
    }));
}

/// The recorded entries belonging to `account_id`.
fn entries_for_account(data_dir: &Path, account_id: &str) -> Vec<RegistryEntry> {
    let _guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    load(data_dir)
        .into_iter()
        .filter(|entry| entry.account_id == account_id)
        .collect()
}

/// The recorded session ids belonging to `account_id`.
#[cfg(test)]
fn session_ids_for_account(data_dir: &Path, account_id: &str) -> Vec<String> {
    entries_for_account(data_dir, account_id)
        .into_iter()
        .map(|entry| entry.id)
        .collect()
}

/// EXP-295's cross-instance ownership test: is this entry's session still
/// owned by a DIFFERENT instance that is alive on this machine? Such an entry
/// is not an orphan — ending it would flip a live row to `ended`, which the
/// owning instance reads as its kill-switch.
///
/// Pure (the liveness probe is injected) so the matrix is unit-testable:
/// - no recorded PID (pre-EXP-295 file) → reconcile, as before;
/// - our OWN pid → reconcile (same process; `LocalSessions` already filtered
///   out everything actually live here);
/// - another pid, still alive → SKIP (a second instance owns it);
/// - another pid, gone → reconcile (the crash/forced-logout case EXP-229 exists
///   for).
fn entry_is_owned_by_live_instance(
    entry_pid: Option<u32>,
    own_pid: u32,
    process_alive: impl Fn(u32) -> bool,
) -> bool {
    match entry_pid {
        Some(pid) if pid != own_pid => process_alive(pid),
        _ => false,
    }
}

/// The reconcile's selection, lifted out of the gpui path so the whole matrix
/// is testable: recorded entries minus (a) what THIS process still has live in
/// `LocalSessions` and (b) what another live instance on this machine owns.
fn stale_ids(
    entries: Vec<RegistryEntry>,
    live_ids: &[String],
    own_pid: u32,
    process_alive: impl Fn(u32) -> bool,
) -> Vec<String> {
    entries
        .into_iter()
        .filter(|entry| {
            if live_ids.contains(&entry.id) {
                return false;
            }
            if entry_is_owned_by_live_instance(entry.pid, own_pid, &process_alive) {
                log::info!(
                    "[session-registry] leaving coding session {} to live instance pid {:?}",
                    entry.id,
                    entry.pid
                );
                return false;
            }
            true
        })
        .map(|entry| entry.id)
        .collect()
}

/// Is `pid` a live process on this machine? `kill(pid, 0)` delivers no signal
/// and only reports reachability: `Ok` = alive, `EPERM` = alive but owned by
/// another user, `ESRCH` = gone. Windows has no cheap equivalent without a new
/// dependency and reports "not alive", i.e. keeps the pre-EXP-295 behavior
/// there (the second-instance case is a Unix dev/test workflow).
#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // `kill` reads a SIGNED pid: 0 means "our whole process group" and any
    // negative value means "that process group", so only values that survive
    // the round-trip into a positive `pid_t` may be probed. A recorded pid
    // outside that range is corrupt — treat it as gone.
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    // SAFETY: `kill` with signal 0 has no side effects beyond setting errno.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    false
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
///
/// EXP-295: `LocalSessions` only knows THIS process, so entries launched by
/// another instance that is still alive on this machine are filtered out
/// first ([`entry_is_owned_by_live_instance`]) — they are that instance's
/// live work, not orphans.
pub(crate) fn reconcile_stale_sessions(account: &api::Account, cx: &mut App) {
    let Some(auth) = cx.try_global::<AuthContext>() else {
        return;
    };
    let data_dir = auth.data_dir.clone();
    let live_ids: Vec<String> = LocalSessions::global_ref(cx)
        .map(|sessions| sessions.read(cx).session_ids())
        .unwrap_or_default();
    let stale = stale_ids(
        entries_for_account(&data_dir, &account.id),
        &live_ids,
        std::process::id(),
        process_is_alive,
    );
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
        assert!(session_ids_for_account(&dir.path, "acct-1").is_empty());
    }

    #[test]
    fn record_roundtrips_and_remove_drops() {
        let dir = TempDir::new("roundtrip");
        record(&dir.path, "sess-1", "acct-1");
        record(&dir.path, "sess-2", "acct-1");
        // Re-recording dedupes.
        record(&dir.path, "sess-1", "acct-1");
        assert_eq!(
            session_ids_for_account(&dir.path, "acct-1"),
            vec!["sess-1".to_string(), "sess-2".to_string()]
        );

        remove(&dir.path, "sess-1");
        assert_eq!(
            session_ids_for_account(&dir.path, "acct-1"),
            vec!["sess-2".to_string()]
        );
        // Unknown-id remove is a no-op.
        remove(&dir.path, "sess-unknown");
        assert_eq!(
            session_ids_for_account(&dir.path, "acct-1"),
            vec!["sess-2".to_string()]
        );
    }

    #[test]
    fn entries_filter_by_account() {
        let dir = TempDir::new("accounts");
        record(&dir.path, "sess-a", "acct-1");
        record(&dir.path, "sess-b", "acct-2");
        assert_eq!(
            session_ids_for_account(&dir.path, "acct-1"),
            vec!["sess-a".to_string()]
        );
        assert_eq!(
            session_ids_for_account(&dir.path, "acct-2"),
            vec!["sess-b".to_string()]
        );
    }

    #[test]
    fn corrupt_file_reads_empty_and_next_record_heals_it() {
        let dir = TempDir::new("corrupt");
        fs::write(registry_path(&dir.path), "{not json").unwrap();
        assert!(session_ids_for_account(&dir.path, "acct-1").is_empty());

        record(&dir.path, "sess-1", "acct-1");
        assert_eq!(
            session_ids_for_account(&dir.path, "acct-1"),
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
        let mut ids = session_ids_for_account(&dir.path, "acct-1");
        ids.sort();
        let expected: Vec<String> = (0..8).map(|n| format!("sess-{n}")).collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn record_stamps_the_owning_pid_and_legacy_entries_stay_readable() {
        let dir = TempDir::new("pid");
        record(&dir.path, "sess-1", "acct-1");
        let entries = entries_for_account(&dir.path, "acct-1");
        assert_eq!(entries[0].pid, Some(std::process::id()));

        // A pre-EXP-295 file (no `pid` key) still loads — and reconciles
        // unconditionally, exactly as it did before.
        fs::write(
            registry_path(&dir.path),
            r#"[{"id":"sess-old","accountId":"acct-1"}]"#,
        )
        .unwrap();
        let legacy = entries_for_account(&dir.path, "acct-1");
        assert_eq!(legacy[0].pid, None);
        assert!(!entry_is_owned_by_live_instance(
            legacy[0].pid,
            std::process::id(),
            |_| true
        ));
    }

    #[test]
    fn live_sibling_instance_owns_its_entries() {
        let own = std::process::id();
        // EXP-295: a second instance started from a COPIED data dir inherits
        // the live instance's entries — its cold-start reconcile must leave
        // them alone (ending them flips rows to `ended`, which the owning
        // instance reads as its kill-switch).
        assert!(entry_is_owned_by_live_instance(Some(own + 1), own, |_| true));
        // Owner gone (the crash / forced-logout orphan EXP-229 exists for).
        assert!(!entry_is_owned_by_live_instance(
            Some(own + 1),
            own,
            |_| false
        ));
        // Our own entries are never "another instance's" — the reconcile's
        // LocalSessions filter is what protects the live ones in-process.
        assert!(!entry_is_owned_by_live_instance(Some(own), own, |_| true));
    }

    #[test]
    fn reconcile_selects_only_real_orphans() {
        let own = std::process::id();
        let sibling = own + 1; // "alive" per the injected probe below.
        let entry = |id: &str, pid: Option<u32>| RegistryEntry {
            id: id.to_string(),
            account_id: "acct-1".to_string(),
            pid,
        };
        let entries = vec![
            entry("live-here", Some(own)),      // running in THIS process
            entry("orphan-here", Some(own)),    // ours, child already gone
            entry("live-sibling", Some(sibling)), // the EXP-295 case
            entry("orphan-sibling", Some(own + 2)), // sibling that crashed
            entry("legacy", None),              // pre-EXP-295 file
        ];
        let live_ids = vec!["live-here".to_string()];

        let stale = stale_ids(entries, &live_ids, own, |pid| pid == sibling);
        assert_eq!(
            stale,
            vec![
                "orphan-here".to_string(),
                "orphan-sibling".to_string(),
                "legacy".to_string()
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_liveness_probe() {
        // This test's own process is trivially alive.
        assert!(process_is_alive(std::process::id()));
        // PID 0 signals our own process group, and anything past pid_t's
        // positive range would wrap into a process-group probe — neither is a
        // session owner, both read as gone.
        assert!(!process_is_alive(0));
        assert!(!process_is_alive(u32::MAX - 1));
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
        assert!(!end_outcome_resolves(&Err(api::ApiError::transport("refused"))));
        assert!(!end_outcome_resolves(&Err(api::ApiError::Unauthorized)));
        // EXP-640: the min-version gate rejects the CALL, not the session —
        // the row is still `running` and must stay recorded.
        assert!(!end_outcome_resolves(&Err(api::ApiError::UpgradeRequired)));
    }

    #[test]
    fn end_outcome_drops_the_entry_only_once_resolved() {
        let dir = TempDir::new("end-outcome");
        record(&dir.path, "sess-1", "acct-1");

        // EXP-640: a 426-gated end (the superseded build's quit-time sweep)
        // keeps the entry for the relaunched build's reconcile.
        on_end_outcome(&dir.path, "sess-1", &Err(api::ApiError::UpgradeRequired));
        assert_eq!(session_ids_for_account(&dir.path, "acct-1"), vec!["sess-1"]);
        on_end_outcome(
            &dir.path,
            "sess-1",
            &Err(api::ApiError::transport("refused")),
        );
        assert_eq!(session_ids_for_account(&dir.path, "acct-1"), vec!["sess-1"]);

        // A resolved end drops it.
        let session: api::coding_sessions::CodingSession =
            serde_json::from_str(r#"{"id":"sess-1"}"#).unwrap();
        on_end_outcome(&dir.path, "sess-1", &Ok(session));
        assert!(session_ids_for_account(&dir.path, "acct-1").is_empty());

        // 404 = already swept server-side: resolved as well.
        record(&dir.path, "sess-2", "acct-1");
        on_end_outcome(
            &dir.path,
            "sess-2",
            &Err(api::ApiError::Http {
                status: 404,
                message: String::new(),
            }),
        );
        assert!(session_ids_for_account(&dir.path, "acct-1").is_empty());
        // Unknown ids are a no-op either way.
        on_end_outcome(&dir.path, "sess-unknown", &Err(api::ApiError::Unauthorized));
    }
}
