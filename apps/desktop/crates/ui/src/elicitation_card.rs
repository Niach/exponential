//! #NEW-146 — THE elicitation card ×4 (`fixtures/elicitation-card.json`
//! RENDER; web `packages/ui/src/elicitation-card.tsx`): a THIRD-PARTY MCP
//! server's elicitation, a `question` card with the `elicitation` sidecar.
//! The question card's chrome with the asking SERVER as its label (the
//! plug beside its name), the message, the device-built surface (the form,
//! or the URL review) in the plan card's frame WITHOUT its `Preview`
//! caption, then ONE end-aligned button row: form = Cancel (ghost) ·
//! Decline (outline) · Submit (the small primary, disabled while
//! `elicitation_missing` names a field, `copy.missing` under the row); url
//! = Decline · I'm done (outline) · Open link (the small primary: the
//! system browser, answers nothing). Answered = closed: the surface inert
//! at its answered data, the buttons gone, THE answered line names the pick.

use domain::elicitation_card::{self as model, copy, ElicitationMode, ElicitationSidecar};
use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, Disableable as _, Icon, Sizable as _,
};
use serde_json::json;

use crate::controls::WebText as _;
use crate::icons::registry;
use crate::markdown::MarkdownView;
use crate::plan_card::QuestionFacts;

/// The header's label: the plug + the asking server's name (the question
/// card's accent, `text-xs` medium, truncated).
fn server_label(server: &str, meta: Option<AnyElement>) -> AnyElement {
    let accent = theme::tokens::YELLOW.to_hsla();
    let icon = registry::concept_by_name(model::icons::SERVER).unwrap_or(registry::SETTINGS_MCP);
    h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            h_flex()
                .id(SharedString::from(model::test_ids::SERVER))
                .min_w_0()
                .gap_1()
                .items_center()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(accent)
                .child(Icon::new(icon).with_size(px(12.)).flex_shrink_0().text_color(accent))
                .child(div().min_w_0().truncate().child(SharedString::from(server.to_string()))),
        )
        .children(meta.map(|meta| div().ml_auto().flex_shrink_0().child(meta)))
        .into_any_element()
}

fn button(test_id: &str, message_id: &str, label: &str) -> Button {
    let id = format!("{test_id}-{message_id}");
    let selector = id.clone();
    Button::new(SharedString::from(id))
        .debug_selector(move || selector)
        .small()
        .cursor_pointer()
        .label(label.to_string())
}

/// THE answer path with the pick (`answers: [label]`) and, for Submit, the
/// surface's data now (`surfaceData`).
fn send(message_id: &str, label: &str, surface_data: Option<serde_json::Value>, cx: &mut App) {
    let input = api::sessions::AnswerInput {
        message_id: message_id.to_string(),
        answer: Some(label.to_string()),
        answers: Some(json!([label])),
        surface_data,
        ..Default::default()
    };
    crate::session_chat::answer_card(input, cx);
}

/// A label-only answer button (Cancel, Decline, I'm done).
fn answer_button(test_id: &str, message_id: &str, label: &'static str) -> Button {
    let id = message_id.to_string();
    button(test_id, message_id, label).on_click(move |_, _, cx| {
        cx.stop_propagation();
        send(&id, label, None, cx);
    })
}

