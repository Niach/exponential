use super::*;

fn status(id: &str, category: &str, name: &str, color: Option<&str>, order: f64, builtin: Option<&str>) -> domain::rows::IssueStatusRow {
    domain::rows::IssueStatusRow { id: id.into(), team_id: "team-shots".into(), category: category.into(), name: name.into(), color: color.map(str::to_string), sort_order: Some(order), builtin_key: builtin.map(str::to_string), created_at: None, updated_at: None }
}

/// The builtins plus two custom statuses (Unstarted stays empty).
pub(super) fn status_rows() -> Vec<domain::rows::IssueStatusRow> {
    vec![
        status("s-backlog", "backlog", "Backlog", None, 0., Some("backlog")),
        status("s-progress", "started", "In Progress", None, 0., Some("in_progress")),
        status("s-blocked", "started", "Blocked", Some("#ef4444"), 1., None),
        status("s-review", "started", "In Review", None, 2., Some("in_review")),
        status("s-done", "completed", "Done", None, 0., Some("done")),
        status("s-verified", "completed", "Verified", Some("#8b5cf6"), 1., None),
        status("s-cancelled", "cancelled", "Cancelled", None, 0., Some("cancelled")),
        status("s-duplicate", "duplicate", "Duplicate", None, 0., Some("duplicate")),
    ]
}

fn status_counts() -> HashMap<String, usize> {
    HashMap::from([("s-backlog".to_string(), 12), ("s-progress".to_string(), 3), ("s-blocked".to_string(), 1), ("s-done".to_string(), 48)])
}

fn statuses_shot(rows: Vec<domain::rows::IssueStatusRow>) -> StatusesShot {
    StatusesShot { counts: status_counts(), ready: true, creating: None, row_error: None, press: Vec::new(), rows }
}

fn statuses(name: &str, shot: StatusesShot, clicks: &'static [(f32, f32)]) {
    shoot_pane_clicked(name, StatusesPane::for_shots, move |pane, cx| pane.show(shot, cx), None, clicks);
}


#[test]
#[ignore]
fn settings_statuses_rows() {
    statuses("statuses-rows", statuses_shot(status_rows()), &[]);
}

#[test]
#[ignore]
fn settings_statuses_color_open() {
    // Before: a click at the Blocked row's glyph (46, 415) opened today's popover.
    let press = vec!["started_row_color.s-blocked.customTrigger|started_row_color.customTrigger.s-blocked".into()];
    statuses("statuses-color-open", StatusesShot { press, ..statuses_shot(status_rows()) }, &[]);
}

#[test]
#[ignore]
fn settings_statuses_menu_open() {
    // Before: a click at the Blocked row's ⋯ (709, 415) only hovered it (the
    // pointer menu does not open headless).
    let press = vec!["started_row_more.s-blocked".into()];
    statuses("statuses-menu-open", StatusesShot { press, ..statuses_shot(status_rows()) }, &[]);
}

#[test]
#[ignore]
fn settings_statuses_create() {
    let shot = StatusesShot { creating: Some((domain::statuses::IssueStatusCategory::Unstarted, String::new())), ..statuses_shot(status_rows()) };
    statuses("statuses-create", shot, &[]);
}

#[test]
#[ignore]
fn settings_statuses_create_duplicate() {
    let shot = StatusesShot { creating: Some((domain::statuses::IssueStatusCategory::Completed, "done".into())), ..statuses_shot(status_rows()) };
    statuses("statuses-create-duplicate", shot, &[]);
}

#[test]
#[ignore]
fn settings_statuses_row_error() {
    let shot = StatusesShot { row_error: Some(("s-blocked".into(), "A status with this name already exists.".into())), ..statuses_shot(status_rows()) };
    statuses("statuses-row-error", shot, &[]);
}

/// Started at its cap: the band's "+" disabled and the cap line.
#[test]
#[ignore]
fn settings_statuses_capped() {
    let mut rows = status_rows();
    rows.push(status("s-qa", "started", "QA", Some("#eab308"), 3., None));
    rows.push(status("s-ready", "unstarted", "Ready", Some("#3b82f6"), 0., None));
    statuses("statuses-capped", statuses_shot(rows), &[]);
}

#[test]
#[ignore]
fn settings_statuses_loading() {
    statuses("statuses-loading", StatusesShot { ready: false, ..statuses_shot(Vec::new()) }, &[]);
}

#[test]
#[ignore]
fn settings_statuses_empty() {
    statuses("statuses-empty", statuses_shot(Vec::new()), &[]);
}

/// The delete dialog's body: the server count, the PR-pin notice and the
/// target list.
#[test]
#[ignore]
fn settings_statuses_delete_pin() {
    let notice = "This status is where issues move when a pull request merges. That automation will fall back to Done.";
    shoot_pane("statuses-delete-pin", move |window, cx| DeleteStatusContent::for_shots(&status_rows(), "s-verified", Some(3), Some(notice), window, cx), |_, _| {});
}

fn delete_body(name: &'static str, count: Option<i64>) {
    shoot_pane(name, move |window, cx| DeleteStatusContent::for_shots(&status_rows(), "s-verified", count, None, window, cx), |_, _| {});
}

/// The count still in flight (or failed): no count line.
#[test]
#[ignore]
fn settings_statuses_delete_loading() {
    delete_body("statuses-delete-loading", None);
}

#[test]
#[ignore]
fn settings_statuses_delete_zero() {
    delete_body("statuses-delete-zero", Some(0));
}

#[test]
#[ignore]
fn settings_statuses_delete_n() {
    delete_body("statuses-delete-n", Some(2));
}

/// More issues than the client synced: some sit on trashed boards.
#[test]
#[ignore]
fn settings_statuses_delete_trashed() {
    delete_body("statuses-delete-trashed", Some(5));
}

/// The second row pressed: it takes the selection from Backlog.
#[test]
#[ignore]
fn settings_statuses_delete_second() {
    let build = move |window: &mut Window, cx: &mut gpui::Context<DeleteStatusContent>| DeleteStatusContent::for_shots(&status_rows(), "s-verified", Some(2), None, window, cx);
    shoot_pane_acted("statuses-delete-second", build, |_, _| {}, None, &[], |pane, _, cx| pane.press_target("s-progress", cx));
}
