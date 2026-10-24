//! Thread 58cb32df (#EXP-1373) → #EXP-1384: `phone-tab-bar` (Special
//! components): THE phone bottom bar (`fixtures/chats.json` `shell.tabs` +
//! `shell.bar`). The IDE has no phone shell, so this twin paints the web
//! entry's specimen statically in the same look: the glass tab capsule (the
//! FAB chrome: the card fill and hairline, fully round) with Board · Chats ·
//! Apps · Team in order, each an icon over its label and as wide as its
//! label (+ `tabPaddingX` each side, at least `tabMinWidth`), the selected
//! one on the glass-active pill. The Board tab = the active board's glyph +
//! name (on the board view the chevrons caret after the label); the Team tab
//! = the active team's avatar + name; a dot on Chats. Beside it, `gap` away,
//! the two-armed new chat (`action-chat`) / new issue (`nav-create-issue`)
//! capsule at the same height. Four states (the web specimen's): Board
//! selected on the board view, Chats and Team selected, then the drag LENS
//! mid-drag (the lens scaled to `lensScale` between Chats and Apps, Apps the
//! candidate in the foreground, the selected tab's pill lifted off).

use gpui::{div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::CHATS;

use crate::icons::registry;

pub(crate) const ID: &str = "phone-tab-bar";
pub(crate) const OWNER: &str = "EXP-1384";

/// The specimen team.
pub(crate) const TEAM: &str = "Acme";
/// The specimen team's boards in sidebar order (name, icon, colour), the
/// screenshot seed's; the first is the active one.
pub(crate) const BOARDS: [(&str, &str, &str); 3] = [
    ("Mobile App", "code", "#6366f1"),
    ("Launch Marketing", "square-kanban", "#f59e0b"),
    ("Product Feedback", "megaphone", "#22c55e"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Board,
    Chats,
    Apps,
    Team,
}

const TABS: [Tab; 4] = [Tab::Board, Tab::Chats, Tab::Apps, Tab::Team];

/// How a tab paints: at rest, selected (the pill), or under the drag lens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Rest,
    Selected,
    Candidate,
}

/// A board's glyph (web `BoardGlyph`): its icon tinted with its colour.
pub(crate) fn board_glyph(icon: &str, color: &str, size: f32) -> AnyElement {
    crate::icons::app_icon(Some(icon), Some(color)).with_size(px(size)).into_any_element()
}

fn tab(tab: Tab, look: Look, caret: bool, cx: &App) -> AnyElement {
    let bar = &CHATS.shell.bar;
    let tabs = &CHATS.copy.tabs;
    let color = if look == Look::Rest { cx.theme().muted_foreground } else { cx.theme().foreground };
    let concept = |icon| Icon::new(icon).with_size(px(20.)).text_color(color).into_any_element();
    let (board, board_icon, board_color) = BOARDS[0];
    let (label, glyph): (String, AnyElement) = match tab {
        Tab::Board => (board.to_string(), board_glyph(board_icon, board_color, 20.)),
        Tab::Chats => (tabs.chats.clone(), concept(registry::NAV_THREADS)),
        Tab::Apps => (tabs.apps.clone(), concept(registry::NAV_APPS)),
        Tab::Team => (TEAM.to_string(), crate::user_avatar::team_avatar(TEAM, 20.)),
    };
    let glyph = div().relative().child(glyph).when_dot(tab == Tab::Chats);
    let mut line = h_flex()
        .items_center()
        .gap(px(2.))
        .child(div().text_size(px(10.)).text_color(color).child(SharedString::from(label)));
    if caret {
        line = line.child(Icon::new(registry::NAV_TEAM_SWITCHER).with_size(px(CHATS.shell.board_tab.caret_size)).text_color(color));
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
    cell.child(glyph).child(line).into_any_element()
}

trait PhoneExt {
    fn when_dot(self, dot: bool) -> Self;
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

/// The tab capsule: `selected` on its pill; `dragging` = the lens over Apps
/// (the selected tab's pill lifted off).
pub(crate) fn tabs(selected: Tab, dragging: bool, cx: &App) -> AnyElement {
    capsule()
        .children(TABS.into_iter().map(|t| {
            let look = match (dragging, t == selected, t == Tab::Apps) {
                (true, _, true) => Look::Candidate,
                (true, _, false) => Look::Rest,
                (false, true, _) => Look::Selected,
                (false, false, _) => Look::Rest,
            };
            tab(t, look, t == Tab::Board && selected == Tab::Board, cx)
        }))
        .into_any_element()
}

/// The two-armed new chat · new issue capsule.
pub(crate) fn launcher(cx: &App) -> AnyElement {
    let arm = |icon| {
        div()
            .size(px(CHATS.shell.bar.height - 10.))
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(icon).with_size(px(20.)).text_color(cx.theme().foreground))
    };
    capsule()
        .px_1()
        .child(arm(registry::ACTION_CHAT))
        .child(div().w(px(1.)).h(px(22.)).bg(theme::tokens::glass::STROKE_CARD.to_hsla()))
        .child(arm(registry::NAV_CREATE_ISSUE))
        .into_any_element()
}

/// The whole strip: the tab capsule, `gap`, the launcher.
pub(crate) fn strip(selected: Tab, dragging: bool, cx: &App) -> AnyElement {
    h_flex()
        .gap(px(CHATS.shell.bar.gap))
        .items_center()
        .child(tabs(selected, dragging, cx))
        .child(launcher(cx))
        .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let state = |caption: &'static str, selected: Tab, dragging: bool, cx: &App| {
        v_flex()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(caption))
            .child(strip(selected, dragging, cx))
    };
    v_flex()
        .gap_8()
        .p_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(state("On the board view (the caret opens the board menu)", Tab::Board, false, cx))
        .child(state("Chats selected", Tab::Chats, false, cx))
        .child(state("Team selected", Tab::Team, false, cx))
        .child(state("Dragging: the lens between Chats and Apps", Tab::Chats, true, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tabs are the fixture's, in order; the caret is its concept.
    #[test]
    fn the_tabs_follow_the_shell() {
        assert_eq!(CHATS.shell.tabs, ["board", "chats", "apps", "team"]);
        assert_eq!(TABS.len(), CHATS.shell.tabs.len());
        assert_eq!(CHATS.shell.board_tab.caret, "nav-team-switcher");
    }
}
