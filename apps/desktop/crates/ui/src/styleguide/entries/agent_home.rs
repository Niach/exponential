//! EXP-1253/EXP-1294 — `agent-home` (3 Special components): HOME
//! (`fixtures/composer-home.json`): the DAWN (FEED-100, the faint brand
//! mark's top half fading toward the card), the composer card with the home
//! placeholder and its options line, nothing beneath it. Drawn from the
//! page's own pieces ([`crate::chat_screen::dawn`], the composer card) over
//! static data; a second specimen shows the card's STARTING row in place of
//! the options line (EXP-1271).

use std::time::{Duration, Instant};

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::home_greeting;
use crate::icons::registry;

pub(crate) const ID: &str = "agent-home";
pub(crate) const OWNER: &str = "EXP-1293";

/// The card at rest: the home placeholder, the "+" and the send.
fn card(id: &'static str, cx: &App) -> Div {
    let placeholder = div()
        .w_full()
        .min_h(px(40.))
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(home_greeting::HOME_PLACEHOLDER);
    let composer = crate::composer::GlassComposer::new(placeholder.into_any_element())
        .tool(Button::new((id, 0usize)).ghost().icon(registry::UI_ADD).into_any_element())
        .submit(
            crate::composer::composer_submit((id, 1usize), registry::UI_SUBMIT, true, cx)
                .into_any_element(),
        );
    crate::composer::glass_composer(composer)
}

fn options_line(cx: &App) -> Div {
    div()
        .px_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child("macbook · Opus 5.5 · Plan off")
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    // EXP-1314: the dawn stands ON the card, no gap (`render_page`).
    let home = v_flex()
        .w(px(640.))
        .items_center()
        .child(crate::chat_screen::dawn(cx))
        .child(v_flex().w_full().gap_2().child(card("sg-agent-home", cx)).child(options_line(cx)));
    let starting = v_flex().w(px(640.)).gap_2().child(card("sg-agent-home-starting", cx)).child(
        div().px_1().child(crate::feedback::starting_row(
            "sg-agent-home-starting-row",
            "macbook",
            Instant::now() - Duration::from_millis(2300),
            crate::surface::page_ground(),
            cx,
        )),
    );
    v_flex().gap_8().child(home).child(starting)
}
