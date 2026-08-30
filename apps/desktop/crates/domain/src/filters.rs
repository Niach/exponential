//! Issue filters — a VERBATIM port of `apps/web/src/lib/filters.ts`
//! (masterplan-v3 §4.7).
//!
//! The `IssueFilters` shape and `matches_filters()` are mirrored across four
//! clients: web (`lib/filters.ts`), iOS (`Domain/IssueFilters.swift`), Android
//! (`domain/IssueFilters.kt`) and this crate. If you change the filter shape
//! or matching semantics here, update the other three to keep the clients in
//! lockstep (no shared package yet).

use crate::enums::IssuePriority;
use crate::rows::Issue;
use crate::statuses::{status_key_matches, ResolvedStatus};

/// `IssueFilters` — web `interface IssueFilters`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IssueFilters {
    /// EXP-314: RESOLVED status group keys (an `issue_statuses` row id, or
    /// `builtin:<key>` while the statuses shape has not synced) — not enum
    /// values. An issue matches when its resolved group key is in the set.
    pub status_keys: Vec<String>,
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

/// Web `matchesFilters(issue, issueLabelIds, filters, resolvedStatus)`. Each
/// active category must match (AND across categories); within a category any
/// value matches (OR). An empty category is a pass. `status` is the issue's
/// RESOLVED status (`domain::statuses::resolve_status`) — matching goes
/// through [`status_key_matches`], so a `builtin:<key>` token stored before
/// the statuses shape synced keeps selecting the synced row it re-keyed to.
pub fn matches_filters(
    issue: &Issue,
    issue_label_ids: &[String],
    status: &ResolvedStatus,
    filters: &IssueFilters,
) -> bool {
    if !filters.status_keys.is_empty()
        && !filters
            .status_keys
            .iter()
            .any(|token| status_key_matches(status, token))
    {
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
    filters.status_keys.len() + filters.priorities.len() + filters.label_ids.len()
}

/// Web `hasActiveFilters(filters)`.
pub fn has_active_filters(filters: &IssueFilters) -> bool {
    active_filter_count(filters) > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{ColorToken, IconGlyph};
    use crate::statuses::{IssueStatusCategory, StatusTint};
    use serde_json::json;

    /// A synced status row as the UI resolved it (`group_key` = row uuid).
    fn synced(group_key: &str, builtin_key: Option<&str>) -> ResolvedStatus {
        ResolvedStatus {
            group_key: group_key.to_string(),
            row_id: Some(group_key.to_string()),
            name: group_key.to_string(),
            category: IssueStatusCategory::Unstarted,
            tint: StatusTint::Token(ColorToken::Foreground),
            glyph: IconGlyph::Circle,
            builtin_key: builtin_key.map(str::to_string),
        }
    }

    /// A CONSTRUCTED fallback (`group_key` = `builtin:<key>`).
    fn fallback(builtin_key: &str) -> ResolvedStatus {
        ResolvedStatus {
            group_key: format!("builtin:{builtin_key}"),
            row_id: None,
            ..synced(builtin_key, Some(builtin_key))
        }
    }

    fn issue(status: &str, priority: &str) -> Issue {
        serde_json::from_value(json!({
            "id": "i-1",
            "board_id": "p-1",
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
        assert!(filters.status_keys.is_empty());
        assert!(filters.priorities.is_empty());
        assert!(filters.label_ids.is_empty());
        assert_eq!(filters, IssueFilters::empty());
    }

    #[test]
    fn matches_filters_passes_everything_when_empty() {
        let filters = empty_filters();
        assert!(matches_filters(&issue("backlog", "none"), &[], &synced("s-1", None), &filters));
        assert!(matches_filters(
            &issue("done", "urgent"),
            &["l-1".to_string()],
            &fallback("done"),
            &filters
        ));
    }

    #[test]
    fn matches_filters_status_category_is_group_keys() {
        let filters = IssueFilters {
            status_keys: vec!["s-backlog".to_string(), "s-qa".to_string()],
            ..Default::default()
        };
        assert!(matches_filters(
            &issue("backlog", "none"),
            &[],
            &synced("s-backlog", Some("backlog")),
            &filters
        ));
        // A custom started status matches by KEY, not by its `in_progress`
        // anchor.
        assert!(matches_filters(
            &issue("in_progress", "none"),
            &[],
            &synced("s-qa", None),
            &filters
        ));
        assert!(!matches_filters(
            &issue("in_progress", "none"),
            &[],
            &synced("s-in-progress", Some("in_progress")),
            &filters
        ));
        // Pre-sync fallback keys work the same way.
        let filters = IssueFilters {
            status_keys: vec!["builtin:backlog".to_string()],
            ..Default::default()
        };
        assert!(matches_filters(
            &issue("backlog", "none"),
            &[],
            &fallback("backlog"),
            &filters
        ));
    }

    #[test]
    fn a_builtin_token_still_matches_the_synced_row_it_rekeyed_to() {
        // Stored while only the constructed vocabulary existed; the shape has
        // since synced and the group key is the row uuid now.
        let filters = IssueFilters {
            status_keys: vec!["builtin:in_progress".to_string()],
            ..Default::default()
        };
        assert!(matches_filters(
            &issue("in_progress", "none"),
            &[],
            &synced("row-wip", Some("in_progress")),
            &filters
        ));
        // …but it must not select a different builtin, nor a custom row.
        assert!(!matches_filters(
            &issue("backlog", "none"),
            &[],
            &synced("row-backlog", Some("backlog")),
            &filters
        ));
        assert!(!matches_filters(
            &issue("in_progress", "none"),
            &[],
            &synced("row-qa", None),
            &filters
        ));
        // A row-uuid token keeps matching exactly one row.
        let filters = IssueFilters {
            status_keys: vec!["row-wip".to_string()],
            ..Default::default()
        };
        assert!(matches_filters(
            &issue("in_progress", "none"),
            &[],
            &synced("row-wip", Some("in_progress")),
            &filters
        ));
        assert!(!matches_filters(
            &issue("in_progress", "none"),
            &[],
            &fallback("in_progress"),
            &filters
        ));
    }

    #[test]
    fn matches_filters_priority_category() {
        let filters = IssueFilters {
            priorities: vec![IssuePriority::Urgent],
            ..Default::default()
        };
        assert!(matches_filters(
            &issue("backlog", "urgent"),
            &[],
            &synced("s-1", None),
            &filters
        ));
        assert!(!matches_filters(&issue("backlog", "low"), &[], &synced("s-1", None), &filters));
    }

    #[test]
    fn matches_filters_labels_are_any_of() {
        let filters = IssueFilters {
            label_ids: vec!["l-1".to_string(), "l-2".to_string()],
            ..Default::default()
        };
        // web: filters.labelIds.some((id) => issueLabelIds.includes(id))
        assert!(matches_filters(
            &issue("backlog", "none"),
            &["l-2".to_string(), "l-9".to_string()],
            &synced("s-1", None),
            &filters
        ));
        assert!(!matches_filters(
            &issue("backlog", "none"),
            &["l-9".to_string()],
            &synced("s-1", None),
            &filters
        ));
        assert!(!matches_filters(&issue("backlog", "none"), &[], &synced("s-1", None), &filters));
    }

    #[test]
    fn matches_filters_is_and_across_categories() {
        let filters = IssueFilters {
            status_keys: vec!["s-backlog".to_string()],
            priorities: vec![IssuePriority::High],
            label_ids: vec!["l-1".to_string()],
        };
        assert!(matches_filters(
            &issue("backlog", "high"),
            &["l-1".to_string()],
            &synced("s-backlog", Some("backlog")),
            &filters
        ));
        // Right status + label, wrong priority → fail.
        assert!(!matches_filters(
            &issue("backlog", "low"),
            &["l-1".to_string()],
            &synced("s-backlog", Some("backlog")),
            &filters
        ));
    }

    #[test]
    fn active_filter_count_sums_all_categories() {
        let filters = IssueFilters {
            status_keys: vec!["s-backlog".to_string(), "s-done".to_string()],
            priorities: vec![IssuePriority::Low],
            label_ids: vec!["l-1".to_string()],
        };
        assert_eq!(active_filter_count(&filters), 4);
        assert!(has_active_filters(&filters));
        assert_eq!(active_filter_count(&empty_filters()), 0);
        assert!(!has_active_filters(&empty_filters()));
    }
}