/// The waiting card's ONE end-aligned button row (FormActions' order).
fn actions(facts: &QuestionFacts, elicitation: &ElicitationSidecar, cx: &App) -> AnyElement {
    let message_id = facts.message_id;
    let row = h_flex().flex_wrap().justify_end().gap_2();
    let (row, missing) = match elicitation.mode {
        ElicitationMode::Url => {
            let url = elicitation.url.clone();
            let open = button(model::test_ids::OPEN_LINK, message_id, copy::OPEN_LINK)
                .primary()
                .disabled(url.is_none())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    // No prefetch, never an in-app view: the system
                    // browser, on the press only; the answer is I'm done.
                    if let Some(url) = url.as_deref() {
                        cx.open_url(url);
                    }
                });
            (
                row.child(answer_button(model::test_ids::DECLINE, message_id, copy::DECLINE).outline())
                    .child(answer_button(model::test_ids::DONE, message_id, copy::DONE).outline())
                    .child(open),
                false,
            )
        }
        ElicitationMode::Form => {
            let data = crate::plan_card::live_surface_data(message_id, facts.surface, cx);
            let missing = !model::elicitation_missing(&elicitation.required, data.as_ref()).is_empty();
            let (id, surface, required) = (message_id.to_string(), facts.surface.cloned(), elicitation.required.clone());
            let submit = button(model::test_ids::SUBMIT, message_id, copy::SUBMIT)
                .primary()
                .disabled(missing)
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let data = crate::plan_card::live_surface_data(&id, surface.as_ref(), cx);
                    if !model::elicitation_missing(&required, data.as_ref()).is_empty() {
                        return;
                    }
                    send(&id, copy::SUBMIT, data, cx);
                });
            (
                row.child(answer_button(model::test_ids::CANCEL, message_id, copy::CANCEL).ghost())
                    .child(answer_button(model::test_ids::DECLINE, message_id, copy::DECLINE).outline())
                    .child(submit),
                missing,
            )
        }
    };
    v_flex()
        .mt_3()
        .w_full()
        .items_end()
        .gap_1()
        .child(row)
        .when(missing, |column| {
            column.child(
                div()
                    .text_2xs()
                    .text_color(theme::tokens::YELLOW.to_hsla())
                    .child(SharedString::from(copy::MISSING)),
            )
        })
        .into_any_element()
}

/// While the form waits, a repaint of its surface that flips "a required
/// field is empty" repaints the card (Submit, the missing line).
fn watch_form(facts: &QuestionFacts, elicitation: &ElicitationSidecar, cx: &mut App) {
    if elicitation.mode != ElicitationMode::Form || elicitation.required.is_empty() {
        return;
    }
    let required = elicitation.required.clone();
    let mut last: Option<bool> = None;
    crate::plan_card::watch_surface(
        facts.message_id,
        move |view, cx| {
            let missing = !model::elicitation_missing(&required, Some(view.read(cx).surface().data())).is_empty();
            if last != Some(missing) {
                last = Some(missing);
                cx.refresh_windows();
            }
        },
        cx,
    );
}

/// THE elicitation card IN THE CONVERSATION (the question card's place,
/// [`crate::session_chat::ask_row_card_with`]): `answer` = the reader may
/// answer (else read-only, the surface inert); `question` = the server's
/// message (`None` when blank); `meta` trails the label,
/// `dismiss` = the waiting card's × (= cancel).
#[allow(clippy::too_many_arguments)] // the row's facts + the painter's
pub(crate) fn elicitation_card(
    question: Option<MarkdownView>,
    facts: QuestionFacts,
    elicitation: &ElicitationSidecar,
    answer: bool,
    meta: Option<AnyElement>,
    dismiss: Option<AnyElement>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let message_id = facts.message_id;
    let answering = crate::session_chat::answering(message_id, cx);
    let interactive = answer && !facts.answered && !answering;
    crate::plan_card::prepare_surface(message_id, facts.surface, facts.answered, facts.surface_data, window, cx);
    if interactive {
        watch_form(&facts, elicitation, cx);
    }
    let controls = if facts.answered {
        let pick = facts.answers.and_then(|answers| answers.first()).map(String::as_str).or(facts.answer);
        crate::steer_viewer::ask_resolved_line(pick, false, cx)
    } else if answering {
        crate::session_chat::card_answering(cx)
    } else if interactive {
        actions(&facts, elicitation, cx)
    } else {
        div().into_any_element()
    };
    let column = v_flex()
        .w_full()
        .min_w_0()
        .children(question.map(|question| crate::steer_viewer::ask_body_text(cx).child(question)))
        .when_some(facts.surface, |column, surface| {
            column.child(crate::plan_card::surface_frame(message_id, surface, false, !interactive, cx))
        })
        .child(controls);
    let card = crate::steer_viewer::ask_card_chrome_with(false, None, Some(server_label(&elicitation.server, meta)), None, dismiss, column, cx);
    crate::waiting_cards::ringed(card, message_id, cx)
        .id(SharedString::from(format!("{}-{message_id}", model::test_ids::CARD)))
        .into_any_element()
}
