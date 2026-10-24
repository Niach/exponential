//! #EXP-1384: `phone-board-menu` (Special components): THE phone board menu
//! (`fixtures/chats.json` `shell.boardMenu`). The IDE has no phone shell, so
//! this twin paints the web entry's specimen statically: the board view's
//! strip (the tab capsule + the new chat / new issue capsule) MORPHED into
//! one glass menu panel, shown OPEN in the strip's frame (same left, right
//! and bottom edges, `radius` corners) over a dimmed board page: the team's
//! boards in sidebar order, `rowHeight` each, the board glyph + name, the
//! active one on the glass-active pill with `ui-check`; then the muted New
//! board row (`ui-add`).

use gpui::{div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::CHATS;

use super::phone_tab_bar::{board_glyph, strip, Tab, BOARDS};
use crate::icons::registry;

pub(crate) const ID: &str = "phone-board-menu";
pub(crate) const OWNER: &str = "EXP-1384";

/// The phone frame (the web island's 390 × 340).
const PHONE_W: f32 = 390.;
const PHONE_H: f32 = 340.;

fn row(glyph: AnyElement, name: &str, active: bool, muted: bool, cx: &App) -> AnyElement {
    let color = if muted { cx.theme().muted_foreground } else { cx.theme().foreground };
    let mut row = h_flex()
        .w_full()
        .h(px(CHATS.shell.board_menu.row_height))
        .px_4()
        .gap_3()
        .items_center()
        .rounded_full()
        .child(div().size(px(20.)).flex().items_center().justify_center().child(glyph))
        .child(div().flex_1().text_size(px(15.)).text_color(color).child(SharedString::from(name.to_string())));
    if active {
        row = row
            .bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
            .child(Icon::new(registry::UI_CHECK).with_size(px(16.)).text_color(color));
    }
    row.into_any_element()
}

fn menu(cx: &App) -> AnyElement {
    let shell = &CHATS.shell;
    let rows = BOARDS
        .iter()
        .enumerate()
        .map(|(index, (name, icon, color))| row(board_glyph(icon, color, 20.), name, index == 0, false, cx));
    let new_board = row(
        Icon::new(registry::UI_ADD).with_size(px(16.)).text_color(cx.theme().muted_foreground).into_any_element(),
        &CHATS.copy.board_menu.new_board,
        false,
        true,
        cx,
    );
    v_flex()
        .w_full()
        .p(px(shell.bar.padding))
        .rounded(px(shell.board_menu.radius))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .children(rows)
        .child(new_board)
        .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let (board, _, _) = BOARDS[0];
    let page = v_flex()
        .p_4()
        .gap_3()
        .opacity(0.4)
        .child(
            div()
                .text_3xl()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(cx.theme().foreground)
                .child(SharedString::from(board)),
        )
        .children((0..3).map(|_| div().h(px(44.)).rounded_xl().bg(theme::tokens::glass::FILL_ROW.to_hsla())));
    // The strip sits under the menu (faded out in the morph), the menu in its
    // frame, both on the page's bottom edge.
    let bottom = div()
        .absolute()
        .left(px(8.))
        .right(px(8.))
        .bottom(px(16.))
        .child(div().opacity(0.).child(strip(Tab::Board, false, cx)))
        .child(div().absolute().left_0().right_0().bottom_0().child(menu(cx)));
    div()
        .relative()
        .w(px(PHONE_W))
        .h(px(PHONE_H))
        .overflow_hidden()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(page)
        .child(bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The menu's geometry and copy are the fixture's.
    #[test]
    fn the_menu_follows_the_shell() {
        assert_eq!(CHATS.shell.board_menu.row_height, 48.);
        assert_eq!(CHATS.copy.board_menu.new_board, "New board");
        assert!(BOARDS.len() + 1 <= CHATS.shell.board_menu.max_visible_rows);
    }
}
