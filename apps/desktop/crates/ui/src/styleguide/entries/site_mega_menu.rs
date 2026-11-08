//! EXP-1740 — `site-mega-menu` (3 Special components): exponential.at's the header's Product and SDK mega menus.
//! The site is a web page, so the IDE has no form to draw; the entry says
//! so and the web styleguide draws the site's own markup and stylesheet.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::ActiveTheme as _;

pub(crate) const ID: &str = "site-mega-menu";
pub(crate) const OWNER: &str = "EXP-1740";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    div()
        .max_w(px(420.))
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child("Web only: exponential.at's the header's Product and SDK mega menus (apps/site). See the web styleguide.")
}
