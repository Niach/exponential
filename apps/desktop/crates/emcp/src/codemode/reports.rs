//! EXP-1279: the side channel from `exec` to the host's activity feed.
//!
//! An exec's call log rides its result's `_meta` (`script-card.json`
//! `metaKey`…), and the agent CLIs do not hand that to the engine alike:
//! claude's stream-json gives the adapter an MCP answer's content blocks,
//! not its `_meta` (which the CLI keeps only up to 8 KB, under a full log),
//! while codex forwards it. The
//! code-mode server runs in the SAME process as the engine that maps the
//! run's activity, so every finished exec also files its log here, keyed by
//! the run and the script it ran, and the engine takes it off when the exec
//! call settles ([`take_report`]). Unclaimed reports are bounded per run and
//! dropped with the run's grant.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};

use crate::codemode::runtime::CallRecord;

/// One finished exec, as the activity feed shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptReport {
    /// Every call the script made (the log stops at its cap, this does not).
    pub count: usize,
    pub elapsed_ms: u64,
    pub calls: Vec<CallRecord>,
}

/// Reports a run's engine never claimed (an exec whose result never settled
/// in the feed) stop piling up here.
const PER_RUN_MAX: usize = 32;

/// Run → its unclaimed reports, oldest first, each under its script's hash.
static REPORTS: LazyLock<Mutex<HashMap<String, VecDeque<(u64, ScriptReport)>>>> = LazyLock::new(Mutex::default);

fn script_key(script: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    script.hash(&mut hasher);
    hasher.finish()
}

/// File the log of an exec of `script` the run `session_id` just finished
/// (the server's own call; public so a host's tests can stand in for it).
pub fn publish(session_id: &str, script: &str, report: ScriptReport) {
    let mut reports = REPORTS.lock().unwrap();
    let queue = reports.entry(session_id.to_string()).or_default();
    if queue.len() >= PER_RUN_MAX {
        queue.pop_front();
    }
    queue.push_back((script_key(script), report));
}

/// The log of the run's oldest unclaimed exec of exactly `script` (the
/// exec call's own `script` input), removed. Two runs of the same script
/// settle in the order they finished.
pub fn take_report(session_id: &str, script: &str) -> Option<ScriptReport> {
    let key = script_key(script);
    let mut reports = REPORTS.lock().unwrap();
    let queue = reports.get_mut(session_id)?;
    let at = queue.iter().position(|(hash, _)| *hash == key)?;
    let (_, report) = queue.remove(at)?;
    if queue.is_empty() {
        reports.remove(session_id);
    }
    Some(report)
}

/// The run ended: its unclaimed reports go with it.
pub(crate) fn forget(session_id: &str) {
    REPORTS.lock().unwrap().remove(session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(count: usize) -> ScriptReport {
        ScriptReport { count, elapsed_ms: 5, calls: Vec::new() }
    }

    #[test]
    fn a_report_is_taken_once_by_its_run_and_script_in_finish_order() {
        publish("rep-a", "return 1", report(1));
        publish("rep-a", "return 2", report(2));
        publish("rep-a", "return 1", report(3));
        assert_eq!(take_report("rep-b", "return 1"), None, "another run's");
        assert_eq!(take_report("rep-a", "return 3"), None, "another script's");
        assert_eq!(take_report("rep-a", "return 1"), Some(report(1)));
        assert_eq!(take_report("rep-a", "return 1"), Some(report(3)));
        assert_eq!(take_report("rep-a", "return 1"), None);
        assert_eq!(take_report("rep-a", "return 2"), Some(report(2)));
        assert!(!REPORTS.lock().unwrap().contains_key("rep-a"), "an emptied run leaves no entry");
    }

    #[test]
    fn unclaimed_reports_are_bounded_and_forgotten_with_the_run() {
        for count in 0..PER_RUN_MAX + 3 {
            publish("rep-c", "x", report(count));
        }
        assert_eq!(REPORTS.lock().unwrap()["rep-c"].len(), PER_RUN_MAX);
        assert_eq!(take_report("rep-c", "x"), Some(report(3)), "the oldest went first");
        forget("rep-c");
        assert_eq!(take_report("rep-c", "x"), None);
    }
}
