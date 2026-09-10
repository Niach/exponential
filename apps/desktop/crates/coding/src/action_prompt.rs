//! The action run's seed prompts (EXP-253): [`render_action_prompt`] — a
//! small fixed preamble + the action's markdown body VERBATIM (the body is
//! the user-authored program; the preamble only frames the execution context
//! and never rewrites it; EXP-257 adds an optional `## Inputs` section
//! between the preamble and the body with the run-time values the launcher
//! or the server resolved for the action's typed inputs schema) — plus the
//! generated prompts of the two server-defined BUILTIN runs:
//! [`create_action_prompt`] ("Create action", EXP-257) and
//! [`fix_pr_conflicts_prompt`] ("Fix merge conflicts", EXP-259 — it replaced
//! the deleted claude-task conflict prompts).

/// One resolved run-time input value (EXP-257): the schema fields the value
/// was filled against plus the human-readable `display` (repo fullName /
/// board name / the text itself) the prompt shows. `display: None` = the
/// value IS the display (text inputs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInputValue {
    pub key: String,
    pub label: String,
    pub input_type: String,
    pub value: String,
    pub display: Option<String>,
}

/// Why an automation-started run fired (EXP-530), rendered as the prompt's
/// `## Trigger` section. Hosts build it from the engine's decision: a
/// schedule carries the [`crate::automations::schedule_phrase`] sentence, an
/// event the pre-rendered per-issue lines (capped at
/// [`crate::automations::TRIGGER_PROMPT_MAX_LINES`], overflow in `omitted`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerNote {
    pub kind: TriggerNoteKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerNoteKind {
    Schedule { phrase: String },
    Event { lines: Vec<String>, omitted: usize },
}

impl TriggerNote {
    /// The `coding_sessions.started_reason` wire value this note implies.
    pub fn started_reason(&self) -> &'static str {
        match self.kind {
            TriggerNoteKind::Schedule { .. } => "schedule",
            TriggerNoteKind::Event { .. } => "event",
        }
    }
}

/// EXP-637: the run's dedicated worktree, rendered as the prompt's
/// `## Workspace` section. Every repo-backed action/chat run gets its own
/// branch cut from `origin/<default_branch>` now (decision 1), so the prompt
/// must say where the agent is, how work leaves the worktree (a PR opened
/// with `repositoryId` + `head` — EXP-626 unlinked PRs), and that a dirty
/// worktree is never acceptable. `None` = a repo-less scratch run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceNote {
    pub branch: String,
    pub default_branch: String,
    /// The team `repositories` row id — the `exponential_pr_open` argument.
    pub repository_id: String,
}

/// Collapse owner-/server-provided text onto one line so a crafted label or
/// display can never fake extra prompt sections or list entries.
fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Render the seed prompt for an action run: preamble [+ inputs] + raw body.
/// An empty `inputs` slice renders byte-identically to the pre-EXP-257
/// prompt (input-less actions must not change what a trusted body executes).
pub fn render_action_prompt(name: &str, body: &str, inputs: &[ActionInputValue]) -> String {
    render_action_prompt_with_trigger(name, body, inputs, None)
}

/// [`render_action_prompt`] plus the optional `## Trigger` section an
/// automation-started run carries between the inputs block and the divider
/// (EXP-530). `None` renders byte-identically to the trigger-less prompt.
pub fn render_action_prompt_with_trigger(
    name: &str,
    body: &str,
    inputs: &[ActionInputValue],
    trigger: Option<&TriggerNote>,
) -> String {
    // EXP-679: a trigger IS the unattended marker, so these two wrappers
    // never need the flag of their own.
    render_action_prompt_full(name, body, inputs, trigger, None, false, None)
}

/// The full renderer (EXP-637): [`render_action_prompt_with_trigger`] plus
/// the optional `## Workspace` section a repo-backed run carries (its own
/// worktree + branch). `workspace: None` renders byte-identically to the
/// pre-EXP-637 prompt, which the two wrappers above rely on.
/// EXP-679: `unattended` picks the close-out — a trigger implies it (an
/// automation's run is unattended by definition), and the launcher passes it
/// for the other unattended reason (`agent`: another coding session started
/// this one).
/// EXP-764: what a repo-less run (chat or team action without a repo) is
/// told about its cwd — the scratch dir is purged whole with the run
/// (`crate::scratch::purge`), so nothing written there survives it.
/// EXP-822: and the other half of it — the run has no repository AT ALL, so
/// it must never resolve its subject by scanning the machine. A sibling
/// clone of the real checkout is the worst case: six weeks stale and every
/// conclusion drawn from it reads authoritative. Ask instead.
pub const SCRATCH_CWD_NOTE: &str = "You have no repository checked out: your working directory \
is a scratch folder that is deleted, with everything in it, the moment this run ends. Leave \
nothing there you want to keep; put results into issue comments or attachments \
(`exponential_attachments_upload`). No repository is attached to this run either, so never go \
looking for one on this machine: any clone you find is someone else's checkout, and a stale one \
reads exactly like the real one. If the work needs a repository, say so and ask which one, so the \
user can start a run with that repository picked.";

