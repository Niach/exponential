//! Thread 58cb32df (#EXP-1373): `phone-tab-bar` (Special components): THE
//! phone bottom bar (`fixtures/chats.json` `shell.tabs`). The IDE has no
//! phone shell, so this twin paints the web entry's specimen statically in
//! the same look: the glass tab capsule (the FAB chrome: the card fill and
//! hairline, fully round) with Board · Devices · Chats · Team in order, each
//! an icon over its label, the selected one on the glass-active pill; the
//! Team tab = the active team's avatar + name; a dot on Chats. Beside it the
//! two-armed new chat / new issue capsule at the same 52px height. Two
//! states: Chats selected with its amber dot, Team selected.

use gpui::{div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::CHATS;

use crate::icons::registry;

pub(crate) const ID: &str = "phone-tab-bar";
pub(crate) const OWNER: &str = "EXP-1373";

/// The bar's height (the two-armed capsule's).
const BAR_H: f32 = 52.;
/// The specimen team.
const TEAM: &str = "Acme";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Board,
    Devices,
    Chats,
    Team,
}

fn tab(tab: Tab, selected: bool, dot: bool, cx: &App) -> AnyElement {
    let tabs = &CHATS.copy.tabs;
    let color = if selected { cx.theme().foreground } else { cx.theme().muted_foreground };
    let (label, glyph): (String, AnyElement) = match tab {
        Tab::Board => (tabs.board.clone(), Icon::new(registry::NAV_ISSUES).with_size(px(20.)).text_color(color).into_any_element()),
        Tab::Devices => (tabs.devices.clone(), Icon::new(registry::NAV_DEVICES).with_size(px(20.)).text_color(color).into_any_element()),
        Tab::Chats => (tabs.chats.clone(), Icon::new(registry::NAV_THREADS).with_size(px(20.)).text_color(color).into_any_element()),
        Tab::Team => (TEAM.to_string(), crate::user_avatar::team_avatar(TEAM, 20.)),
    };
    let glyph = div()
        .relative()
        .child(glyph)
        .when_dot(dot);
    let mut cell = v_flex()
        .h_full()
        .px_3()
        .items_center()
        .justify_center()
        .gap_0p5()
        .rounded_full()
        .child(glyph)
        .child(div().text_2xs_px().text_color(color).child(SharedString::from(label)));
    if selected {
        cell = cell.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla());
    }
    cell.into_any_element()
}

trait PhoneExt {
    fn when_dot(self, dot: bool) -> Self;
    fn text_2xs_px(self) -> Self;
}

impl PhoneExt for Div {
    fn when_dot(self, dot: bool) -> Self {
        if !dot {
            return self;
        }
        self.child(
            div()
                .absolute()
                .top(px(-2.))
                .right(px(-4.))
                .size(px(8.))
                .rounded_full()
                .bg(theme::tokens::YELLOW.to_hsla()),
        )
    }

    fn text_2xs_px(self) -> Self {
        self.text_size(px(10.))
    }
}

fn capsule() -> Div {
    h_flex()
        .h(px(BAR_H))
        .p_1()
        .rounded_full()
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
}

fn bar(selected: Tab, cx: &App) -> AnyElement {
    let tabs = capsule().children(
        [Tab::Board, Tab::Devices, Tab::Chats, Tab::Team]
            .into_iter()
            .map(|t| tab(t, t == selected, t == Tab::Chats, cx)),
    );
    let arm = |icon| {
        div()
            .size(px(BAR_H - 10.))
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(icon).with_size(px(20.)).text_color(cx.theme().foreground))
    };
    let fab = capsule()
        .px_1()
        .child(arm(registry::NAV_THREADS))
        .child(div().w(px(1.)).h(px(22.)).bg(theme::tokens::glass::STROKE_CARD.to_hsla()))
        .child(arm(registry::UI_ADD));
    h_flex().gap_2().items_center().child(tabs).child(fab).into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    v_flex()
        .gap_6()
        .p_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(bar(Tab::Chats, cx))
        .child(bar(Tab::Team, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tabs are the fixture's, in order.
    #[test]
    fn the_tabs_follow_the_shell() {
        assert_eq!(
            CHATS.copy.tabs.board.as_str(),
            "Board",
        );
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../../packages/domain-contract/fixtures/chats.json"
        ))
        .unwrap();
        assert_eq!(fixture["shell"]["tabs"], serde_json::json!(["board", "devices", "chats", "team"]));
    }
}
