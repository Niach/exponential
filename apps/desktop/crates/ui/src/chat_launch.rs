//! EXP-825 — the Agent page composer's launch logic, kept free of gpui so
//! the rules the deleted start-coding dialog enforced by hand stay testable:
//! what the submit pill says, when free text is required, the
//! `steer.startSession` payload for a remote target, and the batch request
//! for a local one. `chat_screen.rs` owns the state and calls in here.
//!
//! The contract (shared ×4): no subject → "Start chat", 1 issue → "Start
//! coding", 2+ → "Start batch · N", an action → "Run action"; free text is
//! the CHAT PROMPT with no subject, the creator REQUEST for the Create-action
//! builtin (both required), and optional additional instructions otherwise.

use std::collections::{BTreeMap, HashSet};

use coding::{
    ActionInputValue, BatchIssueSpec, BatchLaunchRequest, LaunchOptions, LaunchOrigin, RepoGroup,
};

use crate::issue_picker::{IssueRow, MAX_ISSUES_PER_RUN};

/// What the composer is about to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SubjectKind {
    /// No issue, no action: a chat run (`builtin:chat`).
    Chat,
    /// `count` checked issues (1 = single, 2+ = batch).
    Issues { count: usize },
    /// One picked action (`id` — a row id or a builtin literal).
    Action { id: String },
}

/// The submit pill's label (the web `submitLabel`, byte-identical).
pub(crate) fn submit_label(subject: &SubjectKind) -> String {
    match subject {
        SubjectKind::Chat => "Start chat".to_string(),
        SubjectKind::Issues { count: 1 } => "Start coding".to_string(),
        SubjectKind::Issues { count } => format!("Start batch · {count}"),
        SubjectKind::Action { .. } => "Run action".to_string(),
    }
}

/// Whether the free text is REQUIRED for `subject`: a chat has nothing else
/// to send (an image alone counts), and the Create-action builtin authors
/// from the request. Every other subject takes it as optional additional
/// instructions.
pub(crate) fn text_required(subject: &SubjectKind) -> bool {
    match subject {
        SubjectKind::Chat => true,
        SubjectKind::Action { id } => id == api::actions::BUILTIN_CREATE_ACTION_ID,
        SubjectKind::Issues { .. } => false,
    }
}

/// The "Type a message." blocker, when the text is required and the draft
/// (text or images) is empty.
pub(crate) fn text_blocker(subject: &SubjectKind, text: &str, image_count: usize) -> Option<&'static str> {
    if !text_required(subject) {
        return None;
    }
    let has_text = !text.trim().is_empty();
    match subject {
        // An image-only chat message is a message (the steer composer's rule).
        SubjectKind::Chat if has_text || image_count > 0 => None,
        SubjectKind::Chat => Some("Type a message."),
        _ if has_text => None,
        _ => Some("Describe the action to create."),
    }
}

/// The issue-count gates: at least one, at most [`MAX_ISSUES_PER_RUN`].
pub(crate) fn issue_count_blocker(count: usize) -> Option<String> {
    if count == 0 {
        return Some("Select at least one issue.".to_string());
    }
    if count > MAX_ISSUES_PER_RUN {
        return Some(format!(
            "At most {MAX_ISSUES_PER_RUN} issues per run. Split the batch."
        ));
    }
    None
}

/// The composer text as the launcher's `prompt`: trimmed, `None` when
/// blank. Image embeds ride inside it (the steer message shape).
pub(crate) fn prompt_of(message: &str) -> Option<String> {
    let trimmed = message.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The picked repository as the chat builtin's ONE input (web
/// `chatStartInputs`): `None` when no repo was picked, so a repo-less chat
/// reaches the launcher with no `repo` key at all (EXP-739).
pub(crate) fn chat_repo_input(
    team_id: &str,
    repo: Option<(&str, &str)>,
) -> Vec<ActionInputValue> {
    let Some((repository_id, full_name)) = repo else {
        return Vec::new();
    };
    api::actions::builtin_chat_action(team_id)
        .inputs
        .iter()
        .filter(|input| input.key == "repo")
        .map(|input| ActionInputValue {
            key: input.key.clone(),
            label: input.label.clone(),
            input_type: input.input_type.clone(),
            value: repository_id.to_string(),
            display: Some(full_name.to_string()),
        })
        .collect()
}

/// The remote subject half of a [`api::steer::StartSessionInput`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemoteSubject<'a> {
    Chat {
        team_id: &'a str,
        repository_id: Option<&'a str>,
    },
    Issue {
        issue_id: &'a str,
        resume: bool,
    },
    Batch {
        issue_ids: Vec<String>,
    },
    Action {
        action_id: &'a str,
        team_id: &'a str,
        inputs: &'a [ActionInputValue],
    },
}

