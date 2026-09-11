//! The coding-session seed prompt (masterplan-v3 §7.1 step 5), templated
//! with the issue identifier / title / description (the caller fetches them
//! from the sync store; the rendering here is pure text).
//!
//! Delivery ([`deliver_prompt`]) is size-gated: a small prompt rides argv as
//! claude's positional prompt directly ([`PromptDelivery::Direct`] — no
//! `PROMPT.md` indirection); an oversized one is written to `PROMPT.md` and
//! the positional becomes the [`SEED_LINE`] pointer
//! ([`deliver_prompt_file`]).
//!
//! The named MCP tools are real and verified: `exponential_pr_open` (the
//! server opens + links the PR through the GitHub App) and
//! `exponential_comments_list` (accepts human identifiers, so the prompt
//! passes the issue identifier — the launcher never needs the UUID). The
//! desktop never opens the PR itself — Claude does, via MCP. Issue status is
//! NOT Claude's job: the launcher flips backlog issues to `in_progress`
//! at launch (EXP-194 — under plan mode an MCP status call would only land
//! after plan approval), and the PR lifecycle owns in_review/done. The
//! plan/approval gate is NOT prompt text anymore: native plan mode
//! (`--permission-mode plan`, [`crate::argv::permission_args`]) owns it.

/// EXP-637 — the clean-worktree half of the close-out EVERY launcher prompt
/// ends with (issue, batch, action, chat and the two builtins): a run always
/// leaves the tree the way it found it.
pub const WORKTREE_CLEAN: &str = "Before you finish, leave the worktree clean: commit and push \
everything you keep, discard anything you don't (`git checkout -- .`, `git clean -fd` for files \
you created).";

/// EXP-679 — the second half, and it has TWO shapes because the server now
/// registers the `exponential_sessions_end` tool only for UNATTENDED runs
/// (`coding_sessions.started_reason` set: an automation's `schedule`/`event`,
/// or `agent` — a run another coding session started). A person-started run
/// never sees the tool and stays open afterwards (EXP-673), so telling it to
/// call one it doesn't have is a dead end; an unattended run must call it
/// last, because that call is what ends it and nobody is there to reply.
/// Decision 6 is spelled out in both: an agent that merges its own PR keeps
/// running server-side, and would otherwise assume the merge ended it.
pub fn close_out(unattended: bool) -> String {
    if unattended {
        format!(
            "{WORKTREE_CLEAN} Then report with the `exponential_sessions_end` MCP tool: a \
one-paragraph summary of what you did and anything left open. That call ends this run; \
nobody is watching it, so do not wait for replies. Merging your own PR never ends the \
session."
        )
    } else {
        format!(
            "{WORKTREE_CLEAN} This session stays open after you finish: summarize what you did \
here and keep answering follow-ups. Merging your own PR never ends the session."
        )
    }
}

/// EXP-825 — the heading of the section the unified composer's free text
/// lands in. Once a subject (issues, an action, a PR) is picked, whatever the
/// requester typed is not the prompt but a note ON the prompt: every launcher
/// prompt appends it under this heading, byte-identical across the desktop
/// and the CLI, so the agent can tell the program from the requester's
/// additions.
pub const ADDITIONAL_INSTRUCTIONS_HEADING: &str = "## Additional instructions from the requester";

/// EXP-825 — the `## Additional instructions from the requester` block for a
/// composer `prompt`, or the EMPTY string for none/blank, so every existing
/// byte-locked prompt is unchanged when nothing was typed. The block ends
/// with ONE newline; [`append_additional_instructions`] places it after a
/// finished prompt, the action renderer before its `---` divider.
pub fn additional_instructions(prompt: Option<&str>) -> String {
    match prompt.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => format!("{ADDITIONAL_INSTRUCTIONS_HEADING}

{text}
"),
        None => String::new(),
    }
}

