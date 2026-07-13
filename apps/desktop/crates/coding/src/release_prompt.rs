//! The release-run `PROMPT.md` (EXP-56) — the orchestrator seed for "start
//! coding on a whole release": one Claude session delegates ONE subagent per
//! issue in dependency waves, merges each finished issue branch into the
//! pushed integration branch (`exp/rel-<slug>`), reviews the combined diff,
//! and opens the ONE release PR via `exponential_release_pr_open`.
//!
//! Contract anchors (must stay true or the server side breaks):
//! - per-issue PRs keep base = the integration branch (`exponential_pr_open`
//!   accepts a free-form `base`); the 1-issue=1-branch=1-PR webhook contract
//!   is untouched;
//! - the integration branch's last path segment is LOWERCASE by construction
//!   ([`crate::release_launcher::release_slug`]), so the webhook's
//!   `parseIssueIdentifierFromBranch` can never mis-link the release PR —
//!   release PRs resolve by exact `pr_url` only;
//! - merging the release PR AUTO-SHIPS the release (server webhook).
//!
//! This module is pure text (like [`crate::prompt`]); the launcher feeds it
//! everything resolved.

/// Everything the release template needs, resolved by the launcher.
pub struct ReleasePromptArgs<'a> {
    pub release_id: &'a str,
    pub release_name: &'a str,
    /// The workspace `repositories` row id — `exponential_release_pr_open`
    /// input (the MCP tool can't infer the repo from a workspace-level
    /// release).
    pub repository_id: &'a str,
    pub default_branch: &'a str,
    /// `exp/rel-<slug>` — already checked out in the session's worktree.
    pub integration_branch: &'a str,
    pub issues: &'a [ReleasePromptIssue],
}

/// One issue's resolved facts for the template.
pub struct ReleasePromptIssue {
    /// `EXP-42`.
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    /// `exp/EXP-42` (the configured branch prefix applies).
    pub branch: String,
    /// Absolute worktree path, same layout as single-issue launches
    /// (`<clone>.worktrees/exp-EXP-42`).
    pub worktree: String,
    /// The `--agents` definition name (lowercased identifier, e.g. `exp-42`).
    pub agent_name: String,
}

