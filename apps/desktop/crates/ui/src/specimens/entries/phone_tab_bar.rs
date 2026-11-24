//! Session 58cb32df (#EXP-1373) → #EXP-2109: `phone-tab-bar` (Special
//! components): THE phone bottom bar (`fixtures/chats.json` `shell.dock` +
//! `shell.newMenu`). The IDE has no phone shell, so this twin paints the web
//! entry's specimen statically: the docked bar (one hairline on top), five
//! equal cells: Chats (THE count badge) · Teams · the Assistant (the white
//! Exponential mark on its raised disc, no label) · New · Settings. Four
//! states: Chats, Teams and the Assistant selected, then New with its popup.

use gpui::{div, prelude::FluentBuilder as _, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::CHATS;

use crate::icons::registry;

pub(crate) const ID: &str = "phone-tab-bar";
pub(crate) const OWNER: &str = "EXP-2109";

const PHONE_W: f32 = 390.;

fn glyph(tab: &str, selected: bool) -> Icon {
    Icon::new(match tab {
        "chats" => registry::NAV_CHATS,
        "teams" => registry::UI_TEAM,
        "new" if selected => registry::UI_CLOSE,
        "new" => registry::UI_ADD,
        _ => registry::NAV_SETTINGS,
    })
}

fn label(tab: &str) -> String {
    let tabs = &CHATS.copy.tabs;
    match tab {
        "chats" => tabs.chats.clone(),
        "teams" => tabs.teams.clone(),
        "new" => tabs.new.clone(),
        _ => tabs.settings.clone(),
    }
}

fn cell(tab: &str, selected: bool, cx: &App) -> AnyElement {
    let dock = &CHATS.shell.dock;
    let cell = v_flex().flex_1().h_full().items_center().justify_center();
    if tab == "assistant" {
        let ground = cx.theme().background;
        return cell
            .child(
                div().relative().top(px(-(dock.raise + (dock.height - dock.disc) / 2.))).child(
                    div().p(px(dock.ring)).m(px(-dock.ring)).rounded_full().bg(ground).child(
                        div()
                            .size(px(dock.disc))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                            .bg(cx.theme().secondary)
                            .when(selected, |this| {
                                this.shadow(vec![gpui::BoxShadow {
                                    color: cx.theme().foreground.opacity(0.18),
                                    offset: gpui::point(px(0.), px(0.)),
                                    blur_radius: px(dock.glow),
                                    spread_radius: px(dock.ring),
                                    inset: false,
                                }])
                            })
                            .child(Icon::new(crate::icons::ExpIcon::BrandExponential).with_size(px(dock.mark)).text_color(cx.theme().foreground)),
                    ),
                ),
            )
            .into_any_element();
    }
    let color = if selected { cx.theme().foreground } else { cx.theme().muted_foreground };
    let badge = (tab == "chats").then(|| crate::surface::count_badge(2, crate::surface::BadgeTone::Attention, cx)).flatten();
    cell.gap(px(dock.gap))
        .child(
            div()
                .relative()
                .child(glyph(tab, selected).with_size(px(dock.icon)).text_color(color))
                .children(badge.map(|badge| div().absolute().top(px(-6.)).left(px(dock.icon / 2. + 4.)).child(badge))),
        )
        .child(div().text_size(px(dock.label)).text_color(color).child(label(tab)))
        .into_any_element()
}

fn bar(selected: &str, cx: &App) -> Div {
    h_flex()
        .w_full()
        .h(px(CHATS.shell.dock.height))
        .px(px(6.))
        .bg(cx.theme().popover)
        .border_t_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .children(CHATS.shell.tabs.iter().map(|tab| cell(tab, tab == selected, cx)))
}

/// The New cell's popup, ending over the fourth of the five cells.
fn new_menu(cx: &App) -> AnyElement {
    let menu = &CHATS.shell.new_menu;
    let copy = &CHATS.copy.new_menu;
    let rows = menu.items.iter().map(|item| {
        let (icon, label) = match item.as_str() {
            "issue" => (registry::NAV_CREATE_ISSUE, copy.issue.clone()),
            _ => (registry::ACTION_CHAT, copy.chat.clone()),
        };
        h_flex()
            .h(px(menu.row_height))
            .px(px(14.))
            .gap(px(14.))
            .items_center()
            .text_size(px(15.))
            .text_color(cx.theme().foreground)
            .child(Icon::new(icon).with_size(px(20.)).text_color(cx.theme().foreground))
            .child(label)
    });
    h_flex()
        .w_full()
        .justify_end()
        .pr(px(PHONE_W * 0.3 - menu.width / 2.))
        .pb(px(menu.gap))
        .child(
            v_flex()
                .w(px(menu.width))
                .p(px(6.))
                .rounded(px(menu.radius))
                .bg(cx.theme().popover)
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .children(rows),
        )
        .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let state = |caption: &'static str, selected: &str, cx: &App| {
        v_flex()
            .gap(px(24.))
            .child(div().px_4().text_sm().text_color(muted).child(caption))
            .when(selected == "new", |this| this.child(v_flex().child(new_menu(cx)).child(bar(selected, cx))))
            .when(selected != "new", |this| this.child(bar(selected, cx)))
    };
    v_flex()
        .w(px(PHONE_W))
        .gap(px(28.))
        .py_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(state("Chats selected", "chats", cx))
        .child(state("Teams selected", "teams", cx))
        .child(state("Assistant selected", "assistant", cx))
        .child(state("New open", "new", cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_follows_the_shell() {
        assert_eq!(CHATS.shell.tabs, ["chats", "teams", "assistant", "new", "settings"]);
        assert_eq!(CHATS.shell.new_menu.items, ["issue", "chat"]);
    }
}
