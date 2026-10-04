//! EXP-1191 — `jump-to-bottom` (2 General components): the arrow-only
//! circle that returns the run transcript to its newest row — the REAL
//! `controls::jump_to_bottom_button`, at rest over a strip of panel.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use crate::controls;

pub(crate) const ID: &str = "jump-to-bottom";
pub(crate) const OWNER: &str = "EXP-1191";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    div()
        .w(px(320.))
        .h(px(96.))
        .flex()
        .items_end()
        .justify_center()
        .pb(px(12.))
        .child(controls::jump_to_bottom_button("sg-jump-to-bottom", |_, _, _| {}, cx))
}
