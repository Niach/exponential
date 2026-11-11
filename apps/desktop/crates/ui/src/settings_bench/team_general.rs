use super::*;

fn team(icon: bool, zone: Option<&str>) -> domain::rows::Team {
    serde_json::from_value(serde_json::json!({
        "id": "team-acme", "name": "Acme", "slug": "acme",
        "icon_url": if icon { serde_json::json!("https://bench.invalid/team.png") } else { serde_json::Value::Null },
        "time_zone": zone, "merge_mode": "auto",
    }))
    .expect("a team")
}

fn team_plan(plan: &str) -> api::billing::TeamPlanOut {
    serde_json::from_value(serde_json::json!({"plan": plan, "limits": {"seats": 10.0, "storageMb": 10240.0, "widgetConfigs": null}, "usage": {"members": 4.0, "storageMb": 2355.2, "widgetConfigs": 2.0}})).expect("a plan")
}

const TEAM_PROMPT: &str = "# Rules\n\n- Keep PRs small: one concern each.\n- Run `bun run typecheck` before you push.\n- Never touch `apps/web/src/db/out` by hand.\n";

fn general_shot() -> GeneralShot {
    GeneralShot {
        team: Some(team(true, Some("Europe/Vienna"))),
        owner: true,
        picture: Some(picture()),
        name_draft: None,
        saving: false,
        prompt: PromptShot::Ready(TEAM_PROMPT.into(), None, false),
        billing: Some(team_plan("team")),
    }
}

fn general(name: &str, shot: GeneralShot) {
    shoot_pane_acted(name, GeneralPane::for_shots, |_, _| {}, Some(std::time::Duration::from_millis(1200)), &[], move |pane, window, cx| pane.show(shot, window, cx));
}

#[test]
#[ignore]
fn settings_general_owner() {
    general("general-owner", general_shot());
}

#[test]
#[ignore]
fn settings_general_plain() {
    let shot = GeneralShot {
        team: Some(team(false, None)),
        picture: None,
        name_draft: Some("Acme Labs".into()),
        prompt: PromptShot::Ready(TEAM_PROMPT.into(), Some("2020-01-01T00:00:00.000Z".into()), true),
        billing: None,
        ..general_shot()
    };
    general("general-plain", shot);
}

#[test]
#[ignore]
fn settings_general_edited() {
    let shot = GeneralShot { prompt: PromptShot::Ready(String::new(), Some("2020-01-01T00:00:00.000Z".into()), false), billing: Some(team_plan("free")), ..general_shot() };
    general("general-edited", shot);
}

#[test]
#[ignore]
fn settings_general_loading() {
    general("general-loading", GeneralShot { saving: true, name_draft: Some("Acme Labs".into()), prompt: PromptShot::Loading, ..general_shot() });
}

#[test]
#[ignore]
fn settings_general_failed() {
    general("general-failed", GeneralShot { prompt: PromptShot::Failed, billing: Some(team_plan("unlimited")), ..general_shot() });
}

#[test]
#[ignore]
fn settings_general_over_cap() {
    let text = "x".repeat(domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES + 5);
    general("general-over-cap", GeneralShot { prompt: PromptShot::Ready(text, None, false), billing: None, ..general_shot() });
}

#[test]
#[ignore]
fn settings_general_member() {
    general("general-member", GeneralShot { owner: false, ..general_shot() });
}

#[test]
#[ignore]
fn settings_general_no_team() {
    general("general-no-team", GeneralShot { team: None, ..general_shot() });
}

use crate::settings::team_general::DeleteTeamContent;

fn delete_body(name: &'static str, typed: &'static str) {
    shoot_pane(name, move |window, cx| DeleteTeamContent::for_shots("Acme", typed, window, cx), |_, _| {});
}

/// The Delete team alert's body: the type-the-name field, empty.
#[test]
#[ignore]
fn settings_general_delete_empty() {
    delete_body("general-delete-empty", "");
}

/// Partly typed: the name does not match yet.
#[test]
#[ignore]
fn settings_general_delete_partial() {
    delete_body("general-delete-partial", "Acm");
}

/// The name typed in full.
#[test]
#[ignore]
fn settings_general_delete_matching() {
    delete_body("general-delete-matching", "Acme");
}
