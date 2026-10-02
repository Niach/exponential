//! EXP-878 issue drafts — the ONE place the desktop reads the per-user
//! `issue_drafts` shape and fires `issueDrafts.upsert` / `.delete`.
//!
//! A draft is what the New issue PAGE (EXP-1170, `issue_draft_screen`)
//! autosaves while it has something in it. There is no "Discard?" confirm on
//! leaving: content saves, an existing draft emptied out is deleted on leave,
//! and an untouched blank page writes nothing (`draft_editor`).
//!
//! The shape is static per user and NOT team/trash scoped, so — like
//! [`crate::pins`] — every reader here filters to the ACTIVE team and to rows
//! whose BOARD still resolves in the sibling `boards` collection. A draft on
//! a trashed board is simply hidden; it comes back when the board restores.
//!
//! The Drafts list's delete is fire-and-forget off the background executor
//! (the `pins::toggle_pin` recipe): the synced row is the source of truth for
//! the list. The page's own writes are sequenced by `draft_editor`.

use chrono::NaiveDate;
use gpui::App;

use domain::rows::IssueDraftRow;
use domain::{IssuePriority, IssueStatus};
use sync::Store;

use crate::queries;

/// The caller's drafts in `team_id`, most recently touched FIRST, with every
/// row whose board no longer resolves dropped.
pub(crate) fn drafts_in_team(team_id: &str, cx: &App) -> Vec<IssueDraftRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let boards = collections.boards.read(cx);
    let mut rows: Vec<IssueDraftRow> = collections
        .issue_drafts
        .read(cx)
        .iter()
        .filter(|draft| draft.team_id.as_deref() == Some(team_id))
        .filter(|draft| {
            draft
                .board_id
                .as_deref()
                .is_some_and(|board_id| boards.get(board_id).is_some())
        })
        .cloned()
        .collect();
    sort_drafts(&mut rows);
    rows
}

/// Display order: `updated_at` DESC (the row just closed sits on top), then
/// `created_at` desc, then id — total, so the list never reorders between
/// repaints. Pure so it is testable without a store.
pub(crate) fn sort_drafts(drafts: &mut [IssueDraftRow]) {
    drafts.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// A draft's row label: its title, or `None` when it has none (the list
/// renders a muted "Untitled draft" in its place).
pub(crate) fn draft_title(draft: &IssueDraftRow) -> Option<String> {
    draft
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
}

/// Is there anything on this page worth keeping? The ONE rule behind every
/// autosave and leave: a trimmed non-empty title, OR a non-empty description,
/// OR at least one uploaded draft attachment. Property picks alone (a
/// priority, a label) are not content — they are the page's defaults as
/// often as not, and saving on them would litter the list.
pub(crate) fn has_content(title: &str, description: &str, attachment_count: usize) -> bool {
    !title.trim().is_empty() || !description.trim().is_empty() || attachment_count > 0
}

/// One draft write, read off the New issue page on the foreground and
/// consumed on the background executor. `PartialEq` so the autosave can
/// tell an edit from a repaint.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftSave {
    pub(crate) id: String,
    pub(crate) team_id: String,
    pub(crate) board_id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) status_id: Option<String>,
    pub(crate) priority: IssuePriority,
    pub(crate) assignee_id: Option<String>,
    pub(crate) label_ids: Vec<String>,
    pub(crate) due_date: Option<NaiveDate>,
}

impl DraftSave {
    /// The wire input. `priority` rides only when it has a wire value (an
    /// `Unknown` forward-compat value must never be echoed back), and
    /// `label_ids` ALWAYS does — it is the full replacement set.
    pub(crate) fn to_input(&self) -> api::issue_drafts::IssueDraftUpsertInput {
        let mut input = api::issue_drafts::IssueDraftUpsertInput::new(
            self.id.clone(),
            self.team_id.clone(),
            self.board_id.clone(),
        );
        input.title = self.title.clone();
        input.description = self.description.clone();
        input.status_id = self.status_id.clone();
        input.priority = self.priority.as_wire().is_some().then_some(self.priority);
        input.assignee_id = self.assignee_id.clone();
        input.label_ids = self.label_ids.clone();
        input.due_date = self
            .due_date
            .map(|date| date.format("%Y-%m-%d").to_string());
        input
    }
}

impl DraftSave {
    /// The row this save writes, as the synced shape would carry it — what
    /// the New issue page seeds from when it returns to a draft it wrote
    /// whose echo has not synced yet ([`seed_from_row`] takes it as is).
    pub(crate) fn as_row(&self) -> IssueDraftRow {
        IssueDraftRow {
            id: self.id.clone(),
            user_id: None,
            team_id: Some(self.team_id.clone()),
            board_id: Some(self.board_id.clone()),
            title: Some(self.title.clone()),
            description: Some(self.description.clone()),
            status_id: self.status_id.clone(),
            priority: self.priority.as_wire().is_some().then_some(self.priority),
            assignee_id: self.assignee_id.clone(),
            label_ids: self.label_ids.clone(),
            due_date: self
                .due_date
                .map(|date| date.format("%Y-%m-%d").to_string()),
            created_at: None,
            updated_at: None,
        }
    }
}

/// Fire `issueDrafts.delete` for `id` — the Drafts list's row ✕ (the New
/// issue page's own writes are `draft_editor`'s). Fire-and-forget; a row
/// that is already gone answers `deleted: false`.
pub(crate) fn delete_draft(id: String, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::issue_drafts::issue_drafts_delete(&trpc, &id) {
                log::warn!("[ui] issueDrafts.delete({id}) failed: {err}");
            }
        })
        .detach();
}

