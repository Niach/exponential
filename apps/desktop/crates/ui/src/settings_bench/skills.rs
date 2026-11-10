use super::*;

fn skill_row(id: &str, name: &str, description: &str, enabled_by_default: bool, builtin: bool) -> domain::rows::SkillRow {
    serde_json::from_value(serde_json::json!({"id": id, "team_id": "team-shots", "name": name, "description": description, "enabled_by_default": enabled_by_default, "builtin": builtin})).expect("a skill row")
}

fn skill_rows() -> Vec<domain::rows::SkillRow> {
    vec![
        skill_row("s1", "exponential-pr", "Open, update and land pull requests the Exponential way.", false, true),
        skill_row("s2", "review", "Run the checks, then summarise the diff for the reviewer.", true, false),
        skill_row("s3", "release-notes", "Draft the changelog entry from the merged pull requests since the last tag, grouped by area, in the house style.", false, false),
        skill_row("s4", "triage", "", false, false),
    ]
}

fn skills_list(name: &str, team: bool, ready: bool, owner: bool, rows: Vec<domain::rows::SkillRow>) {
    shoot_pane(name, SkillsPane::for_shots, move |pane, cx| pane.show(SkillsListShot { team, ready, owner, rows }, cx));
}

#[test]
#[ignore]
fn settings_skills_rows() {
    skills_list("skills-rows", true, true, true, skill_rows());
}

#[test]
#[ignore]
fn settings_skills_member() {
    skills_list("skills-member", true, true, false, skill_rows());
}

#[test]
#[ignore]
fn settings_skills_empty() {
    skills_list("skills-empty", true, true, true, Vec::new());
}

#[test]
#[ignore]
fn settings_skills_loading() {
    skills_list("skills-loading", true, false, true, Vec::new());
}

#[test]
#[ignore]
fn settings_skills_no_team() {
    skills_list("skills-no-team", false, true, true, Vec::new());
}

fn skill(builtin: bool) -> api::skills::Skill {
    serde_json::from_value(serde_json::json!({"id": "s2", "teamId": "team-shots", "name": "review", "description": "Run the checks, then summarise the diff for the reviewer.", "enabledByDefault": true, "builtin": builtin})).expect("a skill")
}

const SKILL_BODY: &str = "---\nname: review\ndescription: Run the checks, then summarise the diff.\n---\n\n# Review\n\n1. Run `bun run typecheck` and the tests.\n2. Read the diff file by file.\n3. Summarise what changed and what to look at.";

fn skill_page(name: &str, skill: Option<api::skills::Skill>, owner: bool, body: Option<Result<String, ()>>, touched: bool, taken: bool) {
    shoot_pane_acted(name, move |window, cx| SkillPage::new("team-shots".into(), skill, window, cx), |_, _| {}, None, &[], move |page, window, cx| page.show(owner, body, touched, taken, window, cx));
}

#[test]
#[ignore]
fn settings_skill_page() {
    skill_page("skill-page", Some(skill(false)), true, Some(Ok(SKILL_BODY.into())), false, false);
}

/// #EXP-1768: a 30k-char body (no hard break) grows the body to its 40
/// rows, as the gpui pane did, instead of scrolling at 14.
#[test]
#[ignore]
fn settings_skill_long_body() {
    skill_page("skill-long-body", Some(skill(false)), true, Some(Ok("word ".repeat(6_000))), false, false);
}

#[test]
#[ignore]
fn settings_skill_loading() {
    skill_page("skill-loading", Some(skill(false)), true, None, false, false);
}

#[test]
#[ignore]
fn settings_skill_failed() {
    skill_page("skill-failed", Some(skill(false)), true, Some(Err(())), false, false);
}

#[test]
#[ignore]
fn settings_skill_builtin() {
    skill_page("skill-builtin", Some(skill(true)), true, Some(Ok(SKILL_BODY.into())), false, false);
}

#[test]
#[ignore]
fn settings_skill_member() {
    skill_page("skill-member", Some(skill(false)), false, Some(Ok(SKILL_BODY.into())), false, false);
}

#[test]
#[ignore]
fn settings_skill_new() {
    skill_page("skill-new", None, true, Some(Ok(String::new())), false, false);
}

#[test]
#[ignore]
fn settings_skill_new_touched() {
    skill_page("skill-new-touched", None, true, Some(Ok(String::new())), true, false);
}

#[test]
#[ignore]
fn settings_skill_name_taken() {
    skill_page("skill-name-taken", Some(skill(false)), true, Some(Ok(SKILL_BODY.into())), false, true);
}
