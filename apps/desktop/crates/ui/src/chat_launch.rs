//! EXP-825 — the Home composer's launch logic, kept free of gpui so the
//! rules stay testable: what the submit says, when free text is required,
//! and the one `sessions.send` payload a start posts (EXP-1294).
//! `chat_screen.rs` owns the state and calls in here.
//!
//! The contract (shared ×4): no subject → a plain session, 1..N issues → one
//! session linking them all, an action → "Run action"; free text is the
//! prompt with no subject (required) and optional additional instructions
//! otherwise.

use std::collections::HashSet;

use coding::LaunchOptions;

use crate::picker::issue_picker::{IssueRow, MAX_ISSUES_PER_RUN};

/// What the composer is about to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SubjectKind {
    /// No issue, no action: a plain session on the prompt.
    Chat,
    /// `count` checked issues — ONE session linking them all.
    Issues { count: usize },
    /// One picked action.
    Action,
    /// NEW-3: one picked team agent — a message into the caller's DM with it
    /// (`sessions.send({agentId})`), on the agent's own device and login.
    Agent,
}

/// EXP-1019/EXP-1037 — the composer's HEADLINE verb, the launcher's main
/// element above the text field: `Implement` beside the issue chips; NEW-2/3
/// (web `launchHeadlineVerb`): a prompt's or an agent's chip IS the headline. The strings are the shared contract's
/// (`domain::contract::COMPOSER_UI_*_HEADLINE`, ×4), never local literals.
pub(crate) fn headline(subject: &SubjectKind) -> &'static str {
    match subject {
        SubjectKind::Chat => domain::contract::COMPOSER_UI_CHAT_HEADLINE,
        SubjectKind::Issues { .. } => domain::contract::COMPOSER_UI_IMPLEMENT_HEADLINE,
        SubjectKind::Action | SubjectKind::Agent => "",
    }
}

/// The submit's tooltip.
pub(crate) fn submit_label(subject: &SubjectKind) -> String {
    match subject {
        SubjectKind::Chat => "Start session".to_string(),
        SubjectKind::Issues { count: 1 } => "Start coding".to_string(),
        SubjectKind::Issues { count } => format!("Start coding · {count}"),
        // NEW-2 (web `submitLabelFor`): a prompt starts a session.
        SubjectKind::Action => "Start session".to_string(),
        SubjectKind::Agent => "Send".to_string(),
    }
}

/// The "Type a message." blocker: a plain session has nothing else to send
/// (an image alone counts); every subject takes the text as optional
/// additional instructions.
pub(crate) fn text_blocker(subject: &SubjectKind, text: &str, image_count: usize) -> Option<&'static str> {
    match subject {
        SubjectKind::Chat | SubjectKind::Agent if text.trim().is_empty() && image_count == 0 => {
            Some(TYPE_A_MESSAGE)
        }
        _ => None,
    }
}

/// EXP-862 — which blocker earns a NOTE under the composer.
///
/// "Type a message." is the empty composer telling the reader to use the
/// composer they are looking at. It still BLOCKS (the send stays disabled),
/// it just stops being printed. Every other blocker says something the reader
/// cannot see for themselves.
pub(crate) fn note_for_blocker(blocker: Option<&str>) -> Option<&str> {
    blocker.filter(|reason| *reason != TYPE_A_MESSAGE)
}

/// D8: the blocker while the team's actions have not loaded. The note
/// renders it as the INLINE status (spinner + `Loading`), never a
/// "Loading…" sentence.
pub(crate) const ACTIONS_LOADING: &str = crate::feedback::WORD_LOADING;

/// The empty-draft blocker (see [`note_for_blocker`]).
const TYPE_A_MESSAGE: &str = "Type a message.";

/// The issue-count gates: at least one, at most [`MAX_ISSUES_PER_RUN`].
pub(crate) fn issue_count_blocker(count: usize) -> Option<String> {
    if count == 0 {
        return Some("Select at least one issue.".to_string());
    }
    if count > MAX_ISSUES_PER_RUN {
        return Some(format!("At most {MAX_ISSUES_PER_RUN} issues per session."));
    }
    None
}

/// What a start sends besides the run's options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartSubject {
    pub(crate) issue_ids: Vec<String>,
    /// NEW-2: a saved prompt (its body plus the text = the first message).
    pub(crate) prompt_id: Option<String>,
    pub(crate) repository_id: Option<String>,
}

