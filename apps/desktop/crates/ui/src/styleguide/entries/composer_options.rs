//! EXP-1249/EXP-1334 — the `composer-options` entry (3 Special components):
//! the muted line UNDER the one composer card, case for case with the web
//! entry, drawn by the REAL words it is made of: before a start Device ·
//! Account · Model · Plan · Repository (the pin triggers
//! `launch_options::inline_pin_trigger{,_with}`, the real switch); in a
//! session (running, then an idle thread = its dispatcher session) Model ·
//! Plan … the context ring; then the line under the card, in the one
//! composer ([`super::composer_dialog::ComposerSpecimen`]). Effort,
//! Ultracode, MCP servers and Computer use are rows of the composer's "+"
//! menu (the `composer-dialog` entry draws it open).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use super::composer_dialog::{composer_specimen, options_line, thread_placeholder, Phase};

pub(crate) const ID: &str = "composer-options";
pub(crate) const OWNER: &str = "EXP-1249";

fn caption(text: &'static str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let composer = composer_specimen("sg-composer-options-composer", Phase::Running, &thread_placeholder(), window, cx);
    v_flex()
        // Web `max-w-[40rem]`.
        .w(px(640.))
        .max_w_full()
        .gap_3()
        .child(caption("Before a start (Home / New session): device · account · model · Plan · repository", cx))
        .child(options_line("sg-composer-options-start", Phase::Start, cx))
        .child(caption("A running session: model · Plan (live mid-run) … the context ring", cx))
        .child(options_line("sg-composer-options-running", Phase::Running, cx))
        .child(caption("An idle thread = its dispatcher session: the same line", cx))
        .child(options_line("sg-composer-options-idle", Phase::Idle, cx))
        .child(caption("Under the card, in the one composer", cx))
        .child(composer)
}
