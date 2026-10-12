//! EXP-1279 (EXP-1259 D13): `script-card` (Special components): the Script
//! card a `codemode__exec` call renders as — the REAL painter
//! (`crate::script_card::script_card`) over the contract model, fed the
//! fixture's grouped case (`fixtures/script-card.json`) with its Script
//! disclosure open, then a running script.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use steer::script_card::{script_card, ScriptCardInput};

use crate::script_card::ScriptCardProps;

pub(crate) const ID: &str = "script-card";
pub(crate) const OWNER: &str = "EXP-1279";

const FIXTURE: &str =
    include_str!("../../../../../../packages/domain-contract/fixtures/script-card.json");

/// A short script, the shape an agent writes.
const SOURCE: &str = "const ids = [\"EXP-1253\", \"EXP-1254\", \"EXP-1255\"];\nconst issues = await Promise.all(ids.map((id) => tools.exponential.exponential_issues_get({ id })));\nreturn issues.map((issue) => issue.title);";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("script-card.json parses");
    let grouped: ScriptCardInput =
        serde_json::from_value(fixture["cases"][1]["input"].clone()).unwrap_or_default();
    let running = ScriptCardInput { running: true, ..Default::default() };
    let mut column = div().w(px(560.)).flex().flex_col().gap_3();
    for (index, (input, open)) in [(grouped, true), (running, false)].into_iter().enumerate() {
        let card = script_card(&input);
        column = column.child(crate::script_card::script_card(
            ScriptCardProps {
                id: SharedString::from(format!("styleguide-script-card-{index}")),
                card: &card,
                running: input.running,
                failed: false,
                source: Some(SOURCE.to_string()),
                source_open: open,
                on_toggle_source: None,
            },
            cx,
        ));
    }
    column
}
