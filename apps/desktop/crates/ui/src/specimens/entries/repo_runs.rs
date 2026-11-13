//! #EXP-1617 E4: `repo-runs` — THE REPO PAGE's Runs tab over repo-ci.json
//! `runRows` (every status, a missing ref), the REAL page
//! ([`crate::settings::repo_page::RepoPage`]) with pinned data, case for
//! case with the phones' `sg_repo-runs`.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

pub(crate) const ID: &str = "repo-runs";
pub(crate) const OWNER: &str = "EXP-1617";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let page = window.use_keyed_state(SharedString::from("specimen-repo-runs"), cx, |window, cx| {
        crate::settings::repo_page::RepoPage::specimen("runs", window, cx)
    });
    div().w_full().max_w(px(1040.)).p(px(24.)).child(page)
}
