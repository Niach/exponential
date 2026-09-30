//! EXP-1031 — THE toast, the IDE twin of `@exp/ui` `toast.tsx` (sonner), iOS
//! `Toast.swift` and Android `Toast.kt`, all drawn from ONE contract:
//! `packages/domain-contract/fixtures/toast-stack.json`.
//!
//! The IDE owns the whole layer: one [`ToastLayer`] per window (found by
//! window id, rendered by every root via [`render_layer`]) keeps the toasts
//! in gpui-base's PURE lifecycle model (`ToastManager`, enter/exit phases +
//! the pausable 4 s clock) and draws them itself — bottom-right, 24 from the
//! edges, 356 wide; newest in front, the older ones peeking 14 above and
//! shrinking 5% per rank, 3 visible; hover EXPANDS them 14 apart and pauses
//! the clock. The geometry is [`stack_layout`] (fixture-locked); every
//! movement is a `theme::motion` token (enter STANDARD/decelerate, exit
//! FAST/accelerate, stack changes SLOW/standard) — never the crate's easing.
//!
//! The card is the kind colour on the ICON alone (registry concepts
//! `ui-success` / `ui-error` / `ui-info` / `ui-warning`), the text in the
//! foreground, the opaque glass card fill under the card hairline at radius
//! lg, NO shadow, and an always-visible `ui-close` glyph. [`card`] draws the
//! same face for the styleguide; both go through [`face`], so they cannot
//! drift.

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, Context, Div,
    ElementId, Entity, Global, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Task, Window, WindowId,
};
use gpui_base::{
    transition, ElementExt as _, ToastManager, ToastMotion, ToastOptions, ToastTransitionStatus, Transition,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use theme::tokens as t;

use crate::icons::{registry, ExpIcon};

/// The contract's numbers (`toast-stack.json` `constants`), locked by the
/// tests below.
pub(crate) mod constants {
    use std::time::Duration;

    pub(crate) const WIDTH: f32 = 356.;
    pub(crate) const GAP: f32 = 14.;
    pub(crate) const PEEK: f32 = 14.;
    pub(crate) const SCALE_STEP: f32 = 0.05;
    pub(crate) const VISIBLE: usize = 3;
    pub(crate) const VIEWPORT_OFFSET: f32 = 24.;
    /// `durationMs`: the auto-dismiss clock, paused while the stack is hovered.
    pub(crate) const DURATION: Duration = Duration::from_millis(4000);
    /// How far a card travels while it enters or leaves.
    pub(crate) const TRAVEL: f32 = 14.;
}

/// The lifecycle model's motion: our tokens for the enter (STANDARD) and
/// exit (FAST) phases, the fixture's numbers for the stack.
pub(crate) fn motion() -> ToastMotion {
    ToastMotion {
        duration: theme::motion::STANDARD,
        exit_duration: theme::motion::FAST,
        collapsed_peek: px(constants::PEEK),
        expanded_gap: px(constants::GAP),
        collapsed_scale_step: constants::SCALE_STEP,
        collapsed_visible: constants::VISIBLE,
    }
}

/// A toast's kind: it colours the icon, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Success,
    Error,
    Info,
    Warning,
}

impl ToastKind {
    /// Every kind, in the contract's `kinds` order.
    pub(crate) const ALL: [ToastKind; 4] = [Self::Success, Self::Error, Self::Info, Self::Warning];

    /// The contract spelling (`constants.kinds`).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Info => "info",
            Self::Warning => "warning",
        }
    }

    /// The registry concept the kind draws.
    pub(crate) fn icon(self) -> ExpIcon {
        match self {
            Self::Success => registry::UI_SUCCESS,
            Self::Error => registry::UI_ERROR,
            Self::Info => registry::UI_INFO,
            Self::Warning => registry::UI_WARNING,
        }
    }

    /// The icon's colour — the ONLY place a toast wears its kind.
    pub(crate) fn color(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        match self {
            Self::Success => theme.success,
            Self::Error => theme.danger,
            Self::Info => theme.info,
            Self::Warning => theme.warning,
        }
    }
}

