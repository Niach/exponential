//! EXP-1249/EXP-1334/EXP-1252/N-12 — the `composer-options` entry (3
//! Special components), case for case with the web entry, drawn by the
//! REAL words it is made of: before a start the muted line UNDER the card
//! (Device · Account · Model · Plan · Repository: the pin triggers
//! `launch_options::inline_pin_trigger{,_with}`, the real switch); in a
//! session (running, then an idle thread = its root session) the pins IN
//! the card's tool row ([`super::composer_dialog::session_row`]: the Device
//! pin as its glyph alone · Model · Plan; EXP-1252: a pick moves the
//! session to that device); then the one composer in a session
//! ([`super::composer_dialog::ComposerSpecimen`]: the "+", the pins, the
//! context ring beside Stop, nothing under the card). Effort, Ultracode,
//! MCP servers and Computer use are rows of the composer's "+" menu (the
//! `composer-dialog` entry draws it open).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use super::composer_dialog::{composer_specimen, options_line, session_row, Phase};

pub(crate) const ID: &str = "composer-options";
pub(crate) const OWNER: &str = "EXP-1252";

fn caption(text: &'static str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let composer = composer_specimen("sg-composer-options-composer", Phase::Running, "Type / for commands", window, cx);
    v_flex()
        // Web `max-w-[40rem]`.
        .w(px(640.))
        .max_w_full()
        .gap_3()
        .child(caption("Before a start (Home / New session): device · account · model · Plan · repository, under the card", cx))
        .child(options_line("sg-composer-options-start", Phase::Start, cx))
        .child(caption("A running session: the pins in the card's tool row — the device glyph · model · Plan (live mid-run)", cx))
        .child(session_row("sg-composer-options-running", Phase::Running, cx))
        .child(caption("An idle thread = its root session: the same row", cx))
        .child(session_row("sg-composer-options-idle", Phase::Idle, cx))
        .child(caption("The one composer in a session: the \"+\", the pins, the context ring beside Stop; nothing under the card", cx))
        .child(composer)
}
