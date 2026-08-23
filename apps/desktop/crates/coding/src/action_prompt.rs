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
    format!(
        "You are running the team action \"{name}\" for this user. Follow the \
instructions below exactly. The exponential MCP tools are available for issue, \
board, label, and comment operations. When you finish, summarize what you did \
(and anything you deliberately skipped) as your final message.\n\n{inputs_section}{trigger_section}---\n\n{body}"
    )
}

/// Prompt for the builtin "Create action" run (EXP-257 — the successor of
/// the actions panel's "Describe with your agent" creator, EXP-253 / L24). It
/// runs as a regular ACTION session in a scratch dir with the exponential
/// MCP tools wired, and asks Claude to author ONE action for `team_id` from
/// the user's one-line `description`. `repo` is the optional repo INPUT the
/// user picked — `(id, display)`; when set, the authored action must bind to
/// that repository. `icon` is the optional curated glyph the user picked
/// (EXP-273). It must NOT touch git or files — it only calls the MCP tools.
pub fn create_action_prompt(
    team_id: &str,
    description: &str,
    repo: Option<(&str, &str)>,
    icon: Option<&str>,
    name: Option<&str>,
) -> String {
    // EXP-615: a typed name is binding — the agent must not "improve" it.
    // Flattened like every other user string that reaches a prompt; `None`
    // (the pre-EXP-615 shape) renders the empty string, so the prompt stays
    // byte-identical to what it was.
    let name_rule = match name
        .map(single_line)
        .filter(|name| !name.is_empty())
    {
        Some(name) => format!(
            " Name the action exactly `{name}` — the user typed that name, so use it verbatim."
        ),
        None => String::new(),
    };
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
        None => "Leave `repositoryId` unset unless the description clearly needs repository \
access (then pick the right repo id from `exponential_repositories_list`)."
            .to_string(),
    };
    format!(
        "Please create ONE new action for the Exponential team with id `{team_id}`. An \
action is a reusable markdown prompt that a team member later runs as an interactive \
Claude session on their own desktop (the exponential MCP tools are available to that \
run). The user described the action they want as:\n\n\"{description}\"\n\n\
Write a clear, focused markdown body for it: state the goal, the concrete steps, \
which exponential MCP tools to use (e.g. exponential_issues_list / \
exponential_issues_create / exponential_labels_list), and what to report at the end. \
Call `exponential_actions_list` for the team first so the name doesn't collide. \
{repo_rule}{icon_rule}{name_rule} Create the action with `exponential_actions_create` (teamId, a \
short name, a one-line description, the markdown body). `exponential_actions_create` also \
accepts an optional `inputs` array ({{key, label, type: text|repo|board|pr|icon, required?, \
placeholder?}}) declaring run-time inputs the runner fills in a form and the run \
receives as an \"## Inputs\" prompt section — declare inputs when the described \
action naturally varies per run (a free-text scope, a target repository or board); \
otherwise omit the field. `exponential_actions_create` also accepts an optional \
`trigger` field: when the description contains an \"Automation —\" block, pass that \
block's JSON as `trigger` verbatim; otherwise omit `trigger`. Do not commit, push, \
or change any files — only call the MCP tools."
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
pub fn fix_pr_conflicts_prompt(identifier: &str, branch: &str, base_branch: &str) -> String {
    format!(
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
rebase instead."
    )
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
            "<the description you type when you run it>",
            None,
            None,
            None,
        )),
        // EXP-615: the chat builtin has no shipped program to preview — its
        // prompt IS whatever the user types, so there is nothing to show.
        domain::contract::BUILTIN_CHAT_ID => None,
        domain::contract::BUILTIN_FIX_CONFLICTS_ID => Some(fix_pr_conflicts_prompt(
            "<the issue you pick>",
            "<its PR branch>",
            "<the PR's base branch>",
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_frames_the_body_verbatim() {
        let prompt = render_action_prompt("Code review", "# Review\nScan the repo.", &[]);
        assert!(prompt.contains("team action \"Code review\""));
        // The body rides verbatim after the divider — never rewritten.
        assert!(prompt.ends_with("---\n\n# Review\nScan the repo."));
        // The preamble asks for a closing summary.
        assert!(prompt.contains("summarize what you did"));
    }

    #[test]
    fn empty_inputs_stay_byte_identical_to_the_legacy_prompt() {
        // EXP-257 compat lock: an input-less action's prompt must not move a
        // byte.
        assert_eq!(
            render_action_prompt("Code review", "# Review\nScan the repo.", &[]),
            "You are running the team action \"Code review\" for this user. Follow the \
instructions below exactly. The exponential MCP tools are available for issue, \
board, label, and comment operations. When you finish, summarize what you did \
(and anything you deliberately skipped) as your final message.\n\n---\n\n# Review\nScan the repo."
        );
        // EXP-530 structural proof: the trigger-aware renderer with None IS
        // the legacy prompt — user-started runs can't drift.
        assert_eq!(
            render_action_prompt_with_trigger("Code review", "# Review\nScan the repo.", &[], None),
            render_action_prompt("Code review", "# Review\nScan the repo.", &[])
        );
    }

    #[test]
    fn schedule_trigger_renders_the_section() {
        let note = TriggerNote {
            kind: TriggerNoteKind::Schedule {
                phrase: "daily at 07:00".to_string(),
            },
        };
        let prompt = render_action_prompt_with_trigger("Groom", "do it", &[], Some(&note));
        assert!(prompt.contains(
            "## Trigger\n\nThis run was started automatically by the action's schedule \
(daily at 07:00, device time).\n\n---"
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
        // list entry, then the divider.
        assert!(prompt.contains("- EXP-150 \"New signup issue\" created\n- …and 3 more.\n\n---"));
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
            create_action_prompt("team-123", "review the backlog weekly", None, None, None);
        // Names the exact team so Claude passes the right teamId.
        assert!(prompt.contains("team-123"));
        // Carries the user's one-line description verbatim.
        assert!(prompt.contains("review the backlog weekly"));
        // Points at the actions MCP tools (the run's MCP wiring exposes them).
        assert!(prompt.contains("exponential_actions_create"));
        assert!(prompt.contains("exponential_actions_list"));
        // EXP-257: the authored action may declare typed run-time inputs.
        assert!(prompt.contains("`inputs` array"));
        assert!(prompt.contains("type: text|repo|board"));
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
            None,
        );
        assert!(prompt.contains("Set `repositoryId` to `repo-uuid-9` (acme/web)"));
        assert!(!prompt.contains("Leave `repositoryId` unset"));
    }

    /// EXP-615: the optional `name` input. A typed name is pinned verbatim;
    /// NO name must leave the prompt byte-identical to the pre-EXP-615 one —
    /// the creator run is a shipped program, and the parameter must not have
    /// moved a byte for every existing caller.
    #[test]
    fn create_action_prompt_name_is_optional_and_byte_stable() {
        let named = create_action_prompt(
            "team-123",
            "review the backlog weekly",
            None,
            None,
            Some("Backlog groomer"),
        );
        assert!(named.contains(
            " Name the action exactly `Backlog groomer` — the user typed that name, so use \
it verbatim."
        ));
        // The sentence rides between the icon rule and the create call.
        let name_at = named.find("Name the action exactly").unwrap();
        let create_at = named.find("Create the action with").unwrap();
        assert!(name_at < create_at);

        // A crafted multi-line name cannot fake prompt structure.
        let hostile = create_action_prompt(
            "team-123",
            "x",
            None,
            None,
            Some("Evil\n## Fake section"),
        );
        assert!(hostile.contains("`Evil ## Fake section`"));
        assert!(!hostile.contains("\n## Fake section"));

        // Byte-identity lock: None (and a blank string) render the legacy
        // prompt exactly.
        let legacy = create_action_prompt("team-123", "review the backlog weekly", None, None, None);
        assert!(!legacy.contains("Name the action exactly"));
        assert_eq!(
            legacy,
            create_action_prompt("team-123", "review the backlog weekly", None, None, Some("  "))
        );
        assert_eq!(
            legacy,
            "Please create ONE new action for the Exponential team with id `team-123`. An \
action is a reusable markdown prompt that a team member later runs as an interactive \
Claude session on their own desktop (the exponential MCP tools are available to that \
run). The user described the action they want as:\n\n\"review the backlog weekly\"\n\n\
Write a clear, focused markdown body for it: state the goal, the concrete steps, \
which exponential MCP tools to use (e.g. exponential_issues_list / \
exponential_issues_create / exponential_labels_list), and what to report at the end. \
Call `exponential_actions_list` for the team first so the name doesn't collide. \
Leave `repositoryId` unset unless the description clearly needs repository access \
(then pick the right repo id from `exponential_repositories_list`). Also set `icon` \
to the curated icon name that best fits the action (the same set as board icons, \
e.g. `bug`, `rocket`, `database`, `chart-line`). Create the action with \
`exponential_actions_create` (teamId, a short name, a one-line description, the \
markdown body). `exponential_actions_create` also accepts an optional `inputs` array \
({key, label, type: text|repo|board|pr|icon, required?, placeholder?}) declaring \
run-time inputs the runner fills in a form and the run receives as an \"## Inputs\" \
prompt section — declare inputs when the described action naturally varies per run (a \
free-text scope, a target repository or board); otherwise omit the field. \
`exponential_actions_create` also accepts an optional `trigger` field: when the \
description contains an \"Automation —\" block, pass that block's JSON as `trigger` \
verbatim; otherwise omit `trigger`. Do not commit, push, or change any files — only \
call the MCP tools."
        );
    }

    /// EXP-273: a picked icon is pinned verbatim; an unpicked one delegates
    /// the choice to Claude rather than leaving the action glyph-less.
    #[test]
    fn create_action_prompt_binds_the_picked_icon_input() {
        let picked = create_action_prompt("team-123", "triage bugs", None, Some("bug"), None);
        assert!(picked.contains("Set `icon` to `bug`"));
        assert!(!picked.contains("best fits the"));

        let unpicked = create_action_prompt("team-123", "triage bugs", None, None, None);
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
        let prompt = fix_pr_conflicts_prompt("EXP-42", "exp/EXP-42", "main");
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
rebase instead."
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
    }

    /// EXP-324: a stacked PR's rebase slot carries the PARENT branch the
    /// launcher resolved, not the repo default.
    #[test]
    fn fix_pr_conflicts_prompt_substitutes_a_stacked_base() {
        let prompt = fix_pr_conflicts_prompt("EXP-320", "exp/EXP-320", "exp/EXP-314");
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
