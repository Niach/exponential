//! The coding-session seed prompt (masterplan-v3 §7.1 step 5), templated
//! with the issue identifier / title / description (the caller fetches them
//! from the sync store; the rendering here is pure text).
//!
//! EXP-773: the prompt rides the ACP JSON-RPC `session/prompt` payload, so
//! there is no argv cap and no `PROMPT.md` indirection left to size-gate
//! against — the whole text simply reaches the agent.
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

use crate::launcher::StackIssue;

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

/// EXP-897 — everything the `## Stacked work` section needs. Borrowed: the
/// launcher owns the plan ([`crate::launcher::StackLaunch`]) and the branch
/// names it just resolved.
pub struct StackPromptArgs<'a> {
    /// The chain BELOW this run, bottom first (this issue excluded).
    pub chain: &'a [StackIssue],
    /// The issue directly below this run — the foundation. `None` = nothing
    /// to stack on, and the section renders as nothing at all.
    pub lower: Option<&'a StackIssue>,
    /// What this run's branch was actually cut from (the foundation's branch
    /// when it resolved on origin, else the board base).
    pub base_branch: &'a str,
    /// The board's own base branch — what the stack ultimately lands on.
    pub default_branch: &'a str,
    /// This machine's device id, for the `exponential_sessions_start` that
    /// builds a missing foundation. Empty = unknown, and the `deviceId`
    /// argument is left out of the call.
    pub device_id: &'a str,
    /// This run's branch (`exp/<IDENT>`).
    pub branch: &'a str,
}

/// The branch to NAME for a stack member: its recorded PR branch when there
/// is one, else the branch the launcher's default prefix would cut for it
/// (a foundation that has not been started yet has no row to read).
fn stack_branch(issue: &StackIssue) -> String {
    issue
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("exp/{}", issue.identifier))
}

/// EXP-897 — the `## Stacked work` section: the whole stack bottom first,
/// then the procedure. Three renderer rules:
///
/// - step 1 ("build the foundation") is OMITTED when the foundation already
///   has an open pull request — there is nothing to build;
/// - `stackOnIssueId` is omitted from that start when the chain has ONE
///   member: the foundation is the bottom, so it stacks on nothing;
/// - "cut from" names the REAL base, which is the board's branch whenever
///   the foundation's branch could not be resolved on origin.
///
/// Empty string when there is no foundation (a degenerate plan) — the caller
/// then renders the ordinary prompt.
pub fn stack_section(identifier: &str, args: &StackPromptArgs<'_>) -> String {
    let Some(lower) = args.lower else {
        return String::new();
    };
    let lower_branch = stack_branch(lower);
    let mut listing = String::new();
    for (index, issue) in args.chain.iter().enumerate() {
        let state = match issue.open_branch() {
            Some(branch) => format!("pull request open (`{branch}`)"),
            None => "no pull request yet".to_string(),
        };
        listing.push_str(&format!("{}. `{}` - {state}\n", index + 1, issue.identifier));
    }
    listing.push_str(&format!(
        "{}. `{identifier}` - this run (`{}`, cut from `origin/{}`)\n",
        args.chain.len() + 1,
        args.branch,
        args.base_branch
    ));

    let mut steps: Vec<String> = Vec::new();
    if lower.open_branch().is_none() {
        // The issue BELOW the foundation, if any — what the child start
        // stacks on in turn.
        let below = args
            .chain
            .len()
            .checked_sub(2)
            .and_then(|index| args.chain.get(index));
        let device = if args.device_id.is_empty() {
            String::new()
        } else {
            format!("`deviceId: \"{}\"`, ", args.device_id)
        };
        let stack_on = match below {
            Some(below) => format!(" and `stackOnIssueId: \"{}\"`", below.issue_id),
            None => String::new(),
        };
        steps.push(format!(
            "**Build the foundation first if it does not exist.** `{}` has no open pull request \
yet: call `exponential_sessions_start` with {device}`issueId: \"{}\"`{stack_on}, then STOP and \
wait - do not implement `{}` yourself. Its questions and its finish arrive here as \
`[Exponential child run ...]` user messages; answer them with `exponential_sessions_message`. A \
start refused because a run already holds that issue means the foundation is already being \
built: wait for its pull request the same way.",
            lower.identifier, lower.issue_id, lower.identifier
        ));
    }
    steps.push(format!(
        "**Verify the foundation before you build on it.** Run `git fetch origin {lower_branch}`, \
read `git diff origin/{}...origin/{lower_branch}`, and run whatever it touches. If it needs \
refinement, send that back to its run with `exponential_sessions_message` and wait for the next \
finish message - do not fix its code on your branch.",
        args.default_branch
    ));
    steps.push(format!(
        "**Put your work on top of it.** `git fetch origin {lower_branch} && git rebase \
origin/{lower_branch}` - or `git reset --hard origin/{lower_branch}` when you have committed \
nothing yet. Then implement `{identifier}` as usual."
    ));
    steps.push(format!(
        "**Open your pull request on top of it.** Call `exponential_pr_open` with \
`stackOnIssueId: \"{}\"`: it bases your PR on `{lower_branch}` instead of `{}`. When the \
foundation merges, your PR is retargeted for you; if a merge is ever refused for a stale base, \
call `exponential_pr_retarget`, rebase, push with `--force-with-lease`, and merge again.",
        lower.issue_id, args.default_branch
    ));
    steps.push(
        "**Real decisions go UP, never down.** If the foundation turns out to be wrong, ask with \
`exponential_sessions_ask_parent` and `to: \"root\"` (or `\"user\"` when a person has to choose) \
instead of re-planning the stack yourself."
            .to_string(),
    );
    let mut program = String::new();
    for (index, step) in steps.iter().enumerate() {
        program.push_str(&format!("{}. {step}\n", index + 1));
    }

    format!(
        "## Stacked work

**{identifier}** is stacked on work that is not finished yet. The stack, bottom first:

{listing}
`{}` (issueId `{}`) is the foundation directly below you. Work the stack in this order:

{program}",
        lower.identifier, lower.issue_id
    )
}

