//! #EXP-1561 / #EXP-1562 (`fixtures/waiting-cards.json` ×4; web
//! `ask-card.tsx` `AskDismissButton` / `AskDismissedCard`,
//! `session-chat-view.tsx` `ChatWaitingChips`): a WAITING question or
//! plan card's ×, the DISMISSED card's one quiet line and the ring a reveal
//! leaves on the card it scrolled to. #EXP-1732: the chat strip (and its
//! waiting chips) is gone.

use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, point, px, AnyElement, App, ClickEvent, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::ask_dock::ask_dock_title;
use domain::waiting_cards::WAITING_CARDS;

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

/// Scroll the list row `ix` holding the card `id` to the CENTRE of the
/// viewport, then ring the card: the row goes to the top first (it may not
/// be measured yet), and one frame later the measured row is centred.
pub(crate) fn reveal_in_list(list: &gpui::ListState, ix: usize, id: &str, cx: &mut App) {
    list.scroll_to(gpui::ListOffset { item_ix: ix, offset_in_item: px(0.) });
    cx.refresh_windows();
    let (list, id) = (list.clone(), id.to_string());
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(32)).await;
        let _ = cx.update(|cx| {
            if let Some(bounds) = list.bounds_for_item(ix) {
                let viewport = list.viewport_bounds();
                list.scroll_by(bounds.center().y - viewport.center().y);
            }
            highlight(&id, cx);
        });
    })
    .detach();
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

#[cfg(test)]
mod tests {
    use super::*;

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