pub fn render_action_prompt_full(
    name: &str,
    body: &str,
    inputs: &[ActionInputValue],
    trigger: Option<&TriggerNote>,
    workspace: Option<&WorkspaceNote>,
    unattended: bool,
    extra: Option<&str>,
) -> String {
    let inputs_section = if inputs.is_empty() {
        String::new()
    } else {
        let mut section = String::from("## Inputs\n\nThe user provided these values — apply \
them where the instructions reference them:\n\n");
        for input in inputs {
            let label = single_line(&input.label);
            let value = single_line(&input.value);
            let display = single_line(input.display.as_deref().unwrap_or(&input.value));
            section.push_str(&format!("- {label} ({}): {display}", input.input_type));
            // For picked entities the display is a name — carry the raw id
            // too so MCP calls need no lookup.
            if display != value {
                section.push_str(&format!(" (`{value}`)"));
            }
            section.push('\n');
        }
        section.push('\n');
        section
    };
    let trigger_section = match trigger {
        None => String::new(),
        Some(TriggerNote {
            kind: TriggerNoteKind::Schedule { phrase },
        }) => format!(
            "## Trigger\n\nThis run was started automatically by the action's schedule \
({}, device time).\n\n",
            single_line(phrase)
        ),
        Some(TriggerNote {
            kind: TriggerNoteKind::Event { lines, omitted },
        }) => {
            let mut section = String::from(
                "## Trigger\n\nThis run was started automatically because these issues \
changed:\n\n",
            );
            for line in lines {
                section.push_str(&format!("- {}\n", single_line(line)));
            }
            if *omitted > 0 {
                section.push_str(&format!("- …and {omitted} more.\n"));
            }
            section.push('\n');
            section
        }
    };
    let workspace_section = match workspace {
        None => format!("## Workspace\n\n{SCRATCH_CWD_NOTE}\n\n"),
        Some(note) => format!(
            "## Workspace\n\nYou work on branch `{branch}` in a dedicated worktree cut from \
`origin/{default}`. If you change files: commit, `git push -u origin {branch}`, then open a pull \
request with the `exponential_pr_open` MCP tool (`repositoryId: \"{id}\"`, `head: \"{branch}\"`). \
If nothing needs to change, leave the tree clean and report `no_changes`. Never leave the \
worktree dirty. Never force-push; do not use `gh`.\n\n",
            branch = note.branch,
            default = note.default_branch,
            id = note.repository_id,
        ),
    };
    let close_out = crate::prompt::close_out(trigger.is_some() || unattended);
    let extra_section = match crate::prompt::additional_instructions(extra) {
        section if section.is_empty() => section,
        section => format!("{section}\n"),
    };
    format!(
        "You are running the team action \"{name}\" for this user. Follow the \
instructions below exactly. The exponential MCP tools are available for issue, \
board, label, and comment operations. \
{close_out}\n\n{inputs_section}{trigger_section}{workspace_section}{extra_section}---\n\n{body}"
    )
}

/// EXP-615/EXP-637: the chat run's seed prompt. The user's words ride LAST
/// and VERBATIM (this is the "open a terminal tab on the repo" shape —
/// anything we wrap around it is words the user did not write), preceded by
/// a two-line preamble: where the run lives, and how it reports. EXP-679: a
/// chat a person started has no `exponential_sessions_end` tool to call and
/// stays open for follow-ups, so it is simply told to summarize; only an
/// unattended chat (one another coding session started) reports through the
/// tool that ends it.
pub fn chat_prompt(
    user_prompt: &str,
    workspace: Option<&WorkspaceNote>,
    unattended: bool,
) -> String {
    let mut preamble = String::new();
    if workspace.is_none() {
        preamble.push_str(SCRATCH_CWD_NOTE);
        preamble.push('\n');
    }
    if let Some(note) = workspace {
        preamble.push_str(&format!(
            "You work on branch `{branch}` in a dedicated worktree cut from `origin/{default}`. \
If you change files: commit, `git push -u origin {branch}`, then open a pull request with the \
`exponential_pr_open` MCP tool (`repositoryId: \"{id}\"`, `head: \"{branch}\"`). Never leave the \
worktree dirty; never force-push; do not use `gh`.\n",
            branch = note.branch,
            default = note.default_branch,
            id = note.repository_id,
        ));
    }
    preamble.push_str(if unattended {
        "When you are done, report with the `exponential_sessions_end` MCP tool (a one-paragraph \
summary of what you did); that call ends this run, and nobody is watching it.\n"
    } else {
        "When you are done, summarize what you did here; the session stays open afterwards, so \
keep answering follow-ups.\n"
    });
    format!("{preamble}\n---\n\n{user_prompt}")
}

/// Prompt for the builtin "Create action" run (EXP-257 — the successor of
/// the actions panel's "Describe with your agent" creator, EXP-253 / L24). It
/// runs as a regular ACTION session in a scratch dir with the exponential
/// MCP tools wired, and asks the agent to author ONE action for `team_id`
/// from the user's `request` — the composer's free text (EXP-825: the
/// `description`/`name` inputs are gone; the agent derives the name from the
/// request, or uses one the request states verbatim). `repo` is the optional
/// repo INPUT the user picked — `(id, display)`; when set, the authored
/// action must bind to that repository. `icon` is the optional curated glyph
/// the user picked (EXP-273). It must NOT touch git or files — it only calls
/// the MCP tools. EXP-825 also retired free-text action inputs: the prompt
/// tells the agent never to declare one, since the composer text reaches
/// every run as the additional-instructions section instead — and that it
/// may set `promptPlaceholder`, the composer's hint for that text.
pub fn create_action_prompt(
    team_id: &str,
    request: &str,
    repo: Option<(&str, &str)>,
    icon: Option<&str>,
    unattended: bool,
) -> String {
    let icon_rule = match icon {
        Some(name) => format!(
            " Set `icon` to `{name}` — the user picked that glyph for the action."
        ),
        // No pick: let the agent choose, since an unset icon renders as the
        // generic action glyph and the list reads worse.
        None => " Also set `icon` to the curated icon name that best fits the \
action (the same set as board icons, e.g. `bug`, `rocket`, `database`, `chart-line`)."
            .to_string(),
    };
    let repo_rule = match repo {
        Some((id, display)) => format!(
            "Set `repositoryId` to `{id}` ({display}) — the user picked that repository \
as the action's execution context."
        ),
        None => "Leave `repositoryId` unset unless the request clearly needs repository \
access (then pick the right repo id from `exponential_repositories_list`)."
            .to_string(),
    };
    // EXP-679: only an unattended creator run has the close-out tool.
    let report_rule = if unattended {
        "After the action is created, report with `exponential_sessions_end` (a one-paragraph \
summary); that call ends this run."
    } else {
        "After the action is created, report what you created here; the session stays open \
afterwards, so keep answering follow-ups."
    };
    format!(
        "Please create ONE new action for the Exponential team with id `{team_id}`. An \
action is a reusable markdown prompt that a team member later runs as an interactive \
agent session on their own desktop (the exponential MCP tools are available to that \
run). The user requested the action as:\n\n\"{request}\"\n\n\
Derive a short name from the request (if it states a name, use that verbatim). \
Write a clear, focused markdown body for it: state the goal, the concrete steps, \
which exponential MCP tools to use (e.g. exponential_issues_list / \
exponential_issues_create / exponential_labels_list), and what to report at the end. \
Call `exponential_actions_list` for the team first so the name doesn't collide. \
{repo_rule}{icon_rule} Create the action with `exponential_actions_create` (teamId, the \
name, a one-line description, the markdown body). `exponential_actions_create` also \
accepts an optional `inputs` array ({{key, label, type: repo|board|pr|icon, required?, \
placeholder?}}) declaring pick inputs the runner fills before the run and the run \
receives as an \"## Inputs\" prompt section — declare them when the described action \
naturally varies per run (a target repository or board); otherwise omit the field. \
Never declare free-text inputs: whatever the requester types when running the action \
reaches the run as an Additional instructions section. `exponential_actions_create` \
also accepts an optional `promptPlaceholder` (≤200 chars): a short hint for what the \
requester should type there (e.g. \"Scope: which platforms, which version\"), shown as \
the composer's field placeholder while the action is picked — set one when the action \
expects that text; otherwise omit it. `exponential_actions_create` also accepts an \
optional `trigger` field: when the request contains an \"Automation —\" block, pass \
that block's JSON as `trigger` verbatim; otherwise omit `trigger`. Do not commit, \
push, or change any files — only call the MCP tools. {report_rule}"
    )
}

