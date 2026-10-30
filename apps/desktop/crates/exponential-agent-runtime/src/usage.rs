//! Live rate-limit windows (EXP-1426 part 4c, out of `coding::agent_usage`,
//! which re-exports every item here): the window vocabulary, the parsers the
//! adapters read a live session's limits with, and the process-global
//! registry a running session publishes into. No fetch, no credential: the
//! OAuth usage endpoint and the collector stay in `coding`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Unix seconds → the collector's ISO shape (chrono's
/// `to_rfc3339_opts(Millis, true)`, pinned in `coding`'s tests).
fn iso_from_unix_secs(secs: i64) -> Option<String> {
    crate::wire::activity::iso_from_millis(secs.checked_mul(1000)?)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Hard cap on the windows one agent may report — the clients render a list,
/// the server clamps to the same number, and a runaway answer must never
/// become an unbounded jsonb column.
pub const MAX_WINDOWS: usize = 10;

/// One rate-limit window. `key` identifies it across probes (the per-client
/// "which window do I show" preference is stored against it), `label` is the
/// rendered caption, `percent` is 0-100 and `resets_at` is an ISO instant or
/// null.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageWindow {
    pub key: String,
    pub label: String,
    pub percent: u8,
    pub resets_at: Option<String>,
}

/// The locked key/label vocabulary: `session`/`5h`, `weekly`/`Week`, and
/// everything else a per-MODEL window keyed `model:<display lowercased>`.
pub fn claude_window_identity(kind: &str, display: Option<&str>) -> Option<(String, String)> {
    let kind = kind.trim().to_ascii_lowercase();
    match kind.as_str() {
        "session" | "five_hour" | "5h" => {
            Some(("session".to_string(), "5h".to_string()))
        }
        "weekly" | "weekly_all" | "seven_day" | "week" => {
            Some(("weekly".to_string(), "Week".to_string()))
        }
        _ => {
            let display = display.map(str::to_string).or_else(|| {
                (!kind.is_empty()).then(|| kind.clone())
            })?;
            let key = format!("model:{}", display.to_ascii_lowercase());
            Some((clamp_key(key), clamp_label(display)))
        }
    }
}

fn clamp_key(key: String) -> String {
    key.chars().take(64).collect()
}

fn clamp_label(label: String) -> String {
    label.chars().take(32).collect()
}

/// A 0-100 percentage off any of the field names the answers have used.
pub fn read_percent(entry: &Value) -> Option<u8> {
    for field in ["utilization", "used_percent", "usedPercent", "percent"] {
        if let Some(raw) = entry.get(field).and_then(Value::as_f64) {
            return Some(raw.round().clamp(0.0, 100.0) as u8);
        }
    }
    None
}

/// An ISO reset stamp off either an ISO string or a unix-seconds number.
pub fn read_reset(entry: &Value) -> Option<String> {
    for field in ["resets_at", "resetsAt", "reset_at", "resetAt"] {
        match entry.get(field) {
            Some(Value::String(stamp)) if !stamp.trim().is_empty() => {
                return Some(stamp.trim().to_string())
            }
            Some(Value::Number(number)) => {
                if let Some(secs) = number.as_i64() {
                    return iso_from_unix_secs(secs);
                }
            }
            _ => {}
        }
    }
    None
}

/// EXP-819 — the top-level `rateLimitType` + `utilization` (+ `resetsAt`)
/// triple a `rate_limit_event` names for its LIMITING window: the fallback
/// [`parse_claude_rate_limit_windows`] reads when the frame carries no
/// `unifiedWindows` map at all.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClaudeLimitingWindow<'a> {
    /// `five_hour` | `seven_day` | `seven_day_opus` | …
    pub kind: &'a str,
    /// A FRACTION of the window (0-1), the same scale as `unifiedWindows`.
    pub utilization: f64,
    /// Unix seconds.
    pub resets_at_secs: Option<i64>,
}

