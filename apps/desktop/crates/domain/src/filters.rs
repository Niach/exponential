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

/// Web `matchesFilters(issue, issueLabelIds, statusKey, filters)`. Each active
/// category must match (AND across categories); within a category any value
/// matches (OR). An empty category is a pass. `status_key` is the issue's
/// resolved group key (`domain::statuses::resolve_status`).
pub fn matches_filters(
    issue: &Issue,
    issue_label_ids: &[String],
    status_key: &str,
    filters: &IssueFilters,
) -> bool {
    if !filters.status_keys.is_empty() && !filters.status_keys.iter().any(|key| key == status_key) {
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
    use serde_json::json;

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
        assert!(matches_filters(&issue("todo", "none"), &[], "s-1", &filters));
        assert!(matches_filters(
            &issue("done", "urgent"),
            &["l-1".to_string()],
            "builtin:done",
            &filters
        ));
    }

    #[test]
    fn matches_filters_status_category_is_group_keys() {
        let filters = IssueFilters {
            status_keys: vec!["s-todo".to_string(), "s-qa".to_string()],
            ..Default::default()
        };
        assert!(matches_filters(
            &issue("todo", "none"),
            &[],
            "s-todo",
            &filters
        ));
        // A custom started status matches by KEY, not by its `in_progress`
        // anchor.
        assert!(matches_filters(
            &issue("in_progress", "none"),
            &[],
            "s-qa",
            &filters
        ));
        assert!(!matches_filters(
            &issue("in_progress", "none"),
            &[],
            "s-in-progress",
            &filters
        ));
        // Pre-sync fallback keys work the same way.
        let filters = IssueFilters {
            status_keys: vec!["builtin:todo".to_string()],
            ..Default::default()
        };
        assert!(matches_filters(
            &issue("todo", "none"),
            &[],
            "builtin:todo",
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
            &issue("todo", "urgent"),
            &[],
            "s-1",
            &filters
        ));
        assert!(!matches_filters(&issue("todo", "low"), &[], "s-1", &filters));
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
            "s-1",
            &filters
        ));
        assert!(!matches_filters(
            &issue("todo", "none"),
            &["l-9".to_string()],
            "s-1",
            &filters
        ));
        assert!(!matches_filters(&issue("todo", "none"), &[], "s-1", &filters));
    }

    #[test]
    fn matches_filters_is_and_across_categories() {
        let filters = IssueFilters {
            status_keys: vec!["s-todo".to_string()],
            priorities: vec![IssuePriority::High],
            label_ids: vec!["l-1".to_string()],
        };
        assert!(matches_filters(
            &issue("todo", "high"),
            &["l-1".to_string()],
            "s-todo",
            &filters
        ));
        // Right status + label, wrong priority → fail.
        assert!(!matches_filters(
            &issue("todo", "low"),
            &["l-1".to_string()],
            "s-todo",
            &filters
        ));
    }

    #[test]
    fn active_filter_count_sums_all_categories() {
        let filters = IssueFilters {
            status_keys: vec!["s-todo".to_string(), "s-done".to_string()],
            priorities: vec![IssuePriority::Low],
            label_ids: vec!["l-1".to_string()],
        };
        assert_eq!(active_filter_count(&filters), 4);
        assert!(has_active_filters(&filters));
        assert_eq!(active_filter_count(&empty_filters()), 0);
        assert!(!has_active_filters(&empty_filters()));
    }
}
