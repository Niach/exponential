//! #EXP-1629: Settings → Issues (the estimate scale, PR automation).

use super::*;
use super::statuses::status_rows;
use crate::settings::issues::{IssuesPane, IssuesShot};

fn issues_team(estimation: &str, opened: Option<&str>, merged_automation: Option<bool>) -> domain::rows::Team {
    serde_json::from_value(serde_json::json!({
        "id": "team-acme", "name": "Acme", "slug": "acme", "merge_mode": "auto",
        "estimation_type": estimation, "pr_opened_status_id": opened, "pr_merged_automation": merged_automation,
    }))
    .expect("a team")
}

fn issues_shot(team: domain::rows::Team) -> IssuesShot {
    IssuesShot { team, owner: true, statuses: status_rows(), open: None }
}

fn issues(name: &str, shot: IssuesShot) {
    shoot_pane(name, IssuesPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_issues_owner() {
    issues("issues-owner", issues_shot(issues_team("none", None, None)));
}

#[test]
#[ignore]
fn settings_issues_owner_set() {
    issues("issues-owner-set", issues_shot(issues_team("tshirt", Some("s-blocked"), Some(false))));
}

#[test]
#[ignore]
fn settings_issues_member() {
    issues("issues-member", IssuesShot { owner: false, ..issues_shot(issues_team("fibonacci", None, None)) });
}

#[test]
#[ignore]
fn settings_issues_no_statuses() {
    issues("issues-no-statuses", IssuesShot { statuses: Vec::new(), ..issues_shot(issues_team("exponential", None, None)) });
}

#[test]
#[ignore]
fn settings_issues_opened_open() {
    issues("issues-opened-open", IssuesShot { open: Some(0), ..issues_shot(issues_team("none", None, None)) });
}

#[test]
#[ignore]
fn settings_issues_merged_open() {
    issues("issues-merged-open", IssuesShot { open: Some(1), ..issues_shot(issues_team("linear", Some("s-blocked"), Some(false))) });
}

/// The estimate menu open: the hint after each scale (no BEFORE: today's
/// pointer menu does not open headless).
#[test]
#[ignore]
fn settings_after_issues_estimate_open() {
    issues("issues-estimate-open", IssuesShot { open: Some(2), ..issues_shot(issues_team("tshirt", None, None)) });
}