/// Render the seed prompt: the §7.1 step-5 instruction paragraph, then the
/// issue context block it tells Claude to read. No plan-gate sentence —
/// native plan mode owns the approval gate. `unattended` (EXP-679) picks the
/// close-out: only an unattended run is told to call
/// `exponential_sessions_end`. `extra` (EXP-825) is the composer's free text,
/// appended last as the additional-instructions section. `stack` (EXP-897)
/// renders [`stack_section`] between the issue context and that free text;
/// `None` leaves the byte-locked template untouched.
pub fn render_prompt(
    identifier: &str,
    title: &str,
    description: Option<&str>,
    unattended: bool,
    extra: Option<&str>,
    stack: Option<&StackPromptArgs<'_>>,
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
    // EXP-897: between the issue context and the requester's own additions.
    let prompt = match stack.map(|stack| stack_section(identifier, stack)) {
        Some(section) if !section.is_empty() => format!("{prompt}\n{section}"),
        _ => prompt,
    };
    append_additional_instructions(prompt, extra)
}

/// Render the RESUME prompt (EXP-202) — the fallback when no previous
/// conversation is recoverable: a fresh session spawned into the issue's
/// reused worktree, told to pick the existing branch work back up instead of
/// starting over. Today only codex can land here (its exact-session recovery
/// — [`crate::codex_sessions`] — found no rollout for the worktree, e.g. it
/// was coded by another agent or the sessions were pruned); claude always
/// resumes natively via cwd-scoped `--continue`.
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
/// the agent's memory of "my commits are on this branch, my PR is open" may
/// be stale. Prepended to whatever else the resume sends; `alone` (a native
/// resume with no composer text) adds the "wait" so the turn it opens does
/// not put the agent back to work unasked.
///
/// `ahead` = `git rev-list --count origin/<base>..<branch>` in the RE-CREATED
/// worktree, and it decides which of two true stories the agent hears. The
/// automatic reclaimers only delete a branch they found merged, so the branch
/// is cut fresh and `ahead` is 0: the work landed, the PR is closed, new work
/// needs a new PR. A worktree a PERSON removed (the Devices page, or by hand)
/// leaves the branch and its commits alone, so `ahead` is not 0 and the old
/// story would be a lie twice over — it would send the agent to open a second
/// pull request for a head that already has one, which GitHub refuses.
pub fn reclaimed_workspace_note(
    branch: &str,
    default_branch: &str,
    ahead: usize,
    alone: bool,
) -> String {
    let mut note = if ahead == 0 {
        format!(
            "Your worktree was reclaimed after your earlier work landed on `{default_branch}` and has been re-created for this resume: branch `{branch}` now starts fresh from `origin/{default_branch}`, which already contains everything you committed before, so `git log origin/{default_branch}..HEAD` is empty and your previous pull request is closed. Any new change goes on this branch and needs a NEW pull request; if origin still has the old `{branch}`, push with `--force-with-lease`."
        )
    } else {
        let commits = if ahead == 1 { "commit" } else { "commits" };
        format!(
            "Your worktree was removed and has been re-created for this resume: branch `{branch}` still carries its {ahead} {commits} on top of `origin/{default_branch}`, so `git log origin/{default_branch}..HEAD` shows the earlier work and nothing of it was lost. Inspect it before you continue. If you already opened a pull request for `{branch}`, that one is still the PR to update — push to it rather than opening a second one; use `--force-with-lease` if origin disagrees."
        )
    };
    if alone {
        note.push_str(" Nothing has been asked yet: acknowledge this in one line and wait.");
    }
    note
}

