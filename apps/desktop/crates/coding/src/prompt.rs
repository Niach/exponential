//! `PROMPT.md` — the plan-first seed prompt (masterplan-v3 §7.1 step 5),
//! templated with the issue identifier / title / description (the caller
//! fetches them from the sync store; this module is pure text).
//!
//! The named MCP tools are real and verified: `exponential_pr_open` (the
//! server opens + links the PR through the GitHub App) and
//! `exponential_issues_update_status`. The desktop never opens the PR itself
//! — Claude does, via MCP.

use crate::mcp_json::write_private;
use std::io;
use std::path::{Path, PathBuf};

pub const PROMPT_FILE: &str = "PROMPT.md";

/// The line the launcher types into the fresh Claude tab (§7.1 step 7).
pub const SEED_LINE: &str = "Read PROMPT.md in this directory, then follow it.";

/// Render `PROMPT.md`: the §7.1 step-5 instruction paragraph verbatim, then
/// the issue context block it tells Claude to read.
pub fn render_prompt(identifier: &str, title: &str, description: Option<&str>) -> String {
    let body = match description {
        Some(text) if !text.trim().is_empty() => text.trim_end(),
        _ => "(no description)",
    };
    format!(
        "You are working on **{identifier}: {title}** in this repository. Read the issue \
context below. **First, propose a concise plan and WAIT for explicit go-ahead before \
writing code.** Once approved, implement the change, then commit and push your branch \
and open a pull request by calling the `exponential_pr_open` MCP tool. You may set the \
issue status with `exponential_issues_update_status` (`in_progress` when you start, \
`done` when the PR is open). Do not use `gh`.

## Issue context

### {identifier}: {title}

{body}
"
    )
}

/// Write `PROMPT.md` into the worktree root (overwritten every launch so a
/// re-edited issue reseeds correctly).
pub fn write_prompt(
    worktree: &Path,
    identifier: &str,
    title: &str,
    description: Option<&str>,
) -> io::Result<PathBuf> {
    let path = worktree.join(PROMPT_FILE);
    write_private(&path, &render_prompt(identifier, title, description))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §7.1 step-5 template — exact bytes for a described issue.
    const EXPECTED: &str = "You are working on **EXP-42: Fix login flicker** in this repository. Read the issue \
context below. **First, propose a concise plan and WAIT for explicit go-ahead before \
writing code.** Once approved, implement the change, then commit and push your branch \
and open a pull request by calling the `exponential_pr_open` MCP tool. You may set the \
issue status with `exponential_issues_update_status` (`in_progress` when you start, \
`done` when the PR is open). Do not use `gh`.

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
            render_prompt("EXP-42", "Fix login flicker", Some(description)),
            EXPECTED
        );
    }

    #[test]
    fn template_names_the_real_mcp_tools_and_bans_gh() {
        let prompt = render_prompt("EXP-1", "T", None);
        assert!(prompt.contains("`exponential_pr_open`"));
        assert!(prompt.contains("`exponential_issues_update_status`"));
        assert!(prompt.contains("`in_progress` when you start"));
        assert!(prompt.contains("Do not use `gh`."));
        assert!(prompt.contains("WAIT for explicit go-ahead"));
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
        assert_eq!(SEED_LINE, "Read PROMPT.md in this directory, then follow it.");
    }

    #[test]
    fn writes_into_the_worktree_root() {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-prompt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_prompt(&dir, "EXP-9", "Title", Some("Body")).unwrap();
        assert_eq!(path, dir.join("PROMPT.md"));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("**EXP-9: Title**"));
        assert!(content.contains("Body"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
