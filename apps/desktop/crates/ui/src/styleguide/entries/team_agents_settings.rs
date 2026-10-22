//! EXP-1269/EXP-1294 — `team-agents-settings` (3 Special components):
//! Settings → Agents, ONE team agent's page (`fixtures/agent-settings.json`),
//! laid out EXACTLY like the board settings page: glass group 1 = the
//! identity row (the board form's icon picker + colour picker + the name
//! typed straight into the row) over the Device row; group 2 = the Account
//! row over the Schedule row (the first trigger's summary through
//! `domain::agent_settings::trigger_summary`, else "Waits for messages");
//! then the Instructions card with its label under it, and the danger
//! "Delete agent" button. Every label and placeholder is the fixture's.
//!
//! The page itself is bound to a synced agent root and the store, so the
//! entry owns a small demo view holding the same entities over static rows;
//! nothing it draws is a copy of a recipe.

use gpui::{
    div, px, AnyElement, App, AppContext as _, Context, Div, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{InputState, Textarea, TextareaState};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::agent_settings::{trigger_summary, AGENT_SETTINGS};

use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::surface;

pub(crate) const ID: &str = "team-agents-settings";
pub(crate) const OWNER: &str = "EXP-1293";

/// The demo agent: the draft's Triage.
const NAME: &str = "Triage";
const ICON: &str = "bug";
const COLOR: &str = "#a78bfa";
const INSTRUCTIONS: &str =
    "Look after the boards: merge duplicates, add labels and priorities, start a session for anything that needs code.";

/// The fixture's label of field `key`.
fn label(key: &str) -> String {
    AGENT_SETTINGS
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.label.clone())
        .unwrap_or_default()
}

/// The Schedule row's value: the first trigger's summary, else the
/// fixture's "no schedule" copy.
pub(crate) fn schedule_value(triggers: &[serde_json::Value]) -> String {
    triggers
        .first()
        .map(trigger_summary)
        .unwrap_or_else(|| AGENT_SETTINGS.copy.no_schedule.clone())
}

/// `fixtures/agent-settings.json`: its first `summaryCases` row is the demo
/// agent's one trigger.
const AGENT_SETTINGS_JSON: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/agent-settings.json");

/// The fixture's first summary case: (trigger, expected Triggers-row text).
fn first_summary_case() -> (serde_json::Value, String) {
    let fixture: serde_json::Value = serde_json::from_str(AGENT_SETTINGS_JSON).expect("agent-settings.json parses");
    let case = &fixture["summaryCases"][0];
    (case["trigger"].clone(), case["expected"].as_str().unwrap_or_default().to_string())
}

/// The demo agent's one trigger: the fixture's first schedule (every day at
/// 09:00), read through the schedule picker's summary.
fn triggers() -> Vec<serde_json::Value> {
    vec![first_summary_case().0]
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-team-agents-settings", cx, AgentSettingsDemo::new);
    div().w(px(640.)).child(demo)
}

/// The page's state, minus the agent row and the store behind it.
struct AgentSettingsDemo {
    name: Entity<InputState>,
    instructions: Entity<TextareaState>,
}

impl AgentSettingsDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| {
            let mut state =
                InputState::new(window, cx).placeholder(AGENT_SETTINGS.copy.name_placeholder.clone());
            state.set_value(NAME, window, cx);
            state
        });
        let instructions = cx.new(|cx| {
            let mut state = crate::controls::web_textarea(4, 12, window, cx)
                .placeholder(AGENT_SETTINGS.copy.instructions_placeholder.clone());
            state.set_value(INSTRUCTIONS, window, cx);
            state
        });
        Self { name, instructions }
    }
}

