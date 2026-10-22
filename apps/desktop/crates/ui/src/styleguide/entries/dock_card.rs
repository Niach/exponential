//! N-12 — `dock-card` (3 Special components): a PLACEHOLDER. THE Dock's
//! floating card, its pile and its sheet land on the web styleguide first
//! (`apps/styleguide/src/entries/dock-card.tsx`, `@exp/ui` dock.tsx,
//! fixtures/dock.json); the IDE entry fills in with the IDE's `dock.rs`.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::ActiveTheme as _;

pub(crate) const ID: &str = "dock-card";
pub(crate) const OWNER: &str = "N-12";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    div()
        .w(px(320.))
        .text_xs()
        .text_color(muted)
        .child("Lands with the IDE's dock.rs (N-12): the web styleguide holds the reference.")
}
