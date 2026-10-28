//! S-3 — `composer-agent-row` (Inputs & pickers; the web entry's twin): THE
//! agent row over the New-chat composer card, the REAL
//! [`crate::chat_list::composer_agent_row`] over
//! `domain::composer_agent_row::composer_agent_row`, fed
//! `fixtures/composer-home.json` `agentRow.cases[0]` (the viewer's Assistant
//! first with the brand mark and preselected, another member's Assistant
//! out, the dotted Temporary session circle last); a tap moves the pick.
//! Under it a placeholder of the card it stands over.

use gpui::{div, px, App, AppContext as _, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

use domain::composer_agent_row::composer_agent_row;
use domain::rows::AgentDefRow;

pub(crate) const ID: &str = "composer-agent-row";
pub(crate) const OWNER: &str = "S-3";

/// The fixture's first case: its agents (icon / colour null) and viewer.
pub(crate) fn sample() -> (Vec<AgentDefRow>, String) {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../../packages/domain-contract/fixtures/composer-home.json"
    ))
    .expect("composer-home.json parses");
    let case = &fixture["agentRow"]["cases"][0];
    let agents = case["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|agent| {
            serde_json::from_value(serde_json::json!({
                "id": agent["id"], "role": agent["role"], "owner_id": agent["ownerId"],
                "name": agent["name"], "sort_order": agent["sortOrder"],
            }))
            .ok()
        })
        .collect();
    (agents, case["me"].as_str().unwrap_or_default().to_string())
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let picked = window.use_keyed_state("styleguide-composer-agent-row-pick", cx, |_, _| None::<String>);
    let (agents, me) = sample();
    let state = composer_agent_row(&agents, Some(me.as_str()), picked.read(cx).as_deref());
    let on_pick = {
        let picked = picked.clone();
        std::rc::Rc::new(move |pick: String, _: &mut Window, cx: &mut App| {
            picked.update(cx, |picked, cx| {
                *picked = Some(pick);
                cx.notify();
            });
        })
    };
    v_flex()
        .w(px(640.))
        .gap_0()
        .child(crate::chat_list::composer_agent_row(&state, on_pick, cx))
        .child(
            div()
                .h(px(96.))
                .w_full()
                .rounded(px(16.))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(theme::tokens::glass::FILL_CARD.to_hsla()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen replays the fixture's first case: the Assistant
    /// preselected, the dotted circle last.
    #[test]
    fn the_specimen_is_the_fixtures_first_case() {
        let (agents, me) = sample();
        let state = composer_agent_row(&agents, Some(me.as_str()), None);
        assert_eq!(state.options, ["g-me", "g-a", "g-b", "temporary"]);
        assert_eq!(state.label, "To Assistant");
    }
}
