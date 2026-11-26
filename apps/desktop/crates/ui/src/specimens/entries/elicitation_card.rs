//! `elicitation-card` (#NEW-146, `fixtures/elicitation-card.json` ×4): THE
//! elicitation card specimen, the web `ElicitationCardSpecimen` line for
//! line: the fixture's device-built cards (`cardCases[*].expected`) through
//! the domain mirror (`domain::session_tree::ask_card_row`) and painted by
//! the chat's REAL card (`crate::session_chat::ask_row_card`): the waiting
//! form (every field kind, defaults prefilled), the clean url review, the
//! warned url review, and the form answered (Submit, estimate 3, due
//! 2026-12-01). Captions = `copy.specimen`.

use domain::elicitation_card::copy;
use domain::rows::MessageRow;
use domain::session_tree::SessionChatRow;
use gpui::{div, px, App, Div, Entity, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};
use serde_json::{json, Value};

use crate::markdown::ImageCache;
use crate::session_chat::ChatText;

pub(crate) const ID: &str = "elicitation-card";
pub(crate) const OWNER: &str = "NEW-146";

const FIXTURE_JSON: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/elicitation-card.json");

const FORM: &str = "a form with every field kind";
const URL: &str = "a url: the domain";
const WARNED: &str = "a url on http";

/// The fixture's device-built card whose case name starts with `prefix`.
fn card(fixture: &Value, prefix: &str) -> Option<Value> {
    fixture["cardCases"]
        .as_array()?
        .iter()
        .find(|case| case["name"].as_str().is_some_and(|name| name.starts_with(prefix)))
        .map(|case| case["expected"].clone())
        .filter(|card| card.is_object())
}

/// A card as its Conversation row (the real row model).
fn row(id: &str, card: Option<Value>, answered_at: Option<&str>) -> Vec<SessionChatRow> {
    let Some(card) = card else { return Vec::new() };
    let message = MessageRow {
        id: id.to_string(),
        session_id: "s-1".to_string(),
        card: Some(card),
        answered_at: answered_at.map(str::to_string),
        ..Default::default()
    };
    domain::session_tree::ask_card_row(&message).into_iter().collect()
}

/// The four cases, in the specimen's order: (caption, rows, answerable).
pub(crate) fn cases() -> Vec<(&'static str, Vec<SessionChatRow>, bool)> {
    let fixture: Value = serde_json::from_str(FIXTURE_JSON).expect("elicitation-card.json parses");
    let form = card(&fixture, FORM);
    let answered = form.clone().map(|mut card| {
        let mut data = card["surface"]["data"]["form"].clone();
        data["estimate"] = json!(3);
        data["due"] = json!("2026-12-01");
        card["answer"] = json!(copy::SUBMIT);
        card["answers"] = json!([copy::SUBMIT]);
        card["surfaceData"] = json!({ "form": data });
        card
    });
    vec![
        (copy::SPECIMEN_FORM, row("e1", form, None), true),
        (copy::SPECIMEN_URL, row("e2", card(&fixture, URL), None), true),
        (copy::SPECIMEN_WARNED, row("e3", card(&fixture, WARNED), None), true),
        (copy::SPECIMEN_ANSWERED, row("e4", answered, Some("2026-11-26T12:00:00.000Z")), false),
    ]
}

fn demo(label: &str, rows: &[SessionChatRow], answer: bool, images: &Entity<ImageCache>, window: &mut Window, cx: &mut App) -> Div {
    // The waiting cards answer and carry their × (the web specimen's
    // `onAnswer` / `onDismiss`); the answered one reads only.
    let text = ChatText::default().answering(answer);
    let cards: Vec<_> = rows.iter().filter_map(|row| crate::session_chat::ask_row_card(row, &text, images, window, cx)).collect();
    v_flex()
        .w_full()
        .gap_1p5()
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::from(label.to_string())))
        .children(cards)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-elicitation-card-images", cx, |_, _| ImageCache::new(None)).clone();
    let mut column = v_flex().w(px(672.)).gap_6().text_sm();
    for (label, rows, answer) in cases() {
        column = column.child(demo(label, &rows, answer, &images, window, cx));
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::elicitation_card::ElicitationMode;

    /// The four cases are elicitation cards: a waiting form, two waiting
    /// url reviews, the answered form with its data.
    #[test]
    fn the_cases_are_the_fixture_cards() {
        let cases = cases();
        assert_eq!(cases.len(), 4);
        let modes: Vec<_> = cases
            .iter()
            .map(|(_, rows, _)| match rows.as_slice() {
                [SessionChatRow::Question { elicitation: Some(elicitation), answered, .. }] => (elicitation.mode, *answered),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            modes,
            [
                (ElicitationMode::Form, false),
                (ElicitationMode::Url, false),
                (ElicitationMode::Url, false),
                (ElicitationMode::Form, true)
            ]
        );
        let SessionChatRow::Question { answers, surface_data: Some(data), .. } = &cases[3].1[0] else { panic!() };
        assert_eq!(answers.as_deref(), Some(&[copy::SUBMIT.to_string()][..]));
        assert_eq!(data["form"]["estimate"], json!(3));
        assert_eq!(data["form"]["due"], json!("2026-12-01"));
    }

    /// The view's desktop picture (views.json `elicitation-card`), drawn
    /// HEADLESS by the real chat card: on a Linux machine the shots lane
    /// (macOS screencapture) cannot run, so this writes
    /// `$EXP_ELICITATION_SHOT` (a PNG path). CPU rendering works:
    /// `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
    /// EXP_ELICITATION_SHOT=/tmp/e.png cargo test -p ui --lib -- --ignored elicitation_card_shot`.
    #[test]
    #[ignore]
    fn elicitation_card_shot() {
        use gpui::{AppContext as _, IntoElement, Render, Styled as _};
        let Ok(out) = std::env::var("EXP_ELICITATION_SHOT") else { return };
        struct Frame;
        impl Render for Frame {
            fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
                div().size_full().bg(cx.theme().background).p_6().child(super::render(window, cx))
            }
        }
        let mut cx = crate::shell_bench::headless();
        cx.update(|cx| {
            crate::init(cx);
            let store = sync::Store::open(cx, None, None);
            cx.set_global(store);
        });
        let window = cx
            .open_window(gpui::size(px(720.), px(2560.)), |window: &mut Window, cx| {
                let view = cx.new(|_| Frame);
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("a headless window");
        let window: gpui::AnyWindowHandle = window.into();
        for _ in 0..8 {
            crate::transcript_bench::draw(window, &mut cx);
            cx.run_until_parked();
        }
        let shot = cx.capture_screenshot(window).expect("the frame");
        shot.save(&out).expect("the PNG");
        std::mem::forget(cx);
    }
}
