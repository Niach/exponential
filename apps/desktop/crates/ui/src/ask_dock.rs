//! THE ask dock ×desktop (`fixtures/ask-dock.json` ×4; web
//! `packages/ui/src/ask-dock.tsx`, model [`domain::ask_dock`]): the
//! question/plan cards WAITING on the viewer leave the transcript and take
//! the composer's slot, in the session view (`steer_viewer`) and the thread
//! (`thread_screen`) alike. `single`/`focused` = THE big card (the host
//! paints it: [`crate::steer_viewer::ask_card_chrome`] + its options, the
//! [`back_button`] leading the header when focused, the body inline in
//! [`dock_body`], never scrolling on its own); `stack` = [`stack_card`]s `stackGap` apart, a click
//! focuses one. A swap (stack ⇄ a card) animates the dock's height over
//! motion `standard` with the standard easing and fades the new content in
//! (decelerate). gpui has no reduce-motion signal (`theme::motion`), so
//! the animation always runs.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use domain::ask_dock::{AskDockKind, AskDockRow, AskDockView, ASK_DOCK};
use gpui::{
    canvas, div, px, AnyElement, App, ClickEvent, Div, ElementId,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{button::{Button, ButtonVariants as _}, h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::controls::WebControl as _;
use crate::icons::registry;

/// The dock's per-host state: the focused card (a click out of the stack)
/// and the swap animation's bookkeeping.
pub(crate) struct AskDockState {
    focused: Option<String>,
    /// The content painted last frame (`stack` or `card:<id>`).
    content_key: Option<String>,
    /// A running swap: when it began and the height it grows from.
    anim: Option<(Instant, Pixels)>,
    /// The content's height, read back by the paint probe.
    measured: Rc<Cell<Pixels>>,
}

impl Default for AskDockState {
    fn default() -> Self {
        Self { focused: None, content_key: None, anim: None, measured: Rc::new(Cell::new(px(0.))) }
    }
}

impl AskDockState {
    /// A state that opens focused on `id` (the styleguide's focused demo).
    pub(crate) fn focused_on(id: impl Into<String>) -> Self {
        Self { focused: Some(id.into()), ..Self::default() }
    }

    pub(crate) fn focused(&self) -> Option<&str> {
        self.focused.as_deref()
    }

    /// A click on a compact card.
    pub(crate) fn focus(&mut self, id: impl Into<String>) {
        self.focused = Some(id.into());
    }

    /// The back arrow.
    pub(crate) fn back(&mut self) {
        self.focused = None;
    }
}

/// The content key a swap is detected on (web `contentKey`).
fn content_key(view: &AskDockView) -> String {
    match &view.card_id {
        Some(id) => format!("card:{id}"),
        None => "stack".to_string(),
    }
}

/// The dock around its `content` (the big card, or [`stack`]): measures
/// it, and while a swap runs clips to the eased height and fades the new
/// content in. `None` mode = nothing (the composer shows).
pub(crate) fn dock(
    state: &mut AskDockState,
    view: &AskDockView,
    content: AnyElement,
    window: &Window,
) -> AnyElement {
    let key = content_key(view);
    if state.content_key.as_deref() != Some(key.as_str()) {
        let from = state.measured.get();
        if state.content_key.is_some() && from > px(0.) {
            state.anim = Some((Instant::now(), from));
        }
        state.content_key = Some(key);
    }
    let measured = state.measured.clone();
    let probe = canvas(move |bounds, _, _| measured.set(bounds.size.height), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full();
    let mut inner = div().relative().w_full().min_w_0().child(content).child(probe);
    let mut outer = div()
        .id(SharedString::from(ASK_DOCK.test_ids.dock.clone()))
        .w_full()
        .min_w_0();
    if let Some((start, from)) = state.anim {
        let duration = theme::motion::STANDARD.as_secs_f32();
        let progress = start.elapsed().as_secs_f32() / duration;
        if progress >= 1. {
            state.anim = None;
        } else {
            let to = state.measured.get();
            let eased = theme::motion::standard()(progress);
            outer = outer.h(from + (to - from) * eased).overflow_hidden();
            inner = inner.opacity(theme::motion::decelerate()(progress));
            window.request_animation_frame();
        }
    }
    outer.child(inner).into_any_element()
}

/// The big card's body. NEW-6 (N-1): INLINE, never a scroll area of its
/// own (no cap, no scrolled edge): the card grows, the conversation scrolls.
pub(crate) fn dock_body(id: impl Into<ElementId>, body: impl IntoElement, _window: &Window) -> Stateful<Div> {
    div().id(id).w_full().min_w_0().child(body)
}

/// The focused card's back arrow (ghost `icon-xs`, `ui-back`), leading the
/// card's header row ([`crate::steer_viewer::ask_card_chrome`]'s `back`).
pub(crate) fn back_button(
    id: impl Into<ElementId>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    Button::new(id)
        .ghost()
        .web_icon_xs()
        .icon(registry::UI_BACK)
        .tooltip(ASK_DOCK.copy.back.clone())
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx);
        })
        .into_any_element()
}

