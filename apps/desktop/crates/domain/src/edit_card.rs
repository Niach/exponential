//! EXP-916 — the ONE "edited files" card a transcript draws for a run of
//! consecutive file edits, and the ONE rule that decides which tool calls form
//! it. Hand-mirrored ×4 (TS `@exp/domain-contract` `src/edit-card.ts` + web
//! `lib/agent-feed.ts` + `@exp/ui` `EditedFilesCard`, this module behind
//! `steer::feed`, iOS `ExpCore/Sources/Domain/EditCard.swift`, Android
//! `domain/EditCard.kt`) and byte-locked by
//! `packages/domain-contract/fixtures/feed/edit-cards.json`, which every
//! client's feed test replays through its own feed-row projection.
//!
//! Grouping (a render ROW of the feed projection, kind `edits`):
//! - an `edits` row is a MAXIMAL run of consecutive feed items of ONE lane
//!   (`subagent_id`, absent = the main lane) that are all edit calls: kind
//!   `tool`, `tool_kind` ∈ `edit|delete|move`, no `workflow_id` (EXP-850 §3
//!   keeps a workflow call as its own card);
//! - the rule reads ONLY kind/`tool_kind`/`workflow_id`/`subagent_id` — never
//!   settled/failed/diff — so the card exists before any patch lands and a
//!   later `tool_update` can never re-split it;
//! - ANY other item ends the run: narration, a user turn, a question, a tool of
//!   another kind, a workflow call, another lane's item. "Nothing between" is
//!   literal;
//! - the row's id is its FIRST item's id (stable while a live run grows); the
//!   window `start` (EXP-783) opens a fresh card at its boundary like every
//!   other group.
//!
//! The card ([`edit_card`]):
//! - one row per PATH: every member's patch is parsed and folded with
//!   [`merge_files_by_path`] (a file touched twice = one row, counts summed,
//!   hunks concatenated); a pathless patch (bare hunks) borrows its call's
//!   `detail`;
//! - a member WITHOUT a patch still has a row on its `detail` — `pending`
//!   while the call runs, `failed` when the call's own `failed` flag is set,
//!   `done` once it settled without either (a delete, a move, an edit that
//!   changed nothing: the wire carries no patch for those) — unless a patch
//!   for that path already exists in the card;
//! - order: the ready rows in first-touch order, THEN the stubs in first-touch
//!   order; a later settle may upgrade a stub (pending → done / failed) but
//!   never moves it;
//! - `truncated_lines` = the lines the publisher cut off the members' patches
//!   (EXP-786 markers), summed, so the card can say what it is not showing;
//! - `live_index` names the row of the card's LAST member when that member is
//!   the transcript's live tool row: the one row a client opens by itself, the
//!   diff inline. Everything else starts collapsed, and a tap toggles a row in
//!   place — a card never navigates anywhere.
//!
//! gpui-free, like the rest of `domain`.

use crate::diff::{merge_files_by_path, parse_diff, DiffFile, DIFF_LINE_MAX};

/// The tool kinds whose calls form an edited-files card — the contract's
/// `toolKind.editKinds`, generated ×4 so no mirror restates the list.
pub const EDIT_CARD_KINDS: &[&str] = crate::contract::TOOL_KIND_EDIT_VALUES;

/// How many rows a card lists before it folds the rest behind "N more".
pub const EDIT_CARD_PREVIEW: usize = crate::contract::DIFF_UI_CARD_PREVIEW_FILES;

/// Whether a feed item is a call that belongs in an edited-files card.
/// The rule reads ONLY these three things — never settled/failed/diff.
pub fn is_edit_call(is_tool: bool, tool_kind: Option<&str>, is_workflow_call: bool) -> bool {
    if !is_tool || is_workflow_call {
        return false;
    }
    match tool_kind {
        Some(kind) => EDIT_CARD_KINDS.contains(&kind),
        None => false,
    }
}

/// The inclusive end index of the maximal run of same-lane edit calls starting
/// at `start` (which must itself be an edit call). `same_lane_edit(i)` answers
/// "is item i an edit call in the SAME lane as `start`" for the caller's feed,
/// whose length is `len`. A caller's group scan uses this exactly like its
/// tool-run scan.
pub fn edit_run_end(start: usize, len: usize, same_lane_edit: impl Fn(usize) -> bool) -> usize {
    let mut end = start;
    while end + 1 < len && same_lane_edit(end + 1) {
        end += 1;
    }
    end
}

/// A card member, borrowed from whatever feed the caller holds. Deliberately
/// narrow: the card reads `id`/`detail`/`diff`/`settled`/`failed` and nothing
/// else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditCardMember<'a> {
    pub id: u64,
    pub detail: Option<&'a str>,
    pub diff: Option<&'a str>,
    pub settled: bool,
    pub failed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditRowState {
    Ready,
    Pending,
    Done,
    Failed,
}

