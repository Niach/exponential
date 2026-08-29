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
//! NOT Claude's job: the launcher flips backlog/todo issues to `in_progress`
//! at launch (EXP-194 — under plan mode an MCP status call would only land
//! after plan approval), and the PR lifecycle owns in_review/done. The
//! plan/approval gate is NOT prompt text anymore: native plan mode
//! (`--permission-mode plan`, [`crate::argv::permission_args`]) owns it.

use crate::mcp_json::write_private;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const PROMPT_FILE: &str = "PROMPT.md";

/// The fallback seed instruction (§7.1 step 7) — the positional argv prompt
/// when the rendered prompt itself is too big to ride argv (input written to
/// the PTY before the TUI enters raw mode is swallowed, so the prompt must
/// never ride stdin).
pub const SEED_LINE: &str = "Please read PROMPT.md in this directory, then follow it.";

/// Windows CreateProcess caps the whole command line at 32,767 chars —
/// keep ~4KB headroom for program path + flags.
pub const PROMPT_ARGV_MAX_BYTES: usize = 28 * 1024;

/// How the rendered prompt reaches the spawned claude.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptDelivery {
    /// The full rendered prompt rides argv as the positional prompt.
    Direct(String),
    /// The prompt lives in `PROMPT.md`; the positional is [`SEED_LINE`].
    File,
}

impl PromptDelivery {
    /// The positional argv prompt for this delivery.
    pub fn positional(&self) -> &str {
        match self {
            PromptDelivery::Direct(rendered) => rendered,
            PromptDelivery::File => SEED_LINE,
        }
    }
}

/// Size-gated delivery: a prompt within [`PROMPT_ARGV_MAX_BYTES`] goes
/// [`PromptDelivery::Direct`] — any stale `PROMPT.md` from an earlier launch
/// is best-effort removed so claude can never read an outdated copy. Bigger
/// prompts fall back to [`deliver_prompt_file`].
pub fn deliver_prompt(
    worktree: &Path,
    clone: &Path,
    rendered: &str,
) -> io::Result<PromptDelivery> {
    if rendered.len() <= PROMPT_ARGV_MAX_BYTES {
        let _ = fs::remove_file(worktree.join(PROMPT_FILE));
        return Ok(PromptDelivery::Direct(rendered.to_string()));
    }
    deliver_prompt_file(worktree, clone, rendered)
}

/// Unconditional file delivery — the oversized-prompt fallback. Writes the
/// prompt and keeps it git-invisible via the clone's shared
/// `.git/info/exclude` (best-effort by design — see
/// [`crate::git_worktree::ensure_local_excludes`]).
pub fn deliver_prompt_file(
    worktree: &Path,
    clone: &Path,
    rendered: &str,
) -> io::Result<PromptDelivery> {
    write_rendered_prompt(worktree, rendered)?;
    let _ = crate::git_worktree::ensure_local_excludes(clone, &[PROMPT_FILE]);
    Ok(PromptDelivery::File)
}

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

/// Render the seed prompt: the §7.1 step-5 instruction paragraph, then the
/// issue context block it tells Claude to read. No plan-gate sentence —
/// native plan mode owns the approval gate. `unattended` (EXP-679) picks the
/// close-out: only an unattended run is told to call
/// `exponential_sessions_end`.
pub fn render_prompt(
    identifier: &str,
    title: &str,
    description: Option<&str>,
    unattended: bool,
) -> String {
    let body = issue_body(description);
    let close_out = close_out(unattended);
    format!(
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
    )
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
) -> String {
    let close_out = close_out(unattended);
    format!(
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
    )
}

/// The issue-context body.
fn issue_body(description: Option<&str>) -> &str {
    match description {
        Some(text) if !text.trim().is_empty() => text.trim_end(),
        _ => "(no description)",
    }
}