/// Prompt for the builtin "Fix merge conflicts" run (EXP-259): the run is
/// spawned in a worktree checked out to the selected pull request's branch.
/// It rebases onto `origin/<base_branch>` — the PR's LIVE base resolved by
/// the launcher via `issues.prepareConflictFix` (EXP-324: a stacked PR's
/// base is its parent's branch, and a stale base was already retargeted to
/// the repo default before this prompt renders) — resolves the conflicts,
/// verifies the build, force-pushes, and then MERGES the PR via the
/// `exponential_pr_merge` MCP tool — merging completes every linked issue.
/// If the base goes stale MID-RUN (the parent merges while the agent works),
/// the prompt points at `exponential_pr_retarget` as the self-heal.
/// EXP-825: `extra` is the composer's free text, appended last.
pub fn fix_pr_conflicts_prompt(
    identifier: &str,
    branch: &str,
    base_branch: &str,
    unattended: bool,
    extra: Option<&str>,
) -> String {
    // EXP-679: the merge result goes into the conversation for a person's
    // run (no close-out tool there), through the tool for an unattended one.
    let report_rule = if unattended {
        "Finally call `exponential_sessions_end` with the merge result (merged, or why you \
stopped)."
    } else {
        "Finally report the merge result here (merged, or why you stopped)."
    };
    let prompt = format!(
        "The pull request for `{identifier}` (branch `{branch}`) has merge conflicts and \
cannot be merged. You are in a worktree checked out to `{branch}`. First run \
`git fetch origin` and confirm `git rev-parse HEAD` equals \
`git rev-parse origin/{branch}` — if HEAD is missing commits that exist on \
`origin/{branch}`, stop and summarize the mismatch instead (force-pushing from a \
stale checkout would discard remote commits). Then rebase onto \
`origin/{base_branch}` (the pull request's base branch), resolve every conflict \
preserving both sides' intent, and \
verify the build still passes. Then push the branch with `--force-with-lease` and \
merge the pull request by calling the `exponential_pr_merge` MCP tool with issueId \
`{identifier}` — merging completes every issue linked to the PR. If the merge is \
rejected because the base branch is stale, merged, or closed, call the \
`exponential_pr_retarget` MCP tool with the same issueId (omit `base` to retarget \
onto the repository's default branch), rebase onto the new base, push again with \
`--force-with-lease`, and retry the merge. If the conflicts \
cannot be resolved safely, do NOT push or merge: stop and summarize what blocks the \
rebase instead. {report_rule}"
    );
    crate::prompt::append_additional_instructions(prompt, extra)
}

/// EXP-637 — the RESUME fallback prompt: a run is being resumed but its
/// agent's native transcript is gone (pruned, another agent, a machine that
/// never recorded one), so a FRESH session is spawned in the same workspace
/// and told to pick the work back up. Mirrors
/// [`crate::prompt::render_resume_prompt`]'s shape for issue sessions.
pub fn render_run_resume_prompt(
    record: &crate::run_registry::RunRecord,
    unattended: bool,
) -> String {
    // EXP-662: the record's own name for itself — the action's name, or a
    // batch's `EXP-42 +1`.
    let name = single_line(&record.display_name());
    let mut prompt = format!(
        "You are RESUMING the run \"{name}\" — an earlier session already worked here and its \
conversation could not be recovered, so start by INSPECTING what it left behind."
    );
    match (&record.branch, &record.base_branch) {
        (Some(branch), Some(base)) => prompt.push_str(&format!(
            " You are on branch `{branch}` in its worktree: run `git status` and \
`git log origin/{base}..HEAD` to see what was already done."
        )),
        // EXP-739: a repo-LESS run (a repo-less chat, or any scratch-dir
        // action) has NO checkout — its cwd holds only the MCP config, where
        // `git status` errors with "not a git repository" as the resumed
        // session's very first instruction.
        _ if record.clone.is_none() => prompt.push_str(
            " This run has no repository checked out: its working directory holds only the tracker's MCP configuration, so there are no files and no git history to inspect. It is a conversation with the tracker over the `exponential_*` MCP tools — read back what it touched through those instead.",
        ),
        _ => prompt.push_str(" Run `git status` to see what was already done."),
    }
    if !record.inputs.is_empty() {
        prompt.push_str("\n\n## Inputs\n\nThe original run received these values:\n\n");
        for input in &record.inputs {
            let value = single_line(input.display.as_deref().unwrap_or(&input.value));
            prompt.push_str(&format!("- {}: {value}\n", single_line(&input.key)));
        }
    }
    prompt.push_str("\n\nContinue from where it left off. ");
    prompt.push_str(&crate::prompt::close_out(unattended));
    prompt.push('\n');
    prompt
}

