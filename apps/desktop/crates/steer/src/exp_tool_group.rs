//! EXP-948: our OWN tools never hide.
//!
//! An Exponential MCP call is the most meaningful row a transcript has ("Read
//! issue EXP-901", "Opened pull request"), and until now a pile of them
//! vanished inside a collapsed "Ran 1 command · 21 other tools" fold. This
//! module owns the ONE rule that keeps them visible and the ONE caption a run
//! of them renders, over the contract's `expToolDisplay` table (the lookup
//! itself stays in [`crate::exp_tool`]).
//!
//! Hand-mirrored ×4 (web `lib/agent-feed.ts` `scanToolRuns` +
//! `@exp/domain-contract/exp-tool-group`, iOS
//! `ExpCore/Sources/Domain/ExpToolGroup.swift`, Android
//! `domain/ExpToolGroup.kt`) and byte-locked by
//! `packages/domain-contract/fixtures/feed/exp-tool-groups.json`, which every
//! client's feed test replays through its own row projection.
//!
//! Grouping (a render row, [`crate::feed::FeedRow::ExpRun`]):
//! - an Exponential call is a tool item with no workflow id whose NAME
//!   resolves to an `expToolDisplay` row ([`crate::exp_tool::exp_tool_row`]);
//! - an Exponential call NEVER joins a generic tool run — it breaks one
//!   exactly like an edit call does (EXP-916), so it is never counted as
//!   "N other tools";
//! - an `ExpRun` row is a MAXIMAL run of ≥2 consecutive items of ONE lane that
//!   are Exponential calls of the SAME contract row. A single call stays the
//!   single visible row it always was, and two DIFFERENT Exponential tools in
//!   sequence are two rows;
//! - the rule reads ONLY kind/name/workflow/lane — never settled/failed — so a
//!   group never re-splits when a call settles;
//! - the row's id is its FIRST item's id (stable while a live run grows).
//!
//! The caption ([`exp_tool_group_caption`]):
//! - the contract's `progressiveMany` while ANY member is still running, its
//!   `doneMany` once every member settled, `{n}` = the member count
//!   ("Reading 3 issues" → "Read 3 issues");
//! - a failed member appends ` · N failed`, the same tail (and separator)
//!   [`crate::tool_group_summary::tool_group_summary`] writes.

use crate::exp_tool::exp_tool_index;
use crate::tool_group_summary::TOOL_GROUP_SUMMARY_SEPARATOR;

/// One member of a group, as the caption sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpToolGroupCall<'a> {
    /// The call's wire name, whatever namespace the adapter put in front.
    pub name: &'a str,
    pub settled: bool,
    pub failed: bool,
}

/// Whether a feed item is a call to one of OUR tools. The rule reads ONLY
/// these three things — never settled/failed.
pub fn is_exp_tool_call(is_tool: bool, name: Option<&str>, is_workflow_call: bool) -> bool {
    if !is_tool || is_workflow_call {
        return false;
    }
    name.is_some_and(|name| exp_tool_index(name).is_some())
}

/// The inclusive end index of the maximal run of same-lane calls to the SAME
/// Exponential tool starting at `start` (which must itself be one).
/// `same_lane_call(i)` answers "is item i a call to the same contract row in
/// the SAME lane as `start`" for the caller's feed, whose length is `len` —
/// the shape [`domain::edit_card::edit_run_end`] has.
pub fn exp_tool_run_end(start: usize, len: usize, same_lane_call: impl Fn(usize) -> bool) -> usize {
    let mut end = start;
    while end + 1 < len && same_lane_call(end + 1) {
        end += 1;
    }
    end
}