/// EXP-696/EXP-825: the `steer.startSession` payload — the SAME subject the
/// local paths launch, addressed at another machine, with the composer text
/// as `prompt` (LAST on the wire). Blank model/effort are the "CLI default"
/// sentinel and are omitted; the server's per-agent vocabulary has no empty
/// member.
///
/// EXP-849: the account pick rides too. It names a profile on the TARGET
/// machine (the picker only ever offers that device's own logins), so the
/// ambient login — `None` here — is the only safe default.
pub(crate) fn remote_start_input(
    device_id: &str,
    options: &LaunchOptions,
    subject: RemoteSubject<'_>,
    prompt: Option<String>,
) -> api::steer::StartSessionInput {
    let mut input = api::steer::StartSessionInput {
        device_id: device_id.to_string(),
        agent: Some(options.agent.id().to_string()),
        model: Some(options.model.clone()).filter(|model| !model.is_empty()),
        effort: Some(options.effort.clone()).filter(|effort| !effort.is_empty()),
        ultracode: Some(options.ultracode),
        plan_mode: Some(options.plan_mode),
        prompt,
        account: options.account.clone(),
        ..Default::default()
    };
    match subject {
        RemoteSubject::Chat {
            team_id,
            repository_id,
        } => {
            input.action_id = Some(api::actions::BUILTIN_CHAT_ID.to_string());
            // A builtin has no DB row to derive the team from.
            input.team_id = Some(team_id.to_string());
            // EXP-739: no repo picked = no `repo` key on the frame.
            input.inputs = repository_id.map(|id| {
                let mut inputs = BTreeMap::new();
                inputs.insert("repo".to_string(), id.to_string());
                inputs
            });
        }
        RemoteSubject::Issue { issue_id, resume } => {
            input.issue_id = Some(issue_id.to_string());
            // EXP-481: `resume` is a single-issue flag — it continues the
            // machine's existing worktree.
            if resume {
                input.resume = Some(true);
            }
        }
        RemoteSubject::Batch { issue_ids } => input.issue_ids = Some(issue_ids),
        RemoteSubject::Action {
            action_id,
            team_id,
            inputs,
        } => {
            let inputs: BTreeMap<String, String> = inputs
                .iter()
                .map(|filled| (filled.key.clone(), filled.value.clone()))
                .collect();
            input.team_id =
                api::actions::is_builtin_action_id(action_id).then(|| team_id.to_string());
            input.inputs = (!inputs.is_empty()).then_some(inputs);
            input.action_id = Some(action_id.to_string());
        }
    }
    input
}

/// One issue's `repositories.forIssue` probe state.
#[derive(Clone, Debug)]
pub(crate) enum RepoState {
    Loading,
    /// `Ready(None)` = no repository linked (excluded from the run).
    Ready(Option<api::repositories::IssueRepository>),
    /// Transport failure — the issue can't resolve a repo, so it is
    /// excluded like a repo-less one (the message says why).
    Error(String),
}

/// The one-repository / one-base-branch gate over the checked issues
/// (EXP-712: a batch cuts ONE branch, so every issue must resolve to the
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
                    Some(_) => return Some("One repository per run. Deselect the others.".into()),
                }
                bases.push(
                    Some(resolved.default_branch.clone())
                        .filter(|branch| !branch.trim().is_empty()),
                );
            }
            _ => return Some("Checking linked repositories…".into()),
        }
    }
    if let Some((first, second)) = crate::repo_resolver::batch_branch_conflict(&bases) {
        return Some(format!(
            "All issues in a batch must share one base branch ({first} vs {second})."
        ));
    }
    None
}