/// What opening a draft seeds the New issue page with — the synced row's
/// fields resolved into the shapes the page's own state uses.
#[derive(Clone, Debug)]
pub(crate) struct DraftSeed {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) status: crate::pickers::StatusPick,
    pub(crate) priority: IssuePriority,
    pub(crate) assignee_id: Option<String>,
    pub(crate) label_ids: Vec<String>,
    pub(crate) due_date: Option<NaiveDate>,
}

/// Resolve a synced draft row into a [`DraftSeed`]. The status resolves
/// through the team vocabulary exactly like every other status surface
/// ([`queries::resolve_status_ref`]): a `status_id` that no longer names a
/// row falls back to the team's Backlog builtin, never to nothing.
pub(crate) fn seed_from_row(draft: &IssueDraftRow, cx: &App) -> DraftSeed {
    let team_id = draft.team_id.clone().unwrap_or_default();
    let statuses = queries::team_statuses(cx, &team_id);
    let resolved = queries::resolve_status_ref(
        draft.status_id.as_deref(),
        IssueStatus::Backlog,
        &statuses,
    );
    DraftSeed {
        title: draft.title.clone().unwrap_or_default(),
        description: draft.description.clone().unwrap_or_default(),
        status: crate::pickers::StatusPick::from_resolved(&resolved),
        priority: draft.priority.unwrap_or(IssuePriority::None),
        assignee_id: draft
            .assignee_id
            .clone()
            .filter(|id| !id.trim().is_empty()),
        label_ids: draft.label_ids.clone(),
        due_date: draft
            .due_date
            .as_deref()
            .map(str::trim)
            .filter(|date| !date.is_empty())
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(id: &str, updated_at: Option<&str>) -> IssueDraftRow {
        IssueDraftRow {
            id: id.to_string(),
            user_id: Some("me".to_string()),
            team_id: Some("t-1".to_string()),
            board_id: Some("b-1".to_string()),
            title: None,
            description: None,
            status_id: None,
            priority: None,
            assignee_id: None,
            label_ids: Vec::new(),
            due_date: None,
            created_at: Some("2026-09-01T00:00:00Z".to_string()),
            updated_at: updated_at.map(str::to_string),
        }
    }

    #[test]
    fn content_is_title_or_description_or_one_attachment() {
        assert!(!has_content("", "", 0));
        assert!(!has_content("   ", "\n \n", 0));
        assert!(has_content("A title", "", 0));
        assert!(has_content("", "Some body", 0));
        // EXP-878: an uploaded file with neither is still worth keeping —
        // the bytes are on the server already.
        assert!(has_content("", "", 1));
    }

    #[test]
    fn title_falls_back_to_none_for_blanks() {
        let mut row = draft("d", None);
        assert_eq!(draft_title(&row), None);
        row.title = Some("   ".to_string());
        assert_eq!(draft_title(&row), None);
        row.title = Some("  Ship it  ".to_string());
        assert_eq!(draft_title(&row).as_deref(), Some("Ship it"));
    }

    #[test]
    fn sort_is_most_recently_touched_first_and_total() {
        let mut rows = vec![
            draft("b", Some("2026-09-10T00:00:00Z")),
            draft("a", Some("2026-09-12T00:00:00Z")),
            // A row with no updated_at sinks below every stamped one.
            draft("z", None),
            draft("c", Some("2026-09-11T00:00:00Z")),
        ];
        sort_drafts(&mut rows);
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, ["a", "c", "b", "z"]);
    }

    /// EXP-1170: a save read back as a row keeps every field the page seeds.
    #[test]
    fn a_save_reads_back_as_the_row_it_writes() {
        let save = DraftSave {
            id: "d-1".into(),
            team_id: "t-1".into(),
            board_id: "b-1".into(),
            title: "Title".into(),
            description: "Body".into(),
            status_id: Some("s-1".into()),
            priority: IssuePriority::High,
            assignee_id: Some("u-1".into()),
            label_ids: vec!["l-1".into()],
            due_date: NaiveDate::from_ymd_opt(2026, 9, 14),
        };
        let row = save.as_row();
        assert_eq!(row.id, "d-1");
        assert_eq!(row.team_id.as_deref(), Some("t-1"));
        assert_eq!(row.board_id.as_deref(), Some("b-1"));
        assert_eq!(row.title.as_deref(), Some("Title"));
        assert_eq!(row.description.as_deref(), Some("Body"));
        assert_eq!(row.status_id.as_deref(), Some("s-1"));
        assert_eq!(row.priority, Some(IssuePriority::High));
        assert_eq!(row.assignee_id.as_deref(), Some("u-1"));
        assert_eq!(row.label_ids, vec!["l-1".to_string()]);
        assert_eq!(row.due_date.as_deref(), Some("2026-09-14"));
    }

    #[test]
    fn save_input_always_carries_labels_and_drops_an_unknown_priority() {
        let mut save = DraftSave {
            id: "d-1".into(),
            team_id: "t-1".into(),
            board_id: "b-1".into(),
            title: "  Title  ".into(),
            description: "Body".into(),
            status_id: Some("s-1".into()),
            priority: IssuePriority::None,
            assignee_id: None,
            label_ids: Vec::new(),
            due_date: NaiveDate::from_ymd_opt(2026, 9, 14),
        };
        let input = save.to_input();
        // The full replacement set rides even when empty — omitting it would
        // silently keep the row's stale labels.
        assert!(input.label_ids.is_empty());
        assert_eq!(input.priority, Some(IssuePriority::None));
        assert_eq!(input.due_date.as_deref(), Some("2026-09-14"));
        assert_eq!(input.status_id.as_deref(), Some("s-1"));
        // A forward-compat value from a newer server must never be echoed.
        save.priority = IssuePriority::Unknown;
        assert_eq!(save.to_input().priority, None);
    }
}
