//! #EXP-1561 / #EXP-1562 (`fixtures/waiting-cards.json` ×4; web
//! `ask-card.tsx` `AskDismissButton` / `AskDismissedCard`,
//! `session-chat-view.tsx` `ChatWaitingChips`): a WAITING question or
//! plan card's ×, the DISMISSED card's one quiet line, the ring a reveal
//! leaves on the card it scrolled to and (#EXP-1785) the session screen
//! header's waiting chips: the tree's waiting plans AND questions, one
//! StripChip each at the start of the header's trailing slot (the IDE is
//! always the wide `chips` layout).

use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, point, px, AnyElement, App, ClickEvent, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::ask_dock::{ask_dock_title, AskDockKind};
use domain::session_tree::SessionChatRow;
use domain::waiting_cards::{waiting_chips, WaitingCardInput, WaitingChipsLayout, WAITING_CARDS};

use crate::icons::registry;

/// The ring's fade after `constants.highlightMs`: a few STEPPED repaints,
/// never an every-frame animation (the gpui frame cost), then done.
const FADE_STEPS: u32 = 5;
const FADE_STEP: Duration = Duration::from_millis(100);

/// The card a header chip scrolled to, since when; the task that steps its
/// fade (a newer highlight drops the older one).
#[derive(Default)]
struct CardHighlight {
    id: Option<String>,
    at: Option<Instant>,
    _fade: Option<Task<()>>,
}

impl gpui::Global for CardHighlight {}

/// Ring the card `id` (the focus ring, 2 px) for `highlightMs`, then fade
/// it out in [`FADE_STEPS`] repaints and stop.
pub(crate) fn highlight(id: &str, cx: &mut App) {
    let fade = cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(WAITING_CARDS.constants.highlight_ms)).await;
        for _ in 0..FADE_STEPS {
            let _ = cx.update(|cx| cx.refresh_windows());
            cx.background_executor().timer(FADE_STEP).await;
        }
        let _ = cx.update(|cx| {
            let state = cx.default_global::<CardHighlight>();
            state.id = None;
            state.at = None;
            cx.refresh_windows();
        });
    });
    let state = cx.default_global::<CardHighlight>();
    state.id = Some(id.to_string());
    state.at = Some(Instant::now());
    state._fade = Some(fade);
    cx.refresh_windows();
}

/// The card a chip highlighted, while its ring shows.
pub(crate) fn highlighted_id(cx: &App) -> Option<&str> {
    cx.try_global::<CardHighlight>()?.id.as_deref()
}

/// The ring's opacity on the card `id` now (`None` = no ring): full for
/// `highlightMs`, then one step down per [`FADE_STEP`].
pub(crate) fn ring_alpha(id: &str, cx: &App) -> Option<f32> {
    let state = cx.try_global::<CardHighlight>()?;
    if state.id.as_deref() != Some(id) {
        return None;
    }
    let elapsed = state.at?.elapsed();
    let on = Duration::from_millis(WAITING_CARDS.constants.highlight_ms);
    let Some(past) = elapsed.checked_sub(on) else { return Some(1.) };
    let step = (past.as_millis() / FADE_STEP.as_millis()) as u32 + 1;
    (step <= FADE_STEPS).then(|| 1. - step as f32 / (FADE_STEPS + 1) as f32)
}

/// A card frame wearing the ring while [`ring_alpha`] says so (web
/// `ring-2 ring-ring`: a 2 px spread outside the frame, no layout shift).
pub(crate) fn ringed<E: gpui::Styled>(card: E, id: &str, cx: &App) -> E {
    match ring_alpha(id, cx) {
        Some(alpha) => card.shadow(vec![gpui::BoxShadow {
            color: cx.theme().ring.opacity(alpha),
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(2.),
            inset: false,
        }]),
        None => card,
    }
}

