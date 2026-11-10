//! #EXP-1629: a pending pill (`Button::loading(true)`: the spinner in the
//! icon slot) at 2× — the settings panes' Reconnect / Connect / Set key
//! while their write is in flight.
use super::*;

pub(super) struct Pills;

impl Render for Pills {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        use gpui_component::Disableable as _;
        use crate::surface::{glass_pill_button, glass_pill_button_primary, PillSize};
        gpui_component::h_flex().gap_2()
            .child(glass_pill_button("reconnect", PillSize::Sm, cx).icon(crate::icons::registry::UI_WARNING).label("Reconnect").loading(true).disabled(true))
            .child(glass_pill_button_primary("connect", PillSize::Sm).icon(crate::icons::registry::UI_SIGN_IN).label("Connect").loading(true).disabled(true))
            .child(glass_pill_button("add", PillSize::Md, cx).icon(crate::icons::registry::UI_ADD).label("Add server").loading(true))
    }
}

#[test]
#[ignore]
fn settings_pending_pill() {
    shoot_pane("pending-pill", |_, _| Pills, |_, _| {});
}
