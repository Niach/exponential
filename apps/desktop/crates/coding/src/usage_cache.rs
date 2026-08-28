//! EXP-484 — the on-disk agent-usage cache (`<data_dir>/agent-usage.json`)
//! and the poll policy that decides when a window is worth re-reading.
//!
//! Two reasons this is a FILE and not process state:
//!
//! * The desktop IDE and the headless `exponential` daemon can run on the
//!   same machine against the same account. They share one token budget
//!   (the usage endpoint tolerates ~20 requests/hour), so they must share
//!   one "already fetched" fact — [`SHARED_TTL_SECS`].
//! * A restart must not cost a request, and must not lose the numbers the
//!   last run read (they stay, marked stale, until a fetch replaces them).
//!
//! Same idioms as [`crate::run_registry`]: a static mutex around every
//! load-modify-save, per-entry tolerant parsing (an entry a NEWER build
//! wrote survives an older host's rewrite verbatim), and tmp+rename writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent_accounts::AgentAccount;
use crate::agent_usage::{AgentUsage, UsageWindow};

/// The floor between two fetches for the SAME agent, shared across every
/// process on this machine: a second binary that finds numbers this fresh
/// simply reports them instead of spending a request of its own.
pub const SHARED_TTL_SECS: u64 = 180;

/// A fetch that produced NEW numbers earns the fastest cadence.
pub const MIN_POLL_SECS: u64 = 180;

/// Nothing changed: back off 100 s per unchanged run, between these bounds.
pub const UNCHANGED_BASE_SECS: u64 = 300;
pub const UNCHANGED_STEP_SECS: u64 = 100;
pub const UNCHANGED_MAX_SECS: u64 = 600;

/// A 429 must never be retried faster than this (the endpoint's own budget).
pub const RATE_LIMITED_FLOOR_SECS: u64 = 300;

/// A 401 means the credential no longer answers for usage — the fix is a
/// re-login, not a retry.
pub const UNAUTHORIZED_BACKOFF_SECS: u64 = 600;

/// A transport failure retries on the ordinary slow cadence.
pub const FAILED_BACKOFF_SECS: u64 = 300;

/// A window pinned at 100 % cannot move before it resets; poll just after.
pub const RESET_MARGIN_SECS: u64 = 60;

/// A refused/timed-out credential read (the macOS Keychain ACL prompt on a
/// headless daemon) stops the asking for an hour.
pub const CREDENTIAL_DENIED_BACKOFF_SECS: u64 = 3600;

/// Serializes every load-modify-save (the IDE beat and a device-worker task
/// can both land here).
static LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// One agent's cached usage plus everything the poll policy keys on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentCacheEntry {
    /// The last numbers read — kept (and flagged `stale`) through every
    /// failure, so a 401 dims the bar instead of blanking it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgentUsage>,
    /// The identity the probe named, for the agents whose sign-in check
    /// cannot ([`crate::doctor`] can only see that codex IS signed in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AgentAccount>,
    /// Unix seconds of the last ATTEMPT (successful or not) — the shared
    /// TTL's key, so a failing host does not out-poll a healthy one.
    pub fetched_at_secs: u64,
    pub next_poll_at_secs: u64,
    pub unchanged_streak: u32,
    pub last_windows_hash: String,
    /// The soonest reset among the windows sitting at 100 % — nothing can
    /// change before it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earliest_reset_secs: Option<u64>,
    /// Set when the credential STORE refused; no read is attempted before it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_denied_until_secs: Option<u64>,
    /// Fields a newer build wrote that this one does not know — carried
    /// verbatim through every rewrite (the [`crate::run_registry`] promise).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Why the last fetch mattered — the input to [`next_poll_at`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollOutcome {
    /// New numbers.
    Changed,
    /// The same numbers as last time.
    Unchanged,
    /// HTTP 429.
    RateLimited,
    /// HTTP 401/403.
    Unauthorized,
    /// Transport error, unparseable body, missing/expired credential, a
    /// refused app-server probe.
    Failed,
}

/// The whole file: entries this build understands plus the ones it does not
/// (kept so an older host never deletes a newer host's rows).
#[derive(Default, Debug)]
pub struct UsageCache {
    entries: BTreeMap<String, AgentCacheEntry>,
    unknown: BTreeMap<String, Value>,
}

impl UsageCache {
    pub fn get(&self, agent: &str) -> Option<&AgentCacheEntry> {
        self.entries.get(agent)
    }

    pub fn insert(&mut self, agent: String, entry: AgentCacheEntry) {
        self.unknown.remove(&agent);
        self.entries.insert(agent, entry);
    }
}

fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("agent-usage.json")
}

