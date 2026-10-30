//! EXP-1325 — repeating animations on a frame BUDGET.
//!
//! gpui's `with_animation` asks for an animation frame on EVERY frame it
//! paints, and gpui lays out and paints the WHOLE window per frame at the
//! monitor's refresh rate while the window is focused (gpui caps only an
//! UNFOCUSED window, at 30 fps). A working mark that lives in the sidebar
//! for hours therefore re-laid out a 4K issue board 240 times a second on a
//! 240 Hz display — the "fans spin whenever the IDE has focus" of
//! EXP-1325, and the reason it stopped the moment another window took the
//! focus.
//!
//! [`SteppedAnimationExt::with_stepped_animation`] is gpui's element with
//! ONE difference: instead of the next frame it schedules its next repaint
//! on a timer, at the next multiple of `step` since the animation started.
//! A hard-cut sprite (Claude's spark, 90 ms frames) repaints exactly when
//! its frame changes; a smooth loop at [`SMOOTH_STEP`] (30 fps, the rate an
//! unfocused window already ran at) — the same look at a fraction of the
//! draws, focused or not. Everything else matches gpui's element: the
//! animation's clock, easing and `repeat`, and `reduce_motion` (a static
//! frame, nothing scheduled). The clock is gpui's executor, so a
//! `#[gpui::test]` drives it.
//!
//! Use it for every animation that lives longer than a moment (a pulse, a
//! sprite, a skeleton, a ping); one-shot enters and exits keep gpui's
//! `with_animation` — they end within a few hundred milliseconds.

use std::time::{Duration, Instant};

use gpui::{
    Animation, AnyElement, App, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, ParentElement, Window,
};

/// The step of a smooth loop (an opacity pulse, a sweep): 30 fps.
pub const SMOOTH_STEP: Duration = Duration::from_micros(33_333);

/// An extension trait adding the stepped animation wrapper to elements.
pub trait SteppedAnimationExt {
    /// Render this element with `animation`, repainting every `step` of the
    /// animation's own clock instead of every frame.
    fn with_stepped_animation(
        self,
        id: impl Into<ElementId>,
        animation: Animation,
        step: Duration,
        animator: impl Fn(Self, f32) -> Self + 'static,
    ) -> SteppedAnimationElement<Self>
    where
        Self: Sized,
    {
        SteppedAnimationElement {
            id: id.into(),
            element: Some(self),
            animation,
            step,
            animator: Box::new(animator),
        }
    }
}

impl<E: IntoElement + 'static> SteppedAnimationExt for E {}

/// A gpui element applying a [`SteppedAnimationExt`] animation to another
/// element.
pub struct SteppedAnimationElement<E> {
    id: ElementId,
    element: Option<E>,
    animation: Animation,
    step: Duration,
    animator: Box<dyn Fn(E, f32) -> E + 'static>,
}

impl<E: ParentElement> ParentElement for SteppedAnimationElement<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        if let Some(element) = &mut self.element {
            element.extend(elements);
        }
    }
}

impl<E: IntoElement + 'static> IntoElement for SteppedAnimationElement<E> {
    type Element = SteppedAnimationElement<E>;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The element's state across frames: when the animation started, and the
/// instant the pending repaint timer fires (none, or already past = no
/// timer pending).
struct SteppedAnimationState {
    start: Instant,
    armed_until: Option<Instant>,
}

/// How long after `elapsed` the next multiple of `step` falls: never zero
/// (an exact multiple waits a whole step), so a repaint always lands on the
/// NEXT grid line, never on the one just painted.
pub fn until_next_step(elapsed: Duration, step: Duration) -> Duration {
    let step_ns = step.as_nanos().max(1);
    let into = elapsed.as_nanos() % step_ns;
    Duration::from_nanos((step_ns - into) as u64)
}

impl<E: IntoElement + 'static> Element for SteppedAnimationElement<E> {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        window.with_element_state(global_id.unwrap(), |state, window| {
            let now = cx.background_executor().now();
            let mut state = state.unwrap_or_else(|| SteppedAnimationState {
                start: now,
                armed_until: None,
            });
            let elapsed = now.saturating_duration_since(state.start);
            let (delta, done) = if cx.reduce_motion() {
                (if self.animation.oneshot { 1.0 } else { 0.0 }, true)
            } else {
                let mut delta =
                    elapsed.as_secs_f32() / self.animation.duration.as_secs_f32();
                let mut done = false;
                if delta > 1.0 {
                    if self.animation.oneshot {
                        done = true;
                        delta = 1.0;
                    } else {
                        delta %= 1.0;
                    }
                }
                (delta, done)
            };
            let delta = (self.animation.easing)(delta);
            debug_assert!(
                (0.0..=1.0).contains(&delta),
                "delta should always be between 0 and 1"
            );

            let element = self.element.take().expect("should only be called once");
            let mut element = (self.animator)(element, delta).into_any_element();

            if !done {
                arm(&mut state, now, elapsed, self.step, window, cx);
            }

            ((element.request_layout(window, cx), element), state)
        })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: gpui::Bounds<gpui::Pixels>,
        element: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        element.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: gpui::Bounds<gpui::Pixels>,
        element: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.paint(window, cx);
    }
}