/// Render the full release `PROMPT.md`.
pub fn render_release_prompt(args: &ReleasePromptArgs<'_>) -> String {
    let ReleasePromptArgs {
        release_id,
        release_name,
        repository_id,
        default_branch,
        integration_branch,
        issues,
    } = args;

    let mut sections = String::new();
    for issue in *issues {
        let body = match issue.description.as_deref() {
            Some(text) if !text.trim().is_empty() => text.trim_end(),
            _ => "(no description)",
        };
        sections.push_str(&format!(
            "### {id}: {title}\n\nsubagent `{agent}` · branch `{branch}` · worktree `{worktree}`\n\n{body}\n\n",
            id = issue.identifier,
            title = issue.title,
            agent = issue.agent_name,
            branch = issue.branch,
            worktree = issue.worktree,
        ));
    }
    let issue_count = issues.len();

    format!(
        "Please act as the RELEASE ORCHESTRATOR for **{release_name}** in this repository. \
Implement each of the {issue_count} issues below by delegating ONE subagent per issue, run \
independent issues in parallel, and integrate every result into the pushed release \
integration branch `{integration_branch}`.

## Ground rules

- Never implement an issue yourself in this worktree — every issue is a subagent's job. \
Integration, conflict resolution, review, and fixes ARE your job.
- One issue = one branch = one PR: each issue keeps its own branch and its own pull \
request with base `{integration_branch}`. Do NOT open per-issue PRs against \
`{default_branch}`.
- Never force-push, never rebase a branch that has been pushed.
- Do not use `gh` — GitHub writes go through the `exponential_*` MCP tools.

## Step 1 — plan dependency waves

Group the issues into waves: wave 1 holds issues independent of each other; a later wave \
holds issues that build on an earlier wave's changes. When unsure, prefer parallel — the \
incremental merges in step 3 surface conflicts safely.

## Step 2 — publish the integration branch

You are already on `{integration_branch}` (cut from `origin/{default_branch}`). After \
planning is complete (and approved when running in plan mode), publish the integration \
branch first so per-issue PRs can target it:

    git push -u origin {integration_branch}

## Step 3 — run each wave

For each issue in the current wave:

1. Create its worktree, branched from the CURRENT `{integration_branch}` (which already \
contains every previously merged issue). The worktree path is listed per issue below:

       git worktree add <worktree> -b <branch> {integration_branch}

   If the branch already exists from an earlier run, reuse it instead \
(`git worktree add <worktree> <branch>`, or skip if the worktree exists) and merge \
`{integration_branch}` into it before the subagent starts.
2. Spawn its pre-defined subagent (named in the issue list below) in the background, \
in parallel with the wave's other subagents.

As EACH subagent finishes (don't idle while others still run):

3. From THIS worktree, merge its branch into the integration branch and push:

       git merge --no-ff <branch>
       git push origin {integration_branch}

   Resolve any merge conflicts yourself — you hold the context of everything merged so \
far. Pushing makes GitHub mark that issue's PR as merged.

When a wave is fully merged, start the next wave — its worktrees now branch from the \
updated integration branch and see all prior work.

## Step 4 — combined review + the release PR

- If `origin/{default_branch}` has moved since the run started, merge it into \
`{integration_branch}` (resolve, commit, push).
- Review the COMBINED diff: `git diff origin/{default_branch}...{integration_branch}`. \
Hunt cross-issue inconsistencies, duplicated or conflicting edits, and regressions; run \
the repo's checks if it defines any. Fix problems directly on the integration branch \
with plain commits and push.
- Open the ONE release pull request by calling the `exponential_release_pr_open` MCP \
tool with releaseId `{release_id}`, repositoryId `{repository_id}`, head \
`{integration_branch}` (base defaults to `{default_branch}`). Merging that PR later \
ships the release automatically — do NOT merge it yourself.
- If any subagent failed to open its per-issue PR, open it yourself via \
`exponential_pr_open` (the branch is already pushed) with base `{integration_branch}`.

## Step 5 — summarize

End with a per-issue table (issue, branch, PR URL, status), the cross-issue conflicts \
you resolved, and anything a human should double-check.

## Per-subagent contract

Each subagent works exactly ONE issue, entirely inside that issue's worktree (path in \
the issue list — never this directory, never another issue's worktree or branch):

1. `exponential_issues_update_status` → `in_progress`.
2. Implement the issue; commit with clear messages.
3. Push the issue branch: `git push -u origin <branch>`.
4. Open the issue's PR via `exponential_pr_open` with its own issueId and base \
`{integration_branch}`.
5. `exponential_issues_update_status` → `done` once the PR is open.
6. Report back what changed and anything left open. Do not use `gh`.

## Issues in this release

{sections}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(id: &str, title: &str) -> ReleasePromptIssue {
        ReleasePromptIssue {
            identifier: id.to_string(),
            title: title.to_string(),
            description: Some(format!("Body of {id}")),
            branch: format!("exp/{id}"),
            worktree: format!("/repos/acme/web.worktrees/exp-{id}"),
            agent_name: id.to_lowercase(),
        }
    }

    fn args<'a>(issues: &'a [ReleasePromptIssue]) -> ReleasePromptArgs<'a> {
        ReleasePromptArgs {
            release_id: "rel-uuid-1",
            release_name: "0.4",
            repository_id: "repo-uuid-1",
            default_branch: "main",
            integration_branch: "exp/rel-0-4-1dc5fb4a",
            issues,
        }
    }

    #[test]
    fn names_the_real_mcp_tools_and_the_contract_anchors() {
        let issues = [issue("EXP-42", "Fix login"), issue("EXP-43", "Add badge")];
        let prompt = render_release_prompt(&args(&issues));
        // The three MCP tools of the release flow.
        assert!(prompt.contains("`exponential_release_pr_open`"));
        assert!(prompt.contains("`exponential_pr_open`"));
        assert!(prompt.contains("`exponential_issues_update_status`"));
        // Contract anchors: integration base, no per-issue PRs to default,
        // push-after-merge (GitHub auto-marks PRs), no gh, never force-push.
        assert!(prompt.contains("base `exp/rel-0-4-1dc5fb4a`"));
        assert!(prompt.contains("Do NOT open per-issue PRs against `main`"));
        assert!(prompt.contains("git push origin exp/rel-0-4-1dc5fb4a"));
        assert!(prompt.contains("Do not use `gh`"));
        assert!(prompt.contains("Never force-push"));
        // The release PR inputs ride verbatim.
        assert!(prompt.contains("releaseId `rel-uuid-1`"));
        assert!(prompt.contains("repositoryId `repo-uuid-1`"));
        // Per-issue facts.
        assert!(prompt.contains("### EXP-42: Fix login"));
        assert!(prompt.contains("subagent `exp-43`"));
        assert!(prompt.contains("/repos/acme/web.worktrees/exp-EXP-42"));
        assert!(prompt.contains("Body of EXP-43"));
    }

    /// Native plan mode owns the approval gate now: the prompt carries no
    /// gate text, and planning (Step 1) precedes the integration-branch push
    /// (Step 2) — in plan mode, pushes are blocked pre-approval, so the push
    /// must not come first (fix F8).
    #[test]
    fn wave_plan_step_precedes_the_push_and_no_gate_text_remains() {
        let issues = [issue("EXP-1", "T")];
        let prompt = render_release_prompt(&args(&issues));
        let plan_step = prompt
            .find("## Step 1 — plan dependency waves")
            .expect("wave-plan step missing");
        let push_step = prompt
            .find("## Step 2 — publish the integration branch")
            .expect("publish step missing");
        assert!(plan_step < push_step, "planning must precede the push");
        assert!(
            prompt.find("git push -u origin").unwrap() > plan_step,
            "the push command must sit after the wave-plan step"
        );
        assert!(prompt.contains(
            "After planning is complete (and approved when running in plan mode), \
publish the integration branch first so per-issue PRs can target it"
        ));
        // No prompt-text gate in either direction.
        assert!(!prompt.contains("WAIT for explicit go-ahead"));
        assert!(!prompt.contains("Work autonomously"));
    }

    #[test]
    fn blank_description_gets_a_placeholder() {
        let mut one = issue("EXP-9", "T");
        one.description = None;
        let issues = [one];
        let prompt = render_release_prompt(&args(&issues));
        assert!(prompt.contains("(no description)"));
    }
}
