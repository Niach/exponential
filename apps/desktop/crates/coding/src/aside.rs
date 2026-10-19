//! EXP-1330 — a `/btw` aside: a quick question answered off a FORK of its
//! parent's conversation, so the parent's run never sees it. The server
//! queues it as a child `sessions` row with `aside = true` (claude parents
//! only); the pickup claims it like any row, then lands here instead of the
//! launcher's own start:
//!
//! - no worktree, no branch, no brief turn: it runs in the PARENT's
//!   recorded cwd, resuming the parent's native claude conversation (the
//!   engine adds `--fork-session` and the read-only tool posture off the
//!   record's [`crate::run_registry::ASIDE_OF_KEY`]);
//! - the first prompt is the question itself (the brief's `## Request`);
//! - NO MCP servers: the launcher's resume path skips the team pick, code
//!   mode and computer use for an aside record, and the engine renders its
//!   claude config empty (`engine::aside::ASIDE_MCP_CONFIG`) — a side
//!   question never writes through `exponential` under the person's key;
//! - its end never touches the parent's dir: no run cleanup, and the scratch
//!   purge drops only the aside's own record.
//!
//! A codex parent, or a parent with no run here, is refused (replied, the
//! row ended): the server refuses those asides already.
//!
// TODO(EXP-1323): the aside reuses `launcher::prepare` (its resume path) as
// is; an aside launch option on the launcher's own options belongs to the
// parent session (it owns launcher.rs).

use api::session_device::SessionBrief;
use domain::rows::SessionRow;

use crate::launcher::{PrepareRequest, Prepared, PreparedLaunch, ResumeRun, SessionLaunchRequest};
use crate::run_registry::{RunKind, RunRecord};
use crate::{CodingAgent, CodingDeps, CodingError, LaunchOptions};

/// What [`prepare`] made of an aside row.
pub enum AsidePrepared {
    Ready(Box<PreparedLaunch>),
    /// The aside cannot run here: the reason, replied into its conversation.
    Refused(String),
}