/// The read-only prompt an action-detail screen shows for a BUILTIN
/// (EXP-298). Builtins are not DB rows, so `actions.get` has no body to
/// return — but the detail screen must show what the run will actually send.
/// This renders the REAL prompt with placeholder tokens standing in for the
/// values the launcher substitutes per run, so the screen can never drift
/// from the shipped program. `None` = not a builtin id.
pub fn builtin_prompt_preview(action_id: &str) -> Option<String> {
    match action_id {
        domain::contract::BUILTIN_CREATE_ACTION_ID => Some(create_action_prompt(
            "<this team>",
            "<what you type in the composer when you run it>",
            None,
            None,
            // The preview shows what a hand-started run sends (EXP-679).
            false,
        )),
        // EXP-615: the chat builtin has no shipped program to preview — its
        // prompt IS whatever the user types, so there is nothing to show.
        domain::contract::BUILTIN_CHAT_ID => None,
        domain::contract::BUILTIN_FIX_CONFLICTS_ID => Some(fix_pr_conflicts_prompt(
            "<the issue you pick>",
            "<its PR branch>",
            "<the PR's base branch>",
            false,
            None,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`crate::run_registry::RunRecord`] for the resume-prompt tests:
    /// repo-backed by default, `repo_less` strips the checkout the way a
    /// scratch-dir run records it.
    fn resume_record(repo_less: bool) -> crate::run_registry::RunRecord {
        use crate::run_registry::{now_secs, RunKind, RunRecord};
        use crate::CodingAgent;
        use std::collections::BTreeMap;
        use std::path::PathBuf;
        RunRecord {
            session_id: "sess-1".to_string(),
            board_id: None,
            account_id: "acc-1".to_string(),
            agent: CodingAgent::Claude,
            kind: if repo_less {
                RunKind::Chat
            } else {
                RunKind::Team
            },
            action_id: "builtin:chat".to_string(),
            action_name: "Chat".to_string(),
            team_id: "ws-1".to_string(),
            issue_id: None,
            issue_identifier: None,
            batch_id: None,
            issues: Vec::new(),
            cwd: if repo_less {
                PathBuf::from("/data/actions/builtin_chat/1a2b3c4d")
            } else {
                PathBuf::from("/repos/acme/web.worktrees/chat-1a2b3c4d")
            },
            clone: (!repo_less).then(|| PathBuf::from("/repos/acme/web")),
            repo: (!repo_less).then(|| "acme/web".to_string()),
            repository_id: (!repo_less).then(|| "repo-1".to_string()),
            branch: (!repo_less).then(|| "exp/chat-1a2b3c4d".to_string()),
            base_branch: (!repo_less).then(|| "main".to_string()),
            claude_session_id: None,
            pi_session_file: None,
            codex_originator: None,
            inputs: Vec::new(),
            model: String::new(),
            effort: String::new(),
            ultracode: false,
            fix: None,
            started_reason: None,
            resumed_from_id: None,
            recorded_at: now_secs(),
            extra: BTreeMap::new(),
            transport: None,
            acp_session_id: None,
            agent_native_session_id: None,
            acp_child_pid: None,
            host_pid: None,
            external_agent: None,
        }
    }

    /// EXP-739: resuming a repo-LESS run must NOT open with a git command —
    /// its cwd is a scratch dir holding only `.exp-mcp.json`, so `git status`
    /// answers "not a git repository" and the resumed agent starts on an
    /// error. A repo-backed record keeps the worktree instructions.
    #[test]
    fn run_resume_prompt_skips_git_without_a_checkout() {
        let repo_less = render_run_resume_prompt(&resume_record(true), false);
        assert!(repo_less.contains("RESUMING the run \"Chat\""), "{repo_less}");
        assert!(!repo_less.contains("git status"), "{repo_less}");
        assert!(!repo_less.contains("git log"), "{repo_less}");
        assert!(
            repo_less.contains("no repository checked out"),
            "{repo_less}"
        );
        assert!(repo_less.contains("`exponential_*` MCP tools"), "{repo_less}");

        let repo_backed = render_run_resume_prompt(&resume_record(false), false);
        assert!(
            repo_backed.contains("You are on branch `exp/chat-1a2b3c4d` in its worktree"),
            "{repo_backed}"
        );
        assert!(repo_backed.contains("git log origin/main..HEAD"), "{repo_backed}");
    }

    #[test]
    fn prompt_frames_the_body_verbatim() {
        let prompt = render_action_prompt("Code review", "# Review\nScan the repo.", &[]);
        assert!(prompt.contains("team action \"Code review\""));
        // The body rides verbatim after the divider — never rewritten.
        assert!(prompt.ends_with("---\n\n# Review\nScan the repo."));
        // EXP-637: the preamble's last sentence IS the shared close-out —
        // EXP-679: without the tool a person-started run never gets.
        assert!(prompt.contains("leave the worktree clean"));
        assert!(!prompt.contains("exponential_sessions_end"));
        // EXP-764: no repo means the scratch note, never a branch.
        assert!(prompt.contains(SCRATCH_CWD_NOTE), "{prompt}");
        assert!(!prompt.contains("You work on branch"));
    }

    #[test]
    fn empty_inputs_render_the_exact_preamble() {
        // EXP-257 compat lock, EXP-637 refresh: an input-less action's prompt
        // is the preamble (ending in the shared close-out) + the raw body,
        // and nothing else.
        assert_eq!(
            render_action_prompt("Code review", "# Review\nScan the repo.", &[]),
            format!(
                "You are running the team action \"Code review\" for this user. Follow the \
instructions below exactly. The exponential MCP tools are available for issue, \
board, label, and comment operations. {}\n\n## Workspace\n\n{SCRATCH_CWD_NOTE}\n\n---\n\n\
# Review\nScan the repo.",
                crate::prompt::close_out(false)
            )
        );
        // EXP-530/EXP-637 structural proof: the trigger-aware and full
        // renderers with None arguments ARE the plain prompt — user-started,
        // repo-less runs can't drift.
        assert_eq!(
            render_action_prompt_with_trigger("Code review", "# Review\nScan the repo.", &[], None),
            render_action_prompt("Code review", "# Review\nScan the repo.", &[])
        );
        assert_eq!(
            render_action_prompt_full(
                "Code review",
                "# Review\nScan the repo.",
                &[],
                None,
                None,
                false,
                None,
            ),
            render_action_prompt("Code review", "# Review\nScan the repo.", &[])
        );
    }

    /// EXP-679: the close-out is the only thing `unattended` moves — and an
    /// automation's trigger implies it without the flag.
    #[test]
    fn only_an_unattended_action_run_is_told_to_call_the_close_out_tool() {
        let attended = render_action_prompt_full("Code review", "# Review", &[], None, None, false, None);
        assert!(!attended.contains("exponential_sessions_end"));
        assert!(attended.contains("This session stays open after you finish"));

        let unattended = render_action_prompt_full("Code review", "# Review", &[], None, None, true, None);
        assert!(unattended.contains("`exponential_sessions_end`"));
        assert!(unattended.contains("nobody is watching it"));

        // A trigger IS the unattended marker (EXP-530 automation runs).
        let note = TriggerNote {
            kind: TriggerNoteKind::Schedule {
                phrase: "daily at 07:00".to_string(),
            },
        };
        let automated =
            render_action_prompt_full("Code review", "# Review", &[], Some(&note), None, false, None);
        assert!(automated.contains("`exponential_sessions_end`"));
    }

    /// EXP-637: a repo-backed run is told where it lives and how work leaves
    /// the worktree — including the issue-LESS `pr_open` shape (EXP-626).
    #[test]
    fn workspace_section_names_the_branch_and_the_unlinked_pr_call() {
        let workspace = WorkspaceNote {
            branch: "exp/code-review-1a2b3c4d".to_string(),
            default_branch: "main".to_string(),
            repository_id: "repo-1".to_string(),
        };
        let prompt = render_action_prompt_full(
            "Code review",
            "# Review",
            &[],
            None,
            Some(&workspace),
            false,
            None,
        );
        assert!(prompt.contains("## Workspace"));
        assert!(prompt.contains("branch `exp/code-review-1a2b3c4d` in a dedicated worktree"));
        assert!(prompt.contains("cut from `origin/main`"));
        assert!(prompt.contains("`repositoryId: \"repo-1\"`"));
        assert!(prompt.contains("`head: \"exp/code-review-1a2b3c4d\"`"));
        assert!(prompt.contains("report `no_changes`"));
        assert!(prompt.contains("Never force-push; do not use `gh`."));
        // The section sits between the trigger block and the divider, and the
        // body still rides last and verbatim.
        assert!(prompt.ends_with("---\n\n# Review"));
    }

    /// EXP-825: the composer's free text sits BEFORE the `---` divider (after
    /// the workspace section) — the body after the divider is the owner's
    /// program and stays byte-verbatim; blank text renders the plain prompt.
    #[test]
    fn additional_instructions_sit_before_the_divider() {
        let plain = render_action_prompt_full("Weekly", "# Body", &[], None, None, false, None);
        assert_eq!(
            plain,
            render_action_prompt_full("Weekly", "# Body", &[], None, None, false, Some(" \n"))
        );
        let with = render_action_prompt_full(
            "Weekly",
            "# Body",
            &[],
            None,
            None,
            false,
            Some("Only the iOS lane this week."),
        );
        assert!(with.ends_with(
            "## Additional instructions from the requester\n\nOnly the iOS lane this \
week.\n\n---\n\n# Body"
        ));
        assert_eq!(
            with.replace(
                "## Additional instructions from the requester\n\nOnly the iOS lane this \
week.\n\n",
                ""
            ),
            plain
        );
    }

    /// EXP-615/EXP-637: the chat prompt is two preamble lines, a divider,
    /// then the user's own words — byte-for-byte, LAST.
    #[test]
    fn chat_prompt_puts_the_users_words_last_and_verbatim() {
        let workspace = WorkspaceNote {
            branch: "exp/chat-1a2b3c4d".to_string(),
            default_branch: "main".to_string(),
            repository_id: "repo-1".to_string(),
        };
        let prompt = chat_prompt("what does trunk_sync do?", Some(&workspace), false);
        assert!(prompt.starts_with("You work on branch `exp/chat-1a2b3c4d`"));
        // EXP-679: a person's chat has no close-out tool — it just reports
        // here and stays open.
        assert!(!prompt.contains("exponential_sessions_end"));
        assert!(prompt.contains("the session stays open afterwards"));
        assert!(prompt.ends_with("---\n\nwhat does trunk_sync do?"));
        // No workspace = the scratch note (EXP-764) + the close-out line.
        let bare = chat_prompt("hi", None, false);
        assert_eq!(
            bare,
            format!(
                "{SCRATCH_CWD_NOTE}\nWhen you are done, summarize what you did here; the session \
stays open afterwards, so keep answering follow-ups.\n\n---\n\nhi"
            )
        );
        // An unattended chat (another coding session started it) reports
        // through the tool that ends it.
        let unattended = chat_prompt("hi", None, true);
        assert!(unattended.contains("`exponential_sessions_end`"));
        assert!(unattended.contains("that call ends this run"));
    }

    /// EXP-822: a repo-less run once resolved its subject by scanning the
    /// user's home for `.git` directories, found a six-weeks-stale sibling of
    /// the real clone, and reported every wrong conclusion authoritatively.
    /// The note has to close that door explicitly, on the chat preamble AND
    /// on the action `## Workspace` section, and it has to leave the run a
    /// move that is not guessing.
    #[test]
    fn the_repo_less_note_forbids_hunting_for_a_checkout() {
        for prompt in [
            chat_prompt("push the ios release", None, false),
            render_action_prompt_full("Weekly", "# Body", &[], None, None, false, None),
        ] {
            assert!(
                prompt.contains("never go looking for one on this machine"),
                "{prompt}"
            );
            assert!(prompt.contains("someone else's checkout"), "{prompt}");
            assert!(prompt.contains("ask which one"), "{prompt}");
        }
        // A repo-BACKED run says where it is instead, and must never carry
        // the repo-less wording.
        let workspace = WorkspaceNote {
            branch: "exp/chat-1a2b3c4d".to_string(),
            default_branch: "main".to_string(),
            repository_id: "repo-1".to_string(),
        };
        let backed = chat_prompt("push the ios release", Some(&workspace), false);
        assert!(!backed.contains("never go looking"), "{backed}");
    }

    #[test]
    fn schedule_trigger_renders_the_section() {
        let note = TriggerNote {
            kind: TriggerNoteKind::Schedule {
                phrase: "daily at 07:00".to_string(),
            },
        };
        let prompt = render_action_prompt_with_trigger("Groom", "do it", &[], Some(&note));
        // EXP-764: the repo-less workspace note follows the trigger section.
        assert!(prompt.contains(
            "## Trigger\n\nThis run was started automatically by the action's schedule \
(daily at 07:00, device time).\n\n## Workspace"
        ));
        assert!(prompt.ends_with("---\n\ndo it"));
        assert_eq!(note.started_reason(), "schedule");
    }

    #[test]
    fn event_trigger_lists_capped_lines() {
        let note = TriggerNote {
            kind: TriggerNoteKind::Event {
                lines: vec![
                    "EXP-142 \"Fix the flaky test\" status In Progress → In Review".to_string(),
                    "EXP-150 \"New signup issue\" created".to_string(),
                ],
                omitted: 3,
            },
        };
        let prompt = render_action_prompt_with_trigger("Triage", "triage them", &[], Some(&note));
        assert!(prompt
            .contains("## Trigger\n\nThis run was started automatically because these issues \
changed:\n\n"));
        assert!(prompt.contains("- EXP-142 \"Fix the flaky test\" status In Progress → In Review\n"));
        // The host capped the lines — the overflow renders as ONE closing
        // list entry, then the next section (EXP-764: the scratch note).
        assert!(prompt.contains("- EXP-150 \"New signup issue\" created\n- …and 3 more.\n\n## Workspace"));
        assert_eq!(note.started_reason(), "event");

        // No overflow → no "…and more" line.
        let exact = TriggerNote {
            kind: TriggerNoteKind::Event {
                lines: vec!["EXP-1 \"One\" created".to_string()],
                omitted: 0,
            },
        };
        let prompt = render_action_prompt_with_trigger("Triage", "triage them", &[], Some(&exact));
        assert!(!prompt.contains("more."));
    }

    #[test]
    fn trigger_note_lines_are_flattened_to_one_line() {
        // Issue titles are user text — a crafted multi-line title must not
        // fake prompt sections (the single_line defence, like inputs).
        let note = TriggerNote {
            kind: TriggerNoteKind::Event {
                lines: vec!["EXP-9 \"evil\n## Fake section\" created".to_string()],
                omitted: 0,
            },
        };
        let prompt = render_action_prompt_with_trigger("Triage", "do it", &[], Some(&note));
        assert!(prompt.contains("- EXP-9 \"evil ## Fake section\" created\n"));
        assert!(!prompt.contains("\n## Fake section"));
    }

    #[test]
    fn trigger_section_lands_between_inputs_and_divider() {
        let inputs = vec![ActionInputValue {
            key: "scope".to_string(),
            label: "Scope".to_string(),
            input_type: "text".to_string(),
            value: "backlog".to_string(),
            display: None,
        }];
        let note = TriggerNote {
            kind: TriggerNoteKind::Schedule {
                phrase: "daily at 07:00".to_string(),
            },
        };
        let prompt = render_action_prompt_with_trigger("Groom", "do it", &inputs, Some(&note));
        let inputs_at = prompt.find("## Inputs").unwrap();
        let trigger_at = prompt.find("## Trigger").unwrap();
        let divider_at = prompt.find("---").unwrap();
        assert!(inputs_at < trigger_at && trigger_at < divider_at);
        assert!(prompt.ends_with("---\n\ndo it"));
    }

    #[test]
    fn inputs_render_between_preamble_and_divider() {
        let inputs = vec![
            ActionInputValue {
                key: "scope".to_string(),
                label: "Scope".to_string(),
                input_type: "text".to_string(),
                value: "only the backlog".to_string(),
                display: None,
            },
            ActionInputValue {
                key: "target_repo".to_string(),
                label: "Repository".to_string(),
                input_type: "repo".to_string(),
                value: "repo-uuid-1".to_string(),
                display: Some("acme/web".to_string()),
            },
        ];
        let prompt = render_action_prompt("Groom", "do it", &inputs);
        // Text input: display == value, no id suffix.
        assert!(prompt.contains("- Scope (text): only the backlog\n"));
        // Picked entity: readable display + the raw id in backticks.
        assert!(prompt.contains("- Repository (repo): acme/web (`repo-uuid-1`)\n"));
        // Section order: preamble → ## Inputs → divider → body verbatim.
        let inputs_at = prompt.find("## Inputs").unwrap();
        let divider_at = prompt.find("---").unwrap();
        assert!(inputs_at < divider_at);
        assert!(prompt.ends_with("---\n\ndo it"));
    }

    #[test]
    fn labels_and_displays_are_flattened_to_one_line() {
        // A crafted multi-line label/display must not fake prompt structure.
        let inputs = vec![ActionInputValue {
            key: "scope".to_string(),
            label: "Scope\n## Fake section".to_string(),
            input_type: "text".to_string(),
            value: "a\nb".to_string(),
            display: Some("a\r\nb".to_string()),
        }];
        let prompt = render_action_prompt("Groom", "do it", &inputs);
        assert!(prompt.contains("- Scope ## Fake section (text): a b\n"));
        assert!(!prompt.contains("\n## Fake section"));
    }

    #[test]
    fn create_action_prompt_targets_the_team_and_the_mcp_tools() {
        let prompt =
            create_action_prompt("team-123", "review the backlog weekly", None, None, false);
        // Names the exact team so Claude passes the right teamId.
        assert!(prompt.contains("team-123"));
        // Carries the user's one-line description verbatim.
        assert!(prompt.contains("review the backlog weekly"));
        // Points at the actions MCP tools (the run's MCP wiring exposes them).
        assert!(prompt.contains("exponential_actions_create"));
        assert!(prompt.contains("exponential_actions_list"));
        // EXP-257/EXP-825: the authored action may declare PICK inputs only.
        assert!(prompt.contains("`inputs` array"));
        assert!(prompt.contains("type: repo|board|pr|icon"));
        assert!(prompt.contains("Never declare free-text inputs"));
        // EXP-825: the composer hint the authored action may carry.
        assert!(prompt.contains("optional `promptPlaceholder` (≤200 chars)"));
        assert!(prompt.contains("otherwise omit it."));
        // EXP-530: an "Automation —" block in the description becomes the
        // `trigger` field, verbatim — otherwise the field stays absent.
        assert!(prompt.contains("optional `trigger` field"));
        assert!(prompt.contains("\"Automation —\" block"));
        assert!(prompt.contains("otherwise omit `trigger`"));
        // Read-only w.r.t. the tree — this run must not commit or push.
        assert!(prompt.contains("Do not commit, push"));
        // No repo input → Claude decides (default: leave repositoryId unset).
        assert!(prompt.contains("Leave `repositoryId` unset"));
    }

    #[test]
    fn create_action_prompt_binds_the_picked_repo_input() {
        // EXP-257: the builtin's optional repo INPUT pins the authored
        // action's repositoryId — id for the MCP call, display for context.
        let prompt = create_action_prompt(
            "team-123",
            "code review",
            Some(("repo-uuid-9", "acme/web")),
            None,
            false,
        );
        assert!(prompt.contains("Set `repositoryId` to `repo-uuid-9` (acme/web)"));
        assert!(!prompt.contains("Leave `repositoryId` unset"));
    }

    /// EXP-825: the creator prompt is a shipped program — byte-lock it. The
    /// `name`/`description` inputs are gone: the request text IS the
    /// composer's free text, the agent derives the name (or keeps one the
    /// request states), and it is told never to declare free-text inputs
    /// (the composer text reaches every run as an Additional instructions
    /// section instead).
    #[test]
    fn create_action_prompt_is_byte_locked_and_derives_the_name() {
        let prompt =
            create_action_prompt("team-123", "review the backlog weekly", None, None, false);
        assert_eq!(
            prompt,
            "Please create ONE new action for the Exponential team with id `team-123`. An \
action is a reusable markdown prompt that a team member later runs as an interactive \
agent session on their own desktop (the exponential MCP tools are available to that \
run). The user requested the action as:\n\n\"review the backlog weekly\"\n\n\
Derive a short name from the request (if it states a name, use that verbatim). \
Write a clear, focused markdown body for it: state the goal, the concrete steps, \
which exponential MCP tools to use (e.g. exponential_issues_list / \
exponential_issues_create / exponential_labels_list), and what to report at the end. \
Call `exponential_actions_list` for the team first so the name doesn't collide. \
Leave `repositoryId` unset unless the request clearly needs repository access \
(then pick the right repo id from `exponential_repositories_list`). Also set `icon` \
to the curated icon name that best fits the action (the same set as board icons, \
e.g. `bug`, `rocket`, `database`, `chart-line`). Create the action with \
`exponential_actions_create` (teamId, the name, a one-line description, the \
markdown body). `exponential_actions_create` also accepts an optional `inputs` array \
({key, label, type: repo|board|pr|icon, required?, placeholder?}) declaring pick \
inputs the runner fills before the run and the run receives as an \"## Inputs\" \
prompt section — declare them when the described action naturally varies per run (a \
target repository or board); otherwise omit the field. Never declare free-text \
inputs: whatever the requester types when running the action reaches the run as an \
Additional instructions section. `exponential_actions_create` also accepts an \
optional `promptPlaceholder` (≤200 chars): a short hint for what the requester \
should type there (e.g. \"Scope: which platforms, which version\"), shown as the \
composer's field placeholder while the action is picked — set one when the action \
expects that text; otherwise omit it. `exponential_actions_create` also accepts an \
optional `trigger` field: when the request contains an \"Automation —\" block, pass \
that block's JSON as `trigger` verbatim; otherwise omit `trigger`. Do not commit, \
push, or change any files — only call the MCP tools. After the action is created, \
report what you created here; the session stays open afterwards, so keep answering \
follow-ups."
        );
        // The retired input types never come back into the declared set.
        assert!(!prompt.contains("text|"));
        assert!(!prompt.contains("textarea"));
        assert!(!prompt.contains("Name the action exactly"));
        // A multi-line request rides verbatim (an "Automation —" block is
        // multi-line JSON the agent must copy).
        let block = create_action_prompt(
            "team-123",
            "triage\n\nAutomation —\n{\"kind\":\"schedule\"}",
            None,
            None,
            false,
        );
        assert!(block.contains("\"triage\n\nAutomation —\n{\"kind\":\"schedule\"}\""));
        // EXP-679: only the unattended creator run names the close-out tool.
        let unattended =
            create_action_prompt("team-123", "review the backlog weekly", None, None, true);
        assert!(unattended.contains("report with `exponential_sessions_end` (a one-paragraph"));
        assert!(unattended.contains("that call ends this run."));
    }

    /// EXP-273: a picked icon is pinned verbatim; an unpicked one delegates
    /// the choice to Claude rather than leaving the action glyph-less.
    #[test]
    fn create_action_prompt_binds_the_picked_icon_input() {
        let picked =
            create_action_prompt("team-123", "triage bugs", None, Some("bug"), false);
        assert!(picked.contains("Set `icon` to `bug`"));
        assert!(!picked.contains("best fits the"));

        let unpicked = create_action_prompt("team-123", "triage bugs", None, None, false);
        assert!(unpicked.contains("best fits the"));
        assert!(!unpicked.contains("Set `icon` to `"));
    }

    /// EXP-259: the fix-conflicts prompt is the whole builtin's program —
    /// byte-lock it so a drive-by edit can't change what the run executes.
    /// Deliberately rewritten for EXP-324: the rebase slot now carries the
    /// PR's LIVE base branch (launcher-resolved), and the prompt gained the
    /// `exponential_pr_retarget` self-heal for a base that goes stale
    /// mid-run.
    #[test]
    fn fix_pr_conflicts_prompt_rebases_pushes_and_merges_via_mcp() {
        let prompt = fix_pr_conflicts_prompt("EXP-42", "exp/EXP-42", "main", false, None);
        assert_eq!(
            prompt,
            "The pull request for `EXP-42` (branch `exp/EXP-42`) has merge conflicts and \
cannot be merged. You are in a worktree checked out to `exp/EXP-42`. First run \
`git fetch origin` and confirm `git rev-parse HEAD` equals \
`git rev-parse origin/exp/EXP-42` — if HEAD is missing commits that exist on \
`origin/exp/EXP-42`, stop and summarize the mismatch instead (force-pushing from a \
stale checkout would discard remote commits). Then rebase onto \
`origin/main` (the pull request's base branch), resolve every conflict \
preserving both sides' intent, and \
verify the build still passes. Then push the branch with `--force-with-lease` and \
merge the pull request by calling the `exponential_pr_merge` MCP tool with issueId \
`EXP-42` — merging completes every issue linked to the PR. If the merge is \
rejected because the base branch is stale, merged, or closed, call the \
`exponential_pr_retarget` MCP tool with the same issueId (omit `base` to retarget \
onto the repository's default branch), rebase onto the new base, push again with \
`--force-with-lease`, and retry the merge. If the conflicts \
cannot be resolved safely, do NOT push or merge: stop and summarize what blocks the \
rebase instead. Finally report the merge result here (merged, or why you stopped)."
        );
        // EXP-679: the unattended variant swaps ONLY the report sentence.
        let unattended = fix_pr_conflicts_prompt("EXP-42", "exp/EXP-42", "main", true, None);
        assert_eq!(
            unattended,
            prompt.replace(
                "Finally report the merge result here (merged, or why you stopped).",
                "Finally call `exponential_sessions_end` with the merge result (merged, or \
why you stopped)."
            )
        );
        // The worktree prompt DOES push (Claude owns the PR branch) and then
        // merges through the server rails — never `gh`, never a raw API call.
        assert!(prompt.contains("--force-with-lease"));
        assert!(prompt.contains("exponential_pr_merge"));
        // EXP-324: the mid-run self-heal for a base merged while the agent
        // works.
        assert!(prompt.contains("exponential_pr_retarget"));
        assert!(!prompt.contains("gh "));
        // Belt-and-braces alongside the launcher's ensure_branch_at_origin:
        // the agent re-verifies the checkout matches origin before pushing.
        assert!(prompt.contains("git rev-parse origin/exp/EXP-42"));
        // EXP-825: the composer's free text rides last.
        let extra = fix_pr_conflicts_prompt("EXP-42", "exp/EXP-42", "main", false, Some("Keep the lockfile from main."));
        assert_eq!(
            extra,
            format!("{prompt}\n\n## Additional instructions from the requester\n\nKeep the lockfile from main.\n")
        );
    }

    /// EXP-324: a stacked PR's rebase slot carries the PARENT branch the
    /// launcher resolved, not the repo default.
    #[test]
    fn fix_pr_conflicts_prompt_substitutes_a_stacked_base() {
        let prompt = fix_pr_conflicts_prompt("EXP-320", "exp/EXP-320", "exp/EXP-314", false, None);
        assert!(
            prompt.contains("rebase onto `origin/exp/EXP-314` (the pull request's base branch)")
        );
        assert!(!prompt.contains("origin/main"));
    }

    /// EXP-298: the builtin detail screens render these — they must resolve
    /// for both reserved ids, carry the run's real MCP tool, and stay
    /// placeholder-only (no fake team id or branch a reader could mistake for
    /// a real target).
    #[test]
    fn builtin_previews_render_the_real_prompts_with_placeholders() {
        let create = builtin_prompt_preview(domain::contract::BUILTIN_CREATE_ACTION_ID)
            .expect("create-action preview");
        assert!(create.contains("exponential_actions_create"));
        assert!(create.contains("<this team>"));

        let fix = builtin_prompt_preview(domain::contract::BUILTIN_FIX_CONFLICTS_ID)
            .expect("fix-conflicts preview");
        assert!(fix.contains("exponential_pr_merge"));
        assert!(fix.contains("<its PR branch>"));

        assert_eq!(builtin_prompt_preview("not-a-builtin"), None);
    }
}
