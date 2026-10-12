//! EXP-1255 — `work-tabs-strip` (3 Special components): the top tab strip.
//! Every chip's × shows on hover only (the active one too; Cmd+W unchanged)
//! and the ghost "+" after the last chip opens a New run tab (Cmd+T too).
//! Drawn with the strip's own pieces: `surface::rich_tab`,
//! [`crate::screens::hover_close`] and [`crate::screens::new_run_button`].

use gpui::{App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::button::Button;
use gpui_component::{h_flex, Icon};

use crate::icons::registry;
use crate::screens::{hover_close, new_run_button, TAB_HOVER_GROUP};
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
        .group(TAB_HOVER_GROUP)
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
            RichTabStatus::Run(Some(coding::CodingAgent::Claude), None),
            None,
            "UX batch 3",
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
            RichTabStatus::Glyph(Icon::new(registry::ACTION_CHAT)),
            None,
            crate::navigation::NEW_RUN_TITLE,
            cx,
        ))
        .child(new_run_button("sg-work-tab-new"))
}
