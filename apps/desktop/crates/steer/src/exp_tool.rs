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

/// EXP-920: the contract's PREVIEW spec for a tool name — the whitespace-
/// separated `kind@path` terms (`expToolPreview.tools[].refs`) naming what
/// its answer points at. `None` for every tool that is not ours; `Some("")`
/// for one of ours whose answer previews nothing. The engine's
/// `exp_tool_refs` parses it.
pub fn exp_tool_preview_spec(name: &str) -> Option<&'static str> {
    let index = exp_tool_index(name)?;
    domain::contract::EXP_TOOL_PREVIEW_NAMES
        .iter()
        .position(|row| *row == domain::contract::EXP_TOOL_NAMES[index])
        .map(|row| domain::contract::EXP_TOOL_PREVIEW_REFS[row])
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

/// EXP-862 — one built-in Exponential tool as the MCP servers page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinExpTool {
    /// The full wire name (`exponential_issues_create`) — the row's tooltip,
    /// and the only place the wire vocabulary still shows.
    pub name: String,
    /// The contract's human title ("Create issue").
    pub title: &'static str,
    /// The one-line description under it.
    pub blurb: &'static str,
}

/// EXP-862 — every built-in Exponential tool, in contract order, for the
/// "Built-in Exponential tools" section of the MCP page. EXP-1657: a retired
/// row (`aliasOf`, the old `team_config_*` rows kept so old transcripts
/// render; its group is "") is never listed.
/// Mirrored ×2 (web `lib/agent-feed.ts` `builtinExpTools`).
pub fn builtin_tools() -> Vec<BuiltinExpTool> {
    domain::contract::EXP_TOOL_NAMES
        .iter()
        .enumerate()
        .filter(|(index, _)| !domain::contract::EXP_TOOL_ROW_GROUPS[*index].is_empty())
        .map(|(index, row)| builtin_tool(index, row))
        .collect()
}

fn builtin_tool(index: usize, row: &str) -> BuiltinExpTool {
    BuiltinExpTool {
        name: format!("{}{row}", domain::contract::EXP_TOOL_PREFIX),
        title: domain::contract::EXP_TOOL_TITLES[index],
        blurb: domain::contract::EXP_TOOL_BLURBS[index],
    }
}

/// EXP-1657 — one band of the MCP page's built-in section: a contract tool
/// group (`expToolDisplay.groups`) and the op rows it lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinToolBand {
    /// The group id (`work`, `tracker`, …).
    pub id: &'static str,
    /// The band header's label ("Work").
    pub label: &'static str,
    /// The group's rows that pass the filter, in contract table order.
    pub tools: Vec<BuiltinExpTool>,
}

/// Whether a built-in tool passes the page's filter: a case-insensitive
/// substring of its title, its blurb or its wire name, the query trimmed;
/// blank = every tool (fixtures/settings-mcp.json `builtinCases`).
pub fn builtin_tool_matches(tool: &BuiltinExpTool, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty()
        || tool.title.to_lowercase().contains(&needle)
        || tool.blurb.to_lowercase().contains(&needle)
        || tool.name.to_lowercase().contains(&needle)
}

