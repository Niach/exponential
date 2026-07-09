//! Project-board grouping — mirror of `apps/web/src/lib/project-board.ts`
//! (masterplan-v3 §4.1/§4.6). Pure functions over [`Issue`] rows; the `ui`
//! crate's `queries.rs` feeds them collection snapshots.

use std::collections::HashMap;

use crate::enums::{IssuePriority, IssueStatus};
use crate::filters::{matches_filters, IssueFilters};
use crate::rows::{Issue, IssueLabel};

/// Web `priorityRank` — sort weight inside a status group.
fn priority_rank(priority: IssuePriority) -> u8 {
    match priority {
        IssuePriority::Urgent => 0,
        IssuePriority::High => 1,
        IssuePriority::Medium => 2,
        IssuePriority::Low => 3,
        IssuePriority::None => 4,
        // Forward-compat Unknown sorts with None (web can't receive one).
        IssuePriority::Unknown => 4,
    }
}

/// Web `isIssueOverdue`. `today` is a `YYYY-MM-DD` date string; `due_date`
/// compares lexicographically (ISO dates order correctly as strings — the
/// same `<` the TS uses).
pub fn is_issue_overdue(issue: &Issue, today: &str) -> bool {
    match issue.due_date.as_deref() {
        Some(due) => {
            due < today
                && issue.status != IssueStatus::Done
                && issue.status != IssueStatus::Cancelled
                && issue.status != IssueStatus::Duplicate
        }
        None => false,
    }
}

/// Web `compareIssuesForGroup(status, today)` — the EXP-38 canonical
/// per-status comparator, identical on web / iOS / Android / desktop:
///
/// - **backlog / todo / in_progress**: overdue first, then priority rank
///   ascending, then earliest due date (dated before undated), then issue
///   `number` ascending — numerically, never identifier-string order (and
///   never `sort_order`/`created_at`).
/// - **done**: latest completed first — key `completed_at ?? updated_at`,
///   descending.
/// - **cancelled / duplicate**: `updated_at` descending.
fn compare_issues_for_group(
    a: &Issue,
    b: &Issue,
    status: IssueStatus,
    today: &str,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match status {
        IssueStatus::Done => cmp_recency_desc(
            a.completed_at.as_deref().or(a.updated_at.as_deref()),
            b.completed_at.as_deref().or(b.updated_at.as_deref()),
        ),
        IssueStatus::Cancelled | IssueStatus::Duplicate => {
            cmp_recency_desc(a.updated_at.as_deref(), b.updated_at.as_deref())
        }
        _ => {
            let a_overdue = is_issue_overdue(a, today);
            let b_overdue = is_issue_overdue(b, today);
            if a_overdue != b_overdue {
                return if a_overdue { Ordering::Less } else { Ordering::Greater };
            }

            let priority = priority_rank(a.priority).cmp(&priority_rank(b.priority));
            if priority != Ordering::Equal {
                return priority;
            }

            let due = match (a.due_date.as_deref(), b.due_date.as_deref()) {
                (Some(a_due), Some(b_due)) => a_due.cmp(b_due),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            };
            if due != Ordering::Equal {
                return due;
            }

            a.number.cmp(&b.number)
        }
    }
}

