//! #EXP-1617 E4: `repo-ci` — THE REPO PAGE's Settings tab with its CI
//! section (repo-ci.json: pipelines, required checks, secrets, the GitHub
//! switches) over the fixture's runs, the REAL page
//! ([`crate::settings::repo_page::RepoPage`]) with pinned data, case for
//! case with the phones' `sg_repo-ci`.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

pub(crate) const ID: &str = "repo-ci";
pub(crate) const OWNER: &str = "EXP-1617";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let page = window.use_keyed_state(SharedString::from("specimen-repo-ci"), cx, |window, cx| {
        crate::settings::repo_page::RepoPage::specimen("settings", window, cx)
    });
    div().w_full().max_w(px(1040.)).p(px(24.)).child(page)
}