/// EXP-1294 — THE start: one `sessions.send` for a new root in the ACTIVE
/// team on `device_id`, the composer text (trimmed, omitted when blank) and
/// the uploaded attachments, plus the run's options as picked. Blank
/// model/effort are the "CLI default" sentinel and are omitted.
///
/// EXP-849: the account pick names a profile on the TARGET machine (the
/// picker only offers that device's own logins), so `None` — the device's
/// last used login — is the only safe default.
pub(crate) fn send_input(
    team_id: &str,
    device_id: &str,
    options: &LaunchOptions,
    subject: StartSubject,
    text: &str,
    attachment_ids: Vec<String>,
) -> api::sessions::SendInput {
    let text = text.trim();
    api::sessions::SendInput {
        team_id: Some(team_id.to_string()),
        device_id: Some(device_id.to_string()),
        cli: Some(options.agent.id().to_string()),
        account: options.account.clone(),
        model: Some(options.model.clone()).filter(|model| !model.is_empty()),
        effort: Some(options.effort.clone()).filter(|effort| !effort.is_empty()),
        // S-3: no plan toggle (contract launchKeys has no planMode): never sent.
        plan_mode: None,
        // EXP-1249: `None` = the target's own switch.
        computer_use: options.computer_use,
        ultracode: options.ultracode.then_some(true),
        mcp_server_ids: Some(options.mcp_server_ids.clone()).filter(|ids| !ids.is_empty()),
        text: (!text.is_empty()).then(|| text.to_string()),
        attachment_ids: Some(attachment_ids).filter(|ids| !ids.is_empty()),
        issue_ids: Some(subject.issue_ids).filter(|ids| !ids.is_empty()),
        prompt_id: subject.prompt_id,
        repository_id: subject.repository_id,
        ..Default::default()
    }
}

/// NEW-3 — a message to a team AGENT: one `sessions.send` naming the agent,
/// which lands in the caller's own conversation with it (created on first
/// use). The DM runs on the agent's device and login, so no runner pick
/// rides (web `buildSendInput`'s agent arm).
pub(crate) fn agent_send_input(
    team_id: &str,
    agent_id: &str,
    text: &str,
    attachment_ids: Vec<String>,
) -> api::sessions::SendInput {
    let text = text.trim();
    api::sessions::SendInput {
        team_id: Some(team_id.to_string()),
        agent_id: Some(agent_id.to_string()),
        text: (!text.is_empty()).then(|| text.to_string()),
        attachment_ids: Some(attachment_ids).filter(|ids| !ids.is_empty()),
        ..Default::default()
    }
}

/// VAPP-110: a message to an APP (web `AgentComposer`'s app arm): it lands
/// in the caller's own editor session with it, created on first use on
/// `device_id` (the caller's default device).
pub(crate) fn app_send_input(
    team_id: &str,
    app_id: &str,
    device_id: Option<&str>,
    text: &str,
    attachment_ids: Vec<String>,
) -> api::sessions::SendInput {
    let text = text.trim();
    api::sessions::SendInput {
        team_id: Some(team_id.to_string()),
        app_id: Some(app_id.to_string()),
        device_id: device_id.map(str::to_string),
        text: (!text.is_empty()).then(|| text.to_string()),
        attachment_ids: Some(attachment_ids).filter(|ids| !ids.is_empty()),
        ..Default::default()
    }
}

/// One issue's `repositories.forIssue` probe state.
#[derive(Clone, Debug)]
pub(crate) enum RepoState {
    Loading,
    /// `Ready(None)` = no repository linked: the issue stays checked and
    /// the repository gate names it (#N-15).
    Ready(Option<api::repositories::IssueRepository>),
    /// Transport failure — the issue can't resolve a repo; it stays checked
    /// and the gate says why (#N-15).
    Error(String),
}

/// The one-repository / one-base-branch gate over the checked issues
/// (EXP-712: a session cuts ONE branch, so every issue must resolve to the
/// same repository AND the same base). `None` = fine; a checked issue whose
/// probe has not answered blocks with "Checking linked repositories…".
pub(crate) fn repo_blocker(
    rows: &[IssueRow],
    checked: &HashSet<String>,
    repos: &std::collections::HashMap<String, RepoState>,
) -> Option<String> {
    let mut repo: Option<&str> = None;
    let mut bases: Vec<Option<String>> = Vec::new();
    for row in rows {
        if !checked.contains(&row.issue_id) {
            continue;
        }
        match repos.get(&row.issue_id) {
            Some(RepoState::Ready(Some(resolved))) => {
                match repo {
                    None => repo = Some(&resolved.repository_id),
                    Some(existing) if existing == resolved.repository_id => {}
                    Some(_) => return Some("One repository per session. Deselect the others.".into()),
                }
                bases.push(
                    Some(resolved.default_branch.clone())
                        .filter(|branch| !branch.trim().is_empty()),
                );
            }
            // #N-15: say WHICH issue cannot start and why.
            Some(RepoState::Ready(None)) => {
                return Some(format!("{} has no repository linked.", row.identifier));
            }
            Some(RepoState::Error(err)) => {
                return Some(format!("Couldn't check {}'s repository: {err}", row.identifier));
            }
            _ => return Some("Checking linked repositories…".into()),
        }
    }
    if let Some((first, second)) = crate::repo_resolver::batch_branch_conflict(&bases) {
        return Some(format!(
            "All issues in a session must share one base branch ({first} vs {second})."
        ));
    }
    None
}