/// Recency-key compare, latest FIRST; rows missing the key sort last. Electric
/// delivers timestamptz text uniformly (`YYYY-MM-DD hh:mm:ss…+00`), so string
/// order is chronological — the space→`T` normalization (the shared-contract
/// mixed-format gotcha) additionally keeps the compare correct against ISO
/// `…T…Z` strings should one ever appear.
fn cmp_recency_desc(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Some(a), Some(b)) => normalize_timestamp(b).cmp(&normalize_timestamp(a)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// `YYYY-MM-DD hh:mm:ss…` → `YYYY-MM-DDThh:mm:ss…` (first space only).
fn normalize_timestamp(ts: &str) -> String {
    ts.replacen(' ', "T", 1)
}

/// Web `interface IssueGroup`.
#[derive(Clone, Debug, PartialEq)]
pub struct IssueGroup {
    pub status: IssueStatus,
    pub issues: Vec<Issue>,
}

/// Web `buildIssueLabelIdsMap(issueLabels)` — issue id → its label ids.
pub fn build_issue_label_ids_map(issue_labels: &[IssueLabel]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for link in issue_labels {
        map.entry(link.issue_id.clone())
            .or_default()
            .push(link.label_id.clone());
    }
    map
}

/// Web `buildFilteredIssues(issues, issueLabelIdsMap, filters)`.
pub fn build_filtered_issues(
    issues: Vec<Issue>,
    issue_label_ids: &HashMap<String, Vec<String>>,
    filters: &IssueFilters,
) -> Vec<Issue> {
    const NO_LABELS: &[String] = &[];
    issues
        .into_iter()
        .filter(|issue| {
            let label_ids = issue_label_ids
                .get(&issue.id)
                .map(Vec::as_slice)
                .unwrap_or(NO_LABELS);
            matches_filters(issue, label_ids, filters)
        })
        .collect()
}

/// Web `buildVisibleIssueGroups(issues, statuses)`: group by status in the
/// domain display order (web `issueStatusOrder` == `DISPLAY_ORDER`), sort
/// each group with the EXP-38 per-status comparator above, then EITHER keep
/// exactly the status-filtered groups (even when empty) OR hide empty groups
/// when no status filter is active. `today` is `YYYY-MM-DD` (web computes it
/// via `formatDateForMutation(new Date())`).
pub fn build_visible_issue_groups(
    issues: &[Issue],
    statuses: &[IssueStatus],
    today: &str,
) -> Vec<IssueGroup> {
    let mut groups: Vec<IssueGroup> = IssueStatus::DISPLAY_ORDER
        .iter()
        .map(|&status| {
            let mut group_issues: Vec<Issue> = issues
                .iter()
                .filter(|issue| issue.status == status)
                .cloned()
                .collect();
            // `.sort()` in JS is stable; mirror with sort_by (stable in Rust).
            group_issues.sort_by(|a, b| compare_issues_for_group(a, b, status, today));
            IssueGroup {
                status,
                issues: group_issues,
            }
        })
        .collect();

    if !statuses.is_empty() {
        groups.retain(|group| statuses.contains(&group.status));
        return groups;
    }

    groups.retain(|group| !group.issues.is_empty());
    groups
}

/// Web `formatDate` (`lib/utils.ts`): `YYYY-MM-DD` → `"Jul 3"` (en-US short
/// month + numeric day). Returns the input unchanged when it does not parse.
pub fn format_short_date(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = date.splitn(3, '-');
    let (Some(_year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return date.to_string();
    };
    let Ok(month_num) = month.parse::<usize>() else {
        return date.to_string();
    };
    // The day part may carry a time suffix in tolerated inputs — digits only.
    let day_digits: String = day.chars().take_while(|c| c.is_ascii_digit()).collect();
    let Ok(day_num) = day_digits.parse::<u32>() else {
        return date.to_string();
    };
    if !(1..=12).contains(&month_num) || !(1..=31).contains(&day_num) {
        return date.to_string();
    }
    format!("{} {}", MONTHS[month_num - 1], day_num)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn issue(id: &str, status: &str, priority: &str, due: Option<&str>) -> Issue {
        issue_n(id, 1, status, priority, due)
    }

    fn issue_n(id: &str, number: i64, status: &str, priority: &str, due: Option<&str>) -> Issue {
        serde_json::from_value(json!({
            "id": id,
            "project_id": "p-1",
            "number": number,
            "identifier": format!("EXP-{id}"),
            "title": id,
            "status": status,
            "priority": priority,
            "due_date": due
        }))
        .unwrap()
    }

    /// A terminal-group row (done/cancelled/duplicate) with recency keys.
    fn closed_issue(
        id: &str,
        status: &str,
        completed_at: Option<&str>,
        updated_at: Option<&str>,
    ) -> Issue {
        serde_json::from_value(json!({
            "id": id,
            "project_id": "p-1",
            "number": 1,
            "identifier": format!("EXP-{id}"),
            "title": id,
            "status": status,
            "priority": "none",
            "completed_at": completed_at,
            "updated_at": updated_at
        }))
        .unwrap()
    }

    fn link(issue_id: &str, label_id: &str) -> IssueLabel {
        serde_json::from_value(json!({ "issue_id": issue_id, "label_id": label_id })).unwrap()
    }

    const TODAY: &str = "2026-07-03";

    #[test]
    fn overdue_needs_past_due_and_open_status() {
        assert!(is_issue_overdue(
            &issue("a", "todo", "none", Some("2026-07-02")),
            TODAY
        ));
        // Today is not overdue (strict <).
        assert!(!is_issue_overdue(
            &issue("a", "todo", "none", Some("2026-07-03")),
            TODAY
        ));
        // Closed-ish statuses are never overdue.
        for closed in ["done", "cancelled", "duplicate"] {
            assert!(!is_issue_overdue(
                &issue("a", closed, "none", Some("2020-01-01")),
                TODAY
            ));
        }
        assert!(!is_issue_overdue(&issue("a", "todo", "none", None), TODAY));
    }

    #[test]
    fn open_group_sort_is_overdue_then_priority_then_due_then_number() {
        let issues = vec![
            issue_n("none-undated", 3, "todo", "none", None),
            issue_n("low-late-due", 4, "todo", "low", Some("2026-08-01")),
            issue_n("low-early-due", 5, "todo", "low", Some("2026-07-10")),
            issue_n("urgent", 6, "todo", "urgent", None),
            issue_n("overdue-none", 7, "todo", "none", Some("2026-01-01")),
            // Same priority + due date → number breaks the tie.
            issue_n("high-n9", 9, "todo", "high", Some("2026-07-20")),
            issue_n("high-n8", 8, "todo", "high", Some("2026-07-20")),
        ];
        let groups = build_visible_issue_groups(&issues, &[], TODAY);
        assert_eq!(groups.len(), 1);
        let order: Vec<&str> = groups[0].issues.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "overdue-none", // overdue floats above even urgent
                "urgent",
                "high-n8", // priority + due tie → lower number first
                "high-n9",
                "low-early-due", // same priority → earlier due first
                "low-late-due",
                "none-undated", // weakest priority rank (none = 4) sorts last
            ]
        );
    }

    #[test]
    fn number_tiebreak_is_numeric_not_lexicographic() {
        let issues = vec![
            issue_n("n10", 10, "todo", "none", None),
            issue_n("n2", 2, "todo", "none", None),
        ];
        let groups = build_visible_issue_groups(&issues, &[], TODAY);
        let order: Vec<&str> = groups[0].issues.iter().map(|i| i.id.as_str()).collect();
        // "10" < "2" as strings — numeric compare must win.
        assert_eq!(order, vec!["n2", "n10"]);
    }

    #[test]
    fn done_group_sorts_latest_completed_first_with_updated_fallback() {
        let issues = vec![
            closed_issue(
                "old-done",
                "done",
                Some("2026-07-01 08:00:00+00"),
                Some("2026-07-01 08:00:00+00"),
            ),
            closed_issue(
                "new-done",
                "done",
                Some("2026-07-02 09:00:00+00"),
                Some("2026-07-02 09:00:00+00"),
            ),
            // No completed_at → updated_at stands in.
            closed_issue("fallback", "done", None, Some("2026-07-03 10:00:00+00")),
            // Neither key → sorts last.
            closed_issue("keyless", "done", None, None),
        ];
        let groups = build_visible_issue_groups(&issues, &[], TODAY);
        assert_eq!(groups.len(), 1);
        let order: Vec<&str> = groups[0].issues.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(order, vec!["fallback", "new-done", "old-done", "keyless"]);
    }

    #[test]
    fn cancelled_and_duplicate_groups_sort_by_updated_desc() {
        for status in ["cancelled", "duplicate"] {
            let issues = vec![
                closed_issue("older", status, None, Some("2026-06-30 08:00:00+00")),
                closed_issue("newer", status, None, Some("2026-07-02 08:00:00+00")),
                // Mixed-format robustness: an ISO `T…Z` string still lands in
                // chronological position (the space→T normalization).
                closed_issue("newest", status, None, Some("2026-07-02T09:00:00Z")),
            ];
            let groups = build_visible_issue_groups(&issues, &[], TODAY);
            assert_eq!(groups.len(), 1);
            let order: Vec<&str> = groups[0].issues.iter().map(|i| i.id.as_str()).collect();
            assert_eq!(order, vec!["newest", "newer", "older"], "status {status}");
        }
    }

    #[test]
    fn groups_follow_display_order_and_hide_empty() {
        let issues = vec![
            issue("d", "done", "none", None),
            issue("t", "todo", "none", None),
            issue("i", "in_progress", "none", None),
        ];
        let groups = build_visible_issue_groups(&issues, &[], TODAY);
        let statuses: Vec<IssueStatus> = groups.iter().map(|g| g.status).collect();
        // Display order with empty groups (backlog/cancelled/duplicate) hidden.
        assert_eq!(
            statuses,
            vec![IssueStatus::InProgress, IssueStatus::Todo, IssueStatus::Done]
        );
    }

    #[test]
    fn status_filter_keeps_selected_groups_even_when_empty() {
        // web: `if (statuses.length > 0) return groups.filter((g) =>
        // statuses.includes(g.status))` — WITHOUT the emptiness filter.
        let issues = vec![issue("t", "todo", "none", None)];
        let groups = build_visible_issue_groups(
            &issues,
            &[IssueStatus::InProgress, IssueStatus::Todo],
            TODAY,
        );
        let statuses: Vec<IssueStatus> = groups.iter().map(|g| g.status).collect();
        assert_eq!(statuses, vec![IssueStatus::InProgress, IssueStatus::Todo]);
        assert!(groups[0].issues.is_empty());
        assert_eq!(groups[1].issues.len(), 1);
    }

    #[test]
    fn label_map_and_filtering_mirror_web() {
        let links = vec![link("i-1", "l-1"), link("i-1", "l-2"), link("i-2", "l-3")];
        let map = build_issue_label_ids_map(&links);
        assert_eq!(map["i-1"], vec!["l-1".to_string(), "l-2".to_string()]);
        assert_eq!(map["i-2"], vec!["l-3".to_string()]);

        let filters = IssueFilters {
            label_ids: vec!["l-2".to_string()],
            ..Default::default()
        };
        let issues = vec![
            issue("i-1", "todo", "none", None),
            issue("i-2", "todo", "none", None),
            issue("i-3", "todo", "none", None),
        ];
        let filtered = build_filtered_issues(issues, &map, &filters);
        let ids: Vec<&str> = filtered.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["i-1"]);
    }

    #[test]
    fn short_date_formats_like_web() {
        assert_eq!(format_short_date("2026-07-03"), "Jul 3");
        assert_eq!(format_short_date("2026-12-25"), "Dec 25");
        assert_eq!(format_short_date("2026-01-09"), "Jan 9");
        // Unparseable input passes through untouched.
        assert_eq!(format_short_date("soon"), "soon");
    }
}
