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
//! `exponential_issues_update_status`. The desktop never opens the PR itself
//! — Claude does, via MCP. The plan/approval gate is NOT prompt text
//! anymore: native plan mode (`--permission-mode plan`,
//! [`crate::argv::permission_args`]) owns it.

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

/// Render the seed prompt: the §7.1 step-5 instruction paragraph, then the
/// issue context block it tells Claude to read. No plan-gate sentence —
/// native plan mode owns the approval gate.
pub fn render_prompt(identifier: &str, title: &str, description: Option<&str>) -> String {
    let body = issue_body(description);
    format!(
        "Please read the issue context below and work on **{identifier}: {title}** in this \
repository. Implement the change, then commit and push your branch and open a pull \
request by calling the `exponential_pr_open` MCP tool. You may set the issue status \
with `exponential_issues_update_status` (`in_progress` when you start). Opening the PR \
moves the issue to `in_review` automatically, and merging it later completes it to \
`done` — you do not set those yourself. Do not use `gh`.

## Issue context

### {identifier}: {title}

{body}
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

    /// The §7.1 step-5 template — exact bytes for a described issue.
    const EXPECTED: &str = "Please read the issue context below and work on **EXP-42: Fix login flicker** in this \
repository. Implement the change, then commit and push your branch and open a pull \
request by calling the `exponential_pr_open` MCP tool. You may set the issue status \
with `exponential_issues_update_status` (`in_progress` when you start). Opening the PR \
moves the issue to `in_review` automatically, and merging it later completes it to \
`done` — you do not set those yourself. Do not use `gh`.

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
            render_prompt("EXP-42", "Fix login flicker", Some(description)),
            EXPECTED
        );
    }

    #[test]
    fn template_names_the_real_mcp_tools_and_carries_no_plan_gate() {
        let prompt = render_prompt("EXP-1", "T", None);
        assert!(prompt.contains("`exponential_pr_open`"));
        assert!(prompt.contains("`exponential_issues_update_status`"));
        assert!(prompt.contains("`in_progress` when you start"));
        assert!(prompt.contains("Do not use `gh`."));
        // Native plan mode owns the approval gate — the prompt must not
        // re-impose a text gate.
        assert!(!prompt.contains("WAIT for explicit go-ahead"));
        assert!(!prompt.contains("propose a concise plan"));
    }

    #[test]
    fn missing_or_blank_description_gets_a_placeholder() {
        for description in [None, Some(""), Some("   \n  ")] {
            let prompt = render_prompt("EXP-2", "Title", description);
            assert!(prompt.contains("(no description)"), "for {description:?}");
        }
    }

    #[test]
    fn trailing_whitespace_in_description_is_trimmed() {
        let prompt = render_prompt("EXP-3", "T", Some("body text\n\n\n"));
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
            write_rendered_prompt(&dir, &render_prompt("EXP-9", "Title", Some("Body"))).unwrap();
        assert_eq!(path, dir.join("PROMPT.md"));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("**EXP-9: Title**"));
        assert!(content.contains("Body"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
