//! EXP-1273 — `computer-use-banner` (3 Special components): the REAL bar
//! the banner window shows while a session drives the screen
//! (`computer_indicator::banner_bar`, the bulk bar's capsule, off
//! `computer-use-banner.json`), with a session label, over a strip of page.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use crate::computer_indicator;

pub(crate) const ID: &str = "computer-use-banner";
pub(crate) const OWNER: &str = "EXP-1273";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    div()
        .w(px(560.))
        .h(px(72.))
        .flex()
        .items_center()
        .justify_center()
        .child(computer_indicator::banner_bar(Some("EXP-1273".into()), cx))
}
