//! A running app's phone screens bar (`fixtures/chats.json` `shell.bar`): the
//! glass capsule and one tab cell. The app preview's Phone frame
//! (`app_screen`, VAPP-218) draws its screens' tabs with them. The app's own
//! docked bar (#EXP-2109) is the `phone-tab-bar` specimen.

use gpui::{div, prelude::FluentBuilder as _, px, App, Div, ParentElement as _, SharedString, Styled as _};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::CHATS;

/// VAPP-218 (apps.json `screens`): one tab of THIS bar for an app screen in
/// the preview's Phone frame: its icon over its title, `selected` on the
/// glass-active pill.
pub(crate) fn screen_tab(icon: crate::icons::ExpIcon, label: String, selected: bool, cx: &App) -> Div {
    let bar = &CHATS.shell.bar;
    let color = if selected { cx.theme().foreground } else { cx.theme().muted_foreground };
    v_flex()
        .h(px(bar.height - 2. * bar.padding))
        .min_w(px(bar.tab_min_width))
        .px(px(bar.tab_padding_x))
        .items_center()
        .justify_center()
        .gap_1()
        .rounded_full()
        .when(selected, |this| this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
        .child(Icon::new(icon).with_size(px(20.)).text_color(color))
        .child(div().text_size(px(10.)).text_color(color).child(SharedString::from(label)))
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
