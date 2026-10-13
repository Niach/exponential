//! EXP-1255 → EXP-1294: `work-tabs-strip` (Special components): the IDE's
//! top tab strip, drawn with the strip's own pieces — `surface::rich_tab`
//! chips in the strip's hover group, every chip's × shown on hover only
//! ([`crate::screens::hover_close`], the active one too; Cmd+W unchanged),
//! a thread chip's lead = its root's state mark
//! ([`crate::screens::session_tab_status`]), an issue chip's = its status
//! glyph, then the ghost "+" that opens Home
//! ([`crate::screens::home_button`]).

use gpui::{App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::button::Button;
use gpui_component::{h_flex, Icon};

use domain::session_tree::SessionState;

use crate::icons::registry;
use crate::screens::{home_button, hover_close, session_tab_status, TAB_GROUP};
use crate::surface::{rich_tab, RichTab, RichTabStatus};

pub(crate) const ID: &str = "work-tabs-strip";
pub(crate) const OWNER: &str = "EXP-1255";

fn chip(
    ix: usize,
    selected: bool,
    status: RichTabStatus,
    identifier: Option<&'static str>,
    title: &'static str,
    cx: &App,
) -> gpui::AnyElement {
    let mut tab = RichTab::new(("sg-work-tab", ix), selected);
    tab.status = status;
    tab.identifier = identifier.map(Into::into);
    tab.title = Some(title.into());
    tab.closable = true;
    rich_tab(tab, cx)
        .group(TAB_GROUP)
        .child(hover_close(Button::new(("sg-work-tab-close", ix))))
        .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    h_flex()
        .gap_1()
        .items_center()
        .child(chip(
            0,
            false,
            session_tab_status(SessionState::Working, cx),
            None,
            "Login fails on Safari",
            cx,
        ))
        .child(chip(
            1,
            true,
            RichTabStatus::Glyph(Icon::new(registry::STATUS_IN_PROGRESS)),
            Some("EXP-1223"),
            "ExponentialIOS",
            cx,
        ))
        .child(chip(
            2,
            false,
            session_tab_status(SessionState::NeedsYou, cx),
            None,
            "UX batch 3",
            cx,
        ))
        .child(home_button("sg-work-tab-new"))
}
