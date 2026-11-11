use super::*;

fn prompts_rows() -> Vec<api::prompts::Prompt> {
    serde_json::from_value(serde_json::json!([
        {"id": "p1", "name": "Release train", "icon": "rocket"},
        {"id": "p2", "name": "Triage the inbox and file every bug against its board with a reproduction", "icon": "bug"},
        {"id": "p3", "name": "Weekly digest"},
        {"id": "p4", "name": "Unknown glyph", "icon": "not-an-icon"},
    ]))
    .expect("prompt rows")
}

fn prompts(name: &str, shot: PromptsShot) {
    shoot_pane(name, PromptsPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_prompts_owner() {
    prompts("prompts-owner", PromptsShot { team: Some("t".into()), owner: true, prompts: prompts_rows(), ready: true });
}

#[test]
#[ignore]
fn settings_prompts_member() {
    prompts("prompts-member", PromptsShot { team: Some("t".into()), owner: false, prompts: prompts_rows(), ready: true });
}

#[test]
#[ignore]
fn settings_prompts_empty() {
    prompts("prompts-empty", PromptsShot { team: Some("t".into()), owner: true, prompts: Vec::new(), ready: true });
}

#[test]
#[ignore]
fn settings_prompts_loading() {
    prompts("prompts-loading", PromptsShot { team: Some("t".into()), owner: false, prompts: Vec::new(), ready: false });
}

#[test]
#[ignore]
fn settings_prompts_no_team() {
    prompts("prompts-no-team", PromptsShot { team: None, owner: true, prompts: Vec::new(), ready: true });
}

const PROMPT_BODY: &str = "Review the open PRs on this board.\n\n- Summarise each diff in two lines.\n- Flag anything that touches `apps/web/src/db`.\n";

fn prompt_row() -> api::prompts::Prompt {
    serde_json::from_value(serde_json::json!({"id": "p1", "teamId": "team-shots", "name": "Review train", "icon": "rocket"})).expect("a prompt")
}

fn prompt_page(name: &str, prompt: Option<api::prompts::Prompt>, shot: PromptPageShot) {
    shoot_pane_acted(name, move |window, cx| PromptPage::new("team-shots".into(), prompt, window, cx), |_, _| {}, None, &[], move |page, window, cx| page.show(shot, window, cx));
}

fn page_shot(owner: bool, body: Option<Result<String, ()>>) -> PromptPageShot {
    PromptPageShot { owner, body, name: None, touched: false, name_error: None }
}

#[test]
#[ignore]
fn settings_prompt_page() {
    prompt_page("prompt-page", Some(prompt_row()), page_shot(true, Some(Ok(PROMPT_BODY.into()))));
}

#[test]
#[ignore]
fn settings_prompt_dirty() {
    prompt_page("prompt-dirty", Some(prompt_row()), PromptPageShot { name: Some("Review train (weekly)".into()), ..page_shot(true, Some(Ok(PROMPT_BODY.into()))) });
}

#[test]
#[ignore]
fn settings_prompt_loading() {
    prompt_page("prompt-loading", Some(prompt_row()), page_shot(true, None));
}

#[test]
#[ignore]
fn settings_prompt_failed() {
    prompt_page("prompt-failed", Some(prompt_row()), page_shot(true, Some(Err(()))));
}

#[test]
#[ignore]
fn settings_prompt_member() {
    prompt_page("prompt-member", Some(prompt_row()), page_shot(false, Some(Ok(PROMPT_BODY.into()))));
}

#[test]
#[ignore]
fn settings_prompt_new_touched() {
    prompt_page("prompt-new-touched", None, PromptPageShot { touched: true, ..page_shot(true, Some(Ok(String::new()))) });
}

#[test]
#[ignore]
fn settings_prompt_taken() {
    prompt_page("prompt-taken", Some(prompt_row()), PromptPageShot { name: Some("Release train".into()), name_error: Some("A prompt with this name already exists".into()), ..page_shot(true, Some(Ok(PROMPT_BODY.into()))) });
}

#[test]
#[ignore]
fn settings_prompt_too_long() {
    let body = "x ".repeat(crate::settings::prompts::PROMPT_BODY_MAX_CHARS / 2 + 1);
    prompt_page("prompt-too-long", Some(prompt_row()), page_shot(true, Some(Ok(body))));
}
