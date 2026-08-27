//! EXP-637 — the close-outs of runs whose TAB is still open.
//!
//! When an agent ends its own run (`exponential_sessions_end`), a
//! human-started tab is deliberately NOT closed: the person is standing
//! there, and the summary plus a Resume button are exactly what they want to
//! see. The terminal dock renders that as the "ended strip", so it needs the
//! facts keyed by tab — hence this small global.
//!
//! Entries are cleared when the tab closes (nothing outlives its tab) and
//! whenever the run is resumed into a new session.

use std::collections::HashMap;

use gpui::{App, Global};
use terminal::tab::TabId;

use crate::run_outcome::outcome_label;

/// One ended run's strip payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndedRun {
    /// The `coding_sessions` row that ended — the resume subject.
    pub session_id: String,
    /// `done` / `blocked` / `no_changes`; `None` = ended without one.
    pub outcome: Option<String>,
    /// The agent's own close-out, shown COLLAPSED behind a "Show summary"
    /// toggle (decision 5).
    pub summary: Option<String>,
    /// The run registry still holds a resumable workspace for it.
    pub resumable: bool,
    /// Decision 7: the run cleanup found tracked changes it refused to throw
    /// away — the strip warns about the branch they sit on.
    pub left_dirty: Option<String>,
}

impl EndedRun {
    /// The strip's outcome label ("Done" / "Blocked" / "No changes" /
    /// "Ended") — byte-equal across all four clients.
    pub fn outcome_label(&self) -> &'static str {
        outcome_label(self.outcome.as_deref())
    }
}

#[derive(Default)]
pub struct EndedRuns {
    by_tab: HashMap<TabId, EndedRun>,
}

struct EndedRunsGlobal(EndedRuns);
impl Global for EndedRunsGlobal {}

impl EndedRuns {
    fn global_mut(cx: &mut App) -> &mut EndedRuns {
        if cx.try_global::<EndedRunsGlobal>().is_none() {
            cx.set_global(EndedRunsGlobal(EndedRuns::default()));
        }
        &mut cx.global_mut::<EndedRunsGlobal>().0
    }

    /// Record (or replace) the ended run for `tab`.
    pub fn insert(tab: TabId, run: EndedRun, cx: &mut App) {
        Self::global_mut(cx).by_tab.insert(tab, run);
    }

    /// The ended run for `tab`, if the strip should show one.
    pub fn get(tab: TabId, cx: &App) -> Option<EndedRun> {
        cx.try_global::<EndedRunsGlobal>()?.0.by_tab.get(&tab).cloned()
    }

    /// Forget `tab`'s entry — the tab closed, or its run was resumed.
    pub fn remove(tab: TabId, cx: &mut App) {
        Self::global_mut(cx).by_tab.remove(&tab);
    }

    /// EXP-637: stamp the run cleanup's verdict onto an already-recorded
    /// strip (the cleanup runs in the background, after the strip appears).
    /// A no-op when the tab has no entry.
    pub fn note_left_dirty(tab: TabId, branch: String, cx: &mut App) {
        if let Some(entry) = Self::global_mut(cx).by_tab.get_mut(&tab) {
            entry.left_dirty = Some(branch);
        }
    }

    /// The run's workspace is gone (the cleanup reclaimed it), so Resume can
    /// only fail — drop the offer but KEEP the summary, which is the whole
    /// point of the strip.
    pub fn note_not_resumable(tab: TabId, cx: &mut App) {
        if let Some(entry) = Self::global_mut(cx).by_tab.get_mut(&tab) {
            entry.resumable = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_label_covers_every_contract_value_and_the_null_case() {
        let run = |outcome: Option<&str>| EndedRun {
            session_id: "sess-1".to_string(),
            outcome: outcome.map(str::to_string),
            ..EndedRun::default()
        };
        assert_eq!(run(Some("done")).outcome_label(), "Done");
        assert_eq!(run(Some("blocked")).outcome_label(), "Blocked");
        assert_eq!(run(Some("no_changes")).outcome_label(), "No changes");
        // A run that ended without declaring one, and an unknown future
        // value, both degrade to the neutral label — never a raw wire word.
        assert_eq!(run(None).outcome_label(), "Ended");
        assert_eq!(run(Some("telepathy")).outcome_label(), "Ended");
    }
}