/// The toast's one button: it runs `on_click` and dismisses the toast.
#[derive(Clone)]
pub(crate) struct ToastAction {
    pub label: SharedString,
    pub on_click: Rc<dyn Fn(&mut Window, &mut App)>,
}

/// One toast: a sentence (`title`), an optional `description`, an optional
/// `action`, and its `kind`.
#[derive(Clone)]
pub(crate) struct Toast {
    pub kind: ToastKind,
    pub title: SharedString,
    pub description: Option<SharedString>,
    pub action: Option<ToastAction>,
}

impl Toast {
    pub(crate) fn new(kind: ToastKind, title: impl Into<SharedString>) -> Self {
        Self { kind, title: title.into(), description: None, action: None }
    }

    pub(crate) fn success(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Success, title)
    }

    pub(crate) fn error(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Error, title)
    }

    pub(crate) fn info(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Info, title)
    }

    pub(crate) fn warning(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Warning, title)
    }

    #[allow(dead_code)] // part of the shared toast API; the styleguide uses it
    pub(crate) fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[allow(dead_code)] // part of the shared toast API; the styleguide uses it
    pub(crate) fn action(
        mut self,
        label: impl Into<SharedString>,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some(ToastAction { label: label.into(), on_click: Rc::new(on_click) });
        self
    }
}

/// Push `toast` onto `window`'s stack (the window's [`ToastLayer`]).
pub(crate) fn show(toast: Toast, window: &mut Window, cx: &mut App) {
    let layer = layer_for_window(window, cx);
    layer.update(cx, |layer, cx| {
        layer.push(toast, cx);
    });
}

/// Push onto the active (else primary) window — for callers without one.
pub(crate) fn show_in_active_window(toast: Toast, cx: &mut App) {
    crate::navigation::on_active_window(cx, move |window, cx| show(toast, window, cx));
}

pub(crate) fn success(title: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    show(Toast::success(title), window, cx);
}

pub(crate) fn error(title: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    show(Toast::error(title), window, cx);
}

#[allow(dead_code)] // the shared toast API: no IDE caller raises an info toast today
pub(crate) fn info(title: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    show(Toast::info(title), window, cx);
}

pub(crate) fn warning(title: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    show(Toast::warning(title), window, cx);
}

/// The card: opaque glass card fill, card hairline, radius lg, no shadow,
/// 16 in from every edge.
fn chrome<T: Styled>(el: T, cx: &App) -> T {
    el.bg(cx.theme().popover)
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .rounded(px(t::radius::LG))
        .shadow_none()
        .p_4()
}

fn kind_icon(kind: ToastKind, cx: &App) -> Icon {
    Icon::new(kind.icon()).size(px(16.)).text_color(kind.color(cx))
}

/// Title (foreground, medium) over the description (muted).
fn body(toast: &Toast, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_sm()
                .line_height(px(LINE))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(toast.title.clone()),
        )
        .when_some(toast.description.clone(), |this, description| {
            this.child(div().text_sm().text_color(theme.muted_foreground).child(description))
        })
}

type Dismiss = Rc<dyn Fn(&mut Window, &mut App)>;

/// The title's line height: the icon, the action and the close box centre
/// on it (web: `items-start`, each 20 tall).
const LINE: f32 = 20.;