/// A grouped row's trailing value: the board page's trigger recipe (a ghost
/// button, an optional glyph, the value, chevron-right).
fn row_value(id: &'static str, icon: Option<Icon>, value: impl Into<SharedString>, cx: &App) -> Button {
    let mut button = Button::new(id)
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_0()
        .py_0()
        .text_color(cx.theme().foreground.opacity(0.7));
    if let Some(icon) = icon {
        button = button.icon(icon);
    }
    button
        .child(surface::picker_value_label(value.into()))
        .child(surface::picker_row_chevron(cx))
}

impl Render for AgentSettingsDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let icon_picker = crate::board_form::icon_picker(
            "sg-agent-settings",
            registry::PICKABLE_ICONS,
            Some(ICON),
            Some(COLOR),
            false,
            |_, _, _| {},
            cx,
        );
        let color_picker = crate::board_form::color_picker("sg-agent-settings", COLOR, |_, _, _| {}, cx);
        let identity = surface::glass_row_shell()
            .gap_2()
            .child(icon_picker)
            .child(color_picker)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(surface::glass_row_input(glass_input(&self.name, window, cx)).text_left()),
            );
        let devices = vec![crate::picker::device_picker::DevicePickerDevice {
            id: "sg-device".to_string(),
            name: "Acme build server".to_string(),
            icon: Some("server".to_string()),
            server: true,
            description: None,
            disabled: false,
        }];
        let device: AnyElement = crate::picker::device_picker::device_picker(
            &devices,
            Some("sg-device".to_string()),
            row_value(
                "sg-agent-settings-device",
                Some(Icon::new(crate::icons::device_icon(Some("server"), true)).xsmall()),
                "Acme build server",
                cx,
            )
            .into_any_element(),
            super::picker::inert(),
        )
        .id("sg-agent-settings-device-picker")
        .render(window, cx);
        let accounts = vec![coding::AccountOption {
            id: "sg-account".to_string(),
            agent: coding::CodingAgent::Claude,
            email: "demo@exponential.at".to_string(),
            is_last_used: true,
            health: coding::Health::Ok,
            limits: None,
        }];
        let account: AnyElement = crate::picker::account_picker::account_picker(
            &accounts,
            Some("sg-account".to_string()),
            row_value(
                "sg-agent-settings-account",
                Some(crate::coding_selects::agent_mark(coding::CodingAgent::Claude).xsmall()),
                "demo@exponential.at",
                cx,
            )
            .into_any_element(),
            super::picker::inert(),
        )
        .id("sg-agent-settings-account-picker")
        .render(window, cx);
        let schedule =
            row_value("sg-agent-settings-schedule", None, schedule_value(&triggers()), cx).into_any_element();
        let instructions = v_flex()
            .gap_1()
            .child(
                surface::glass_group().child(
                    Textarea::new(&self.instructions)
                        .appearance(false)
                        .w_full()
                        .px_4()
                        .py_3(),
                ),
            )
            .child(div().px_1().text_xs().text_color(muted).child(label("instructions")));
        v_flex()
            .id(SharedString::from(AGENT_SETTINGS.test_ids.page.clone()))
            .gap_4()
            .child(surface::glass_group_rows(vec![
                identity,
                surface::glass_picker_row(label("device"), None, device, cx),
            ]))
            .child(surface::glass_group_rows(vec![
                surface::glass_picker_row(label("account"), None, account, cx),
                surface::glass_picker_row(label("schedule"), None, schedule, cx),
            ]))
            .child(instructions)
            .child(
                h_flex().child(
                    Button::new("sg-agent-settings-delete")
                        .danger()
                        .cursor_pointer()
                        .web_sm()
                        .label(label("delete")),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows read the fixture's words: the schedule's summary, and the
    /// empty schedule's copy.
    #[test]
    fn the_demo_reads_the_fixtures_words() {
        assert_eq!(schedule_value(&triggers()), first_summary_case().1);
        assert_eq!(schedule_value(&triggers()), "Every day at 09:00");
        assert_eq!(schedule_value(&[]), "Waits for messages");
        assert_eq!(label("delete"), "Delete agent");
        assert_eq!(label("instructions"), "Instructions");
    }
}