/// The caption an `ExpRun` row reads: the contract's plural copy for the run's
/// tool, progressive while any member is still in flight, done once they all
/// settled, plus ` · N failed` when members failed. Empty for a run of a tool
/// that is not ours (which the grouping rule never forms).
pub fn exp_tool_group_caption(calls: &[ExpToolGroupCall<'_>]) -> String {
    let Some(index) = calls.first().and_then(|call| exp_tool_index(call.name)) else {
        return String::new();
    };
    let running = calls.iter().any(|call| !call.settled);
    let template = if running {
        domain::contract::EXP_TOOL_PROGRESSIVE_MANY[index]
    } else {
        domain::contract::EXP_TOOL_DONE_MANY[index]
    };
    let caption = template.replace("{n}", &calls.len().to_string());
    let failed = calls.iter().filter(|call| call.failed).count();
    if failed == 0 {
        caption
    } else {
        format!("{caption}{TOOL_GROUP_SUMMARY_SEPARATOR}{failed} failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule reads the NAME, nothing else: any adapter namespace resolves,
    /// another server's tool does not, and a workflow's own call is its card.
    #[test]
    fn only_our_tools_group() {
        assert!(is_exp_tool_call(
            true,
            Some("mcp__exponential__exponential_issues_get"),
            false
        ));
        assert!(is_exp_tool_call(true, Some("exponential_issues_get"), false));
        assert!(!is_exp_tool_call(true, Some("Bash"), false));
        assert!(!is_exp_tool_call(true, Some("mcp__linear__issues_get"), false));
        assert!(!is_exp_tool_call(true, None, false));
        assert!(!is_exp_tool_call(
            true,
            Some("exponential_issues_get"),
            true
        ));
        assert!(!is_exp_tool_call(
            false,
            Some("exponential_issues_get"),
            false
        ));
    }

    /// The copy is the contract's: plural, progressive while any call runs,
    /// done once they all settled, failures counted last.
    #[test]
    fn the_caption_is_the_contract_copy() {
        let read = |n: usize, settled: bool| {
            let calls: Vec<_> = (0..n)
                .map(|_| ExpToolGroupCall {
                    name: "exponential_issues_get",
                    settled,
                    failed: false,
                })
                .collect();
            exp_tool_group_caption(&calls)
        };
        assert_eq!(read(3, true), "Read 3 issues");
        assert_eq!(read(3, false), "Reading 3 issues");
        assert_eq!(read(21, true), "Read 21 issues");
        assert_eq!(
            exp_tool_group_caption(&[
                ExpToolGroupCall {
                    name: "exponential_issues_get",
                    settled: true,
                    failed: false,
                },
                ExpToolGroupCall {
                    name: "exponential_issues_get",
                    settled: true,
                    failed: true,
                },
            ]),
            "Read 2 issues · 1 failed"
        );
        assert_eq!(
            exp_tool_group_caption(&[
                ExpToolGroupCall {
                    name: "exponential_issues_list",
                    settled: true,
                    failed: false,
                },
                ExpToolGroupCall {
                    name: "exponential_issues_list",
                    settled: true,
                    failed: false,
                },
            ]),
            "Listed issues 2 times"
        );
        assert_eq!(
            exp_tool_group_caption(&[ExpToolGroupCall {
                name: "Bash",
                settled: true,
                failed: false,
            }]),
            ""
        );
        assert_eq!(exp_tool_group_caption(&[]), "");
    }

    /// The contract tables stay parallel — a regeneration that adds a tool
    /// without its plural copy would index out of bounds at runtime.
    #[test]
    fn the_plural_tables_stay_parallel() {
        let rows = domain::contract::EXP_TOOL_NAMES.len();
        assert_eq!(domain::contract::EXP_TOOL_PROGRESSIVE_MANY.len(), rows);
        assert_eq!(domain::contract::EXP_TOOL_DONE_MANY.len(), rows);
        for caption in domain::contract::EXP_TOOL_PROGRESSIVE_MANY
            .iter()
            .chain(domain::contract::EXP_TOOL_DONE_MANY)
        {
            assert!(caption.contains("{n}"), "{caption} has no count");
        }
    }
}
