//! Issue filters — a VERBATIM port of `apps/web/src/lib/filters.ts`
//! (masterplan-v3 §4.7).
//!
//! Tab presets and `matches_filters()` are mirrored across four clients: web
//! (`lib/filters.ts`), iOS (`Domain/IssueFilters.swift`), Android
//! (`domain/IssueFilters.kt`) and this crate. If you change the active/backlog
//! status mapping or the filter shape here, update the other three to keep the
//! clients in lockstep (no shared package yet).
//!
//! Behavior is byte-identical to the TS: the `active` set is
//! `{in_progress, in_review, todo}` — memorize it; do not "fix" it (§4.7).

use crate::enums::{IssuePriority, IssueStatus};
use crate::rows::Issue;

/// `IssueFilters` — web `interface IssueFilters`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IssueFilters {
    pub statuses: Vec<IssueStatus>,
    pub priorities: Vec<IssuePriority>,
    pub label_ids: Vec<String>,
}

/// Web `emptyFilters`.
pub fn empty_filters() -> IssueFilters {
    IssueFilters::default()
}

impl IssueFilters {
    /// Web `emptyFilters` as an associated constructor.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Web `type TabPreset = "all" | "active" | "backlog"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabPreset {
    All,
    Active,
    Backlog,
}

/// Web `tabPresetStatuses` — the status set each tab preselects.
pub fn tab_preset_statuses(preset: TabPreset) -> &'static [IssueStatus] {
    match preset {
        TabPreset::All => &[],
        TabPreset::Active => &[
            IssueStatus::InProgress,
            IssueStatus::InReview,
            IssueStatus::Todo,
        ],
        TabPreset::Backlog => &[IssueStatus::Backlog],
    }
}

/// Web `deriveActiveTab(statuses)`: which tab a status set corresponds to.
/// Exact-set comparison (order-insensitive), anything else falls back to All —
/// mirroring the TS sort-and-compare.
pub fn derive_active_tab(statuses: &[IssueStatus]) -> TabPreset {
    if statuses.is_empty() {
        return TabPreset::All;
    }
    let sorted = sorted_wire(statuses);
    if sorted == sorted_wire(tab_preset_statuses(TabPreset::Active)) {
        return TabPreset::Active;
    }
    if sorted == sorted_wire(tab_preset_statuses(TabPreset::Backlog)) {
        return TabPreset::Backlog;
    }
    TabPreset::All
}

/// The TS compares `[...statuses].sort()` — i.e. the canonical wire strings in
/// lexicographic order. Mirror that exactly (enum-ordinal sorting would be a
/// silent behavioral fork).
fn sorted_wire(statuses: &[IssueStatus]) -> Vec<&'static str> {
    let mut wire: Vec<&'static str> = statuses
        .iter()
        .map(|s| s.as_wire().unwrap_or("unknown"))
        .collect();
    wire.sort_unstable();
    wire
}

/// Web `matchesFilters(issue, issueLabelIds, filters)`. Each active category
/// must match (AND across categories); within a category any value matches
/// (OR). An empty category is a pass.
pub fn matches_filters(issue: &Issue, issue_label_ids: &[String], filters: &IssueFilters) -> bool {
    if !filters.statuses.is_empty() && !filters.statuses.contains(&issue.status) {
        return false;
    }
    if !filters.priorities.is_empty() && !filters.priorities.contains(&issue.priority) {
        return false;
    }
    if !filters.label_ids.is_empty()
        && !filters
            .label_ids
            .iter()
            .any(|id| issue_label_ids.contains(id))
    {
        return false;
    }
    true
}

/// Web `activeFilterCount(filters)`.
pub fn active_filter_count(filters: &IssueFilters) -> usize {
    filters.statuses.len() + filters.priorities.len() + filters.label_ids.len()
}

