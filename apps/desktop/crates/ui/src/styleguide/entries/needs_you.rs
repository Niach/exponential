//! #FEED-191: `needs-you` (`fixtures/needs-you.json`; web entry `needs-you`,
//! iOS + Android twins): `cases[1]`'s rows in THE inbox page's reading row and
//! the phones' entry row (the Archived row's box, drawn here for parity: the IDE
//! itself takes the rail's entry). #EXP-1710: ONE stuck PR row (merge-train
//! `stuckRow` cases[0]) under `cases[1]`'s rows, drawn by the real stuck row.

use gpui::{div, px, AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::CHATS;
use domain::needs_you::{needs_you, NeedsYouInput, NEEDS_YOU};

use crate::icons::registry;

pub(crate) const ID: &str = "needs-you";
pub(crate) const OWNER: &str = "FEED-191";

/// The specimen's card width (web's `NeedsYouSpecimen` default).
const WIDTH: f32 = 560.;

const NEEDS_YOU_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/needs-you.json");

const MERGE_TRAIN_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/merge-train.json");

/// `cases[1]`: the fold of its input.
fn specimen_rows() -> Vec<domain::needs_you::NeedsYouRow> {
    let fixture: serde_json::Value = serde_json::from_str(NEEDS_YOU_JSON).expect("needs-you.json parses");
    let input: NeedsYouInput = serde_json::from_value(fixture["cases"][1]["input"].clone()).expect("cases[1] input");
    needs_you(&input)
}

/// merge-train `stuckRow` cases[0] (`Auto-merge stuck on #12`, `3 bounces`,
/// Retry · Merge now · Close PR) as ONE `pr` row of `cases[1]`'s `Palette`
/// chat (web `NeedsYouSpecimen`'s `pr12`), folded under the decisions.
fn specimen_with_stuck() -> crate::needs_you::NeedsYouRows {
    let fixture: serde_json::Value = serde_json::from_str(MERGE_TRAIN_JSON).expect("merge-train.json parses");
    let input = &fixture["stuckRow"][0]["input"];
    let ms = |value: &serde_json::Value| value.as_str().and_then(domain::session_tree::iso_ms);
    let view = domain::merge_train::stuck_row(
        input["number"].as_i64().expect("number"),
        input["state"].as_str().expect("state"),
        input["holdReason"].as_str(),
        ms(&input["deviceOfflineSince"]),
        ms(&input["now"]).expect("now"),
        input["owner"].as_bool().expect("owner"),
    )
    .expect("stuckRow cases[0] is a row");
    let row = domain::needs_you::NeedsYouRow {
        message_id: "pr12".into(),
        session_id: "r2".into(),
        root_id: "r2".into(),
        kind: domain::needs_you::NeedsYouKind::Pr,
        opens: domain::needs_you::NeedsYouOpens::Results,
        title: view.title,
        chat: "Palette".into(),
        meta: view.meta,
        age: "3h".into(),
        created_at: "2027-01-15T05:00:00.000Z".into(),
    };
    let stuck = crate::needs_you::StuckPr { pull_request_id: "pr12".into(), actions: view.actions };
    crate::needs_you::with_stuck_rows(specimen_rows(), vec![(row, stuck)])
}

/// The phones' entry: the glyph in the needs-you tone centred in the lead
/// disc's column, the title, the count Badge in the same tone trailing.
fn entry_row(count: usize, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let yellow = theme::tokens::YELLOW.to_hsla();
    h_flex()
        .id(SharedString::from(NEEDS_YOU.test_ids.entry.clone()))
        .w_full()
        .gap_4()
        .items_center()
        .child(
            div()
                .w(px(CHATS.shell.lead_disc))
                .flex_shrink_0()
                .flex()
                .justify_center()
                .child(Icon::new(registry::NAV_NEEDS_YOU).with_size(px(CHATS.shell.archived_glyph)).text_color(yellow)),
        )
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .gap_2()
                .items_center()
                .py_3()
                .border_b_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .child(div().flex_1().min_w_0().truncate().text_size(px(15.)).font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(SharedString::from(NEEDS_YOU.copy.entry.clone())))
                .child(
                    div()
                        .flex_shrink_0()
                        .min_w(px(20.))
                        .h(px(20.))
                        .px_1p5()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(yellow)
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(gpui::black())
                        .child(SharedString::from(count.to_string())),
                ),
        )
        .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let rows = specimen_with_stuck();
    let background = cx.theme().background;
    v_flex().items_start().child(
        v_flex()
            .id("needs-you-specimen")
            .w(px(WIDTH))
            .p(px(20.))
            .gap_4()
            .rounded(px(24.))
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .bg(background)
            .child(entry_row(rows.rows.len(), cx))
            .child(crate::needs_you::needs_you_list(&rows.rows, &rows.stuck, cx)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cases[1]`'s fold names its plan and its question, newest first.
    #[test]
    fn the_specimen_is_the_fixture_fold() {
        let rows = specimen_rows();
        assert!(!rows.is_empty());
        assert!(rows.iter().any(|row| row.kind == domain::needs_you::NeedsYouKind::Plan));
        assert!(rows.iter().any(|row| row.kind == domain::needs_you::NeedsYouKind::Question));
    }

    /// #EXP-1710: ONE stuck row (merge-train `stuckRow` cases[0]) under
    /// `cases[1]`'s rows, with its Button row.
    #[test]
    fn the_specimen_adds_one_stuck_row_last() {
        let folded = specimen_with_stuck();
        assert_eq!(folded.rows.len(), specimen_rows().len() + 1);
        let last = folded.rows.last().unwrap();
        assert_eq!(last.title, "Auto-merge stuck on #12");
        assert_eq!(last.meta, "3 bounces");
        let ids: Vec<&str> = folded.stuck["pr12"].actions.iter().map(|action| action.id()).collect();
        assert_eq!(ids, ["retry", "merge-now", "close-pr"]);
    }
}
