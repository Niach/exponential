//! #EXP-1611 / #EXP-1710: `pr-checks` — CI and the merge train on the
//! chat's PR (pr-checks.json CHECKS LIST, merge-train.json STATUS PILL),
//! case for case with the web specimen (`packages/ui/src/pr-checks.tsx`
//! `PrChecksSpecimen`): THE header merge STATUS pill in every merge-train
//! `statusPill` case (case order, wrapping) over
//! `domain::merge_train::status_pill`, then the `…` menu's open Checks
//! submenu drawn as a static menu panel over pr-checks.json `specimen` (its
//! checks at its `now`).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

use domain::merge_train::{status_pill, StatusPillInput, StatusPillView};

pub(crate) const ID: &str = "pr-checks";
pub(crate) const OWNER: &str = "EXP-1611";

const FIXTURE: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/pr-checks.json");
const MERGE_TRAIN: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/merge-train.json");

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

/// Every merge-train.json `statusPill` case's view, in case order.
fn status_pills() -> Vec<StatusPillView> {
    let fixture: serde_json::Value = serde_json::from_str(MERGE_TRAIN).expect("merge-train.json parses");
    fixture["statusPill"]
        .as_array()
        .map(|cases| {
            cases
                .iter()
                .map(|case| {
                    let input = &case["input"];
                    status_pill(StatusPillInput {
                        state: input["state"].as_str().unwrap_or_default(),
                        wait: input["wait"].as_str(),
                        position: input["position"].as_i64(),
                        batch_size: input["batchSize"].as_i64(),
                        hold_reason: input["holdReason"].as_str(),
                        stack: input["stack"].as_bool().unwrap_or(false),
                        checks: input["checks"].as_u64().unwrap_or(0) as usize,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    // Web `max-w-xl flex-wrap gap-2`.
    let mut pills = div().flex().flex_row().flex_wrap().items_center().gap_2().max_w(px(576.));
    for (index, view) in status_pills().iter().enumerate() {
        pills = pills.child(crate::work_header::status_pill_face(
            gpui::SharedString::from(format!("sg-pr-status-{index}")),
            view,
            crate::surface::PillSize::Md,
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

    /// #EXP-1710: one pill per `statusPill` case, Needs approval the only
    /// primary ones, the readonly case without a tap.
    #[test]
    fn every_status_pill_case_draws() {
        let views = status_pills();
        assert_eq!(views.len(), 17);
        assert_eq!(views[0].label, "Draft");
        assert!(views.iter().filter(|view| view.primary).all(|view| view.label == "Needs approval"));
        assert!(views.iter().any(|view| view.tap.is_none()));
    }
}