impl EditRowState {
    /// The canonical wire value (what [`render_edit_card`] prints).
    pub fn as_str(&self) -> &'static str {
        match self {
            EditRowState::Ready => "ready",
            EditRowState::Pending => "pending",
            EditRowState::Done => "done",
            EditRowState::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditCardRow {
    pub path: String,
    pub state: EditRowState,
    /// The merged patch for the path; `None` for a `pending`/`done`/`failed`
    /// row.
    pub file: Option<DiffFile>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditCardView {
    /// `1 file edited` / `N files edited`.
    pub title: String,
    pub rows: Vec<EditCardRow>,
    /// The row the client opens by itself, or `None`.
    pub live_index: Option<usize>,
    /// Lines the publisher cut off the members' patches, summed (0 = whole).
    pub truncated_lines: u32,
}

/// A member's `detail`, trimmed; an empty one is no path at all.
fn detail_path<'a>(member: &EditCardMember<'a>) -> Option<&'a str> {
    member
        .detail
        .map(str::trim)
        .filter(|detail| !detail.is_empty())
}

/// A member's patch, if it carries one (an empty string is none — the JS
/// twin's truthiness).
fn member_diff<'a>(member: &EditCardMember<'a>) -> Option<&'a str> {
    member.diff.filter(|diff| !diff.is_empty())
}

