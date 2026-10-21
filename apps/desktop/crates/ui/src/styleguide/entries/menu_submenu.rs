//! SLOP-28 (EXP-1259 D5): `menu-submenu` (General components): a menu row
//! with a › opens its submenu to the side (hover or Right; Left at the field's
//! start or Escape hands the keys back). The composer's Implement issue ›
//! and Run action › bodies are THE search list — ONE row height, at most
//! eight rows, an action row = its icon and its NAME. The specimen draws the
//! "+" menu's first group (`chat_screen::composer_menu_rows`, the real
//! `controls::menu_row`s) beside the Run action submenu's surface; the live
//! one is `search_list::MenuSearchList` inside a `PopupMenu`.

use gpui::{
    div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, ScrollHandle,
    SharedString, Styled as _, Window,
};
use gpui_component::{input::InputState, ActiveTheme as _};

use crate::chat_screen::{ComposerMenuRow, MENU_ADD_FILE, MENU_IMPLEMENT_ISSUE};
use crate::picker::search_list::{action_row_body, row, surface, SearchList};

pub(crate) const ID: &str = "menu-submenu";
pub(crate) const OWNER: &str = "SLOP-28";

/// The demo actions: (name, curated icon).
const ACTIONS: [(&str, &str); 5] = [
    ("Fix merge conflicts", "git-branch"),
    ("Create action", "sparkles"),
    ("Release all", "rocket"),
    ("Refresh docs and marketing", "megaphone"),
    ("Tidy up", "wrench"),
];

fn plus_rows(cx: &App) -> Div {
    let rows = [
        (ComposerMenuRow::ImplementIssue, MENU_IMPLEMENT_ISSUE, true),
        (ComposerMenuRow::AddFile, MENU_ADD_FILE, false),
    ];
    let mut column = surface(cx).w(px(260.)).p_1();
    for (index, (row_kind, label, chevron)) in rows.into_iter().enumerate() {
        let trailing = chevron.then(|| crate::controls::menu_row_value(None, true, cx));
        let mut item = crate::controls::menu_row(row_kind.icon(), label, trailing, false, cx)
            .px_2()
            .rounded(cx.theme().radius);
        // The Implement issue row is the open one.
        if index == 0 {
            item = item.bg(cx.theme().accent);
        }
        column = column.child(item);
    }
    column
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let input = window.use_keyed_state("styleguide-menu-submenu-input", cx, |window, cx| {
        InputState::new(window, cx).placeholder("Search actions")
    });
    let scroll = window
        .use_keyed_state("styleguide-menu-submenu-scroll", cx, |_, _| ScrollHandle::new())
        .read(cx)
        .clone();
    let rows: Vec<AnyElement> = ACTIONS
        .iter()
        .enumerate()
        .map(|(index, (name, icon))| {
            row(SharedString::from(format!("styleguide-menu-submenu-{index}")), index == 0, cx)
                .child(action_row_body(
                    crate::icons::action_icon(Some(icon)),
                    *name,
                    cx,
                ))
                .into_any_element()
        })
        .collect();
    let count = rows.len();
    let list = SearchList::new("styleguide-menu-submenu-rows", &input, &scroll)
        .rows(rows, count)
        .count(None)
        .render(cx);
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_1()
        .pt_2()
        .child(plus_rows(cx))
        .child(
            surface(cx)
                .mt(px(36.))
                .w(px(crate::picker::search_list::MENU_LIST_WIDTH))
                .child(list),
        )
}
