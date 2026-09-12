//! EXP-846 — how an EXPONENTIAL MCP call renders.
//!
//! An agent's work on the product itself (filing an issue, opening a PR,
//! listing boards) used to read as `mcp__exponential__exponential_issues_create`
//! beside a pile of `Bash` rows: the most meaningful thing in a transcript,
//! written in wire vocabulary. The contract's `expToolDisplay` tables give each
//! of our tools a progressive caption ("Creating issue"), a done caption
//! ("Created issue"), the INPUT field that names its subject (`title`, `id`, …)
//! and the KIND of thing it answers with (`issue`, `pr`, `list`, …). This module
//! is the lookup over those parallel arrays — the ONE place any client asks
//! "is this one of ours, and what does it say?".
//!
//! Mirrored ×4 (web `lib/agent-feed.ts` `expToolDisplay`/`expToolCaption`,
//! iOS `ExpToolDisplay.swift`,
//! Android `ExpToolDisplay.kt`); the tables themselves are generated from
//! `packages/domain-contract/contract.json`, so nothing here is hand-written
//! copy.

/// What one of our tools renders as, at one moment of its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpToolDisplay {
    /// The contract row (`issues_create`) — the stable id of the tool.
    pub row: &'static str,
    /// The caption for this moment: the progressive form while the call runs,
    /// the done form once it has settled.
    pub caption: &'static str,
    /// The input field that names the call's SUBJECT (`title` for an issue
    /// create, `issueId` for a comment); empty when the call has no subject.
    pub subject_key: &'static str,
    /// The contract result kind the answer carries (`issue`, `pr`, `list`,
    /// `none`, …) — what a settled row may preview.
    pub result: &'static str,
}

/// The index of the contract row a tool NAME belongs to, or `None` for any
/// other server's tool.
///
/// The name may carry any namespace an adapter puts in front of it
/// (`mcp__exponential__exponential_issues_create` on claude, the bare
/// `exponential_pr_open` elsewhere); the contract PREFIX is REQUIRED, which is
/// what keeps another server's `issues_create` out.
pub fn exp_tool_index(name: &str) -> Option<usize> {
    let name = name.trim();
    domain::contract::EXP_TOOL_NAMES.iter().position(|row| {
        name.len() > row.len()
            && name.ends_with(*row)
            && name[..name.len() - row.len()].ends_with(domain::contract::EXP_TOOL_PREFIX)
    })
}

/// The contract row name for a tool name — [`exp_tool_index`]'s label.
pub fn exp_tool_row(name: &str) -> Option<&'static str> {
    exp_tool_index(name).map(|index| domain::contract::EXP_TOOL_NAMES[index])
}

/// The INPUT field naming this tool's subject, when it has one (an empty
/// contract entry means the call's subject is the team itself — a list, a
/// lookup by nothing).
pub fn exp_tool_subject_key(name: &str) -> Option<&'static str> {
    let key = domain::contract::EXP_TOOL_SUBJECT_KEYS[exp_tool_index(name)?];
    (!key.is_empty()).then_some(key)
}

/// How a call to `name` renders right now: `settled` picks the done caption
/// over the progressive one. `None` for every tool that is not ours.
pub fn exp_tool_display(name: &str, settled: bool) -> Option<ExpToolDisplay> {
    let index = exp_tool_index(name)?;
    let captions = if settled {
        domain::contract::EXP_TOOL_DONE
    } else {
        domain::contract::EXP_TOOL_PROGRESSIVE
    };
    Some(ExpToolDisplay {
        row: domain::contract::EXP_TOOL_NAMES[index],
        caption: captions[index],
        subject_key: domain::contract::EXP_TOOL_SUBJECT_KEYS[index],
        result: domain::contract::EXP_TOOL_RESULTS[index],
    })
}

/// The contract result kinds a settled row can preview, as named constants —
/// the match arms every client's renderer forks on.
pub mod result {
    pub const NONE: &str = "none";
    pub const ISSUE: &str = "issue";
    pub const PR: &str = "pr";
    pub const COMMENT: &str = "comment";
    pub const SESSION: &str = "session";
    pub const BOARD: &str = "board";
    pub const ACTION: &str = "action";
    pub const AUTOMATION: &str = "automation";
    pub const LIST: &str = "list";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every namespace an adapter can put in front of one of our tools resolves
    /// to the same contract row; another server's same-named tool does not.
    #[test]
    fn our_tools_are_recognised_through_every_namespace() {
        assert_eq!(
            exp_tool_row("mcp__exponential__exponential_issues_create"),
            Some("issues_create")
        );
        assert_eq!(exp_tool_row("exponential_pr_open"), Some("pr_open"));
        assert_eq!(
            exp_tool_row("  exponential.exponential_issues_list  "),
            Some("issues_list")
        );
        assert_eq!(exp_tool_row("mcp__linear__issues_create"), None);
        assert_eq!(exp_tool_row("issues_create"), None);
        assert_eq!(exp_tool_row("Bash"), None);
        assert_eq!(exp_tool_row("exponential_issues_invented"), None);
    }

    /// The display is the contract's own copy: progressive while the call runs,
    /// done once it settles, plus the subject key and the result kind.
    #[test]
    fn the_display_is_the_contract_copy() {
        let running = exp_tool_display("mcp__exponential__exponential_issues_create", false)
            .expect("ours");
        assert_eq!(running.caption, "Creating issue");
        assert_eq!(running.subject_key, "title");
        assert_eq!(running.result, result::ISSUE);
        let done =
            exp_tool_display("exponential_issues_create", true).expect("ours");
        assert_eq!(done.caption, "Created issue");

        // A list answers with a count and names no subject of its own.
        let list = exp_tool_display("exponential_issues_list", true).expect("ours");
        assert_eq!((list.caption, list.result), ("Listed issues", result::LIST));
        assert_eq!(exp_tool_subject_key("exponential_issues_list"), None);
        assert_eq!(
            exp_tool_subject_key("exponential_issues_create"),
            Some("title")
        );
        assert!(exp_tool_display("Bash", false).is_none());
    }

    /// The four tables are parallel — a contract regeneration that adds a tool
    /// without its copy would index out of bounds at runtime.
    #[test]
    fn the_contract_tables_stay_parallel() {
        let rows = domain::contract::EXP_TOOL_NAMES.len();
        assert_eq!(domain::contract::EXP_TOOL_PROGRESSIVE.len(), rows);
        assert_eq!(domain::contract::EXP_TOOL_DONE.len(), rows);
        assert_eq!(domain::contract::EXP_TOOL_SUBJECT_KEYS.len(), rows);
        assert_eq!(domain::contract::EXP_TOOL_RESULTS.len(), rows);
        // Every result kind is one of the contract's own values.
        for kind in domain::contract::EXP_TOOL_RESULTS {
            assert!(
                domain::contract::EXP_TOOL_RESULT_KINDS.contains(kind),
                "{kind} is not a contract result kind"
            );
        }
    }
}
