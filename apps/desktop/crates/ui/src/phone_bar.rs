//! THE phone bottom bar's pieces (`fixtures/chats.json` `shell.bar`): the
//! glass capsule and one tab cell. The app preview's Phone frame
//! (`app_screen`, VAPP-218) draws its screens' tabs with them; the dev-only
//! `phone-tab-bar` specimen (`crate::specimens`) paints the whole strip.

use gpui::{div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::CHATS;

use crate::icons::registry;

/// How a tab paints: at rest, selected (the pill), or under the drag lens.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Look {
    Rest,
    Selected,
    Candidate,
}

pub(crate) fn look_color(look: Look, cx: &App) -> gpui::Hsla {
    if look == Look::Rest { cx.theme().muted_foreground } else { cx.theme().foreground }
}

/// VAPP-218 (apps.json `screens`): one tab of THIS bar for an app screen in
/// the preview's Phone frame: its icon over its title, `selected` on the
/// glass-active pill.
pub(crate) fn screen_tab(icon: crate::icons::ExpIcon, label: String, selected: bool, cx: &App) -> Div {
    let look = if selected { Look::Selected } else { Look::Rest };
    let glyph = Icon::new(icon).with_size(px(20.)).text_color(look_color(look, cx)).into_any_element();
    tab_cell(glyph, label, false, look, cx)
}

/// One tab cell: the glyph (+ the caret beside it) over its label, `look` = at
/// rest, the selected pill, or under the drag lens.
pub(crate) fn tab_cell(glyph: AnyElement, label: String, caret: bool, look: Look, cx: &App) -> Div {
    let bar = &CHATS.shell.bar;
    let color = look_color(look, cx);
    let line = div().text_size(px(10.)).text_color(color).child(SharedString::from(label));
    // Out of flow, `caretGap` after the glyph's right edge and centred on it:
    // showing it never changes the tab's width.
    let mut glyph = div().relative().text_color(color).child(glyph);
    if caret {
        let board_tab = &CHATS.shell.board_tab;
        glyph = glyph.child(
            h_flex()
                .absolute()
                .top_0()
                .bottom_0()
                .left_full()
                .ml(px(board_tab.caret_gap))
                .items_center()
                .child(Icon::new(registry::NAV_TEAM_SWITCHER).with_size(px(board_tab.caret_size)).text_color(color)),
        );
    }
    let mut cell = v_flex()
        .relative()
        .h(px(bar.height - 2. * bar.padding))
        .min_w(px(bar.tab_min_width))
        .px(px(bar.tab_padding_x))
        .items_center()
        .justify_center()
        .gap_1()
        .rounded_full();
    match look {
        Look::Selected => cell = cell.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()),
        Look::Candidate => {
            // The lens: the glass-active pill scaled to `lensScale`, the
            // finger 10px left of the candidate's centre.
            let grow = (bar.drag.lens_scale - 1.) * (bar.height - 2. * bar.padding) / 2.;
            cell = cell.child(
                div()
                    .absolute()
                    .top(px(-grow))
                    .bottom(px(-grow))
                    .left(px(-10. - grow))
                    .right(px(10. - grow))
                    .rounded_full()
                    .bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()),
            );
        }
        Look::Rest => {}
    }
    cell.child(glyph).child(line)
}

/// The FAB chrome: the card fill + hairline, fully round, `bar.height` tall.
pub(crate) fn capsule() -> Div {
    h_flex()
        .h(px(CHATS.shell.bar.height))
        .p(px(CHATS.shell.bar.padding))
        .rounded_full()
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
}