/// THE card's × (`icons.dismiss`, tooltip `copy.dismiss`, `testIds.dismiss`):
/// a 24 px ghost icon button at the top-right of the header row (web
/// `-mt-0.5 -mr-1`, overhanging the line instead of growing it). A press
/// dismisses at once (no confirm) through THE answer path.
pub(crate) fn dismiss_button(message_id: &str, cx: &App) -> AnyElement {
    let message = message_id.to_string();
    dismiss_button_with(message_id, move |_, cx| crate::session_chat::dismiss_card(message.clone(), cx), cx)
}

/// [`dismiss_button`] with its own press (the session view's engine ask,
/// which locks its feed card while the dismiss is on its way).
pub(crate) fn dismiss_button_with(
    message_id: &str,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let id = SharedString::from(format!("{}-{message_id}", WAITING_CARDS.test_ids.dismiss));
    let muted = cx.theme().muted_foreground;
    div()
        .id(id.clone())
        .flex_shrink_0()
        .mt(px(-2.))
        .mr(px(-4.))
        .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new(WAITING_CARDS.copy.dismiss.clone()).build(window, cx))
        .child(
            crate::controls::ghost_icon_button(
                SharedString::from(format!("{id}-x")),
                Icon::new(registry::UI_CLOSE).xsmall().text_color(muted),
                cx,
            )
            .accessibility_label(WAITING_CARDS.copy.dismiss.clone())
            .size(px(24.))
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                cx.stop_propagation();
                on_press(window, cx);
            }),
        )
        .into_any_element()
}

/// A DISMISSED card (`testIds.dismissed`): THE card frame at 80 % holding
/// ONE quiet muted line, `copy.dismissed` + the card's title (truncated);
/// no body, no ×, no options (the superseded line's look).
pub(crate) fn dismissed_card(plan: bool, text: &str, message_id: &str, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let title = ask_dock_title(text);
    let line = h_flex()
        .w_full()
        .min_w_0()
        // The 16 px line centred on the kind glyph (which sits 4 px down
        // when the card has no heading).
        .mt(px(3.))
        .gap_1p5()
        .items_center()
        .text_xs()
        .text_color(muted)
        .child(
            div()
                .flex_shrink_0()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(SharedString::from(WAITING_CARDS.copy.dismissed.clone())),
        )
        .when_some((!title.is_empty()).then_some(title), |line, title| {
            line.child(div().min_w_0().truncate().child(SharedString::from(title)))
        });
    let card = crate::steer_viewer::ask_card_chrome(plan, None, None, None, v_flex().w_full().min_w_0().child(line), cx)
        .opacity(0.8);
    ringed(card, message_id, cx)
        .id(SharedString::from(format!("{}-{message_id}", WAITING_CARDS.test_ids.dismissed)))
        .into_any_element()
}

/// The kind's concept glyph (`icons[kind]`) in the ask-dock tint (a plan
/// primary, a question yellow), web `size-3.5`.
pub(crate) fn kind_icon(kind: AskDockKind, cx: &App) -> Icon {
    let (glyph, tint) = match kind {
        AskDockKind::Plan => (registry::CODING_PLAN, cx.theme().primary),
        AskDockKind::Question => (registry::UI_HELP, theme::tokens::YELLOW.to_hsla()),
    };
    Icon::new(glyph).with_size(px(14.)).flex_shrink_0().text_color(tint)
}

/// What a chip's press does: the card it stands for.
pub(crate) type OnWaitingPick = std::rc::Rc<dyn Fn(&WaitingCardInput, &mut Window, &mut App)>;