/// Snapshot the checked set into a [`BatchLaunchRequest`] (2+ checked).
/// `None` on a racing probe (the blocker just re-checked) — bail quietly.
pub(crate) fn batch_request(
    team_id: &str,
    rows: &[IssueRow],
    checked: &HashSet<String>,
    repos: &std::collections::HashMap<String, RepoState>,
    options: LaunchOptions,
    prompt: Option<String>,
) -> Option<BatchLaunchRequest> {
    let mut repo: Option<RepoGroup> = None;
    let mut issues: Vec<BatchIssueSpec> = Vec::new();
    // EXP-712: the board whose branch the batch cuts from. The blocker
    // already refused a set whose boards resolve to different branches, so
    // the FIRST board's branch is every checked issue's branch.
    let mut board_id: Option<String> = None;
    for row in rows {
        if !checked.contains(&row.issue_id) {
            continue;
        }
        board_id.get_or_insert_with(|| row.board_id.clone());
        let Some(RepoState::Ready(Some(resolved))) = repos.get(&row.issue_id) else {
            return None;
        };
        if repo.is_none() {
            repo = Some(RepoGroup {
                repository_id: resolved.repository_id.clone(),
                full_name: resolved.full_name.clone(),
                default_branch: resolved.default_branch.clone(),
            });
        }
        issues.push(BatchIssueSpec {
            issue_id: row.issue_id.clone(),
            issue_identifier: row.identifier.clone(),
            title: row.title.clone(),
            description: row.description.clone(),
            status: row.status,
        });
    }
    Some(BatchLaunchRequest {
        batch_id: coding::new_batch_id(),
        team_id: team_id.to_string(),
        board_id,
        repo: repo?,
        issues,
        device_label: coding::default_device_label(),
        origin: LaunchOrigin::Local,
        options,
        prompt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use coding::CodingAgent;

    fn options() -> LaunchOptions {
        LaunchOptions {
            agent: CodingAgent::Claude,
            model: "opus".to_string(),
            effort: String::new(),
            ultracode: false,
            plan_mode: true,
            mcp_server_ids: Vec::new(),
            account: None,
            external: None,
        }
    }

    /// The four labels of the shared contract, byte for byte.
    #[test]
    fn submit_labels_follow_the_subject() {
        assert_eq!(submit_label(&SubjectKind::Chat), "Start chat");
        assert_eq!(submit_label(&SubjectKind::Issues { count: 1 }), "Start coding");
        assert_eq!(submit_label(&SubjectKind::Issues { count: 2 }), "Start batch · 2");
        assert_eq!(submit_label(&SubjectKind::Issues { count: 30 }), "Start batch · 30");
        assert_eq!(
            submit_label(&SubjectKind::Action {
                id: "act-1".into()
            }),
            "Run action"
        );
    }

    /// Free text is required for a chat (an image alone counts) and the
    /// creator builtin, optional additional instructions everywhere else.
    #[test]
    fn text_is_required_only_for_chat_and_create_action() {
        assert_eq!(text_blocker(&SubjectKind::Chat, "  ", 0), Some("Type a message."));
        assert_eq!(text_blocker(&SubjectKind::Chat, "", 1), None);
        assert_eq!(text_blocker(&SubjectKind::Chat, "hi", 0), None);
        let create = SubjectKind::Action {
            id: api::actions::BUILTIN_CREATE_ACTION_ID.into(),
        };
        assert_eq!(text_blocker(&create, "", 2), Some("Describe the action to create."));
        assert_eq!(text_blocker(&create, "triage", 0), None);
        let fix = SubjectKind::Action {
            id: api::actions::BUILTIN_FIX_CONFLICTS_ID.into(),
        };
        assert_eq!(text_blocker(&fix, "", 0), None);
        assert_eq!(text_blocker(&SubjectKind::Issues { count: 2 }, "", 0), None);
        assert_eq!(issue_count_blocker(0).as_deref(), Some("Select at least one issue."));
        assert!(issue_count_blocker(1).is_none());
        assert!(issue_count_blocker(MAX_ISSUES_PER_RUN).is_none());
        assert!(issue_count_blocker(MAX_ISSUES_PER_RUN + 1).is_some());
        assert_eq!(prompt_of("  x \n"), Some("x".into()));
        assert_eq!(prompt_of(" \n"), None);
    }

    /// EXP-739: the chat's repo input exists only when a repo was picked,
    /// and it is the builtin's own definition (label/type byte-locked ×4).
    #[test]
    fn chat_repo_input_is_omitted_without_a_pick() {
        assert!(chat_repo_input("team-1", None).is_empty());
        let inputs = chat_repo_input("team-1", Some(("repo-1", "acme/web")));
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].key, "repo");
        assert_eq!(inputs[0].input_type, "repo");
        assert_eq!(inputs[0].value, "repo-1");
        assert_eq!(inputs[0].display.as_deref(), Some("acme/web"));
    }

    /// EXP-825: the remote payload per subject — exactly one subject, the
    /// composer text as `prompt`, a builtin's `teamId`, blank picks omitted.
    #[test]
    fn remote_start_input_carries_one_subject_and_the_prompt() {
        let chat = remote_start_input(
            "dev-1",
            &options(),
            RemoteSubject::Chat {
                team_id: "team-1",
                repository_id: Some("repo-1"),
            },
            Some("hello".into()),
        );
        assert_eq!(chat.action_id.as_deref(), Some("builtin:chat"));
        assert_eq!(chat.team_id.as_deref(), Some("team-1"));
        assert_eq!(chat.inputs.as_ref().unwrap().get("repo").map(String::as_str), Some("repo-1"));
        assert_eq!(chat.prompt.as_deref(), Some("hello"));
        assert_eq!(chat.effort, None, "blank effort is omitted");
        assert_eq!(chat.model.as_deref(), Some("opus"));
        assert_eq!(chat.plan_mode, Some(true));
        assert!(chat.issue_id.is_none() && chat.issue_ids.is_none());
        // EXP-849: no account picked = the target's ambient login.
        assert_eq!(chat.account, None);
        let picked = remote_start_input(
            "dev-1",
            &LaunchOptions {
                account: Some("0a1b2c3d".into()),
                ..options()
            },
            RemoteSubject::Issue {
                issue_id: "i-1",
                resume: false,
            },
            None,
        );
        assert_eq!(picked.account.as_deref(), Some("0a1b2c3d"));

        let repo_less = remote_start_input(
            "dev-1",
            &options(),
            RemoteSubject::Chat {
                team_id: "team-1",
                repository_id: None,
            },
            None,
        );
        assert_eq!(repo_less.inputs, None);

        let issue = remote_start_input(
            "dev-1",
            &options(),
            RemoteSubject::Issue {
                issue_id: "i-1",
                resume: true,
            },
            Some("mind the retry".into()),
        );
        assert_eq!(issue.issue_id.as_deref(), Some("i-1"));
        assert_eq!(issue.resume, Some(true));
        assert_eq!(issue.prompt.as_deref(), Some("mind the retry"));
        let fresh = remote_start_input(
            "dev-1",
            &options(),
            RemoteSubject::Issue {
                issue_id: "i-1",
                resume: false,
            },
            None,
        );
        assert_eq!(fresh.resume, None);

        let batch = remote_start_input(
            "dev-1",
            &options(),
            RemoteSubject::Batch {
                issue_ids: vec!["a".into(), "b".into()],
            },
            None,
        );
        assert_eq!(batch.issue_ids, Some(vec!["a".to_string(), "b".to_string()]));

        let filled = vec![ActionInputValue {
            key: "pr".into(),
            label: "Pull request".into(),
            input_type: "pr".into(),
            value: "i-9".into(),
            display: Some("#4".into()),
        }];
        let builtin = remote_start_input(
            "dev-1",
            &options(),
            RemoteSubject::Action {
                action_id: api::actions::BUILTIN_FIX_CONFLICTS_ID,
                team_id: "team-1",
                inputs: &filled,
            },
            None,
        );
        assert_eq!(builtin.team_id.as_deref(), Some("team-1"));
        assert_eq!(builtin.inputs.as_ref().unwrap().get("pr").map(String::as_str), Some("i-9"));
        let team_action = remote_start_input(
            "dev-1",
            &options(),
            RemoteSubject::Action {
                action_id: "act-1",
                team_id: "team-1",
                inputs: &[],
            },
            None,
        );
        assert_eq!(team_action.team_id, None, "a row action never sends teamId");
        assert_eq!(team_action.inputs, None);
    }
}