/// Read the cache, tolerating a missing/corrupt file (→ empty) and entries
/// this build cannot parse (→ carried verbatim).
pub fn load(data_dir: &Path) -> UsageCache {
    let _guard = locked();
    load_unlocked(data_dir)
}

fn load_unlocked(data_dir: &Path) -> UsageCache {
    let Ok(raw) = std::fs::read_to_string(cache_path(data_dir)) else {
        return UsageCache::default();
    };
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&raw) else {
        return UsageCache::default();
    };
    let mut cache = UsageCache::default();
    for (agent, value) in object {
        match serde_json::from_value::<AgentCacheEntry>(value.clone()) {
            Ok(entry) => {
                cache.entries.insert(agent, entry);
            }
            Err(_) => {
                cache.unknown.insert(agent, value);
            }
        }
    }
    cache
}

/// Persist the cache (tmp + rename). Best-effort: a failed write only means
/// the next run re-polls.
pub fn save(data_dir: &Path, cache: &UsageCache) {
    let _guard = locked();
    let mut object = serde_json::Map::new();
    for (agent, entry) in &cache.entries {
        let Ok(value) = serde_json::to_value(entry) else {
            return;
        };
        object.insert(agent.clone(), value);
    }
    for (agent, value) in &cache.unknown {
        object.entry(agent.clone()).or_insert_with(|| value.clone());
    }
    let Ok(json) = serde_json::to_string_pretty(&Value::Object(object)) else {
        return;
    };
    let path = cache_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::create_dir_all(data_dir);
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// A login just ended on this machine for `agent`: drop its cached identity
/// and numbers so the next beat polls afresh and names the NEW account,
/// instead of the old email riding out its backoff (up to 10 min).
pub fn forget(data_dir: &Path, agent: &str) {
    let mut cache = load(data_dir);
    let removed = cache.entries.remove(agent).is_some() | cache.unknown.remove(agent).is_some();
    if removed {
        save(data_dir, &cache);
    }
}

/// Whether this agent may be polled right now: past its scheduled next poll,
/// past the machine-wide shared TTL, and not inside a credential-refusal
/// backoff.
pub fn poll_due(entry: &AgentCacheEntry, now: u64) -> bool {
    if entry
        .credential_denied_until_secs
        .is_some_and(|until| now < until)
    {
        return false;
    }
    now >= entry.next_poll_at_secs
        && now.saturating_sub(entry.fetched_at_secs) >= SHARED_TTL_SECS
}

/// When this agent may be polled again, given what the last attempt did.
/// A window sitting at 100 % pins the answer to just past its reset — the
/// numbers physically cannot move before then.
pub fn next_poll_at(entry: &AgentCacheEntry, outcome: PollOutcome, now: u64) -> u64 {
    let delay = match outcome {
        PollOutcome::Changed => MIN_POLL_SECS,
        PollOutcome::Unchanged => (UNCHANGED_BASE_SECS
            + UNCHANGED_STEP_SECS * u64::from(entry.unchanged_streak))
        .clamp(UNCHANGED_BASE_SECS, UNCHANGED_MAX_SECS),
        PollOutcome::RateLimited => RATE_LIMITED_FLOOR_SECS,
        PollOutcome::Unauthorized => UNAUTHORIZED_BACKOFF_SECS,
        PollOutcome::Failed => FAILED_BACKOFF_SECS,
    };
    let scheduled = now + delay;
    match entry.earliest_reset_secs {
        Some(reset) => scheduled.max(reset + RESET_MARGIN_SECS),
        None => scheduled,
    }
}

/// Fold one attempt's result into the entry: keep or replace the numbers,
/// move the unchanged streak, and schedule the next poll.
pub fn apply_outcome(
    entry: &mut AgentCacheEntry,
    outcome: PollOutcome,
    windows: Option<Vec<UsageWindow>>,
    now: u64,
    stamp: &str,
) {
    entry.fetched_at_secs = now;
    let outcome = match (outcome, windows) {
        (PollOutcome::Changed | PollOutcome::Unchanged, Some(windows)) => {
            let hash = windows_hash(&windows);
            let unchanged = entry.usage.is_some() && hash == entry.last_windows_hash;
            entry.unchanged_streak = if unchanged {
                entry.unchanged_streak.saturating_add(1)
            } else {
                0
            };
            entry.last_windows_hash = hash;
            entry.earliest_reset_secs = earliest_maxed_reset(&windows);
            entry.usage = Some(AgentUsage {
                fetched_at: stamp.to_string(),
                stale: false,
                windows,
            });
            // A successful read proves the credential store answers again.
            entry.credential_denied_until_secs = None;
            if unchanged {
                PollOutcome::Unchanged
            } else {
                PollOutcome::Changed
            }
        }
        (outcome, _) => {
            // Keep the old numbers — dimmed, never blanked, never invented.
            if let Some(usage) = &mut entry.usage {
                usage.stale = true;
            }
            outcome
        }
    };
    entry.next_poll_at_secs = next_poll_at(entry, outcome, now);
}

/// A stable digest of the rendered windows — the change detector. SHA-256
/// (not `DefaultHasher`, which is explicitly unstable across releases) so a
/// compiler bump cannot make every cached entry look changed.
pub fn windows_hash(windows: &[UsageWindow]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for window in windows {
        hasher.update(window.key.as_bytes());
        hasher.update([0]);
        hasher.update(window.percent.to_string().as_bytes());
        hasher.update([0]);
        hasher.update(window.resets_at.as_deref().unwrap_or_default().as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())[..16].to_string()
}

/// The soonest reset among windows at 100 % (unix seconds).
fn earliest_maxed_reset(windows: &[UsageWindow]) -> Option<u64> {
    windows
        .iter()
        .filter(|window| window.percent >= 100)
        .filter_map(|window| {
            let stamp = window.resets_at.as_deref()?;
            chrono::DateTime::parse_from_rfc3339(stamp)
                .ok()
                .map(|at| at.timestamp())
                .filter(|secs| *secs > 0)
                .map(|secs| secs as u64)
        })
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forget_drops_one_agent_and_makes_its_poll_due() {
        let dir = std::env::temp_dir().join(format!("exp-usage-cache-forget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut cache = UsageCache::default();
        let mut entry = AgentCacheEntry::default();
        entry.next_poll_at_secs = 10_000;
        cache.insert("codex".to_string(), entry.clone());
        cache.insert("claude".to_string(), entry);
        save(&dir, &cache);
        forget(&dir, "codex");
        let reloaded = load(&dir);
        assert!(reloaded.get("codex").is_none());
        assert!(reloaded.get("claude").is_some());
        assert!(poll_due(&AgentCacheEntry::default(), 5_000));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn window(key: &str, percent: u8, resets_at: Option<&str>) -> UsageWindow {
        UsageWindow {
            key: key.to_string(),
            label: key.to_string(),
            percent,
            resets_at: resets_at.map(str::to_string),
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-usage-cache-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The whole policy truth table in one place — every arm of the
    /// cadence, the streak's clamp, and the maxed-window floor.
    #[test]
    fn poll_policy_schedules_every_outcome() {
        let now = 1_000_000;
        let fresh = AgentCacheEntry::default();
        assert_eq!(next_poll_at(&fresh, PollOutcome::Changed, now), now + 180);
        assert_eq!(next_poll_at(&fresh, PollOutcome::Unchanged, now), now + 300);
        assert_eq!(next_poll_at(&fresh, PollOutcome::RateLimited, now), now + 300);
        assert_eq!(next_poll_at(&fresh, PollOutcome::Unauthorized, now), now + 600);
        assert_eq!(next_poll_at(&fresh, PollOutcome::Failed, now), now + 300);

        // The unchanged streak backs off 100 s a run and clamps at 600.
        for (streak, expected) in [(0, 300), (1, 400), (3, 600), (9, 600)] {
            let entry = AgentCacheEntry {
                unchanged_streak: streak,
                ..AgentCacheEntry::default()
            };
            assert_eq!(
                next_poll_at(&entry, PollOutcome::Unchanged, now),
                now + expected,
                "streak {streak}"
            );
        }

        // A window at 100 % pins every outcome past its reset + 60.
        let maxed = AgentCacheEntry {
            earliest_reset_secs: Some(now + 4000),
            ..AgentCacheEntry::default()
        };
        assert_eq!(
            next_poll_at(&maxed, PollOutcome::Changed, now),
            now + 4000 + 60
        );
        // A reset already past never PULLS the schedule forward.
        let stale_reset = AgentCacheEntry {
            earliest_reset_secs: Some(now - 5000),
            ..AgentCacheEntry::default()
        };
        assert_eq!(
            next_poll_at(&stale_reset, PollOutcome::Changed, now),
            now + 180
        );
    }

    #[test]
    fn poll_due_respects_the_schedule_the_shared_ttl_and_a_refusal() {
        let now = 1_000_000;
        // A never-polled entry is due immediately.
        assert!(poll_due(&AgentCacheEntry::default(), now));

        let entry = AgentCacheEntry {
            fetched_at_secs: now - 200,
            next_poll_at_secs: now - 10,
            ..AgentCacheEntry::default()
        };
        assert!(poll_due(&entry, now));

        // Scheduled, but ANOTHER process fetched 60 s ago: the shared TTL
        // holds this one back (one token budget per machine).
        let shared = AgentCacheEntry {
            fetched_at_secs: now - 60,
            next_poll_at_secs: now - 10,
            ..AgentCacheEntry::default()
        };
        assert!(!poll_due(&shared, now));

        // Not yet scheduled.
        let early = AgentCacheEntry {
            fetched_at_secs: now - 400,
            next_poll_at_secs: now + 10,
            ..AgentCacheEntry::default()
        };
        assert!(!poll_due(&early, now));

        // A refused credential store parks the agent for an hour.
        let denied = AgentCacheEntry {
            credential_denied_until_secs: Some(now + 10),
            ..AgentCacheEntry::default()
        };
        assert!(!poll_due(&denied, now));
        assert!(poll_due(&denied, now + 10));
    }

    #[test]
    fn apply_outcome_keeps_old_numbers_stale_on_failure() {
        let now = 1_000_000;
        let mut entry = AgentCacheEntry::default();
        let windows = vec![window("session", 40, None)];
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(windows.clone()),
            now,
            "T0",
        );
        assert_eq!(entry.usage.as_ref().unwrap().fetched_at, "T0");
        assert!(!entry.usage.as_ref().unwrap().stale);
        assert_eq!(entry.unchanged_streak, 0);
        assert_eq!(entry.next_poll_at_secs, now + 180);

        // The same numbers again: unchanged, slower cadence, same values.
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(windows.clone()),
            now + 200,
            "T1",
        );
        assert_eq!(entry.unchanged_streak, 1);
        // The streak the run just earned is the one that schedules it.
        assert_eq!(entry.next_poll_at_secs, now + 200 + 400);
        assert_eq!(entry.usage.as_ref().unwrap().fetched_at, "T1");

        // A 401: the numbers STAY, flagged stale, and back off 600 s.
        apply_outcome(&mut entry, PollOutcome::Unauthorized, None, now + 600, "T2");
        let usage = entry.usage.as_ref().unwrap();
        assert!(usage.stale);
        assert_eq!(usage.fetched_at, "T1", "the numbers keep their own age");
        assert_eq!(usage.windows, windows);
        assert_eq!(entry.next_poll_at_secs, now + 600 + 600);

        // New numbers clear the flag and reset the streak.
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![window("session", 55, None)]),
            now + 1300,
            "T3",
        );
        assert!(!entry.usage.as_ref().unwrap().stale);
        assert_eq!(entry.unchanged_streak, 0);
    }

    #[test]
    fn apply_outcome_pins_the_next_poll_past_a_maxed_windows_reset() {
        let now = 1_756_000_000;
        let mut entry = AgentCacheEntry::default();
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![
                window("session", 100, Some("2025-08-24T03:26:40Z")),
                window("weekly", 12, Some("2025-08-30T00:00:00Z")),
            ]),
            now,
            "T0",
        );
        // Only the MAXED window's reset counts.
        assert_eq!(entry.earliest_reset_secs, Some(1_756_006_000));
        assert_eq!(entry.next_poll_at_secs, 1_756_006_000 + 60);
    }

    #[test]
    fn windows_hash_ignores_labels_and_tracks_values() {
        let a = vec![window("session", 40, Some("R"))];
        let mut relabelled = a.clone();
        relabelled[0].label = "Five hours".into();
        assert_eq!(windows_hash(&a), windows_hash(&relabelled));
        assert_ne!(windows_hash(&a), windows_hash(&[window("session", 41, Some("R"))]));
        assert_ne!(windows_hash(&a), windows_hash(&[]));
    }

    #[test]
    fn cache_round_trips_and_keeps_unknown_fields_and_entries() {
        let dir = temp_dir("roundtrip");
        let mut cache = UsageCache::default();
        let mut entry = AgentCacheEntry {
            fetched_at_secs: 100,
            next_poll_at_secs: 280,
            unchanged_streak: 2,
            last_windows_hash: "abc".into(),
            ..AgentCacheEntry::default()
        };
        entry
            .extra
            .insert("futureField".into(), Value::String("keep me".into()));
        cache.insert("claude".into(), entry.clone());
        save(&dir, &cache);

        // A row a NEWER build wrote, of a shape this one cannot parse.
        let path = cache_path(&dir);
        let mut object: serde_json::Map<String, Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        object.insert("qwen".into(), Value::String("not an entry".into()));
        std::fs::write(&path, serde_json::to_string(&object).unwrap()).unwrap();

        let reloaded = load(&dir);
        assert_eq!(reloaded.get("claude"), Some(&entry));
        assert_eq!(
            reloaded.get("claude").unwrap().extra["futureField"],
            Value::String("keep me".into())
        );
        // Rewriting must not drop the foreign row.
        save(&dir, &reloaded);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("qwen"), "{raw}");
        assert!(raw.contains("futureField"), "{raw}");

        // A corrupt file reads as empty, never as a panic.
        std::fs::write(&path, "{{{").unwrap();
        assert!(load(&dir).get("claude").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