/// Write an already-rendered prompt into the worktree root (overwritten
/// every launch so a re-edited issue reseeds correctly).
pub fn write_rendered_prompt(worktree: &Path, content: &str) -> io::Result<PathBuf> {
    let path = worktree.join(PROMPT_FILE);
    write_private(&path, content)?;
    Ok(path)
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

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-prompt-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn renders_the_exact_template() {
        let description =
            "The login page flickers on slow connections.\n\n- Reproduce with network throttling\n- Fix the flash of unstyled content";
        assert_eq!(
            render_prompt("EXP-42", "Fix login flicker", Some(description), false),
            EXPECTED
        );
    }

    /// EXP-679: the close-out is the ONLY difference between an attended and
    /// an unattended run's prompt — and the `exponential_sessions_end`
    /// sentence appears in exactly one of them (the server registers that
    /// tool only for unattended runs).
    #[test]
    fn only_the_unattended_prompt_names_the_close_out_tool() {
        let attended = render_prompt("EXP-42", "Fix login flicker", None, false);
        assert!(!attended.contains("exponential_sessions_end"));
        assert!(attended.contains("This session stays open after you finish"));

        let unattended = render_prompt("EXP-42", "Fix login flicker", None, true);
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
        let prompt = render_prompt("EXP-1", "T", None, true);
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
    fn resume_template_names_the_real_mcp_tools_and_inspects_existing_work() {
        let prompt = render_resume_prompt("EXP-42", "Fix login flicker", "main", true);
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
            let prompt = render_prompt("EXP-2", "Title", description, false);
            assert!(prompt.contains("(no description)"), "for {description:?}");
        }
    }

    #[test]
    fn trailing_whitespace_in_description_is_trimmed() {
        let prompt = render_prompt("EXP-3", "T", Some("body text\n\n\n"), false);
        assert!(prompt.ends_with("body text\n"));
    }

    #[test]
    fn seed_line_matches_the_spec() {
        assert_eq!(SEED_LINE, "Please read PROMPT.md in this directory, then follow it.");
    }

    /// A small prompt goes Direct — no `PROMPT.md` on disk, and a STALE copy
    /// from an earlier (oversized or pre-rework) launch is removed.
    #[test]
    fn small_prompt_delivers_direct_and_removes_the_stale_file() {
        let dir = temp_dir("direct");
        std::fs::write(dir.join(PROMPT_FILE), "stale from an earlier launch").unwrap();
        let delivery = deliver_prompt(&dir, &dir, "small prompt").unwrap();
        assert_eq!(delivery, PromptDelivery::Direct("small prompt".to_string()));
        assert_eq!(delivery.positional(), "small prompt");
        assert!(!dir.join(PROMPT_FILE).exists(), "stale PROMPT.md must be removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The size gate is exact: PROMPT_ARGV_MAX_BYTES rides argv, one byte
    /// more falls back to the file + seed-line pointer.
    #[test]
    fn delivery_flips_to_file_exactly_past_the_argv_budget() {
        let dir = temp_dir("boundary");
        let at_limit = "x".repeat(PROMPT_ARGV_MAX_BYTES);
        match deliver_prompt(&dir, &dir, &at_limit).unwrap() {
            PromptDelivery::Direct(rendered) => assert_eq!(rendered.len(), PROMPT_ARGV_MAX_BYTES),
            PromptDelivery::File => panic!("at-limit prompt must ride argv"),
        }
        assert!(!dir.join(PROMPT_FILE).exists());

        let over_limit = "x".repeat(PROMPT_ARGV_MAX_BYTES + 1);
        let delivery = deliver_prompt(&dir, &dir, &over_limit).unwrap();
        assert_eq!(delivery, PromptDelivery::File);
        assert_eq!(delivery.positional(), SEED_LINE);
        assert_eq!(
            std::fs::read_to_string(dir.join(PROMPT_FILE)).unwrap(),
            over_limit
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Explicit file delivery lands even a tiny prompt in `PROMPT.md`, and
    /// the clone's `.git/info/exclude` keeps it git-invisible.
    #[test]
    fn file_delivery_is_unconditional_and_excluded_from_git() {
        let dir = temp_dir("file");
        let clone = dir.join("clone");
        std::fs::create_dir_all(clone.join(".git")).unwrap();
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();

        let delivery = deliver_prompt_file(&worktree, &clone, "tiny").unwrap();
        assert_eq!(delivery, PromptDelivery::File);
        assert_eq!(delivery.positional(), SEED_LINE);
        assert_eq!(
            std::fs::read_to_string(worktree.join(PROMPT_FILE)).unwrap(),
            "tiny"
        );
        let exclude = std::fs::read_to_string(clone.join(".git/info/exclude")).unwrap();
        assert!(exclude.lines().any(|line| line == PROMPT_FILE), "exclude: {exclude}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_into_the_worktree_root() {
        let dir = temp_dir("write");
        let path =
            write_rendered_prompt(&dir, &render_prompt("EXP-9", "Title", Some("Body"), false)).unwrap();
        assert_eq!(path, dir.join("PROMPT.md"));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("**EXP-9: Title**"));
        assert!(content.contains("Body"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
