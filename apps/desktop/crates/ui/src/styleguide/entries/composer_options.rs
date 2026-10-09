//! EXP-1249 — the `composer-options` entry (3 Special components): the
//! Agent composer's options LINE under the card and the "+" that took the
//! rest of the run's options.
//!
//! The line keeps Device · Account · Model · Plan (· Resume · Repository):
//! the REAL pin triggers (`launch_options::inline_pin_trigger{,_with}`) and
//! the real switch, the same muted `text_xs` ghosts the composer draws. The
//! `⋯` overflow EXP-991 hung at its end is gone: Effort, Subagents,
//! Ultracode, MCP servers and Computer use live in the "+" menu (the `menu`
//! entry draws it open, `chat_screen::plus_menu`).

use gpui::{div, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::icons::registry;
use crate::launch_options::{inline_pin_trigger, inline_pin_trigger_with};

pub(crate) const ID: &str = "composer-options";
pub(crate) const OWNER: &str = "EXP-1249";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let line = h_flex()
        .flex_wrap()
        .gap_1()
        .items_center()
        .px_1()
        .text_xs()
        .text_color(muted)
        .child(inline_pin_trigger_with(
            SharedString::from("sg-composer-options-device"),
            Some(registry::NAV_COMPUTER),
            "mint — This device".to_string(),
            cx,
        ))
        .child(inline_pin_trigger(
            SharedString::from("sg-composer-options-account"),
            "danny@yourev.at".to_string(),
            cx,
        ))
        .child(inline_pin_trigger(
            SharedString::from("sg-composer-options-model"),
            "Fable".to_string(),
            cx,
        ))
        .child(
            h_flex()
                .gap_1p5()
                .items_center()
                .px_1()
                .child("Plan")
                .child(crate::controls::web_switch("sg-composer-options-plan").checked(false)),
        )
        .child(div().flex_1())
        .child(inline_pin_trigger_with(
            SharedString::from("sg-composer-options-repo"),
            Some(registry::UI_REPOSITORY),
            "Niach/exponential".to_string(),
            cx,
        ));
    v_flex()
        .w(gpui::px(560.))
        .gap_2()
        .child(
            h_flex()
                .px_1()
                .child(
                    crate::composer::composer_tool("sg-composer-options-plus", registry::UI_ADD, cx)
                        .tooltip(crate::chat_screen::PLUS_TOOLTIP)
                        .into_any_element(),
                ),
        )
        .child(line)
}
