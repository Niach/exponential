//! EXP-1269 — `team-agents-settings` (3 Special components): a PLACEHOLDER. The draft of
//! Team settings → Team agents lives on the web styleguide
//! (`apps/styleguide/src/entries/team-agents-settings.tsx`); the IDE entry fills in when the
//! surface is built.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::ActiveTheme as _;

pub(crate) const ID: &str = "team-agents-settings";
pub(crate) const OWNER: &str = "EXP-1269";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    div()
        .w(px(320.))
        .text_xs()
        .text_color(muted)
        .child("Draft on the web styleguide (EXP-1269); nothing is built on the IDE yet.")
}