/// EXP-935: what a resume that SWITCHES the account opens its first turn
/// with. A mid-run switch (`steer.startSession({resumeSessionId, account})`)
/// carries no prompt of its own, so a native reload used to sit on
/// `RESUMED_IDLE_CAPTION` until a person typed — from the outside, switching
/// the account did nothing at all. The run says why it was restarted and
/// carries on by itself.
pub const ACCOUNT_SWITCH_CONTINUE_PROMPT: &str = "Your account was switched; the previous run \
ended on purpose. Continue exactly where you left off: pick up the task in progress, do not \
restart or re-plan finished work, and report when done.";

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
            render_prompt("EXP-42", "Fix login flicker", Some(description), false, None, None),
            EXPECTED
        );
    }

    /// EXP-679: the close-out is the ONLY difference between an attended and
    /// an unattended run's prompt — and the `exponential_sessions_end`
    /// sentence appears in exactly one of them (the server registers that
    /// tool only for unattended runs).
    #[test]
    fn only_the_unattended_prompt_names_the_close_out_tool() {
        let attended = render_prompt("EXP-42", "Fix login flicker", None, false, None, None);
        assert!(!attended.contains("exponential_sessions_end"));
        assert!(attended.contains("This session stays open after you finish"));

        let unattended = render_prompt("EXP-42", "Fix login flicker", None, true, None, None);
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
        let prompt = render_prompt("EXP-1", "T", None, true, None, None);
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
        let alone = reclaimed_workspace_note("exp/chat-1a2b3c4d", "main", 0, true);
        assert!(alone.contains("branch `exp/chat-1a2b3c4d` now starts fresh from `origin/main`"), "{alone}");
        assert!(alone.contains("needs a NEW pull request"), "{alone}");
        assert!(alone.contains("`--force-with-lease`"), "{alone}");
        assert!(alone.ends_with("acknowledge this in one line and wait."), "{alone}");
        let with_text = reclaimed_workspace_note("exp/chat-1a2b3c4d", "main", 0, false);
        assert!(!with_text.contains("acknowledge"), "{with_text}");
    }

    /// A worktree a person removed keeps the branch: the agent must hear that
    /// its commits survived and that an existing PR is still the one to push
    /// to, never "open a NEW pull request" (GitHub refuses a second PR for
    /// the same head).
    #[test]
    fn reclaimed_workspace_note_says_the_work_survived_when_the_branch_is_ahead() {
        let one = reclaimed_workspace_note("exp/EXP-42", "main", 1, false);
        assert!(one.contains("still carries its 1 commit on"), "{one}");
        assert!(!one.contains("NEW pull request"), "{one}");
        assert!(one.contains("still the PR to update"), "{one}");
        let many = reclaimed_workspace_note("exp/EXP-42", "main", 3, true);
        assert!(many.contains("still carries its 3 commits on"), "{many}");
        assert!(many.contains("nothing of it was lost"), "{many}");
        assert!(many.ends_with("acknowledge this in one line and wait."), "{many}");
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
            let prompt = render_prompt("EXP-2", "Title", description, false, None, None);
            assert!(prompt.contains("(no description)"), "for {description:?}");
        }
    }

    #[test]
    fn trailing_whitespace_in_description_is_trimmed() {
        let prompt = render_prompt("EXP-3", "T", Some("body text\n\n\n"), false, None, None);
        assert!(prompt.ends_with("body text\n"));
    }

    /// EXP-825: the composer's free text rides LAST under the shared
    /// heading, one blank line after the issue context; `None` and a blank
    /// string leave the byte-locked template untouched.
    #[test]
    fn additional_instructions_ride_last_and_blank_is_byte_identical() {
        let base = render_prompt("EXP-42", "Fix login flicker", Some("body"), false, None, None);
        assert_eq!(
            base,
            render_prompt("EXP-42", "Fix login flicker", Some("body"), false, Some("  \n"), None)
        );
        let with = render_prompt(
            "EXP-42",
            "Fix login flicker",
            Some("body"),
            false,
            Some("  Focus on the retry path.\n"),
            None,
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


    // ---- EXP-897: the stacked-work section ----

    fn stack_issue(identifier: &str, branch: Option<&str>, pr_state: Option<&str>) -> StackIssue {
        StackIssue {
            issue_id: format!("id-{}", identifier.to_lowercase()),
            identifier: identifier.to_string(),
            branch: branch.map(str::to_string),
            pr_state: pr_state.map(str::to_string),
        }
    }

    /// The chain is listed bottom first, each member with its real PR state,
    /// and this run closes it with the base it was actually cut from.
    #[test]
    fn stack_section_lists_the_chain_bottom_first() {
        let chain = vec![
            stack_issue("EXP-10", None, None),
            stack_issue("EXP-11", Some("exp/EXP-11"), Some("open")),
        ];
        let section = stack_section(
            "EXP-12",
            &StackPromptArgs {
                chain: &chain,
                lower: chain.last(),
                base_branch: "exp/EXP-11",
                default_branch: "main",
                device_id: "dev-1",
                branch: "exp/EXP-12",
            },
        );
        assert!(section.starts_with("## Stacked work\n"), "{section}");
        assert!(section.contains("1. `EXP-10` - no pull request yet\n"), "{section}");
        assert!(
            section.contains("2. `EXP-11` - pull request open (`exp/EXP-11`)\n"),
            "{section}"
        );
        assert!(
            section.contains("3. `EXP-12` - this run (`exp/EXP-12`, cut from `origin/exp/EXP-11`)"),
            "{section}"
        );
        assert!(
            section.contains("`EXP-11` (issueId `id-exp-11`) is the foundation directly below you."),
            "{section}"
        );
        // ASCII only: the playbook's no-em-dash rule holds for the prompt's
        // stack section too.
        assert!(!section.contains('\u{2014}'), "{section}");
    }

    /// A foundation WITH an open PR is already built: the section opens on
    /// "verify", never on "start it".
    #[test]
    fn stack_section_skips_the_build_step_when_the_foundation_has_a_pr() {
        let chain = vec![stack_issue("EXP-11", Some("exp/EXP-11"), Some("open"))];
        let section = stack_section(
            "EXP-12",
            &StackPromptArgs {
                chain: &chain,
                lower: chain.last(),
                base_branch: "exp/EXP-11",
                default_branch: "main",
                device_id: "dev-1",
                branch: "exp/EXP-12",
            },
        );
        assert!(!section.contains("Build the foundation first"), "{section}");
        assert!(!section.contains("exponential_sessions_start"), "{section}");
        assert!(
            section.contains("1. **Verify the foundation before you build on it.**"),
            "{section}"
        );
        assert!(section.contains("4. **Real decisions go UP, never down.**"), "{section}");
    }

    /// An UNBUILT foundation gets step 1: start it, with this device and —
    /// when something sits below it — its own `stackOnIssueId`, then stop.
    #[test]
    fn stack_section_tells_an_unbuilt_foundation_to_be_started_first() {
        let chain = vec![
            stack_issue("EXP-10", None, None),
            stack_issue("EXP-11", None, None),
        ];
        let section = stack_section(
            "EXP-12",
            &StackPromptArgs {
                chain: &chain,
                lower: chain.last(),
                base_branch: "main",
                default_branch: "main",
                device_id: "dev-1",
                branch: "exp/EXP-12",
            },
        );
        assert!(
            section.contains("1. **Build the foundation first if it does not exist.**"),
            "{section}"
        );
        assert!(
            section.contains(
                "call `exponential_sessions_start` with `deviceId: \"dev-1\"`, \
`issueId: \"id-exp-11\"` and `stackOnIssueId: \"id-exp-10\"`"
            ),
            "{section}"
        );
        assert!(section.contains("then STOP and wait"), "{section}");
        assert!(section.contains("[Exponential child run ...]"), "{section}");
        assert!(section.contains("5. **Real decisions go UP, never down.**"), "{section}");
        // The bottom of the stack stacks on nothing, and a device-less host
        // simply leaves the argument out.
        let bottom = vec![stack_issue("EXP-11", None, None)];
        let alone = stack_section(
            "EXP-12",
            &StackPromptArgs {
                chain: &bottom,
                lower: bottom.last(),
                base_branch: "main",
                default_branch: "main",
                device_id: "",
                branch: "exp/EXP-12",
            },
        );
        assert!(
            alone.contains("call `exponential_sessions_start` with `issueId: \"id-exp-11\"`, then STOP"),
            "{alone}"
        );
        // The bottom of the stack has nothing below it, so its start carries
        // no `stackOnIssueId` (step 4's `pr_open` still does).
        assert!(
            !alone.contains("`issueId: \"id-exp-11\"` and `stackOnIssueId"),
            "{alone}"
        );
        assert!(!alone.contains("deviceId"), "{alone}");
    }

    /// The four tools the procedure names, and the one it must NOT: the
    /// close-out lives in `close_out(unattended)` alone.
    #[test]
    fn stack_section_names_stack_on_issue_id_and_never_the_close_out() {
        let chain = vec![stack_issue("EXP-11", Some("exp/EXP-11"), Some("open"))];
        let section = stack_section(
            "EXP-12",
            &StackPromptArgs {
                chain: &chain,
                lower: chain.last(),
                base_branch: "exp/EXP-11",
                default_branch: "main",
                device_id: "dev-1",
                branch: "exp/EXP-12",
            },
        );
        assert!(
            section.contains("Call `exponential_pr_open` with `stackOnIssueId: \"id-exp-11\"`"),
            "{section}"
        );
        assert!(section.contains("`exponential_sessions_message`"), "{section}");
        assert!(section.contains("`exponential_pr_retarget`"), "{section}");
        assert!(
            section.contains("`exponential_sessions_ask_parent` and `to: \"root\"`"),
            "{section}"
        );
        assert!(!section.contains("exponential_sessions_end"), "{section}");
        assert!(!section.contains("leave the worktree clean"), "{section}");
        // Nothing to stack on renders nothing at all.
        assert_eq!(
            stack_section(
                "EXP-12",
                &StackPromptArgs {
                    chain: &[],
                    lower: None,
                    base_branch: "main",
                    default_branch: "main",
                    device_id: "dev-1",
                    branch: "exp/EXP-12",
                },
            ),
            ""
        );
    }

    /// In the prompt the section sits between the issue context and the
    /// requester's own additions — and `None` leaves the byte-locked template
    /// untouched.
    #[test]
    fn stacked_prompt_rides_between_the_issue_context_and_the_extra() {
        let chain = vec![stack_issue("EXP-11", Some("exp/EXP-11"), Some("open"))];
        let args = StackPromptArgs {
            chain: &chain,
            lower: chain.last(),
            base_branch: "exp/EXP-11",
            default_branch: "main",
            device_id: "dev-1",
            branch: "exp/EXP-12",
        };
        let plain = render_prompt("EXP-12", "T", Some("body"), false, None, None);
        assert_eq!(plain, render_prompt("EXP-12", "T", Some("body"), false, None, None));
        assert!(!plain.contains("## Stacked work"));

        let stacked = render_prompt("EXP-12", "T", Some("body"), false, None, Some(&args));
        assert_eq!(stacked, format!("{plain}\n{}", stack_section("EXP-12", &args)));
        let context = stacked.find("## Issue context").expect("issue context");
        let stack = stacked.find("## Stacked work").expect("stack section");
        assert!(context < stack, "{stacked}");

        let with_extra = render_prompt(
            "EXP-12",
            "T",
            Some("body"),
            false,
            Some("Mind the retry path."),
            Some(&args),
        );
        let extra = with_extra
            .find(ADDITIONAL_INSTRUCTIONS_HEADING)
            .expect("extra section");
        assert!(with_extra.find("## Stacked work").expect("stack") < extra, "{with_extra}");
    }

}
