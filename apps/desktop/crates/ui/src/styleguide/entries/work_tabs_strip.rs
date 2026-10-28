//! EXP-1255 → EXP-1294: `work-tabs-strip` (Special components): the IDE's
//! top tab strip, drawn with the strip's own pieces, case for case with the
//! web entry: `surface::rich_tab` chips (EXP-1334: THE strip chip box,
//! `surface::strip_chip_frame`, the thread header's too) in the strip's
//! hover group, every chip's × shown on hover only
//! ([`crate::screens::hover_close`], the active one too; Cmd+W unchanged;
//! the second chip is drawn HOVERED, its × out), an issue chip's lead = its
//! status glyph, the open New session tab's = the chat glyph, the "+N" of
//! the tabs that did not fit ([`crate::surface::strip_overflow_button`]),
//! then the ghost "+" that opens Home ([`crate::screens::home_button`]).

use gpui::{App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{h_flex, ActiveTheme as _, Icon};
use theme::tokens as t;

use domain::statuses::constructed_default;
use domain::IssueStatus;

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::screens::{home_button, hover_close, TAB_GROUP};
use crate::surface::{rich_tab, strip_overflow_button, RichTab, RichTabStatus};
use crate::thread_screen::issue_glyph;

pub(crate) const ID: &str = "work-tabs-strip";
pub(crate) const OWNER: &str = "EXP-1255";

/// An issue chip's lead: its status glyph (the builtin default of `status`).
fn status_lead(status: IssueStatus, cx: &App) -> RichTabStatus {
    RichTabStatus::Glyph(issue_glyph(Some(&constructed_default(status)), cx).expect("a status draws its glyph"))
}

fn tab(
    ix: usize,
    selected: bool,
    status: RichTabStatus,
    identifier: Option<&'static str>,
    title: &'static str,
) -> RichTab {
    let mut tab = RichTab::new(("sg-work-tab", ix), selected);
    tab.status = status;
    tab.identifier = identifier.map(Into::into);
    tab.title = Some(title.into());
    tab.closable = true;
    tab
}

/// A chip at rest or active: its × in the strip's hover group.
fn chip(
    ix: usize,
    selected: bool,
    status: RichTabStatus,
    identifier: Option<&'static str>,
    title: &'static str,
    cx: &App,
) -> gpui::AnyElement {
    rich_tab(tab(ix, selected, status, identifier, title), cx)
        .group(TAB_GROUP)
        .child(hover_close(Button::new(("sg-work-tab-close", ix))))
        .into_any_element()
}

/// The HOVERED chip: the glass active wash, the foreground, its × out.
fn hovered_chip(
    ix: usize,
    status: RichTabStatus,
    identifier: Option<&'static str>,
    title: &'static str,
    cx: &App,
) -> gpui::AnyElement {
    rich_tab(tab(ix, false, status, identifier, title), cx)
        .bg(t::glass::FILL_ACTIVE.to_hsla())
        .text_color(cx.theme().foreground)
        .child(
            Button::new(("sg-work-tab-close", ix))
                .ghost()
                .web_icon_xs()
                .icon(registry::UI_CLOSE),
        )
        .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    h_flex()
        .gap_1()
        .items_center()
        .child(chip(0, true, status_lead(IssueStatus::InProgress, cx), Some("EXP-1223"), "ExponentialOS", cx))
        .child(hovered_chip(1, status_lead(IssueStatus::Backlog, cx), Some("EXP-563"), "google keywords", cx))
        .child(chip(
            2,
            false,
            RichTabStatus::Glyph(Icon::new(registry::ACTION_CHAT).text_color(cx.theme().muted_foreground)),
            None,
            "New session",
            cx,
        ))
        .child(strip_overflow_button("sg-work-tab-more", 2, cx))
        .child(home_button("sg-work-tab-new"))
}