/// EXP-819 — the windows a LIVE claude session's `rate_limit_event`
/// (`rate_limit_info.unifiedWindows`, EXP-784) reports, in the vocabulary
/// `coding::agent_usage::parse_claude_usage` locks: `five_hour` → `session`, `seven_day` →
/// `weekly`.
///
/// Measured off the CLI's own schema (2.1.267): three OPTIONAL arms —
/// `five_hour`, `seven_day`, `seven_day_overage_included` — each
/// `{ utilization, resetsAt }`, `utilization` a FRACTION of the window (0-1;
/// above 1 when usage legitimately runs past the cap) and `resetsAt` unix
/// SECONDS, read off the `anthropic-ratelimit-unified-*` response headers on
/// every turn (an older build, 2.1.263, carried the fraction alone). The
/// overage-included arm is a per-MODEL bucket whose endpoint counterpart is
/// unmeasured, so it is deliberately not published: a guessed key would sit
/// beside the endpoint's model-scoped weekly as a duplicate row. Everything
/// the frame does not carry stays what the endpoint last said — see
/// [`merge_live_windows`].
///
/// `limiting` is the frame's top-level triple, read only for a window the
/// map does not already carry (and never for a model-scoped kind, for the
/// same duplicate-row reason).
pub fn parse_claude_rate_limit_windows(
    unified_windows: Option<&Value>,
    limiting: Option<ClaudeLimitingWindow<'_>>,
) -> Vec<UsageWindow> {
    let mut windows: Vec<UsageWindow> = Vec::new();
    if let Some(unified) = unified_windows.and_then(Value::as_object) {
        for field in ["five_hour", "seven_day"] {
            let Some(entry) = unified.get(field) else {
                continue;
            };
            let Some(utilization) = entry.get("utilization").and_then(Value::as_f64) else {
                continue;
            };
            let Some((key, label)) = claude_window_identity(field, None) else {
                continue;
            };
            windows.push(UsageWindow {
                key,
                label,
                percent: fraction_percent(utilization),
                resets_at: read_reset(entry),
            });
        }
    }
    if let Some(limiting) = limiting {
        if let Some((key, label)) = claude_window_identity(limiting.kind, None)
            .filter(|(key, _)| !key.starts_with("model:"))
        {
            if !windows.iter().any(|window| window.key == key) {
                windows.push(UsageWindow {
                    key,
                    label,
                    percent: fraction_percent(limiting.utilization),
                    resets_at: limiting.resets_at_secs.and_then(iso_from_unix_secs),
                });
            }
        }
    }
    windows.truncate(MAX_WINDOWS);
    windows
}

/// A 0-1 fraction (the rate-limit headers' scale) as the wire's 0-100.
fn fraction_percent(fraction: f64) -> u8 {
    if !fraction.is_finite() {
        return 0;
    }
    (fraction * 100.0).round().clamp(0.0, 100.0) as u8
}

/// EXP-819 — a live frame's windows laid OVER the last full report, by key.
///
/// A key the frame carries takes the frame's percent, and its reset when the
/// frame names one (the report's stays otherwise); every other window — the
/// endpoint's model-scoped weekly, its credits — keeps the report's numbers
/// rather than vanishing; a key the report never had is appended. The order
/// is the report's, so the bar never reshuffles between a poll and a turn.
///
/// EXP-881: laying over is only ever sound when the frame is NEWER than the
/// report — the caller (`coding::agent_usage::live_probe`) checks that against
/// `endpoint_fetched_at_secs` first. This function merges what it is given
/// and asks no questions about time.
pub fn merge_live_windows(reported: &[UsageWindow], live: &[UsageWindow]) -> Vec<UsageWindow> {
    let mut merged: Vec<UsageWindow> = reported.to_vec();
    for window in live {
        match merged.iter_mut().find(|slot| slot.key == window.key) {
            Some(slot) => {
                slot.percent = window.percent;
                if window.resets_at.is_some() {
                    slot.resets_at = window.resets_at.clone();
                }
            }
            None => merged.push(window.clone()),
        }
    }
    merged.truncate(MAX_WINDOWS);
    merged
}

/// `account/rateLimits/read` → windows. Codex reports up to two windows by
/// DURATION rather than by name, so the labels derive from the duration:
/// 300 min = the 5h session, 10080 = the week, 43200 = the month, anything
/// else keeps its raw minutes. A credits BALANCE is not a window (it has no
/// percentage and no reset) and is deliberately dropped.
pub fn parse_codex_rate_limits(value: &Value) -> Vec<UsageWindow> {
    let limits = value
        .get("rateLimits")
        .or_else(|| value.get("rate_limits"))
        .unwrap_or(value);
    let mut windows = Vec::new();
    for field in ["primary", "secondary"] {
        let Some(entry) = limits.get(field) else {
            continue;
        };
        let Some(percent) = read_percent(entry) else {
            continue;
        };
        let minutes = ["windowDurationMins", "window_duration_mins", "windowMinutes", "window_minutes", "durationMins"]
            .into_iter()
            .find_map(|name| entry.get(name).and_then(Value::as_i64));
        let (key, label) = codex_window_identity(minutes, field);
        windows.push(UsageWindow {
            key,
            label,
            percent,
            resets_at: read_reset(entry),
        });
    }
    windows.truncate(MAX_WINDOWS);
    windows
}

fn codex_window_identity(minutes: Option<i64>, field: &str) -> (String, String) {
    match minutes {
        Some(300) => ("session".to_string(), "5h".to_string()),
        Some(10080) => ("weekly".to_string(), "Week".to_string()),
        Some(43200) => ("43200".to_string(), "Month".to_string()),
        Some(mins) => (mins.to_string(), format!("{mins}m")),
        // No duration reported: key by the slot so primary and secondary
        // never collide (a shared key would break per-window selection).
        None => (
            field.to_string(),
            match field {
                "primary" => "Primary".to_string(),
                _ => "Secondary".to_string(),
            },
        ),
    }
}