/// EXP-1657 — the built-in section as bands: one per contract tool group, in
/// group order, each listing its rows that match `query` in contract table
/// order; a band with no match is dropped. Retired (alias) rows belong to no
/// group and never show. Mirrors the TS reference
/// `packages/domain-contract/src/settings-mcp.ts` `builtinToolBands`.
pub fn builtin_tool_bands(query: &str) -> Vec<BuiltinToolBand> {
    domain::contract::EXP_TOOL_GROUP_IDS
        .iter()
        .zip(domain::contract::EXP_TOOL_GROUP_LABELS)
        .filter_map(|(id, label)| {
            let tools: Vec<BuiltinExpTool> = domain::contract::EXP_TOOL_NAMES
                .iter()
                .enumerate()
                .filter(|(index, _)| domain::contract::EXP_TOOL_ROW_GROUPS[*index] == *id)
                .map(|(index, row)| builtin_tool(index, row))
                .filter(|tool| builtin_tool_matches(tool, query))
                .collect();
            (!tools.is_empty()).then_some(BuiltinToolBand { id, label, tools })
        })
        .collect()
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
    pub const LIST: &str = "list";
    /// EXP-933 → EXP-1291: the retired Guide's kind (S-3: no tool answers
    /// with it since `guide_write` left the surface; kept as a contract value).
    pub const RESULTS: &str = "results";
    /// EXP-1172: a contract kind for ONE picture; no tool answers with it
    /// since EXP-1291 (the server cards a Guide picture itself).
    pub const PICTURE: &str = "picture";
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
        // EXP-862: the settings copy is two more parallel tables.
        assert_eq!(domain::contract::EXP_TOOL_TITLES.len(), rows);
        assert_eq!(domain::contract::EXP_TOOL_BLURBS.len(), rows);
        // Every result kind is one of the contract's own values.
        for kind in domain::contract::EXP_TOOL_RESULTS {
            assert!(
                domain::contract::EXP_TOOL_RESULT_KINDS.contains(kind),
                "{kind} is not a contract result kind"
            );
        }
        // EXP-920: the preview table covers every tool, in its own order.
        assert_eq!(domain::contract::EXP_TOOL_PREVIEW_NAMES.len(), rows);
        assert_eq!(domain::contract::EXP_TOOL_PREVIEW_REFS.len(), rows);
        for name in domain::contract::EXP_TOOL_NAMES {
            assert!(
                domain::contract::EXP_TOOL_PREVIEW_NAMES.contains(name),
                "{name} has no preview spec"
            );
        }
    }

    /// EXP-920: the preview spec resolves through the same namespaces; a tool
    /// with nothing to preview answers the empty spec, a stranger none.
    #[test]
    fn the_preview_spec_is_the_contract_row() {
        assert_eq!(
            exp_tool_preview_spec("mcp__exponential__exponential_issues_create"),
            Some("issue@")
        );
        assert_eq!(
            exp_tool_preview_spec("exponential_pr_open"),
            Some("issue@$issueId issue@$issueIds[]")
        );
        assert_eq!(exp_tool_preview_spec("exponential_events_emit"), Some(""));
        assert_eq!(exp_tool_preview_spec("exponential_actions_delete"), None, "NEW-2: actions_* are gone");
        assert_eq!(exp_tool_preview_spec("mcp__other__issues_create"), None);
        assert_eq!(exp_tool_preview_spec("Bash"), None);
    }

    /// EXP-933: every named result kind is one the contract lists.
    #[test]
    fn every_named_result_kind_is_a_contract_kind() {
        for kind in [
            result::NONE,
            result::ISSUE,
            result::PR,
            result::COMMENT,
            result::SESSION,
            result::BOARD,
            result::ACTION,
            result::LIST,
            result::RESULTS,
        ] {
            assert!(
                domain::contract::EXP_TOOL_RESULT_KINDS.contains(&kind),
                "{kind} is not a contract result kind"
            );
        }
    }

    /// S-3: `guide_write` left the surface; an old transcript's call is no
    /// longer one of ours and falls back to the generic row.
    #[test]
    fn a_retired_guide_write_call_is_generic() {
        assert!(exp_tool_display("mcp__exponential__exponential_guide_write", false).is_none());
        assert_eq!(exp_tool_row("exponential_guide_write"), None);
        let show = exp_tool_display("mcp__exponential__exponential_show", true).expect("ours");
        assert_eq!(show.row, "show");
    }

    /// EXP-862 — the settings list: every built-in tool, its wire name whole
    /// (the row's tooltip) and human copy beside it.
    #[test]
    fn the_builtin_list_carries_the_wire_name_and_the_copy() {
        let tools = builtin_tools();
        let live = domain::contract::EXP_TOOL_ROW_GROUPS
            .iter()
            .filter(|group| !group.is_empty())
            .count();
        assert_eq!(tools.len(), live);
        assert!(
            tools.iter().all(|tool| !tool.name.contains("team_config")),
            "EXP-1657: a retired alias row is never listed"
        );
        let create = tools
            .iter()
            .find(|tool| tool.name == "exponential_issues_create")
            .expect("issues_create is built in");
        assert_eq!(create.title, "Create issue");
        assert!(!create.blurb.is_empty());
        for tool in &tools {
            assert!(tool.name.starts_with(domain::contract::EXP_TOOL_PREFIX));
            assert!(!tool.title.is_empty() && !tool.blurb.is_empty());
            assert!(tool.blurb.len() <= 120, "{} blurb is long", tool.name);
        }
    }

    /// EXP-1657: a retired `team_config_*` row still renders an old
    /// transcript's call; its replacement resolves to its own row.
    #[test]
    fn a_retired_team_config_call_still_resolves() {
        assert_eq!(
            exp_tool_row("mcp__exponential__exponential_team_config_labels_list"),
            Some("team_config_labels_list")
        );
        assert_eq!(exp_tool_row("exponential_labels_list"), Some("labels_list"));
        assert_eq!(
            domain::contract::EXP_TOOL_ROW_GROUPS.len(),
            domain::contract::EXP_TOOL_NAMES.len()
        );
    }

    /// EXP-1657: fixtures/settings-mcp.json `builtinCases` replay — per query
    /// the bands shown (id, label, row count, and the wire names in order when
    /// the case lists them).
    #[test]
    fn the_builtin_bands_replay_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/settings-mcp.json"
        ))
        .expect("settings-mcp.json parses");
        let cases = fixture["builtinCases"].as_array().expect("builtinCases");
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap_or_default();
            let query = case["query"].as_str().expect("query");
            let bands = builtin_tool_bands(query);
            let expected = case["expected"].as_array().expect("expected");
            assert_eq!(bands.len(), expected.len(), "{name}: band count");
            for (band, want) in bands.iter().zip(expected) {
                assert_eq!(band.id, want["id"].as_str().unwrap(), "{name}");
                assert_eq!(band.label, want["label"].as_str().unwrap(), "{name}");
                assert_eq!(
                    band.tools.len() as u64,
                    want["count"].as_u64().unwrap(),
                    "{name}: {} count",
                    band.id
                );
                if let Some(rows) = want["rows"].as_array() {
                    let got: Vec<&str> = band.tools.iter().map(|tool| tool.name.as_str()).collect();
                    let want: Vec<&str> = rows.iter().map(|row| row.as_str().unwrap()).collect();
                    assert_eq!(got, want, "{name}: {} rows", band.id);
                }
            }
        }
        // Blank lists every live row exactly once.
        let total: usize = builtin_tool_bands("").iter().map(|band| band.tools.len()).sum();
        assert_eq!(total, builtin_tools().len());
    }
}
