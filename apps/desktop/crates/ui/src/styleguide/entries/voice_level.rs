//! VAPP-162: `voice-level` (General components): THE voice level meter, the
//! Exponential UI VoiceWave `variant: bars` at sm, md, lg and xl (speaking
//! at level 0.7), painted by the gpui painter from the catalog specimen's
//! bars cases (`fixtures/specimens.json` `exponential-ui-voice-wave`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "voice-level";
pub(crate) const OWNER: &str = "VAPP-162";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let cases: Vec<serde_json::Value> = (6..=9)
        .map(|i| specimen_node("exponential-ui-voice-wave", &format!("specimen-voice-wave-case-{i}")).expect("a bars case"))
        .collect();
    let tree = serde_json::json!({"id": "sg-voice-level", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": "$spacing.lg", "padding": "$spacing.xl"}, "children": cases});
    div().w_full().child(styleguide_surface("sg-voice-level", tree, window, cx))
}
