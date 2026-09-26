//! EXP-1074 — `menu` (2 General components): the one floating-menu surface
//! and its row recipe, in two densities (`theme::tokens::menu::{pointer,
//! touch}`).
//!
//! The IDE's menus are gpui-component's `PopupMenu` (`.context_menu(…)` on
//! the issue rows, the row/tab overflow menus): their geometry — 26px rows,
//! 8px side padding — is the crate's, not ours; only `min_w`/`max_w` are.
//! The tokens record the pointer density every other pointer surface (web at
//! md+) draws, 36 / 8 / 8, so the IDE's row height is a LEFTOVER by decision:
//! the IDE reads well at its size and patching the vendored crate is its own
//! issue.
//!
//! EXP-1092: the demo is a REAL `PopupMenu`, built once and kept alive by
//! `window.use_keyed_state` (a menu is an entity), drawn open in place: the
//! header label, plain rows with their glyphs, a checked row, a submenu
//! trigger, a disabled row and the red destructive row
//! (`controls::danger_menu_item`) with no divider above it (EXP-697).

use gpui::{div, App, Context, Div, Entity, IntoElement, ParentElement as _, Render, Window};
use gpui_component::{
    menu::{PopupMenu, PopupMenuItem},
    Icon, Side,
};

use crate::icons::{registry, ExpIcon};

pub(crate) const ID: &str = "menu";
pub(crate) const OWNER: &str = "EXP-1074";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-menu", cx, MenuDemo::new);
    div().child(demo)
}

/// Holds the menu entity so it survives frames; draws it as-is.
struct MenuDemo {
    menu: Entity<PopupMenu>,
}

impl MenuDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let menu = PopupMenu::build(window, cx, |menu, window, cx| {
            menu.check_side(Side::Right)
                .label("EXP-42")
                .item(PopupMenuItem::new("Open issue").icon(Icon::from(ExpIcon::Pencil)))
                .item(PopupMenuItem::new("Copy issue ID").icon(Icon::from(ExpIcon::Copy)))
                .item(
                    PopupMenuItem::new("Show sub-issues")
                        .icon(Icon::new(registry::UI_ASSIGNEE))
                        .checked(true),
                )
                .submenu_with_icon(
                    Some(Icon::from(ExpIcon::Tag)),
                    "Labels",
                    window,
                    cx,
                    |menu, _, _| {
                        menu.check_side(Side::Right)
                            .item(PopupMenuItem::new("Bug").checked(true))
                            .item(PopupMenuItem::new("Feature"))
                    },
                )
                .item(
                    PopupMenuItem::new("Move to board")
                        .icon(Icon::from(ExpIcon::SquareKanban))
                        .disabled(true),
                )
                .item(crate::controls::danger_menu_item(
                    "Delete issue",
                    Icon::new(registry::UI_DELETE),
                    cx,
                ))
        });
        Self { menu }
    }
}

impl Render for MenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.menu.clone()
    }
}