/// The repository the checked issues resolved to (the gate above already
/// made it ONE), so the start names it.
pub(crate) fn issues_repository(
    checked: &HashSet<String>,
    repos: &std::collections::HashMap<String, RepoState>,
) -> Option<String> {
    checked.iter().find_map(|issue_id| match repos.get(issue_id) {
        Some(RepoState::Ready(Some(resolved))) => Some(resolved.repository_id.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use coding::CodingAgent;

    fn issue_row(id: &str, identifier: &str) -> IssueRow {
        IssueRow {
            issue_id: id.to_string(),
            board_id: "b".to_string(),
            identifier: identifier.to_string(),
            title: "t".to_string(),
            description: None,
            created_at: None,
            updated_at: None,
            status: domain::enums::IssueStatus::Backlog,
            priority: domain::enums::IssuePriority::None,
            resolved: domain::statuses::constructed_default(domain::enums::IssueStatus::Backlog),
            state_hint: None,
        }
    }

    /// #N-15: a seeded issue whose repository cannot resolve stays the
    /// subject, and the gate names it instead of "Select at least one
    /// issue." (the old probe unchecked it and left the chip up).
    #[test]
    fn the_repository_gate_names_an_issue_that_cannot_resolve() {
        let rows = vec![issue_row("i1", "APP-3")];
        let checked: HashSet<String> = ["i1".to_string()].into_iter().collect();
        let mut repos = std::collections::HashMap::new();
        assert_eq!(issue_count_blocker(checked.len()), None);
        repos.insert("i1".to_string(), RepoState::Loading);
        assert_eq!(repo_blocker(&rows, &checked, &repos).as_deref(), Some("Checking linked repositories…"));
        repos.insert("i1".to_string(), RepoState::Ready(None));
        assert_eq!(repo_blocker(&rows, &checked, &repos).as_deref(), Some("APP-3 has no repository linked."));
        repos.insert("i1".to_string(), RepoState::Error("HTTP 412".to_string()));
        assert_eq!(
            repo_blocker(&rows, &checked, &repos).as_deref(),
            Some("Couldn't check APP-3's repository: HTTP 412")
        );
        repos.insert(
            "i1".to_string(),
            RepoState::Ready(Some(api::repositories::IssueRepository {
                repository_id: "r".to_string(),
                full_name: "acme/app".to_string(),
                default_branch: "main".to_string(),
            })),
        );
        assert_eq!(repo_blocker(&rows, &checked, &repos), None);
    }

    fn options() -> LaunchOptions {
        LaunchOptions {
            agent: CodingAgent::Claude,
            model: "opus".to_string(),
            effort: String::new(),
            ultracode: false,
            plan_mode: true,
            subagent_model: String::new(),
            mcp_server_ids: Vec::new(),
            account: None,
            computer_use: None,
        }
    }


    /// EXP-1019: the headline verb per subject kind — the contract's strings.
    #[test]
    fn headlines_follow_the_subject_kind() {
        assert_eq!(headline(&SubjectKind::Chat), "Ask the agent");
        assert_eq!(headline(&SubjectKind::Issues { count: 1 }), "Implement");
        assert_eq!(headline(&SubjectKind::Issues { count: 4 }), "Implement");
        assert_eq!(headline(&SubjectKind::Action), "");
        assert_eq!(headline(&SubjectKind::Agent), "");
    }

    /// EXP-1294: N issues are ONE session — never a "batch".
    #[test]
    fn submit_labels_follow_the_subject() {
        assert_eq!(submit_label(&SubjectKind::Chat), "Start session");
        assert_eq!(submit_label(&SubjectKind::Issues { count: 1 }), "Start coding");
        assert_eq!(submit_label(&SubjectKind::Issues { count: 3 }), "Start coding · 3");
        assert_eq!(submit_label(&SubjectKind::Action), "Start session");
        assert_eq!(submit_label(&SubjectKind::Agent), "Send");
    }

    /// Free text is required with no subject (an image alone counts),
    /// optional additional instructions everywhere else.
    #[test]
    fn text_is_required_only_without_a_subject() {
        assert_eq!(text_blocker(&SubjectKind::Chat, "  ", 0), Some(TYPE_A_MESSAGE));
        assert_eq!(text_blocker(&SubjectKind::Chat, "", 1), None);
        assert_eq!(text_blocker(&SubjectKind::Chat, "hi", 0), None);
        assert_eq!(text_blocker(&SubjectKind::Action, "", 0), None);
        // NEW-3: a DM needs something to say.
        assert_eq!(text_blocker(&SubjectKind::Agent, " ", 0), Some(TYPE_A_MESSAGE));
        assert_eq!(text_blocker(&SubjectKind::Agent, "", 1), None);
        assert_eq!(text_blocker(&SubjectKind::Issues { count: 2 }, "", 0), None);
        assert_eq!(issue_count_blocker(0).as_deref(), Some("Select at least one issue."));
        assert!(issue_count_blocker(1).is_none());
        assert!(issue_count_blocker(MAX_ISSUES_PER_RUN).is_none());
        assert!(issue_count_blocker(MAX_ISSUES_PER_RUN + 1).is_some());
    }

    /// EXP-862: the empty composer's own blocker is never printed under the
    /// composer; every other reason still is.
    #[test]
    fn the_empty_draft_blocker_earns_no_note() {
        assert_eq!(note_for_blocker(None), None);
        assert_eq!(note_for_blocker(text_blocker(&SubjectKind::Chat, "", 0)), None);
        assert!(text_blocker(&SubjectKind::Chat, "", 0).is_some());
        assert_eq!(
            note_for_blocker(Some("Select at least one issue.")),
            Some("Select at least one issue.")
        );
    }

    /// NEW-3: a message to an agent names the team, the agent, the text and
    /// the uploads, and NO runner pick (the DM runs on the agent's device).
    #[test]
    fn the_agent_send_input_carries_no_runner_picks() {
        let input = agent_send_input("team-1", "agent-1", "  hello \n", vec!["att-1".into()]);
        assert_eq!(input.team_id.as_deref(), Some("team-1"));
        assert_eq!(input.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(input.text.as_deref(), Some("hello"));
        assert_eq!(input.attachment_ids, Some(vec!["att-1".to_string()]));
        assert!(input.device_id.is_none() && input.cli.is_none() && input.model.is_none());
        assert!(input.plan_mode.is_none() && input.ultracode.is_none());
        let bare = agent_send_input("team-1", "agent-1", " ", Vec::new());
        assert!(bare.text.is_none() && bare.attachment_ids.is_none());
    }

    /// EXP-1294: ONE send per start — the active team, the device, the
    /// options as picked (blank model/effort omitted), the trimmed text, and
    /// N issues on ONE root.
    #[test]
    fn the_send_input_carries_the_start() {
        let issues = send_input(
            "team-1",
            "dev-1",
            &options(),
            StartSubject {
                issue_ids: vec!["i-1".into(), "i-2".into()],
                repository_id: Some("repo-1".into()),
                ..Default::default()
            },
            "  mind the retry \n",
            vec!["att-1".into()],
        );
        assert_eq!(issues.team_id.as_deref(), Some("team-1"));
        assert_eq!(issues.device_id.as_deref(), Some("dev-1"));
        assert_eq!(issues.cli.as_deref(), Some("claude"));
        assert_eq!(issues.model.as_deref(), Some("opus"));
        assert_eq!(issues.effort, None, "blank effort is omitted");
        assert_eq!(issues.plan_mode, None, "S-3: plan mode is never sent");
        assert_eq!(issues.text.as_deref(), Some("mind the retry"));
        assert_eq!(issues.attachment_ids, Some(vec!["att-1".to_string()]));
        assert_eq!(issues.issue_ids, Some(vec!["i-1".to_string(), "i-2".to_string()]));
        assert_eq!(issues.repository_id.as_deref(), Some("repo-1"));
        assert!(issues.prompt_id.is_none());
        // EXP-849: no account picked = the target's last used login; no
        // computer-use pick leaves it to the target.
        assert_eq!(issues.account, None);
        assert_eq!(issues.computer_use, None);
        assert!(issues.session_id.is_none() && issues.parent_id.is_none());

        let action = send_input(
            "team-1",
            "dev-1",
            &LaunchOptions {
                account: Some("0a1b2c3d".into()),
                computer_use: Some(false),
                ..options()
            },
            StartSubject {
                prompt_id: Some("p-1".into()),
                ..Default::default()
            },
            " ",
            Vec::new(),
        );
        assert_eq!(action.prompt_id.as_deref(), Some("p-1"));
        assert_eq!(action.text, None);
        assert_eq!(action.attachment_ids, None);
        assert_eq!(action.issue_ids, None);
        assert_eq!(action.account.as_deref(), Some("0a1b2c3d"));
        assert_eq!(action.computer_use, Some(false));
    }
}
