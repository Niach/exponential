//! #EXP-1804: `ci-run` — THE CI RUN PAGE over ci-run.json `specimen` (run
//! #12 of `ci` on mint, `test (linux)` failed and selected), the REAL page
//! ([`crate::ci_run::CiRunView`]) with its fixture data, case for case with
//! the web's `?dev=ci-run` and the phones' `sg_ci-run`.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

pub(crate) const ID: &str = "ci-run";
pub(crate) const OWNER: &str = "EXP-1804";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let page = window.use_keyed_state(SharedString::from("specimen-ci-run"), cx, |window, cx| {
        crate::ci_run::CiRunView::specimen(window, cx)
    });
    div().w_full().max_w(px(1040.)).h(px(860.)).child(page)
}
