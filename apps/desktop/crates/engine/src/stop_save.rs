//! FEED-63: a run a PERSON stopped keeps its uncommitted work as a local WIP
//! commit on its branch (never pushed). Shared by both [`crate::EngineHost`]
//! implementations (the CLI daemon's and the desktop IDE's, FEED-67), so the
//! rule and the log lines are one.

use std::path::Path;

use crate::host::EngineExit;

/// FEED-63: whether a run ended because it was STOPPED from outside, the one
/// end whose uncommitted work gets saved as a WIP commit: the `killed`
/// outcome of a hard stop (a person's Stop through the relay `kill` frame, a
/// host's own forced end). EXP-1292: the `sessions` row carries no
/// `ended_by` any more, so the outcome alone decides — an agent close-out and
/// an account switch end `ended`, a child's exit `exit:<code>`, and none of
/// them is a stop. A spurious save costs nothing (a local commit, never
/// pushed); a lost one loses work.
pub fn stopped_by_person(outcome: &str) -> bool {
    outcome == "killed"
}

/// FEED-63: on a run's exit, save `worktree`'s uncommitted work as a WIP
/// commit when [`stopped_by_person`] says a person stopped it. Logs what it
/// did; a clean tree or a repo-less dir is silent. Blocking git calls: run it
/// off any UI thread (both hosts call it from `EngineHost::on_exit`).
pub fn save_stopped_run_work(exit: &EngineExit, worktree: &Path) {
    if !stopped_by_person(&exit.outcome) {
        return;
    }
    match coding::save_wip_commit(worktree) {
        coding::WipSave::Committed { branch } => log::info!(
            "coding session {}: saved uncommitted work as a WIP commit on {branch} in {}",
            exit.session_id,
            worktree.display()
        ),
        coding::WipSave::Failed(detail) => log::warn!(
            "coding session {}: could not save uncommitted work in {}: {detail}",
            exit.session_id,
            worktree.display()
        ),
        coding::WipSave::Clean | coding::WipSave::NotARepo => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FEED-63: only a hard stop saves a WIP commit.
    #[test]
    fn only_a_hard_stop_saves_the_worktree() {
        assert!(stopped_by_person("killed"));
        assert!(!stopped_by_person("ended"));
        assert!(!stopped_by_person("exit:0"));
        assert!(!stopped_by_person("exit:1"));
    }
}