/// Web `hasActiveFilters(filters)`.
pub fn has_active_filters(filters: &IssueFilters) -> bool {
    active_filter_count(filters) > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn issue(status: &str, priority: &str) -> Issue {
        serde_json::from_value(json!({
            "id": "i-1",
            "project_id": "p-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "t",
            "status": status,
            "priority": priority
        }))
        .unwrap()
    }

    #[test]
    fn empty_filters_is_empty() {
        let filters = empty_filters();
        assert!(filters.statuses.is_empty());
        assert!(filters.priorities.is_empty());
        assert!(filters.label_ids.is_empty());
        assert_eq!(filters, IssueFilters::empty());
    }

    #[test]
    fn tab_presets_mirror_web() {
        // web: all: [], active: [in_progress, in_review, todo], backlog: [backlog]
        assert_eq!(tab_preset_statuses(TabPreset::All), &[] as &[IssueStatus]);
        assert_eq!(
            tab_preset_statuses(TabPreset::Active),
            &[
                IssueStatus::InProgress,
                IssueStatus::InReview,
                IssueStatus::Todo
            ]
        );
        assert_eq!(
            tab_preset_statuses(TabPreset::Backlog),
            &[IssueStatus::Backlog]
        );
    }

    #[test]
    fn derive_active_tab_matches_web_semantics() {
        // Empty → all.
        assert_eq!(derive_active_tab(&[]), TabPreset::All);
        // Exact active set, any order.
        assert_eq!(
            derive_active_tab(&[
                IssueStatus::InProgress,
                IssueStatus::InReview,
                IssueStatus::Todo
            ]),
            TabPreset::Active
        );
        assert_eq!(
            derive_active_tab(&[
                IssueStatus::Todo,
                IssueStatus::InProgress,
                IssueStatus::InReview
            ]),
            TabPreset::Active
        );
        // Exact backlog set.
        assert_eq!(derive_active_tab(&[IssueStatus::Backlog]), TabPreset::Backlog);
        // Superset / subset / anything else → all.
        assert_eq!(
            derive_active_tab(&[
                IssueStatus::InProgress,
                IssueStatus::Todo,
                IssueStatus::Done
            ]),
            TabPreset::All
        );
        // A subset of the active set (missing in_review) is not "active".
        assert_eq!(
            derive_active_tab(&[IssueStatus::InProgress, IssueStatus::Todo]),
            TabPreset::All
        );
        assert_eq!(derive_active_tab(&[IssueStatus::Todo]), TabPreset::All);
        assert_eq!(derive_active_tab(&[IssueStatus::Done]), TabPreset::All);
    }

    #[test]
    fn matches_filters_passes_everything_when_empty() {
        let filters = empty_filters();
        assert!(matches_filters(&issue("todo", "none"), &[], &filters));
        assert!(matches_filters(
            &issue("done", "urgent"),
            &["l-1".to_string()],
            &filters
        ));
    }

    #[test]
    fn matches_filters_status_category() {
        let filters = IssueFilters {
            statuses: vec![IssueStatus::Todo, IssueStatus::InProgress],
            ..Default::default()
        };
        assert!(matches_filters(&issue("todo", "none"), &[], &filters));
        assert!(matches_filters(&issue("in_progress", "none"), &[], &filters));
        assert!(!matches_filters(&issue("done", "none"), &[], &filters));
    }

    #[test]
    fn matches_filters_priority_category() {
        let filters = IssueFilters {
            priorities: vec![IssuePriority::Urgent],
            ..Default::default()
        };
        assert!(matches_filters(&issue("todo", "urgent"), &[], &filters));
        assert!(!matches_filters(&issue("todo", "low"), &[], &filters));
    }

    #[test]
    fn matches_filters_labels_are_any_of() {
        let filters = IssueFilters {
            label_ids: vec!["l-1".to_string(), "l-2".to_string()],
            ..Default::default()
        };
        // web: filters.labelIds.some((id) => issueLabelIds.includes(id))
        assert!(matches_filters(
            &issue("todo", "none"),
            &["l-2".to_string(), "l-9".to_string()],
            &filters
        ));
        assert!(!matches_filters(
            &issue("todo", "none"),
            &["l-9".to_string()],
            &filters
        ));
        assert!(!matches_filters(&issue("todo", "none"), &[], &filters));
    }

    #[test]
    fn matches_filters_is_and_across_categories() {
        let filters = IssueFilters {
            statuses: vec![IssueStatus::Todo],
            priorities: vec![IssuePriority::High],
            label_ids: vec!["l-1".to_string()],
        };
        assert!(matches_filters(
            &issue("todo", "high"),
            &["l-1".to_string()],
            &filters
        ));
        // Right status + label, wrong priority → fail.
        assert!(!matches_filters(
            &issue("todo", "low"),
            &["l-1".to_string()],
            &filters
        ));
    }

    #[test]
    fn active_filter_count_sums_all_categories() {
        let filters = IssueFilters {
            statuses: vec![IssueStatus::Todo, IssueStatus::Done],
            priorities: vec![IssuePriority::Low],
            label_ids: vec!["l-1".to_string()],
        };
        assert_eq!(active_filter_count(&filters), 4);
        assert!(has_active_filters(&filters));
        assert_eq!(active_filter_count(&empty_filters()), 0);
        assert!(!has_active_filters(&empty_filters()));
    }
}