/// EXP-754 — the rate-limit windows a LIVE agent session already knows.
///
/// One machine runs ONE `codex app-server` per session, and that connection
/// is pushed `account/rateLimits/updated` on every turn. Those numbers are
/// fresher than anything `coding::agent_usage::collect_if_due` could fetch, and fetching them
/// itself costs a SECOND app-server spawn against the same account (the very
/// contention `coding::usage_cache::SHARED_TTL_SECS` exists to bound). So
/// the adapter publishes here and the collector reads here.
///
/// EXP-819: a claude session publishes too — its stream-json prints a
/// `rate_limit_event` per turn whose `unifiedWindows` carry the session and
/// weekly windows ([`super::parse_claude_rate_limit_windows`]). That frame
/// is a SUBSET of the OAuth usage endpoint's report (no model-scoped weekly,
/// no credits), so the collector lays it over the endpoint's last report by
/// key ([`super::merge_live_windows`]) instead of replacing it.
///
/// EXP-909: the registry is keyed by `(agent, profile)` — the LOGIN the run
/// spends, not just the agent — because a run on one account must never
/// move another's numbers. A transcript
/// REPLAY attaches nothing at all: it spends no tokens.
///
/// Process-global on purpose: publisher (the engine's codex and claude
/// adapters) and reader (the desktop's device-sync beat, the daemon's
/// device worker) live in the same process. A SIBLING process still shares the on-disk
/// `coding::usage_cache`, which this path writes exactly like a fetch would.
///
/// The runtime owns the registry (EXP-1426 part 4c) so its adapters publish
/// with a plain function call; `coding`'s collector reads it here.
pub mod live {
    use std::collections::HashMap;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::UsageWindow;
    use crate::agent::CodingAgent;

    /// What one LOGIN's live sessions have published on this machine.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct LiveUsage {
        /// The last windows a session reported (empty = a session is
        /// attached but has not heard a rate-limit frame yet).
        pub windows: Vec<UsageWindow>,
        /// Unix seconds of the last [`publish`].
        pub updated_at_secs: Option<u64>,
        /// How many sessions are attached RIGHT NOW to this login. EXP-881:
        /// `> 0` only WIDENS how long a frame is trusted (a live run refreshes
        /// its own numbers every turn, so a frame within the endpoint's own
        /// cadence is as good as a fetch); it never makes an OLD frame
        /// current. An idle turn-less session publishes nothing, and its last
        /// frame ages out exactly like a detached one's.
        pub sessions: usize,
    }

    /// `(agent, profile)` — the profile is `coding::agent_profiles::profile_id`
    /// of the run's launch account.
    type LiveKey = (CodingAgent, String);

    static LIVE: OnceLock<Mutex<HashMap<LiveKey, LiveUsage>>> = OnceLock::new();

    fn live() -> MutexGuard<'static, HashMap<LiveKey, LiveUsage>> {
        let lock = LIVE.get_or_init(|| Mutex::new(HashMap::new()));
        match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// A live session for `agent` on `profile` has started. Hold the guard
    /// for the run.
    #[must_use = "dropping the guard immediately detaches the session"]
    pub fn attach(agent: CodingAgent, profile: &str) -> Attached {
        live()
            .entry((agent, profile.to_string()))
            .or_default()
            .sessions += 1;
        Attached(agent, profile.to_string())
    }

    /// One attached session. `Drop` is the release path, so a panicked or
    /// abandoned run detaches itself; an owner that knows the session ended
    /// simply drops it earlier.
    pub struct Attached(CodingAgent, String);

    impl Drop for Attached {
        fn drop(&mut self) {
            let mut live = live();
            let entry = live.entry((self.0, self.1.clone())).or_default();
            // Saturating: a double release must never wrap to usize::MAX and
            // pin the numbers "live" forever.
            entry.sessions = entry.sessions.saturating_sub(1);
        }
    }

    /// A session reported new windows for the login it runs on. Latest-wins,
    /// like every other usage slot; the stamp is what decides whether the
    /// frame still answers (`coding::agent_usage::live_probe`).
    pub fn publish(agent: CodingAgent, profile: &str, windows: Vec<UsageWindow>) {
        let mut live = live();
        let entry = live.entry((agent, profile.to_string())).or_default();
        entry.windows = windows;
        entry.updated_at_secs = Some(super::now_secs());
    }

    /// What this machine's sessions on `profile` last said about `agent`, if
    /// anything. A run on another login is INVISIBLE here — that is the point
    /// of the key.
    pub fn snapshot(agent: CodingAgent, profile: &str) -> Option<LiveUsage> {
        live().get(&(agent, profile.to_string())).cloned()
    }

    /// Tests only: the registry is process-global, so a test that asserts on
    /// it starts from empty (and takes the suite's own lock to stay so).
    /// `pub` because the hosts' suites (`coding`, `engine`) reset it too.
    #[doc(hidden)]
    pub fn reset() {
        live().clear();
    }
}