/// THE face, live or at rest — web's row inside the card's 16 padding:
/// [icon 16 in the kind colour][title + description, flex 1][action][close
/// 20 box, 14 glyph, muted until hovered], top-aligned with the icon, the
/// action and the close box centred on the title's line. `content` fades
/// everything inside the card (a card behind the front one in a collapsed
/// stack shows only its edge, like the web's `data-[front=false]`).
/// `dismiss` = the live layer's close; the action runs its own handler,
/// then dismisses.
fn face(toast: &Toast, content: f32, dismiss: Option<Dismiss>, cx: &App) -> Div {
    let theme = cx.theme();
    let line = || h_flex().flex_none().h(px(LINE)).items_center().opacity(content);
    let action = toast.action.clone().map(|action| {
        let button = Button::new("toast-action").label(action.label.clone()).primary().small();
        let button = match dismiss.clone() {
            Some(dismiss) => button.on_click(move |_, window, cx| {
                (action.on_click)(window, cx);
                dismiss(window, cx);
            }),
            None => button,
        };
        line().child(button)
    });
    let close = div()
        .id("toast-close")
        .size(px(LINE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(t::radius::SM))
        .cursor_pointer()
        .text_color(theme.muted_foreground)
        .hover(|this| this.text_color(theme.foreground))
        .child(Icon::new(registry::UI_CLOSE).size(px(14.)))
        .when_some(dismiss, |this, dismiss| {
            this.on_click(move |_, window, cx| dismiss(window, cx))
        });
    chrome(h_flex().w(px(constants::WIDTH)).items_start().gap_3(), cx)
        .child(line().child(kind_icon(toast.kind, cx)))
        .child(v_flex().flex_1().min_w_0().opacity(content).child(body(toast, cx)))
        .children(action)
        .child(line().child(close))
}

/// The toast AT REST as a plain element (the styleguide's specimen): the
/// same [`face`] the live layer draws.
pub(crate) fn card(toast: &Toast, cx: &App) -> Div {
    face(toast, 1., None, cx)
}

/// A card BEHIND the front one in a collapsed stack: the edge alone.
pub(crate) fn card_back(toast: &Toast, cx: &App) -> Div {
    face(toast, 0., None, cx)
}

// ---------------------------------------------------------------------------
// The live layer
// ---------------------------------------------------------------------------

/// How often the lifecycle clock advances while a toast is up.
const TICK: Duration = Duration::from_millis(50);
/// A card's height before its first measurement.
const ESTIMATED_HEIGHT: f32 = 54.;

/// Where a card was last placed: its bottom edge above the stack box's,
/// its side inset, whether it draws, whether its content shows, and — for a
/// card behind the front one in a collapsed stack — the front card's height
/// it is drawn at. An ENDING card keeps the place it had, the rest re-stack
/// without it.
#[derive(Clone, Copy)]
struct Place {
    bottom: f32,
    inset: f32,
    visible: bool,
    content: bool,
    clamp: Option<f32>,
}

/// One window's toast stack.
pub(crate) struct ToastLayer {
    manager: ToastManager<u64, Toast>,
    next_id: u64,
    hovered: bool,
    heights: Rc<RefCell<HashMap<u64, f32>>>,
    /// When each toast's current phase (entering / ending) began.
    phase_at: HashMap<u64, Instant>,
    places: HashMap<u64, Place>,
    ticking: bool,
    _ticker: Option<Task<()>>,
}

impl ToastLayer {
    pub(crate) fn new() -> Self {
        Self {
            manager: ToastManager::new(motion()),
            next_id: 0,
            hovered: false,
            heights: Rc::default(),
            phase_at: HashMap::new(),
            places: HashMap::new(),
            ticking: false,
            _ticker: None,
        }
    }

    /// Push a newest toast; returns its id.
    pub(crate) fn push(&mut self, toast: Toast, cx: &mut Context<Self>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let now = cx.background_executor().now();
        self.manager.push(id, toast, ToastOptions { timeout: Some(constants::DURATION) }, now);
        self.phase_at.insert(id, now);
        self.ensure_ticking(cx);
        cx.notify();
        id
    }

    /// Start `id`'s exit (close glyph, action, middle click).
    pub(crate) fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        if self.manager.dismiss(&id, now) {
            self.phase_at.insert(id, now);
            cx.notify();
        }
    }

    /// Hovering expands the stack and pauses the clock.
    pub(crate) fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.hovered != hovered {
            self.hovered = hovered;
            cx.notify();
        }
    }

    /// Mounted toasts, ending ones included.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn len(&self) -> usize {
        self.manager.len()
    }

    fn ensure_ticking(&mut self, cx: &mut Context<Self>) {
        if self.ticking {
            return;
        }
        self.ticking = true;
        self._ticker = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(TICK).await;
            match this.update(cx, |this, cx| this.tick(cx)) {
                Ok(true) => {}
                _ => break,
            }
        }));
    }

    /// Advance the lifecycle clock; `false` once the stack is empty (the
    /// ticker stops until the next push).
    fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        let now = cx.background_executor().now();
        let advance = self.manager.advance(now, self.hovered);
        for id in advance.ending {
            self.phase_at.insert(id, now);
        }
        for (id, _) in advance.removed {
            self.phase_at.remove(&id);
            self.places.remove(&id);
            self.heights.borrow_mut().remove(&id);
        }
        if advance.changed {
            cx.notify();
        }
        if self.manager.is_empty() {
            self.hovered = false;
            self.ticking = false;
            return false;
        }
        true
    }
}

