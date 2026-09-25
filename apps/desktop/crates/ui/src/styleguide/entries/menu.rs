//! EXP-1074 — `menu` (2 General components): the one floating-menu surface
//! and its row recipe, in two densities (`theme::tokens::menu::{pointer,
//! touch}`).
//!
//! The IDE's menus are gpui-component's `PopupMenu` (`.context_menu(…)` on
//! the issue rows, the row/tab overflow menus): their geometry — 26px rows,
//! 8px side padding — is the crate's, not ours; only `min_w`/`max_w` are.
//! The tokens record the pointer density every other pointer surface (web at
//! md+) draws, 36 / 8 / 8, so this entry is a LEFTOVER by decision: the IDE
//! reads well at its size and patching the vendored crate is its own issue.
//!
//! This still describes rather than paints; drawing the real `PopupMenu`
//! here is EXP-1092 (entries get the window and the app since EXP-1063).

use gpui::{div, App, Div, ParentElement as _, Window};

use theme::tokens::menu::{pointer, touch};

pub(crate) const ID: &str = "menu";
pub(crate) const OWNER: &str = "EXP-1074";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    div()
        .child("Menu — the opaque card fill under a hairline, radius 12, no blur, no shadow;")
        .child("rows on ONE geometry per density, a destructive row red with no divider above it.")
        .child(format!(
            "pointer (web ≥ md): {} row · {} pad · {} gap · {} glyph · {}–{} wide",
            pointer::ITEM_HEIGHT,
            pointer::ITEM_PADDING_X,
            pointer::ITEM_GAP,
            pointer::ICON_SIZE,
            pointer::MIN_WIDTH,
            pointer::MAX_WIDTH
        ))
        .child(format!(
            "touch (phones): {} row · {} pad · {} gap · {} glyph",
            touch::ITEM_HEIGHT,
            touch::ITEM_PADDING_X,
            touch::ITEM_GAP,
            touch::ICON_SIZE
        ))
        .child("IDE: gpui_component::menu::PopupMenu — 26px rows / 8px pad, the crate's (leftover)")
}