/// One compact card of the stack (web `AskDockStackCard`): the AskCard
/// chrome (icon + label + the session caption, muted and truncating) over
/// the one-line title (text-sm, foreground/90, truncating) and the open
/// chevron. The caller wires the click (focus).
pub(crate) fn stack_card(row: &AskDockRow, cx: &App) -> Stateful<Div> {
    let muted = cx.theme().muted_foreground;
    let caption = row.caption.clone().map(|caption| {
        div()
            .min_w_0()
            .truncate()
            .text_xs()
            .text_color(muted)
            .child(SharedString::from(caption))
            .into_any_element()
    });
    let body = h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(cx.theme().foreground.opacity(0.9))
                .child(SharedString::from(row.title.clone())),
        )
        .child(Icon::new(registry::UI_CHEVRON_RIGHT).with_size(px(14.)).flex_shrink_0().text_color(muted));
    crate::steer_viewer::ask_card_chrome(
        row.kind == AskDockKind::Plan,
        Some(SharedString::from(row.label.clone())),
        caption,
        None,
        body,
        cx,
    )
    .id(SharedString::from(format!("{}-{}", ASK_DOCK.test_ids.row, row.id)))
    .cursor_pointer()
    .hover(|style| style.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
}

/// The stack: every row's [`stack_card`], `stackGap` apart, oldest first;
/// a click calls `on_open` with the row's id.
pub(crate) fn stack(
    rows: &[AskDockRow],
    on_open: Rc<dyn Fn(&str, &mut Window, &mut App)>,
    cx: &App,
) -> AnyElement {
    v_flex()
        .w_full()
        .min_w_0()
        .gap(px(domain::ask_dock::stack_gap()))
        .children(rows.iter().map(|row| {
            let id = row.id.clone();
            let on_open = on_open.clone();
            stack_card(row, cx).on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_open(&id, window, cx);
            })
        }))
        .into_any_element()
}

/// Whether `view` holds anything ([`dock`] paints nothing for `none`).
pub(crate) fn shows(view: &AskDockView) -> bool {
    view.mode != domain::ask_dock::AskDockMode::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ask_dock::{ask_dock, AskDockPending};

    fn pending(id: &str, at: i64) -> AskDockPending {
        AskDockPending {
            id: id.into(),
            kind: AskDockKind::Plan,
            text: "1. One".into(),
            session_name: None,
            created_at: at,
        }
    }

    /// A swap is keyed on the card (or the stack), never on the rows.
    #[test]
    fn the_content_key_follows_the_card() {
        let single = ask_dock(&[pending("p1", 1)], None);
        assert_eq!(content_key(&single), "card:p1");
        let stack = ask_dock(&[pending("p1", 1), pending("p2", 2)], None);
        assert_eq!(content_key(&stack), "stack");
        let focused = ask_dock(&[pending("p1", 1), pending("p2", 2)], Some("p2"));
        assert_eq!(content_key(&focused), "card:p2");
        assert!(shows(&stack) && !shows(&ask_dock(&[], None)));
    }

    #[test]
    fn focus_and_back_round_trip() {
        let mut state = AskDockState::default();
        state.focus("p2");
        assert_eq!(state.focused(), Some("p2"));
        state.back();
        assert_eq!(state.focused(), None);
        assert_eq!(AskDockState::focused_on("q1").focused(), Some("q1"));
    }
}
