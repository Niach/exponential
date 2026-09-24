//! EXP-1029 contract — the issue picker (single or multi): rows read
//! `IDENT Title`, search matches the identifier and the title. The composer
//! picks several (a batch); relations, duplicates and the stack dialog pick
//! one. `issue_picker.rs` (EXP-892's search sheet) moves onto this.

use gpui::{AnyElement, Hsla};
use gpui_component::Icon;

use domain::rows::Issue;
use domain::statuses::ResolvedStatus;

use super::{OnPickerChange, Picker, PickerItem};

/// One issue row as the picker reads it (EXP-1021 review round).
///
/// `icon` + `color` — the names all four platforms carry — are the OPTIONAL
/// leading glyph: the issue's own coloured STATUS mark, which the relations
/// linker's rows lead with. The CALLER resolves it, because the team's
/// status vocabulary is per-team data and a typed constructor takes no
/// `&App`. The label stays `IDENT Title` either way — that one-line label is
/// the ×4 contract and the fixture; only the glyph is optional.
pub(crate) struct IssuePickerIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub icon: Option<Icon>,
    pub color: Option<Hsla>,
}

impl IssuePickerIssue {
    /// A row with no leading glyph.
    pub(crate) fn new(issue: &Issue) -> Self {
        Self {
            id: issue.id.clone(),
            identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            icon: None,
            color: None,
        }
    }

    /// The row WITH its status glyph, in the status' own colour — what every
    /// IDE surface that lists issues for picking draws.
    pub(crate) fn with_status(issue: &Issue, status: &ResolvedStatus) -> Self {
        Self {
            icon: Some(crate::icons::glyph_icon(status.glyph)),
            color: Some(crate::icons::static_status_tint_color(&status.tint)),
            ..Self::new(issue)
        }
    }
}

pub(crate) fn issue_rows(rows: &[IssuePickerIssue]) -> Vec<PickerItem<String>> {
    rows.iter()
        .map(|row| {
            let mut item =
                PickerItem::new(row.id.clone(), format!("{} {}", row.identifier, row.title))
                    .keywords(vec![row.identifier.clone().into(), row.title.clone().into()]);
            if let Some(icon) = row.icon.clone() {
                item = item.icon(icon);
            }
            if let Some(color) = row.color {
                item = item.color(color);
            }
            item
        })
        .collect()
}

pub(crate) fn issue_items(issues: &[Issue]) -> Vec<PickerItem<String>> {
    let rows: Vec<IssuePickerIssue> = issues.iter().map(IssuePickerIssue::new).collect();
    issue_rows(&rows)
}

pub(crate) fn issue_picker(
    issues: &[Issue],
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(issue_items(issues), value, trigger, on_change)
        .search(true)
        .empty_text("No issues")
}

pub(crate) fn issue_multi_picker(
    issues: &[Issue],
    value: Vec<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::multi(issue_items(issues), value, trigger, on_change)
        .search(true)
        .empty_text("No issues")
}
