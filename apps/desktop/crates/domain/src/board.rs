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

/// Web `compareIssuesForGroup(today)`: overdue first, then priority rank, then
/// earliest due date (dated before undated), else stable.
fn compare_issues_for_group(a: &Issue, b: &Issue, today: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let a_overdue = is_issue_overdue(a, today);
    let b_overdue = is_issue_overdue(b, today);
    if a_overdue != b_overdue {
        return if a_overdue { Ordering::Less } else { Ordering::Greater };
    }

    let priority = priority_rank(a.priority).cmp(&priority_rank(b.priority));
    if priority != Ordering::Equal {
        return priority;
    }

    match (a.due_date.as_deref(), b.due_date.as_deref()) {
        (Some(a_due), Some(b_due)) => a_due.cmp(b_due),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
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
/// each group overdue→priority→due-date, then EITHER keep exactly the
/// status-filtered groups (even when empty) OR hide empty groups when no
/// status filter is active. `today` is `YYYY-MM-DD` (web computes it via
/// `formatDateForMutation(new Date())`).
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
            group_issues.sort_by(|a, b| compare_issues_for_group(a, b, today));
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

/// `YYYY-MM-DD` for a Unix timestamp (UTC civil date, Howard Hinnant's
/// `civil_from_days`). The desktop has no timezone database dependency, so
/// "today" for the overdue boundary is the UTC date — a ≤1-day skew vs. web's
/// local-time date for users west of UTC, acceptable for row ordering.
pub fn utc_date_string(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
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
        serde_json::from_value(json!({
            "id": id,
            "project_id": "p-1",
            "number": 1,
            "identifier": format!("EXP-{id}"),
            "title": id,
            "status": status,
            "priority": priority,
            "due_date": due
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
    fn group_sort_is_overdue_then_priority_then_due() {
        let issues = vec![
            issue("none-undated", "todo", "none", None),
            issue("low-late-due", "todo", "low", Some("2026-08-01")),
            issue("low-early-due", "todo", "low", Some("2026-07-10")),
            issue("urgent", "todo", "urgent", None),
            issue("overdue-none", "todo", "none", Some("2026-01-01")),
        ];
        let groups = build_visible_issue_groups(&issues, &[], TODAY);
        assert_eq!(groups.len(), 1);
        let order: Vec<&str> = groups[0].issues.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "overdue-none",  // overdue floats above even urgent
                "urgent",
                "low-early-due", // same priority → earlier due first
                "low-late-due",
                "none-undated", // weakest priority rank (none = 4) sorts last
            ]
        );
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
    fn utc_date_string_is_civil_correct() {
        assert_eq!(utc_date_string(0), "1970-01-01");
        assert_eq!(utc_date_string(86_400), "1970-01-02");
        // 2026-07-03T00:00:00Z (20637 days) and mid-day same date.
        assert_eq!(utc_date_string(20_637 * 86_400), "2026-07-03");
        assert_eq!(utc_date_string(20_637 * 86_400 + 43_200), "2026-07-03");
        // Leap day.
        assert_eq!(utc_date_string(1_709_164_800), "2024-02-29");
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