fn slow() -> Transition {
    Transition::new(theme::motion::SLOW).ease(theme::motion::standard())
}

/// Progress of a phase that began at `since` and lasts `duration`, eased.
fn phase(now: Instant, since: Option<Instant>, duration: Duration, ease: impl Fn(f32) -> f32) -> (f32, bool) {
    let elapsed = since.map_or(duration, |since| now.saturating_duration_since(since));
    let raw = (elapsed.as_secs_f32() / duration.as_secs_f32()).min(1.);
    (ease(raw), raw < 1.)
}

impl Render for ToastLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.manager.is_empty() {
            return div().into_any_element();
        }
        let now = cx.background_executor().now();
        let expanded = self.hovered;

        // Lay out the live toasts (ending ones keep their last place).
        let active: Vec<u64> = self
            .manager
            .iter()
            .filter(|(_, _, status)| *status != ToastTransitionStatus::Ending)
            .map(|(id, _, _)| *id)
            .collect();
        let heights: Vec<f32> = {
            let measured = self.heights.borrow();
            active.iter().map(|id| measured.get(id).copied().unwrap_or(ESTIMATED_HEIGHT)).collect()
        };
        // Collapsed, every card takes the FRONT card's height (sonner's
        // `--initial-height`), so each older one peeks exactly `PEEK` above
        // it whatever its own height; expanded, each has its own.
        let front = active.last().copied();
        let front_height = heights.last().copied().unwrap_or(ESTIMATED_HEIGHT);
        let heights = if expanded { heights } else { vec![front_height; heights.len()] };
        let (box_height, items) = stack_layout(&heights, expanded, true);
        for ((id, item), height) in active.iter().zip(&items).zip(&heights) {
            let is_front = Some(*id) == front;
            self.places.insert(
                *id,
                Place {
                    bottom: box_height - item.offset - height,
                    inset: constants::WIDTH * (1. - item.scale) / 2.,
                    visible: item.visible,
                    content: expanded || is_front,
                    clamp: (!expanded && !is_front).then_some(front_height),
                },
            );
        }

        let layer = cx.entity().downgrade();
        let entity_id = cx.entity_id();
        let mut animating = false;
        let mut cards: Vec<AnyElement> = Vec::with_capacity(self.manager.len());
        for (&id, toast, status) in self.manager.iter() {
            let place = self.places.get(&id).copied().unwrap_or(Place {
                bottom: 0.,
                inset: 0.,
                visible: true,
                content: true,
                clamp: None,
            });
            let key = ElementId::NamedInteger("exp-toast".into(), id);
            let bottom = transition((key.clone(), "bottom"), px(place.bottom), slow(), window, cx);
            let inset = transition((key.clone(), "inset"), px(place.inset), slow(), window, cx);
            let shown = transition(
                (key.clone(), "shown"),
                if place.visible { 1f32 } else { 0. },
                slow(),
                window,
                cx,
            );
            let content = transition(
                (key.clone(), "content"),
                if place.content { 1f32 } else { 0. },
                slow(),
                window,
                cx,
            );
            // Enter: fade + rise from TRAVEL below; exit: fade + sink.
            let since = self.phase_at.get(&id).copied();
            let (fade, drop) = match status {
                ToastTransitionStatus::Starting => {
                    let (p, running) =
                        phase(now, since, theme::motion::STANDARD, theme::motion::decelerate());
                    animating |= running;
                    (p, constants::TRAVEL * (1. - p))
                }
                ToastTransitionStatus::Present => (1., 0.),
                ToastTransitionStatus::Ending => {
                    let (p, running) =
                        phase(now, since, theme::motion::FAST, theme::motion::accelerate());
                    animating |= running;
                    (1. - p, constants::TRAVEL * p)
                }
            };
            let dismiss: Dismiss = {
                let layer = layer.clone();
                Rc::new(move |_, cx| {
                    let _ = layer.update(cx, |layer, cx| layer.dismiss(id, cx));
                })
            };
            let middle = dismiss.clone();
            let measured = self.heights.clone();
            let clamped = place.clamp.is_some();
            cards.push(
                div()
                    .id(key)
                    .absolute()
                    .left(inset)
                    .right(inset)
                    .bottom(bottom - px(drop))
                    .opacity(shown * fade)
                    .when_some(place.clamp, |this, height| this.h(px(height)).overflow_hidden())
                    .on_mouse_down(MouseButton::Middle, move |_, window, cx| middle(window, cx))
                    .on_prepaint(move |bounds, _, cx| {
                        // A clamped card's bounds are the front's height.
                        if clamped {
                            return;
                        }
                        let height = f32::from(bounds.size.height);
                        let mut heights = measured.borrow_mut();
                        if heights.get(&id).is_none_or(|h| (h - height).abs() > 0.5) {
                            heights.insert(id, height);
                            cx.notify(entity_id);
                        }
                    })
                    .child(face(toast, content, Some(dismiss), cx).w_full())
                    .into_any_element(),
            );
        }
        if animating {
            window.request_animation_frame();
        }

        div()
            .id("exp-toast-stack")
            .absolute()
            .right(px(constants::VIEWPORT_OFFSET))
            .bottom(px(constants::VIEWPORT_OFFSET))
            .w(px(constants::WIDTH))
            .h(px(box_height))
            .occlude()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_hovered(*hovered, cx)))
            .children(cards)
            .into_any_element()
    }
}