/// Append [`additional_instructions`] to a finished prompt as its LAST
/// section (one blank line before the heading). `None`/blank returns the
/// prompt untouched.
pub fn append_additional_instructions(mut prompt: String, extra: Option<&str>) -> String {
    let section = additional_instructions(extra);
    if section.is_empty() {
        return prompt;
    }
    if !prompt.ends_with('\n') {
        prompt.push('\n');
    }
    prompt.push('\n');
    prompt.push_str(&section);
    prompt
}

/// Render the seed prompt: the §7.1 step-5 instruction paragraph, then the
/// issue context block it tells Claude to read. No plan-gate sentence —
/// native plan mode owns the approval gate. `unattended` (EXP-679) picks the
/// close-out: only an unattended run is told to call
/// `exponential_sessions_end`. `extra` (EXP-825) is the composer's free text,
/// appended last as the additional-instructions section.
pub fn render_prompt(
    identifier: &str,
    title: &str,
    description: Option<&str>,
    unattended: bool,
    extra: Option<&str>,
) -> String {
    let body = issue_body(description);
    let close_out = close_out(unattended);
    let prompt = format!(
        "Please read the issue context below and work on **{identifier}: {title}** in this \
repository. BEFORE implementing anything, read the issue's full comment thread by \
calling the `exponential_comments_list` MCP tool with issueId `{identifier}` — \
comments often refine or override the description and are part of the requirements. \
Implement the change, then commit and push your branch and open a pull \
request by calling the `exponential_pr_open` MCP tool. Opening the PR \
moves the issue to `in_review` automatically, and merging it later completes it to \
`done` — you do not set the issue status yourself. Do not use `gh`. {close_out}

## Issue context

### {identifier}: {title}

{body}
"
    );
    append_additional_instructions(prompt, extra)
}

/// Render the RESUME prompt (EXP-202) — the fallback when no previous
/// conversation is recoverable: a fresh session spawned into the issue's
/// reused worktree, told to pick the existing branch work back up instead of
/// starting over. Today only codex can land here (its exact-session recovery
/// — [`crate::codex_sessions`] — found no rollout for the worktree, e.g. it
/// was coded by another agent or the sessions were pruned); claude/pi always
/// resume natively via cwd-scoped `--continue`.
pub fn render_resume_prompt(
    identifier: &str,
    title: &str,
    default_branch: &str,
    unattended: bool,
    extra: Option<&str>,
) -> String {
    let close_out = close_out(unattended);
    let prompt = format!(
        "You are RESUMING work on **{identifier}: {title}** in this repository — a previous \
coding session already worked on this branch. First inspect the existing work: run \
`git log origin/{default_branch}..HEAD`, `git status`, and `git diff origin/{default_branch}` \
to see what was already done, and read the issue's full comment thread by calling the \
`exponential_comments_list` MCP tool with issueId `{identifier}` — comments often refine or \
override the requirements. Then continue the implementation from where it left off. When \
done, commit and push this branch; if no pull request exists yet, open one by calling the \
`exponential_pr_open` MCP tool — if one already exists, just push your commits to update it. \
Opening the PR moves the issue to `in_review` automatically, and merging it later completes \
it to `done` — you do not set the issue status yourself. Do not use `gh`. {close_out}
"
    );
    append_additional_instructions(prompt, extra)
}

/// The first thing a RESUMED run hears when its worktree had to be
/// re-created ([`crate::run_registry::RunRecord::workspace_reclaimed`]):
/// the prune reclaimed it once the PR landed, so the branch now starts
/// fresh from `origin/<base>` and the agent's memory of "my commits are on
/// this branch, my PR is open" is stale. Prepended to whatever else the
/// resume sends; `alone` (a native resume with no composer text) adds the
/// "wait" so the turn it opens does not put the agent back to work unasked.
pub fn reclaimed_workspace_note(branch: &str, default_branch: &str, alone: bool) -> String {
    let mut note = format!(
        "Your worktree was reclaimed after your earlier work landed on `{default_branch}` and has been re-created for this resume: branch `{branch}` now starts fresh from `origin/{default_branch}`, which already contains everything you committed before, so `git log origin/{default_branch}..HEAD` is empty and your previous pull request is closed. Any new change goes on this branch and needs a NEW pull request; if origin still has the old `{branch}`, push with `--force-with-lease`."
    );
    if alone {
        note.push_str(" Nothing has been asked yet: acknowledge this in one line and wait.");
    }
    note
}

