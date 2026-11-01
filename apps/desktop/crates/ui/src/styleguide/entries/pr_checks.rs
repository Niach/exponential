//! #EXP-1611: `pr-checks` — CI on the thread's PR (pr-checks.json MERGE PILL
//! / CHECKS LIST): THE header Merge pill in its four check states (none,
//! pending = THE spinner, success, failure = `Checks failing`) over
//! `domain::pr_checks::merge_pill`, then the `…` menu's open Checks submenu
//! drawn as a static menu panel over the fixture `specimen` (its checks at
//! its `now`). No `sync::Store` here: the pills' stack and merge reads are
//! `try_global`, so each reads as a plain, idle PR.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{h_flex, v_flex};

use domain::contract::{
    PR_CHECKS_STATE_FAILURE, PR_CHECKS_STATE_NONE, PR_CHECKS_STATE_PENDING, PR_CHECKS_STATE_SUCCESS,
};

pub(crate) const ID: &str = "pr-checks";
pub(crate) const OWNER: &str = "EXP-1611";

const FIXTURE: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/pr-checks.json");

/// The fixture `specimen`: its rows and its `now` (UTC ms).
fn specimen() -> (Vec<domain::rows::PullRequestCheckRow>, i64) {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("pr-checks.json parses");
    let specimen = &fixture["specimen"];
    let now = specimen["now"]
        .as_str()
        .and_then(domain::instant::parse_instant)
        .unwrap_or_default();
    (domain::pr_checks::fixture_checks(&specimen["checks"]), now)
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let mut pills = h_flex().items_center().gap_3();
    for state in [
        PR_CHECKS_STATE_NONE,
        PR_CHECKS_STATE_PENDING,
        PR_CHECKS_STATE_SUCCESS,
        PR_CHECKS_STATE_FAILURE,
    ] {
        pills = pills.child(crate::work_header::merge_pill_with_checks(
            gpui::SharedString::from(format!("sg-pr-checks-merge-{state}")),
            &format!("sg-pr-checks-{state}"),
            true,
            crate::surface::PillSize::Md,
            Some(state),
            cx,
        ));
    }
    let (rows, now) = specimen();
    v_flex()
        .items_start()
        .gap_6()
        .pt_2()
        .child(pills)
        .child(div().w(px(560.)).child(crate::pr_checks::checks_panel(&rows, now, cx)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_specimen_parses_with_its_clock() {
        let (rows, now) = specimen();
        assert_eq!(rows.len(), 5);
        assert!(now > 0);
        let list = domain::pr_checks::checks_list(&rows, now);
        assert_eq!(list[0].name, "gate");
        assert_eq!(list[1].duration.as_deref(), Some("2m 3s"));
    }
}