/// The card for one run of edit calls, plus the row the caller's live tool row
/// names (`live_item_id`).
pub fn edit_card(items: &[EditCardMember<'_>], live_item_id: Option<u64>) -> EditCardView {
    let mut ready: Vec<DiffFile> = Vec::new();
    // An ORDER-PRESERVING map (the JS twin's `Map`): first touch wins the
    // position, a later settle may still upgrade the state in place.
    let mut stubs: Vec<(String, EditRowState)> = Vec::new();
    let mut truncated_lines: u32 = 0;
    let last_id = items.last().map(|member| member.id);
    // The path the LAST member names — its patch's first file, else its
    // `detail` — recorded while its patch is parsed once, never re-parsed.
    let mut last_path: Option<String> = None;
    for (at, member) in items.iter().enumerate() {
        let is_last = at + 1 == items.len();
        if let Some(diff) = member_diff(member) {
            let parsed = parse_diff(diff);
            // Every marker is already clamped to `DIFF_LINE_MAX`, so the SUM
            // saturates there too — the count is a caption, not an
            // accumulator, and `+=` on `u32` would panic in a debug build.
            truncated_lines = truncated_lines
                .saturating_add(parsed.truncated_lines.unwrap_or(0))
                .min(DIFF_LINE_MAX);
            for file in parsed.files {
                // A pathless section (hunks with no header) borrows the call's
                // own subject — the engine names the file in `detail`.
                let path = if file.path.is_empty() {
                    match detail_path(member) {
                        Some(detail) => detail.to_string(),
                        None => continue,
                    }
                } else {
                    file.path.clone()
                };
                if is_last && last_path.is_none() {
                    last_path = Some(path.clone());
                }
                ready.push(if file.path == path {
                    file
                } else {
                    DiffFile { path, ..file }
                });
            }
            if is_last && last_path.is_none() {
                last_path = detail_path(member).map(str::to_string);
            }
            continue;
        }
        let path = detail_path(member);
        if is_last {
            last_path = path.map(str::to_string);
        }
        let path = match path {
            Some(path) => path,
            None => continue,
        };
        let state = if member.failed {
            EditRowState::Failed
        } else if member.settled {
            EditRowState::Done
        } else {
            EditRowState::Pending
        };
        // First touch wins the position; a later settle upgrades a pending
        // stub (pending → done / failed) and never demotes a settled one.
        match stubs.iter_mut().find(|(seen, _)| seen == path) {
            None => stubs.push((path.to_string(), state)),
            Some((_, seen)) => {
                if *seen == EditRowState::Pending {
                    *seen = state;
                }
            }
        }
    }
    let merged = merge_files_by_path(&ready);
    let mut rows: Vec<EditCardRow> = merged
        .iter()
        .map(|file| EditCardRow {
            path: file.path.clone(),
            state: EditRowState::Ready,
            file: Some(file.clone()),
        })
        .collect();
    for (path, state) in &stubs {
        if merged.iter().any(|file| &file.path == path) {
            continue;
        }
        rows.push(EditCardRow {
            path: path.clone(),
            state: *state,
            file: None,
        });
    }
    let mut live_index: Option<usize> = None;
    if let (Some(last_id), Some(live), Some(path)) = (last_id, live_item_id, last_path.as_deref()) {
        if live == last_id {
            live_index = rows.iter().position(|row| row.path == path);
        }
    }
    EditCardView {
        title: edit_card_title(rows.len()),
        rows,
        live_index,
        truncated_lines,
    }
}

/// The card's title — `1 file edited` / `4 files edited`, ×4.
pub fn edit_card_title(count: usize) -> String {
    if count == 1 {
        crate::contract::DIFF_UI_EDITED_FILES_ONE.to_string()
    } else {
        crate::contract::DIFF_UI_EDITED_FILES_MANY.replace("{n}", &count.to_string())
    }
}

/// The fold row under the first [`EDIT_CARD_PREVIEW`] rows, or `None`.
pub fn edit_card_more_label(count: usize) -> Option<String> {
    let rest = count.saturating_sub(EDIT_CARD_PREVIEW);
    if rest == 0 {
        return None;
    }
    Some(crate::contract::DIFF_UI_MORE_FILES.replace("{n}", &rest.to_string()))
}

/// The byte-lock projection of a card: `title | path +a -d | path pending |
/// path done | path failed | live=path | truncated=N`. Deliberately ASCII (`-d`, unlike
/// [`crate::diff::deletions_label`]), like [`crate::diff::render_diff`].
pub fn render_edit_card(view: &EditCardView) -> String {
    let mut parts: Vec<String> = vec![view.title.clone()];
    for row in &view.rows {
        parts.push(match (row.state, &row.file) {
            (EditRowState::Ready, Some(file)) => format!(
                "{} +{} -{}",
                row.path, file.additions, file.deletions
            ),
            _ => format!("{} {}", row.path, row.state.as_str()),
        });
    }
    if let Some(row) = view.live_index.and_then(|at| view.rows.get(at)) {
        parts.push(format!("live={}", row.path));
    }
    if view.truncated_lines > 0 {
        parts.push(format!("truncated={}", view.truncated_lines));
    }
    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The contract fixture, byte-locked ×4 (TS `edit-card.test.ts`, iOS
    /// `EditCardTests`, Android `EditCardTest`) — the SAME five test names
    /// everywhere. Every client replays it through ITS OWN feed-row
    /// projection; this module replays it through the REFERENCE projection
    /// below, which knows only the item kinds the fixture uses.
    const CASES: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/feed/edit-cards.json");

    /// A fixture feed item: `id`, `kind`, and for tools `toolKind`, `detail`,
    /// `settled`, `failed`, `diff`, `workflowId`, `subagentId`.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureItem {
        id: u64,
        kind: String,
        #[serde(default)]
        tool_kind: Option<String>,
        #[serde(default)]
        workflow_id: Option<String>,
        #[serde(default)]
        subagent_id: Option<String>,
        #[serde(default)]
        detail: Option<String>,
        #[serde(default)]
        diff: Option<String>,
        #[serde(default)]
        settled: Option<bool>,
        #[serde(default)]
        failed: Option<bool>,
    }

    /// A case: the `feed`, an optional `start` (the render window's first
    /// index, EXP-783), an optional `lane` (project THAT subagent's items
    /// instead of the main lane), an optional `live` (the id the transcript's
    /// live tool row would name), and `expected` = one string per row.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureCase {
        name: String,
        feed: Vec<FixtureItem>,
        #[serde(default)]
        start: Option<usize>,
        #[serde(default)]
        lane: Option<String>,
        #[serde(default)]
        live: Option<u64>,
        expected: Vec<String>,
    }

    fn cases() -> Vec<FixtureCase> {
        serde_json::from_str(CASES).expect("the edit-card fixture parses")
    }

    fn edit_call(item: &FixtureItem) -> bool {
        is_edit_call(
            item.kind == "tool",
            item.tool_kind.as_deref(),
            item.workflow_id.is_some(),
        )
    }

    fn member(item: &FixtureItem) -> EditCardMember<'_> {
        EditCardMember {
            id: item.id,
            detail: item.detail.as_deref(),
            diff: item.diff.as_deref(),
            settled: item.settled.unwrap_or(false),
            failed: item.failed.unwrap_or(false),
        }
    }

    fn ids(items: &[&FixtureItem]) -> String {
        items
            .iter()
            .map(|item| item.id.to_string())
            .collect::<Vec<String>>()
            .join(",")
    }

    /// The reference projection — the fixture's own grouping rule:
    ///   `narration@id` · `user@id` · `tool@id` (a lone non-edit tool, or a
    ///   workflow call) · `run@id[ids]` (a tool run) · `subagent@id(lane)` ·
    ///   `card@id[ids]: <render_edit_card>`.
    fn project(case: &FixtureCase) -> Vec<String> {
        let lane = case.lane.as_deref();
        let feed: Vec<&FixtureItem> = match lane {
            None => case.feed.iter().collect(),
            Some(lane) => case
                .feed
                .iter()
                .filter(|row| row.subagent_id.as_deref() == Some(lane))
                .collect(),
        };
        let live = case.live;
        let mut rows: Vec<String> = Vec::new();
        let mut seen_lanes: Vec<&str> = Vec::new();
        let mut i = case.start.unwrap_or(0);
        while i < feed.len() {
            let row = feed[i];
            if lane.is_none() {
                if let Some(sub) = row.subagent_id.as_deref() {
                    if !seen_lanes.contains(&sub) {
                        seen_lanes.push(sub);
                        rows.push(format!("subagent@{}({sub})", row.id));
                    }
                    i += 1;
                    continue;
                }
            }
            if edit_call(row) {
                let start_lane = row.subagent_id.as_deref();
                let end = edit_run_end(i, feed.len(), |at| {
                    edit_call(feed[at]) && feed[at].subagent_id.as_deref() == start_lane
                });
                let members: Vec<EditCardMember<'_>> =
                    feed[i..=end].iter().map(|item| member(item)).collect();
                let view = edit_card(&members, live);
                // EXP-938: a run whose every member is dropped is no card.
                if !view.rows.is_empty() {
                    rows.push(format!(
                        "card@{}[{}]: {}",
                        row.id,
                        ids(&feed[i..=end]),
                        render_edit_card(&view)
                    ));
                }
                i = end + 1;
                continue;
            }
            if row.kind == "tool" && row.workflow_id.is_none() {
                let mut end = i;
                while end + 1 < feed.len()
                    && feed[end + 1].kind == "tool"
                    && feed[end + 1].workflow_id.is_none()
                    && feed[end + 1].subagent_id.as_deref() == lane
                    && !edit_call(feed[end + 1])
                {
                    end += 1;
                }
                rows.push(if end == i {
                    format!("tool@{}", row.id)
                } else {
                    format!("run@{}[{}]", row.id, ids(&feed[i..=end]))
                });
                i = end + 1;
                continue;
            }
            if row.kind == "tool" {
                rows.push(format!("tool@{}", row.id));
                i += 1;
                continue;
            }
            let kind = if row.kind == "user_message" {
                "user"
            } else {
                row.kind.as_str()
            };
            rows.push(format!("{kind}@{}", row.id));
            i += 1;
        }
        rows
    }

    /// every fixture case projects byte-exact
    #[test]
    fn every_fixture_case_projects_byte_exact() {
        for case in &cases() {
            assert_eq!(project(case), case.expected, "{}", case.name);
        }
    }

    /// the fixture covers a split, a live row, a stub, a lane and a window
    #[test]
    fn the_fixture_covers_a_split_a_live_row_a_stub_a_lane_and_a_window() {
        let names = cases()
            .iter()
            .map(|case| case.name.clone())
            .collect::<Vec<String>>()
            .join("\n");
        assert!(names.contains("splits"));
        assert!(names.contains("live"));
        assert!(names.contains("failed"));
        assert!(names.contains("subagent"));
        assert!(names.contains("window"));
    }

    /// the rule reads only kind, toolKind and workflowId
    #[test]
    fn the_rule_reads_only_kind_tool_kind_and_workflow_id() {
        assert!(is_edit_call(true, Some("edit"), false));
        assert!(is_edit_call(true, Some("delete"), false));
        assert!(is_edit_call(true, Some("move"), false));
        assert!(!is_edit_call(true, Some("read"), false));
        assert!(!is_edit_call(true, None, false));
        assert!(!is_edit_call(true, Some("edit"), true));
        assert!(!is_edit_call(false, Some("edit"), false));
    }

    /// the copy is the contract's
    #[test]
    fn the_copy_is_the_contracts() {
        assert_eq!(edit_card_title(1), "1 file edited");
        assert_eq!(edit_card_title(4), "4 files edited");
        assert_eq!(EDIT_CARD_PREVIEW, 5);
        assert_eq!(edit_card_more_label(5), None);
        assert_eq!(edit_card_more_label(8).as_deref(), Some("3 more"));
    }

    /// a live row is only the card's LAST member
    #[test]
    fn a_live_row_is_only_the_cards_last_member() {
        let items = [
            EditCardMember {
                id: 1,
                detail: Some("a.ts"),
                ..EditCardMember::default()
            },
            EditCardMember {
                id: 2,
                detail: Some("b.ts"),
                ..EditCardMember::default()
            },
        ];
        assert_eq!(edit_card(&items, Some(1)).live_index, None);
        assert_eq!(edit_card(&items, Some(2)).live_index, Some(1));
        assert_eq!(edit_card(&items, None).live_index, None);
    }
}