/// Every window's layer, by window id; a closed window's goes with it.
#[derive(Default)]
struct LayerRegistry {
    by_window: HashMap<WindowId, Entity<ToastLayer>>,
    _closed: Option<Subscription>,
}

impl Global for LayerRegistry {}

/// This window's toast layer, created on first access.
pub(crate) fn layer_for_window(window: &Window, cx: &mut App) -> Entity<ToastLayer> {
    let window_id = window.window_handle().window_id();
    if let Some(existing) = cx
        .try_global::<LayerRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
    {
        return existing;
    }
    let layer = cx.new(|_| ToastLayer::new());
    if !cx.has_global::<LayerRegistry>() {
        let closed = cx.on_window_closed(|cx, window_id| {
            if cx.has_global::<LayerRegistry>() {
                cx.global_mut::<LayerRegistry>().by_window.remove(&window_id);
            }
        });
        cx.default_global::<LayerRegistry>()._closed = Some(closed);
    }
    cx.default_global::<LayerRegistry>().by_window.insert(window_id, layer.clone());
    layer
}

/// The toast layer as a root overlay (after the dialog layer, so a toast
/// sits over a dialog): every window root renders this.
pub(crate) fn render_layer(window: &mut Window, cx: &mut App) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .child(layer_for_window(window, cx))
        .into_any_element()
}

/// One item of a laid-out stack: its top inside the stack box, its width
/// factor, whether it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StackItem {
    pub offset: f32,
    pub scale: f32,
    pub visible: bool,
}