/// The issue-context body.
fn issue_body(description: Option<&str>) -> &str {
    match description {
        Some(text) if !text.trim().is_empty() => text.trim_end(),
        _ => "(no description)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §7.1 step-5 template — exact bytes for a described issue a PERSON
    /// started (EXP-679: no `exponential_sessions_end`, the tool that run
    /// doesn't get).
    const EXPECTED: &str = "Please read the issue context below and work on **EXP-42: Fix login flicker** in this \
repository. BEFORE implementing anything, read the issue's full comment thread by \
calling the `exponential_comments_list` MCP tool with issueId `EXP-42` — \
comments often refine or override the description and are part of the requirements. \
Implement the change, then commit and push your branch and open a pull \
request by calling the `exponential_pr_open` MCP tool. Opening the PR \
moves the issue to `in_review` automatically, and merging it later completes it to \
`done` — you do not set the issue status yourself. Do not use `gh`. Before you finish, leave the \
worktree clean: commit and push everything you keep, discard anything you don't (`git checkout -- \
.`, `git clean -fd` for files you created). This session stays open after you finish: summarize \
what you did here and keep answering follow-ups. Merging your own PR never ends the session.

## Issue context

### EXP-42: Fix login flicker

The login page flickers on slow connections.

- Reproduce with network throttling
- Fix the flash of unstyled content
";

    #[test]
    fn renders_the_exact_template() {
        let description =
            "The login page flickers on slow connections.\n\n- Reproduce with network throttling\n- Fix the flash of unstyled content";
        assert_eq!(
            render_prompt("EXP-42", "Fix login flicker", Some(description), false, None),
            EXPECTED
        );
    }

    /// EXP-679: the close-out is the ONLY difference between an attended and
    /// an unattended run's prompt — and the `exponential_sessions_end`
    /// sentence appears in exactly one of them (the server registers that
    /// tool only for unattended runs).
    #[test]
    fn only_the_unattended_prompt_names_the_close_out_tool() {
        let attended = render_prompt("EXP-42", "Fix login flicker", None, false, None);
        assert!(!attended.contains("exponential_sessions_end"));
        assert!(attended.contains("This session stays open after you finish"));

        let unattended = render_prompt("EXP-42", "Fix login flicker", None, true, None);
        assert!(unattended.contains("`exponential_sessions_end`"));
        assert!(unattended.contains("That call ends this run; nobody is watching it"));
        assert!(!unattended.contains("This session stays open after you finish"));

        // Both leave the tree clean and both spell out decision 6.
        for prompt in [&attended, &unattended] {
            assert!(prompt.contains("leave the worktree clean"));
            assert!(prompt.contains("Merging your own PR never ends the session."));
        }
        // Same prompt otherwise — only the close-out swaps.
        assert_eq!(
            attended.replace(&close_out(false), ""),
            unattended.replace(&close_out(true), "")
        );
    }

    #[test]
    fn template_names_the_real_mcp_tools_and_carries_no_plan_gate() {
        let prompt = render_prompt("EXP-1", "T", None, true, None);
        assert!(prompt.contains("`exponential_pr_open`"));
        assert!(prompt.contains("`exponential_comments_list` MCP tool with issueId `EXP-1`"));
        assert!(prompt.contains("Do not use `gh`."));
        // The LAUNCHER owns the in_progress flip (EXP-194) — the prompt must
        // not delegate issue status to the agent (under plan mode that MCP
        // call would only land after plan approval).
        assert!(!prompt.contains("`exponential_issues_update_status`"));
        assert!(!prompt.contains("`in_progress` when you start"));
        // Native plan mode owns the approval gate — the prompt must not
        // re-impose a text gate.
        assert!(!prompt.contains("WAIT for explicit go-ahead"));
        assert!(!prompt.contains("propose a concise plan"));
        // EXP-637: the shared close-out — a clean worktree and a reported
        // summary are part of every run's contract now.
        assert!(prompt.contains("leave the worktree clean"));
        assert!(prompt.contains("`exponential_sessions_end`"));
    }

    #[test]
    fn reclaimed_workspace_note_names_the_branch_and_base() {
        let alone = reclaimed_workspace_note("exp/chat-1a2b3c4d", "main", true);
        assert!(alone.contains("branch `exp/chat-1a2b3c4d` now starts fresh from `origin/main`"), "{alone}");
        assert!(alone.contains("needs a NEW pull request"), "{alone}");
        assert!(alone.contains("`--force-with-lease`"), "{alone}");
        assert!(alone.ends_with("acknowledge this in one line and wait."), "{alone}");
        let with_text = reclaimed_workspace_note("exp/chat-1a2b3c4d", "main", false);
        assert!(!with_text.contains("acknowledge"), "{with_text}");
    }

    #[test]
    fn resume_template_names_the_real_mcp_tools_and_inspects_existing_work() {
        let prompt = render_resume_prompt("EXP-42", "Fix login flicker", "main", true, None);
        assert!(prompt.contains("RESUMING work on **EXP-42: Fix login flicker**"));
        assert!(prompt.contains("`exponential_comments_list` MCP tool with issueId `EXP-42`"));
        assert!(prompt.contains("`exponential_pr_open`"));
        assert!(prompt.contains("git diff origin/main"));
        assert!(prompt.contains("git log origin/main..HEAD"));
        assert!(prompt.contains("Do not use `gh`."));
        // Same rules as the seed prompt: status is never the agent's job and
        // there is no text plan gate.
        assert!(!prompt.contains("`exponential_issues_update_status`"));
        assert!(!prompt.contains("WAIT for explicit go-ahead"));
        assert!(prompt.contains("leave the worktree clean"));
        assert!(prompt.contains("`exponential_sessions_end`"));
    }

    #[test]
    fn missing_or_blank_description_gets_a_placeholder() {
        for description in [None, Some(""), Some("   \n  ")] {
            let prompt = render_prompt("EXP-2", "Title", description, false, None);
            assert!(prompt.contains("(no description)"), "for {description:?}");
        }
    }

    #[test]
    fn trailing_whitespace_in_description_is_trimmed() {
        let prompt = render_prompt("EXP-3", "T", Some("body text\n\n\n"), false, None);
        assert!(prompt.ends_with("body text\n"));
    }

    /// EXP-825: the composer's free text rides LAST under the shared
    /// heading, one blank line after the issue context; `None` and a blank
    /// string leave the byte-locked template untouched.
    #[test]
    fn additional_instructions_ride_last_and_blank_is_byte_identical() {
        let base = render_prompt("EXP-42", "Fix login flicker", Some("body"), false, None);
        assert_eq!(
            base,
            render_prompt("EXP-42", "Fix login flicker", Some("body"), false, Some("  \n"))
        );
        let with = render_prompt(
            "EXP-42",
            "Fix login flicker",
            Some("body"),
            false,
            Some("  Focus on the retry path.\n"),
        );
        assert_eq!(
            with,
            format!("{base}\n## Additional instructions from the requester\n\nFocus on the retry path.\n")
        );
        assert!(with.ends_with("body\n\n## Additional instructions from the requester\n\nFocus on the retry path.\n"));

        let resume = render_resume_prompt("EXP-42", "T", "main", false, Some("Rebase first."));
        let bare = render_resume_prompt("EXP-42", "T", "main", false, None);
        assert_eq!(
            resume,
            format!("{bare}\n## Additional instructions from the requester\n\nRebase first.\n")
        );
        assert_eq!(additional_instructions(None), "");
        assert_eq!(
            additional_instructions(Some("x")),
            format!("{ADDITIONAL_INSTRUCTIONS_HEADING}\n\nx\n")
        );
    }

}
