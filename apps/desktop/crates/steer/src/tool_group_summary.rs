//! EXP-785: the ONE caption a collapsed tool group renders, derived from the
//! group's tool rows. Hand-mirrored ×4 (web `@exp/domain-contract`
//! `toolGroupSummary`, Android `domain/ToolGroupSummary.kt`, iOS
//! `ExpCore/Sources/Domain/ToolGroupSummary.swift`) and byte-locked by
//! `packages/domain-contract/fixtures/tool-group-summary.json`, which every
//! client's test runs.
//!
//! Rules:
//! - `kind` is a contract `toolKind` value (`read`, `edit`, `delete`, `move`,
//!   `search`, `execute`, `think`, `fetch`, `switch_mode`, `other`); an
//!   unknown kind counts as `other`, so an older client never chokes on a
//!   newer agent. Kept a `&str` on purpose: the caption must not couple to
//!   the wire enum.
//! - `edit`/`delete`/`move` are "edited N files", `read` is "read N files",
//!   each DEDUPED by `detail` (the path). A `None`/empty detail is its own
//!   distinct file every time.
//! - Fixed segment order: ran N commands · edited N files · read N files ·
//!   searched N times · fetched N pages · N other tools · N failed. Zero
//!   segments are omitted; `failed` counts calls with `failed` of ANY kind
//!   and is always last.
//! - Only the first character of the whole caption is capitalised.
//! - Every call `think`/`switch_mode`/`other` → `Used N tools` (plus
//!   ` · N failed`); no calls at all → `No tool calls`.

use std::collections::HashSet;

/// The segment separator: space, MIDDLE DOT (U+00B7), space.
pub const TOOL_GROUP_SUMMARY_SEPARATOR: &str = " · ";

/// One tool call as the caption sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolCallSummary<'a> {
    pub kind: &'a str,
    pub detail: Option<&'a str>,
    pub failed: bool,
}

fn count(n: usize, singular: &str, plural: &str) -> String {
    format!("{n} {}", if n == 1 { singular } else { plural })
}

pub fn tool_group_summary(calls: &[ToolCallSummary<'_>]) -> String {
    if calls.is_empty() {
        return "No tool calls".to_string();
    }
    let mut commands = 0usize;
    let mut searches = 0usize;
    let mut fetches = 0usize;
    let mut other = 0usize;
    let mut failed = 0usize;
    let mut edited: HashSet<&str> = HashSet::new();
    let mut edited_blank = 0usize;
    let mut read: HashSet<&str> = HashSet::new();
    let mut read_blank = 0usize;
    for call in calls {
        if call.failed {
            failed += 1;
        }
        let detail = call.detail.filter(|detail| !detail.is_empty());
        match call.kind {
            "execute" => commands += 1,
            "edit" | "delete" | "move" => match detail {
                Some(path) => {
                    edited.insert(path);
                }
                None => edited_blank += 1,
            },
            "read" => match detail {
                Some(path) => {
                    read.insert(path);
                }
                None => read_blank += 1,
            },
            "search" => searches += 1,
            "fetch" => fetches += 1,
            _ => other += 1,
        }
    }
    let edited_count = edited.len() + edited_blank;
    let read_count = read.len() + read_blank;
    let mut segments: Vec<String> = Vec::new();
    if commands > 0 {
        segments.push(format!("ran {}", count(commands, "command", "commands")));
    }
    if edited_count > 0 {
        segments.push(format!("edited {}", count(edited_count, "file", "files")));
    }
    if read_count > 0 {
        segments.push(format!("read {}", count(read_count, "file", "files")));
    }
    if searches > 0 {
        segments.push(format!("searched {}", count(searches, "time", "times")));
    }
    if fetches > 0 {
        segments.push(format!("fetched {}", count(fetches, "page", "pages")));
    }
    if segments.is_empty() {
        segments.push(format!("used {}", count(other, "tool", "tools")));
    } else if other > 0 {
        segments.push(count(other, "other tool", "other tools"));
    }
    if failed > 0 {
        segments.push(format!("{failed} failed"));
    }
    let text = segments.join(TOOL_GROUP_SUMMARY_SEPARATOR);
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The contract fixture, byte-locked ×4 (web `tool-group-summary.test.ts`,
    /// Android `ToolGroupSummaryTest`, iOS `ToolGroupSummaryTests`).
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/tool-group-summary.json");

    #[derive(Deserialize)]
    struct FixtureCall {
        kind: String,
        #[serde(default)]
        detail: Option<String>,
        failed: bool,
    }

    #[derive(Deserialize)]
    struct FixtureCase {
        name: String,
        calls: Vec<FixtureCall>,
        expected: String,
    }

    fn cases() -> Vec<FixtureCase> {
        serde_json::from_str(FIXTURE).expect("the fixture parses")
    }

    #[test]
    fn every_fixture_case_renders_byte_exact() {
        let cases = cases();
        assert!(cases.len() >= 12);
        for case in &cases {
            let calls: Vec<ToolCallSummary<'_>> = case
                .calls
                .iter()
                .map(|call| ToolCallSummary {
                    kind: &call.kind,
                    detail: call.detail.as_deref(),
                    failed: call.failed,
                })
                .collect();
            assert_eq!(tool_group_summary(&calls), case.expected, "{}", case.name);
        }
    }

    #[test]
    fn the_fixture_covers_every_segment() {
        let expectations: Vec<String> = cases().into_iter().map(|case| case.expected).collect();
        let covers = |needle: &str| expectations.iter().any(|text| text.contains(needle));
        assert!(covers("No tool calls"));
        assert!(covers("Used 1 tool"));
        assert!(covers("Used 3 tools"));
        for first in ["Ran ", "Edited ", "Read ", "Searched ", "Fetched "] {
            assert!(expectations.iter().any(|text| text.starts_with(first)), "{first}");
        }
        for segment in [
            "1 command",
            "2 commands",
            "1 file",
            "2 files",
            "1 time",
            "2 times",
            "1 page",
            "2 pages",
            "1 other tool",
            "2 other tools",
            "1 failed",
            "2 failed",
        ] {
            assert!(covers(segment), "{segment}");
        }
        assert_eq!(TOOL_GROUP_SUMMARY_SEPARATOR, " \u{00B7} ");
        assert!(covers(TOOL_GROUP_SUMMARY_SEPARATOR));
    }
}
