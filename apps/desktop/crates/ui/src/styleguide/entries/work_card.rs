//! EXP-1240 (EXP-1259 D12): `work-card` (Special components): THE Work card
//! the session view draws for a claude workflow, the run's task list and a
//! subagent — the REAL painter (`crate::work_card::work_card`) over the
//! contract model, fed the fixture's own cases (`fixtures/work-card.json`):
//! a workflow mid-review (its folded row open), a task list in progress, a
//! finished subagent.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use steer::work_card::{work_card, work_card_lanes, WorkCardInput};

use crate::work_card::WorkCardProps;

pub(crate) const ID: &str = "work-card";
pub(crate) const OWNER: &str = "EXP-1240";

const FIXTURE: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/work-card.json");

/// The fixture cases the demo draws, by index: the workflow mid-review, the
/// task list in progress, the finished subagent.
const CASES: [usize; 3] = [0, 5, 9];

fn inputs() -> Vec<WorkCardInput> {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("work-card.json parses");
    CASES
        .iter()
        .filter_map(|&index| {
            serde_json::from_value(fixture["cases"][index]["input"].clone()).ok()
        })
        .collect()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let mut column = div().w(px(560.)).flex().flex_col().gap_3();
    for (index, input) in inputs().iter().enumerate() {
        let card = work_card(input);
        let lanes = work_card_lanes(input)
            .into_iter()
            .map(|lanes| lanes.into_iter().map(|(_, lane)| lane).collect())
            .collect();
        column = column.child(crate::work_card::work_card(
            WorkCardProps {
                id: SharedString::from(format!("styleguide-work-card-{index}")),
                card: &card,
                lanes,
                folded_open: index == 0,
                on_toggle_folded: None,
                on_lane: None,
                on_card: None,
                footer: Vec::new(),
            },
            cx,
        ));
    }
    column
}
