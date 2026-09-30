//! FEED-63: a run a PERSON stopped keeps its uncommitted work as a local WIP
//! commit on its branch (never pushed). Shared by both [`crate::EngineHost`]
//! implementations (the CLI daemon's and the desktop IDE's, FEED-67), so the
//! rule and the log lines are one.

use std::path::Path;

use crate::host::EngineExit;

/// FEED-63: whether a run ended because a PERSON stopped it, the one end
/// whose uncommitted work gets saved as a WIP commit. The engine outcome is
/// `killed` for every hard stop (a relay `kill` frame, the kill poll's
/// `Now`), a merge end and a withdrawn share included, so the row's
/// `ended_by` (echoed back by the end call, which keeps an existing end)
/// decides: `user` (web/mobile Stop, MCP `exponential_sessions_kill`) or
/// `client` (this host's own forced end). An agent close-out and an
/// account switch end `ended`; a merge/system end is not a stop.
///
/// FEED-67: a FAILED end call (offline, a 5xx) still saves — the outcome is
/// a hard stop and the save is a local commit that is never pushed, so a
/// spurious one (a merge end whose echo was lost) costs nothing while a lost
/// one loses work. A replay skips the end call (`None`): it spawned nothing
/// and has no worktree work to keep.
pub fn stopped_by_person(
    outcome: &str,
    end: Option<&Result<api::coding_sessions::CodingSession, api::ApiError>>,
) -> bool {
    if outcome != "killed" {
        return false;
    }
    match end {
        Some(Ok(row)) => matches!(
            row.ended_by.as_deref(),
            Some(domain::contract::CODING_SESSION_ENDED_BY_USER)
                | Some(domain::contract::CODING_SESSION_ENDED_BY_CLIENT)
        ),
        Some(Err(_)) => true,
        None => false,
    }
}

/// FEED-63: on a run's exit, save `worktree`'s uncommitted work as a WIP
/// commit when [`stopped_by_person`] says a person stopped it. Logs what it
/// did; a clean tree or a repo-less dir is silent. Blocking git calls: run it
/// off any UI thread (both hosts call it from `EngineHost::on_exit`).
pub fn save_stopped_run_work(exit: &EngineExit, worktree: &Path) {
    if !stopped_by_person(&exit.outcome, exit.end.as_ref()) {
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

    /// An ended row that also says WHO ended it.
    fn ended_by(ended_by: &str) -> api::coding_sessions::CodingSession {
        serde_json::from_value(serde_json::json!({
            "id": "sess-1",
            "userId": "me",
            "status": "ended",
            "endedBy": ended_by,
        }))
        .expect("fixture decodes")
    }

    /// FEED-63: only a person's hard stop saves a WIP commit.
    #[test]
    fn only_a_person_stop_saves_the_worktree() {
        let end = |by: &str| Some(Ok(ended_by(by)));
        assert!(stopped_by_person("killed", end("user").as_ref()));
        assert!(stopped_by_person("killed", end("client").as_ref()));
        assert!(!stopped_by_person("killed", end("merge").as_ref()));
        assert!(!stopped_by_person("killed", end("system").as_ref()));
        assert!(!stopped_by_person("ended", end("user").as_ref()));
        assert!(!stopped_by_person("ended", end("agent").as_ref()));
        assert!(!stopped_by_person("exit:0", end("client").as_ref()));
        assert!(!stopped_by_person("killed", None));
    }

    /// FEED-67: the end call failing (offline, a 5xx) must not lose the
    /// stopped run's work — a `killed` outcome with a failed end saves; an
    /// agent close-out whose end failed still does not.
    #[test]
    fn a_failed_end_call_still_saves_a_killed_run() {
        let failed = Some(Err(api::ApiError::UpgradeRequired));
        assert!(stopped_by_person("killed", failed.as_ref()));
        assert!(!stopped_by_person("ended", failed.as_ref()));
        assert!(!stopped_by_person("exit:1", failed.as_ref()));
    }
}