/// The contract's stack geometry (`toast-stack.json` `geometry`, locked by
/// the tests): `heights` oldest first, returns the stack box height and one
/// item per toast in the same order. Rank 0 = the newest (front). Collapsed:
/// every older toast's edge moves `PEEK` per rank away from the anchor and
/// its width shrinks `SCALE_STEP` per rank (capped at the last visible
/// rank); only `VISIBLE` draw. Expanded: full size, `GAP` apart, all drawn.
pub(crate) fn stack_layout(
    heights: &[f32],
    expanded: bool,
    anchored_bottom: bool,
) -> (f32, Vec<StackItem>) {
    use constants::{GAP, PEEK, SCALE_STEP, VISIBLE};
    let n = heights.len();
    let rank = |i: usize| n - 1 - i;
    // Distance of each item's anchor-side edge from the box's anchor edge.
    let lead: Vec<f32> = if expanded {
        (0..n)
            .map(|i| heights[i + 1..].iter().map(|h| h + GAP).sum::<f32>())
            .collect()
    } else {
        (0..n).map(|i| PEEK * rank(i) as f32).collect()
    };
    let height = (0..n).map(|i| lead[i] + heights[i]).fold(0f32, f32::max);
    let items = (0..n)
        .map(|i| {
            let r = rank(i);
            let offset =
                if anchored_bottom { height - lead[i] - heights[i] } else { lead[i] };
            let (scale, visible) = if expanded {
                (1.0, true)
            } else {
                (1.0 - SCALE_STEP * r.min(VISIBLE - 1) as f32, r < VISIBLE)
            };
            StackItem { offset, scale, visible }
        })
        .collect();
    (height, items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/toast-stack.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("toast-stack.json parses")
    }

    fn num(v: &Value, key: &str) -> f64 {
        v["constants"][key].as_f64().unwrap_or_else(|| panic!("constants.{key}"))
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    /// The lifecycle model runs on our motion tokens and the contract's
    /// stack numbers.
    #[test]
    fn motion_is_our_tokens_and_the_fixture() {
        let f = fixture();
        let m = motion();
        assert_eq!(m.duration, Duration::from_millis(180));
        assert_eq!(m.duration, theme::motion::STANDARD);
        assert_eq!(m.exit_duration, theme::motion::FAST);
        assert!(close(f64::from(f32::from(m.collapsed_peek)), num(&f, "peek")));
        assert!(close(f64::from(f32::from(m.expanded_gap)), num(&f, "gap")));
        assert!(close(f64::from(m.collapsed_scale_step), num(&f, "scaleStep")));
        assert_eq!(m.collapsed_visible as f64, num(&f, "visible"));
        assert_eq!(constants::DURATION.as_millis() as f64, num(&f, "durationMs"));
    }

    #[test]
    fn constants_match_the_fixture() {
        let f = fixture();
        assert!(close(constants::WIDTH.into(), num(&f, "width")));
        assert!(close(constants::GAP.into(), num(&f, "gap")));
        assert!(close(constants::PEEK.into(), num(&f, "peek")));
        assert!(close(constants::SCALE_STEP.into(), num(&f, "scaleStep")));
        assert_eq!(constants::VISIBLE as f64, num(&f, "visible"));
        assert!(close(constants::VIEWPORT_OFFSET.into(), num(&f, "viewportOffset")));
        let kinds: Vec<&str> = f["constants"]["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        let ours: Vec<&str> = ToastKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(ours, kinds);
    }

    #[test]
    fn every_kind_draws_its_concept_icon() {
        use gpui_component::IconNamed as _;
        let pairs = [
            (ToastKind::Success, registry::UI_SUCCESS),
            (ToastKind::Error, registry::UI_ERROR),
            (ToastKind::Info, registry::UI_INFO),
            (ToastKind::Warning, registry::UI_WARNING),
        ];
        for (kind, concept) in pairs {
            assert_eq!(kind.icon().path(), concept.path(), "{}", kind.as_str());
        }
    }

    #[test]
    fn stack_layout_matches_every_fixture_case() {
        let f = fixture();
        for case in f["geometry"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let heights: Vec<f32> = case["heights"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_f64().unwrap() as f32)
                .collect();
            let (height, items) = stack_layout(
                &heights,
                case["expanded"].as_bool().unwrap(),
                case["anchoredBottom"].as_bool().unwrap(),
            );
            assert!(close(height.into(), case["height"].as_f64().unwrap()), "{name}: height");
            let want = case["items"].as_array().unwrap();
            assert_eq!(items.len(), want.len(), "{name}: items");
            for (i, (got, want)) in items.iter().zip(want).enumerate() {
                assert!(close(got.offset.into(), want["offset"].as_f64().unwrap()), "{name}[{i}] offset");
                assert!(close(got.scale.into(), want["scale"].as_f64().unwrap()), "{name}[{i}] scale");
                assert_eq!(got.visible, want["visible"].as_bool().unwrap(), "{name}[{i}] visible");
            }
        }
    }

    struct Host {
        layer: gpui::Entity<ToastLayer>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().relative().size_full().child(self.layer.clone())
        }
    }

    fn host(cx: &mut gpui::TestAppContext) -> (gpui::Entity<Host>, &mut gpui::VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        cx.add_window_view(|window, cx| Host { layer: layer_for_window(window, cx) })
    }

    /// Step the fake clock `total` in `TICK`-sized strides, letting the
    /// ticker run after each.
    fn run_for(cx: &mut gpui::VisualTestContext, total: Duration) {
        let mut elapsed = Duration::ZERO;
        while elapsed < total {
            cx.background_executor.advance_clock(TICK);
            cx.run_until_parked();
            elapsed += TICK;
        }
    }

    /// `show` lands on the window's layer; the 4 s clock ends it when
    /// nothing hovers the stack and pauses while something does.
    #[gpui::test]
    fn the_clock_runs_four_seconds_and_pauses_on_hover(cx: &mut gpui::TestAppContext) {
        let (host, cx) = host(cx);
        let layer = host.read_with(cx, |host, _| host.layer.clone());
        cx.update(|window, cx| show(Toast::error("Could not merge the pull request"), window, cx));
        assert_eq!(layer.read_with(cx, |layer, _| layer.len()), 1);

        // Hovered: well past 4 s, still up.
        layer.update(cx, |layer, cx| layer.set_hovered(true, cx));
        run_for(cx, Duration::from_secs(6));
        assert_eq!(layer.read_with(cx, |layer, _| layer.len()), 1);

        // Released: still up just short of 4 s, gone after it (+ the exit).
        layer.update(cx, |layer, cx| layer.set_hovered(false, cx));
        run_for(cx, Duration::from_millis(3800));
        assert_eq!(layer.read_with(cx, |layer, _| layer.len()), 1);
        run_for(cx, Duration::from_millis(500));
        assert_eq!(layer.read_with(cx, |layer, _| layer.len()), 0);
    }

    /// The close glyph's dismiss ends a toast after the FAST exit.
    #[gpui::test]
    fn dismiss_leaves_after_the_exit(cx: &mut gpui::TestAppContext) {
        let (host, cx) = host(cx);
        let layer = host.read_with(cx, |host, _| host.layer.clone());
        let id = layer.update(cx, |layer, cx| layer.push(Toast::info("Hello"), cx));
        run_for(cx, Duration::from_millis(300));
        layer.update(cx, |layer, cx| layer.dismiss(id, cx));
        assert_eq!(layer.read_with(cx, |layer, _| layer.len()), 1);
        run_for(cx, Duration::from_millis(200));
        assert_eq!(layer.read_with(cx, |layer, _| layer.len()), 0);
    }

    /// Two toasts (one with a description + action) paint, collapsed and
    /// expanded.
    #[gpui::test]
    fn a_window_paints_two_toasts(cx: &mut gpui::TestAppContext) {
        let (host, cx) = host(cx);
        let layer = host.read_with(cx, |host, _| host.layer.clone());
        cx.update(|window, cx| {
            error("Could not merge the pull request", window, cx);
            let toast = Toast::success("Issue created")
                .description("EXP-1031 is on the board")
                .action("Open", |_, _| {});
            show(toast, window, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        run_for(cx, Duration::from_millis(300));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(layer.read_with(cx, |layer, _| layer.len()), 2);
        layer.update(cx, |layer, cx| layer.set_hovered(true, cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(layer.read_with(cx, |layer, _| layer.len()), 2);
    }
}