/// The person's question: the brief's `## Request` section (the server
/// composes an aside's brief from its text), else the row's title.
pub fn question(brief: &str, row: &SessionRow) -> Option<String> {
    request_section(brief)
        .or_else(|| row.title.clone())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// The `## Request` section's body: from its heading to the next section
/// heading the brief composer writes (`Action: …`, `Issues`, `Parent`).
fn request_section(brief: &str) -> Option<String> {
    const HEADING: &str = "## Request\n\n";
    let body = brief.strip_prefix(HEADING)?;
    let end = ["\n\n## Action: ", "\n\n## Issues\n", "\n\n## Parent\n"]
        .iter()
        .filter_map(|next| body.find(next))
        .min()
        .unwrap_or(body.len());
    Some(body[..end].to_string())
}

/// The aside's own run record, cut from its parent's: the parent's cwd,
/// clone and branch (it works where the parent works), its NATIVE claude
/// conversation as the resume handle (the newest one: a `/clear` moved it),
/// none of the parent's bookkeeping.
pub fn aside_record(parent: &RunRecord, row: &SessionRow, question: &str) -> Result<RunRecord, String> {
    if parent.agent != CodingAgent::Claude {
        return Err(format!(
            "{} cannot answer an aside. Ask in the session instead.",
            parent.agent.label()
        ));
    }
    let native = parent
        .agent_native_session_id
        .clone()
        .or_else(|| parent.claude_session_id.clone())
        .ok_or_else(|| "Couldn't answer this aside: its session has not started yet.".to_string())?;
    let mut record = RunRecord {
        session_id: row.id.clone(),
        kind: RunKind::Thread,
        title: Some(question.to_string()),
        claude_session_id: Some(native),
        // No ACP handle: a `session/load` would replay the parent's whole
        // transcript into the aside's view; the native resume forks it.
        transport: None,
        acp_session_id: None,
        agent_native_session_id: None,
        acp_child_pid: None,
        host_pid: None,
        recorded_at: crate::run_registry::now_secs(),
        ..parent.clone()
    };
    record.set_pending_consumes(&[]);
    record.set_unreported_end(false);
    record.set_context_base(None);
    record.set_aside_of(&parent.session_id);
    Ok(record)
}

/// Prepare an aside row the pickup claimed. Blocking (the launcher's resume
/// path: doctor, token, the parent's worktree re-check).
pub fn prepare(
    row: &SessionRow,
    brief: &SessionBrief,
    options: &LaunchOptions,
    device_label: &str,
    deps: &CodingDeps,
) -> Result<AsidePrepared, CodingError> {
    let Some(parent_id) = row.parent_id.as_deref() else {
        return Ok(AsidePrepared::Refused("Couldn't answer this aside: it has no session.".to_string()));
    };
    let Some(question) = question(&brief.brief, row) else {
        return Ok(AsidePrepared::Refused("Couldn't answer this aside: it asks nothing.".to_string()));
    };
    let Some(parent) = crate::run_registry::get(&deps.data_dir, parent_id) else {
        return Ok(AsidePrepared::Refused(
            "Couldn't answer this aside: its session has no run on this device.".to_string(),
        ));
    };
    let record = match aside_record(&parent, row, &question) {
        Ok(record) => record,
        Err(reason) => return Ok(AsidePrepared::Refused(reason)),
    };
    let req = SessionLaunchRequest {
        session: row.clone(),
        // The system-append layers only: a fallback past a lost transcript
        // must not send the brief as a turn.
        brief: SessionBrief { brief: String::new(), ..brief.clone() },
        issue_identifiers: parent.issues.clone(),
        resume: Some(ResumeRun { record, prompt: question, message_id: None, reconfigure: None, handoff: None, fresh: false }),
        device_label: device_label.to_string(),
        // The parent's recorded login: no switch.
        options: LaunchOptions { account: None, ..options.clone() },
    };
    match crate::prepare(&PrepareRequest::Session(req), deps)? {
        Prepared::Ready(mut prepared) => {
            // The worktree is the parent's: never cleaned up on the aside's end.
            prepared.run_cleanup = None;
            Ok(AsidePrepared::Ready(Box::new(prepared)))
        }
        Prepared::Disabled(reason) => Ok(AsidePrepared::Refused(reason.message())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> SessionRow {
        SessionRow {
            id: "aside-1".to_string(),
            parent_id: Some("sess-1".to_string()),
            aside: true,
            title: Some("Where is the total computed?".to_string()),
            ..SessionRow::default()
        }
    }

    #[test]
    fn the_question_is_the_briefs_request() {
        let brief = "## Request\n\nWhere is the cart total computed?\n\nAnd why?\n\n## Parent\n\nYou are a child…";
        assert_eq!(
            question(brief, &row()).as_deref(),
            Some("Where is the cart total computed?\n\nAnd why?")
        );
        assert_eq!(question("Wait for the next message.", &row()).as_deref(), Some("Where is the total computed?"));
        let blank = SessionRow { title: None, ..row() };
        assert_eq!(question("", &blank), None);
    }

    #[test]
    fn the_aside_record_forks_the_parents_native_conversation() {
        let mut parent = crate::run_registry::sample_record("sess-1");
        parent.transport = Some(crate::launcher::ACP_TRANSPORT.to_string());
        parent.acp_session_id = Some("acp-1".to_string());
        parent.agent_native_session_id = Some("native-2".to_string());
        parent.host_pid = Some(42);
        parent.set_pending_consumes(&["m-1".to_string()]);
        let record = aside_record(&parent, &row(), "Where?").unwrap();
        assert_eq!(record.session_id, "aside-1");
        assert_eq!(record.aside_of(), Some("sess-1"));
        assert_eq!(record.claude_session_id.as_deref(), Some("native-2"));
        assert_eq!(record.acp_session_id, None);
        assert!(!record.is_acp());
        assert_eq!(record.host_pid, None);
        assert!(record.pending_consumes().is_empty());
        assert_eq!(record.cwd, parent.cwd);
        assert_eq!(record.branch, parent.branch);
        assert_eq!(record.title.as_deref(), Some("Where?"));
        assert!(!parent.is_aside());
    }

    #[test]
    fn codex_and_unstarted_parents_are_refused() {
        let mut codex = crate::run_registry::sample_record("sess-1");
        codex.agent = CodingAgent::Codex;
        assert!(aside_record(&codex, &row(), "Where?").is_err());
        let mut unstarted = crate::run_registry::sample_record("sess-1");
        unstarted.claude_session_id = None;
        assert!(aside_record(&unstarted, &row(), "Where?").is_err());
    }
}
