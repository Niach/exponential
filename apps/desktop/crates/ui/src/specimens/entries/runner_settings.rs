//! #EXP-1804: `runner-settings` — a device's CI RUNNER SECTION over
//! runner-settings.json `specimen` (mint's enabled runner, the owner
//! viewing, one label, two slots, 1 h) with `recentJobs[0]`'s jobs at its
//! `now`: the REAL section ([`crate::ci_run::RunnerSection`]), writes kept
//! local.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

pub(crate) const ID: &str = "runner-settings";
pub(crate) const OWNER: &str = "EXP-1804";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let section = window.use_keyed_state(SharedString::from("specimen-runner-settings"), cx, |window, cx| {
        let mut section = crate::ci_run::RunnerSection::new("specimen-runner-settings", window, cx);
        section.set_input(crate::ci_run::runner_specimen_input(), cx);
        section
    });
    div().w_full().max_w(px(560.)).child(section)
}
