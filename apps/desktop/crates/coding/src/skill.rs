//! EXP-763 — the run playbook: how Exponential works, loaded into EVERY
//! coding session's system prompt at start (issue, batch, action, chat,
//! resume, agent shell; PTY and ACP; claude, codex and pi).
//!
//! Why a system-prompt append and not a bigger launch prompt or a skill file:
//! the seed prompts stay lean (and a resume gets none), a skill directory
//! only lands a DESCRIPTION in context until the model decides to read the
//! body, and the MCP `instructions` string is capped at 2k chars. Each agent
//! has an additive text channel that is rebuilt on every launch, resume
//! included: claude `--append-system-prompt`, codex `developer_instructions`
//! (its own developer message, additive to AGENTS.md; `-c` on the PTY,
//! `developerInstructions` on `thread/start`/`thread/resume` in the
//! app-server), pi `--append-system-prompt`. All three take TEXT, so nothing
//! new is written into the user's repo, scratch dir or agent home.
//!
//! The document is markdown without front matter and names tools by their
//! exact `exponential_*` names so a deferred tool is one search away. The web
//! test `apps/web/src/lib/mcp/context-budget.test.ts` reads the same file
//! and fails when it names a tool the server does not register or grows
//! past [`RUN_SKILL_MAX_BYTES`].
//!
//! External ACP agents (`AgentKind::External`) get nothing: they have no
//! known additive instruction channel.

/// The playbook, verbatim.
pub const RUN_SKILL: &str = include_str!("skill.md");

/// Hard ceiling — the PTY path carries the text on argv next to the seed
/// prompt ([`crate::prompt::prompt_argv_budget`]), so the playbook is
/// budgeted in the same Windows command-line cap.
pub const RUN_SKILL_MAX_BYTES: usize = 6 * 1024;

/// Every `exponential_*` name the playbook mentions, deduplicated, in order
/// of first appearance. The web drift gate checks the same set against the
/// registered tool surface.
pub fn mentioned_tools(text: &str) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(offset) = text[i..].find("exponential_") {
        let start = i + offset;
        let mut end = start + "exponential_".len();
        while end < bytes.len() && (bytes[end].is_ascii_lowercase() || bytes[end] == b'_') {
            end += 1;
        }
        let name = &text[start..end];
        // The bare prefix (`exponential_*`) is prose, not a tool.
        if name.len() > "exponential_".len() && !names.contains(&name) {
            names.push(name);
        }
        i = end;
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_playbook_stays_inside_its_byte_budget() {
        assert!(!RUN_SKILL.trim().is_empty());
        assert!(
            RUN_SKILL.len() <= RUN_SKILL_MAX_BYTES,
            "skill.md is {} bytes, cap {RUN_SKILL_MAX_BYTES}",
            RUN_SKILL.len()
        );
    }

    #[test]
    fn the_playbook_teaches_the_deferred_tools_by_exact_name() {
        // EXP-763: the whole point — the tools a run never discovered on its
        // own, spelled the way tool search finds them.
        let tools = mentioned_tools(RUN_SKILL);
        for name in [
            "exponential_sessions_start",
            "exponential_issues_create",
            "exponential_issue_relations_add",
            "exponential_report_bug",
            "exponential_devices_list",
            "exponential_sessions_message",
            "exponential_sessions_end",
            "exponential_sessions_ask_parent",
        ] {
            assert!(tools.contains(&name), "playbook never names {name}");
        }
        // The ref contract every client renders.
        assert!(RUN_SKILL.contains("`#IDENT`"));
    }

    #[test]
    fn the_playbook_carries_no_front_matter_dashes_or_unshipped_params() {
        assert!(RUN_SKILL.starts_with("# "), "plain markdown, no YAML front matter");
        assert!(!RUN_SKILL.contains('\u{2014}'), "no em dashes");
        // `parentId` on issues_create lands with EXP-760; until then the
        // playbook teaches relations_add `parent`.
        assert!(!RUN_SKILL.contains("parentId"));
        assert!(RUN_SKILL.contains("Never use `gh`"));
        assert!(!RUN_SKILL.contains("expu_"));
    }

    #[test]
    fn mentioned_tools_dedups_and_skips_the_bare_prefix() {
        let names = mentioned_tools(
            "search exponential_* then exponential_issues_get, exponential_issues_get again (exponential_pr_open).",
        );
        assert_eq!(names, vec!["exponential_issues_get", "exponential_pr_open"]);
    }
}
