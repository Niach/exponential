//! #EXP-2172: `ci` — THE CI PAGE over ci-page.json `scenario` (#EXP-2214:
//! the Merge queue tab, `EXP_DEV_CI_TAB` another), the REAL page ([`crate::ci_page::CiPageView`]) with its fixture
//! data, case for case with the web's `?dev=ci` and the phones' `sg_ci`.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

pub(crate) const ID: &str = "ci";
pub(crate) const OWNER: &str = "EXP-2172";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let page = window.use_keyed_state(SharedString::from("specimen-ci"), cx, |window, cx| crate::ci_page::CiPageView::specimen(window, cx));
    div().w_full().max_w(px(1040.)).h(px(860.)).child(page)
}
