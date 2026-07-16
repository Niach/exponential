//! The batch-run seed prompt — ONE Claude session implementing SEVERAL
//! issues directly on the batch branch. Deliberately loose (no dependency
//! waves, no per-issue worktrees, no pre-defined subagents): the issues may
//! overlap, so Claude organizes the work itself and lands everything on one
//! branch, then opens ONE combined PR via `exponential_pr_open` with
//! `issueIds` (the server links every issue to it; merging the PR later
//! completes them all).
//!
//! Pure text, like [`crate::prompt`]. Delivery is the normal size-gated
//! [`crate::prompt::deliver_prompt`] — the old release-run "always a file"
//! rule existed only because `--agents` subagent prompts referenced
//! `PROMPT.md` sections, and `--agents` is gone.

use crate::batch_launcher::BatchIssueSpec;

/// Inputs for [`render_batch_prompt`] — everything is snapshotted by the
/// dialog / resolved by the launcher before rendering.
pub struct BatchPromptArgs<'a> {
    pub default_branch: &'a str,
    /// The batch working branch (`exp/batch-<id8>`), already checked out.
    pub branch: &'a str,
    pub issues: &'a [BatchIssueSpec],
}

/// Render the batch seed prompt: ground rules + workflow + one context
/// section per issue. Mirrors the single-issue template's contract anchors
/// (`exponential_pr_open`, `exponential_issues_update_status`, `in_progress`,
/// no `gh`) and carries NO plan-gate text — native plan mode owns the approval
/// gate. Opening the PR flips every issue to `in_review` server-side and
/// merging it completes them to `done`, so the agent only sets `in_progress`.
pub fn render_batch_prompt(args: &BatchPromptArgs<'_>) -> String {
    let n = args.issues.len();
    let branch = args.branch;
    let default_branch = args.default_branch;
    let issue_ids = args
        .issues
        .iter()
        .map(|issue| format!("\"{}\"", issue.issue_id))
        .collect::<Vec<_>>()
        .join(", ");

    let mut prompt = format!(
        "Please read the issue context below and implement ALL {n} issues in this repository, \
working directly on the current branch `{branch}`.

## Ground rules

- The issues may overlap (shared files, related behavior). Handle overlap sensibly: \
implement shared changes once and keep the combined result coherent — you own the \
full set, not one issue at a time.
- Organize the work however you see fit — your own subagents, parallel exploration, \
any order. All changes land on `{branch}`.
- Never force-push. Do not use `gh` — GitHub writes go through the `exponential_*` \
MCP tools.

## Workflow

1. You may set each issue's status with `exponential_issues_update_status` \
(`in_progress` when you start it). Opening the combined PR moves every issue to \
`in_review` automatically, and merging it later completes them to `done` — you \
do not set those yourself.
2. Implement the issues; commit with clear messages and push the branch: \
`git push -u origin {branch}`.
3. Open ONE combined pull request for the whole batch by calling the \
`exponential_pr_open` MCP tool with `issueIds: [{issue_ids}]` and \
`head: \"{branch}\"` (base defaults to `{default_branch}`).
4. End with a short per-issue summary (what changed, anything left open).

## Issue context
"
    );

    for issue in args.issues {
        let identifier = &issue.issue_identifier;
        let title = &issue.title;
        let body = issue_body(issue.description.as_deref());
        prompt.push_str(&format!(
            "\n### {identifier}: {title}\n\nissueId `{}`\n\n{body}\n",
            issue.issue_id
        ));
    }

    prompt
}

/// The issue-context body (mirrors [`crate::prompt`]'s placeholder).
fn issue_body(description: Option<&str>) -> &str {
    match description {
        Some(text) if !text.trim().is_empty() => text.trim_end(),
        _ => "(no description)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues() -> Vec<BatchIssueSpec> {
        vec![
            BatchIssueSpec {
                issue_id: "11111111-1111-4111-8111-111111111111".to_string(),
                issue_identifier: "EXP-42".to_string(),
                title: "Fix login flicker".to_string(),
                description: Some("Steps in the issue.".to_string()),
            },
            BatchIssueSpec {
                issue_id: "22222222-2222-4222-8222-222222222222".to_string(),
                issue_identifier: "EXP-43".to_string(),
                title: "Add badge".to_string(),
                description: None,
            },
        ]
    }

    fn rendered() -> String {
        render_batch_prompt(&BatchPromptArgs {
            default_branch: "main",
            branch: "exp/batch-a1b2c3d4",
            issues: &issues(),
        })
    }

    /// The contract anchors: the real MCP tools, the combined-PR call with
    /// every issue UUID + the head branch, the push line, and the status
    /// phrasing shared with the single-issue template.
    #[test]
    fn template_names_the_real_mcp_tools_and_the_combined_pr_contract() {
        let prompt = rendered();
        assert!(prompt.contains("implement ALL 2 issues"));
        assert!(prompt.contains("`exponential_pr_open`"));
        assert!(prompt.contains(
            "issueIds: [\"11111111-1111-4111-8111-111111111111\", \
\"22222222-2222-4222-8222-222222222222\"]"
        ));
        assert!(prompt.contains("head: \"exp/batch-a1b2c3d4\""));
        assert!(prompt.contains("base defaults to `main`"));
        assert!(prompt.contains("git push -u origin exp/batch-a1b2c3d4"));
        assert!(prompt.contains("`exponential_issues_update_status`"));
        assert!(prompt.contains("`in_progress` when you start"));
        assert!(prompt.contains("Do not use `gh`"));
        assert!(prompt.contains("Never force-push"));
        // The old orchestrator contract must be GONE: no release PR tool, no
        // pre-defined subagents, no wave plan, no per-issue worktrees.
        assert!(!prompt.contains("exponential_release_pr_open"));
        assert!(!prompt.contains("pre-defined subagent"));
        assert!(!prompt.contains("wave"));
        assert!(!prompt.contains("worktree"));
    }

    /// Per-issue sections carry identifier, title, UUID, and the description
    /// (or its placeholder).
    #[test]
    fn issue_sections_carry_identifier_uuid_and_description() {
        let prompt = rendered();
        assert!(prompt.contains("### EXP-42: Fix login flicker"));
        assert!(prompt.contains("issueId `11111111-1111-4111-8111-111111111111`"));
        assert!(prompt.contains("Steps in the issue."));
        assert!(prompt.contains("### EXP-43: Add badge"));
        assert!(prompt.contains("(no description)"));
    }

    /// Native plan mode owns the approval gate — no textual plan gate.
    #[test]
    fn no_plan_gate_text() {
        let prompt = rendered();
        assert!(!prompt.contains("WAIT for explicit go-ahead"));
        assert!(!prompt.contains("propose a concise plan"));
    }
}