/// THE strip chip box at rest (web `stripTabClass`, entity-chip.json tab
/// size): 32 px tall, 6 px radius, 10 px sides, gap 4, `text_sm`, the row
/// hairline, muted text that turns foreground under the pointer.
fn strip_chip(id: SharedString, cx: &App) -> gpui::Stateful<gpui::Div> {
    let (foreground, muted) = (cx.theme().foreground, cx.theme().muted_foreground);
    div()
        .id(id)
        .h(px(32.))
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(4.))
        .px(px(10.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
        .text_sm()
        .font_weight(gpui::FontWeight::NORMAL)
        .cursor_pointer()
        .text_color(muted)
        .hover(move |style| style.text_color(foreground))
}

/// #EXP-1785 THE header's WAITING chips (`waitingChips`, `chips` layout):
/// THE StripChip per card in Conversation order — the tinted kind glyph,
/// the clipped label, the full title as its tooltip — side by side in a row
/// that shrinks (labels truncating) first. `None` when none waits (nothing
/// drawn, no placeholder).
pub(crate) fn header_chips(cards: &[WaitingCardInput], on_pick: OnWaitingPick, cx: &App) -> Option<AnyElement> {
    let chips = waiting_chips(cards, false);
    if chips.layout == WaitingChipsLayout::Hidden {
        return None;
    }
    let mut row = h_flex()
        .id("session-screen-waiting-chips")
        .min_w_0()
        .gap_1()
        .items_center()
        .overflow_x_hidden();
    // Web `shrink-[2]`: the chips give way first.
    row.style().flex_shrink = Some(2.);
    for (chip, card) in chips.chips.into_iter().zip(cards.iter().cloned()) {
        let title = Some(ask_dock_title(&card.text)).filter(|title| !title.is_empty()).unwrap_or_else(|| chip.label.clone());
        let on_pick = on_pick.clone();
        let mut element = strip_chip(SharedString::from(format!("{}-{}", WAITING_CARDS.test_ids.chip, chip.id)), cx)
            .child(kind_icon(chip.kind, cx))
            .child(div().min_w_0().truncate().child(SharedString::from(chip.label)))
            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(title.clone()).build(window, cx))
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| on_pick(&card, window, cx));
        element.style().flex_shrink = Some(1.);
        row = row.child(element);
    }
    Some(row.into_any_element())
}

/// #EXP-1785: the TREE's waiting cards for the chips: ask-dock
/// `chat_pending_asks` over the chat view's rows (the root and every
/// session under it, plans included, Conversation order), each with its
/// asking session.
pub(crate) fn chat_waiting_cards(rows: &[SessionChatRow]) -> Vec<WaitingCardInput> {
    domain::ask_dock::chat_pending_asks(rows, |_| None)
        .into_iter()
        .map(|ask| {
            let session_id = rows
                .iter()
                .find_map(|row| match row {
                    SessionChatRow::Plan { id, session_id, .. } | SessionChatRow::Question { id, session_id, .. }
                        if *id == ask.id =>
                    {
                        Some(session_id.clone())
                    }
                    _ => None,
                })
                .unwrap_or_default();
            WaitingCardInput { id: ask.id, kind: ask.kind, text: ask.text, session_id }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1785: plans are chips again; every card keeps its asking session.
    #[test]
    fn the_chips_list_plans_and_questions_of_the_tree() {
        let rows = [
            SessionChatRow::Plan {
                id: "p".into(),
                session_id: "c1".into(),
                plan: "# Plan: one".into(),
                questions: Vec::new(),
                surface: None,
                decision: None,
                fresh: false,
                answers: None,
                feedback: None,
                surface_data: None,
            },
            SessionChatRow::Question {
                id: "q".into(),
                session_id: "r".into(),
                question: "Which?".into(),
                options: Vec::new(),
                custom: true,
                multi: false,
                surface: None,
                answer: None,
                answers: None,
                surface_data: None,
                answered: false,
                dismissed: None,
            },
        ];
        let got: Vec<(String, String)> =
            chat_waiting_cards(&rows).into_iter().map(|card| (card.id, card.session_id)).collect();
        assert_eq!(got, [("p".to_string(), "c1".to_string()), ("q".to_string(), "r".to_string())]);
    }

    #[gpui::test]
    fn the_ring_steps_out_and_ends(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            highlight("c1", cx);
            assert_eq!(ring_alpha("c1", cx), Some(1.));
            assert_eq!(ring_alpha("c2", cx), None);
        });
        cx.executor().advance_clock(Duration::from_millis(WAITING_CARDS.constants.highlight_ms) + FADE_STEP * (FADE_STEPS + 1));
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(ring_alpha("c1", cx), None, "the fade ends and the highlight clears"));
    }
}