/// Schedule the repaint for the next step line, unless one is already
/// pending (a repaint for another reason between two lines must not add a
/// second timer). The timer notifies the view the element renders in; a
/// view gone by then notifies nothing.
fn arm(
    state: &mut SteppedAnimationState,
    now: Instant,
    elapsed: Duration,
    step: Duration,
    window: &Window,
    cx: &mut App,
) {
    if state.armed_until.is_some_and(|until| now < until) {
        return;
    }
    let wait = until_next_step(elapsed, step);
    state.armed_until = Some(now + wait);
    let view = window.current_view();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(wait).await;
        cx.update(|cx| cx.notify(view));
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{div, px, size, Context, Render, Styled as _, TestAppContext, WindowHandle};

    use super::*;

    const STEP: Duration = Duration::from_millis(90);

    struct Probe {
        renders: Rc<Cell<usize>>,
        deltas: Rc<Cell<f32>>,
    }

    impl Render for Probe {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let deltas = self.deltas.clone();
            div().size_full().child(div().with_stepped_animation(
                "probe",
                Animation::new(Duration::from_millis(720)).repeat(),
                STEP,
                move |this, delta| {
                    deltas.set(delta);
                    this
                },
            ))
        }
    }

    fn open(cx: &mut TestAppContext) -> (Rc<Cell<usize>>, Rc<Cell<f32>>, WindowHandle<Probe>) {
        let renders = Rc::new(Cell::new(0));
        let deltas = Rc::new(Cell::new(0.));
        let window = cx.open_window(size(px(100.), px(100.)), {
            let (renders, deltas) = (renders.clone(), deltas.clone());
            move |_, _| Probe { renders, deltas }
        });
        cx.run_until_parked();
        (renders, deltas, window)
    }

    #[test]
    fn the_next_step_line_is_never_the_one_just_painted() {
        assert_eq!(until_next_step(Duration::ZERO, STEP), STEP);
        assert_eq!(until_next_step(Duration::from_millis(89), STEP), Duration::from_millis(1));
        assert_eq!(until_next_step(STEP, STEP), STEP);
        assert_eq!(until_next_step(Duration::from_millis(100), STEP), Duration::from_millis(80));
        assert_eq!(until_next_step(Duration::from_millis(1), Duration::ZERO), Duration::from_nanos(1));
    }

    /// The heart of EXP-1325: a repeating stepped animation requests NO
    /// animation frame — the next frame runs no callbacks and repaints
    /// nothing — and repaints exactly when its step line passes.
    #[gpui::test]
    async fn a_stepped_loop_repaints_on_its_step_not_on_every_frame(cx: &mut TestAppContext) {
        let (renders, deltas, window) = open(cx);
        assert_eq!(renders.get(), 1);
        assert_eq!(deltas.get(), 0.);

        let callbacks = window
            .update(cx, |_, window, cx| window.simulate_next_frame(cx))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(callbacks, 0, "no animation frame may be requested");
        assert_eq!(renders.get(), 1);

        cx.executor().advance_clock(STEP / 2);
        cx.run_until_parked();
        assert_eq!(renders.get(), 1, "half a step: nothing to repaint yet");

        cx.executor().advance_clock(STEP / 2);
        cx.run_until_parked();
        assert_eq!(renders.get(), 2, "the step line passed: one repaint");
        assert!((deltas.get() - 0.125).abs() < 1e-3, "one 90 ms frame into a 720 ms loop");

        cx.executor().advance_clock(STEP * 3);
        cx.run_until_parked();
        assert_eq!(renders.get(), 5, "one repaint per step line passed, never more");
    }

    /// A repaint between two step lines (another reason, say a hover) must
    /// not add a second timer.
    #[gpui::test]
    async fn an_unrelated_repaint_between_steps_adds_no_timer(cx: &mut TestAppContext) {
        let (renders, _, window) = open(cx);
        cx.executor().advance_clock(STEP / 3);
        window.update(cx, |_, _, cx| cx.notify()).unwrap();
        cx.run_until_parked();
        assert_eq!(renders.get(), 2);

        cx.executor().advance_clock(STEP * 2 / 3);
        cx.run_until_parked();
        assert_eq!(renders.get(), 3, "the original step line");

        cx.executor().advance_clock(STEP / 3);
        cx.run_until_parked();
        assert_eq!(renders.get(), 3, "no second timer from the unrelated repaint");
    }

    /// Reduce motion: the start frame, nothing scheduled — as gpui's element.
    #[gpui::test]
    async fn reduce_motion_paints_one_static_frame(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_reduce_motion(true));
        let (renders, deltas, _) = open(cx);
        assert_eq!(renders.get(), 1);
        assert_eq!(deltas.get(), 0.);
        cx.executor().advance_clock(STEP * 4);
        cx.run_until_parked();
        assert_eq!(renders.get(), 1);
    }
}
