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
//! on a timer, when its own next `step` has passed — snapped up to the next
//! tick of ONE [`SMOOTH_STEP`] grid the whole app shares (#LOGS-3). A
//! hard-cut sprite (Claude's spark, 90 ms frames) repaints on the first tick
//! after each of its frames; a smooth loop at [`SMOOTH_STEP`] (30 fps, the
//! rate an unfocused window already ran at) on every tick — the same look at
//! a fraction of the draws, focused or not, and however many animations run
//! at whatever steps, the window redraws at most 30 times a second. Everything else matches gpui's element: the
//! animation's clock, easing and `repeat`, and `reduce_motion` (a static
//! frame, nothing scheduled). The clock is gpui's executor, so a
//! `#[gpui::test]` drives it.
//!
//! Use it for every animation that lives longer than a moment (a pulse, a
//! sprite, a skeleton, a ping); one-shot enters and exits keep gpui's
//! `with_animation` — they end within a few hundred milliseconds.

use std::cell::Cell;
use std::time::{Duration, Instant};

use gpui::{
    Animation, AnyElement, App, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, ParentElement, Window,
};

/// The step of a smooth loop (a spinner, a sweep): 30 fps, and the tick of
/// the shared grid every stepped animation repaints on.
pub const SMOOTH_STEP: Duration = Duration::from_micros(33_333);

/// #LOGS-3: the step of a slow opacity PULSE that lives as long as what it
/// marks (an agent's working pulse): every second tick of the grid, 15 fps.
pub const PULSE_STEP: Duration = Duration::from_micros(66_666);

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

/// The element's state across frames: when the animation started (moved
/// back onto its step's shared lines, [`aligned_start`]), and the instant
/// the pending repaint timer fires (none, or already past = no timer
/// pending).
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
        // #LOGS-3: frame stats name what animates (the label is built only
        // while something listens).
        let _span = tracing::enabled!(tracing::Level::TRACE).then(|| {
            let id = format!("{:?}", self.id);
            tracing::trace_span!("anim.step", id = id.as_str()).entered()
        });
        // #LOGS-4: an unfocused window shows the static face (gpui refreshes
        // it when the focus comes back).
        let paused = super::glyph_layer::animation_paused(window, cx);
        window.with_element_state(global_id.unwrap(), |state, window| {
            let now = cx.background_executor().now();
            let mut state = state.unwrap_or_else(|| SteppedAnimationState {
                start: aligned_start(now, self.step, step_epoch(now, cx)),
                armed_until: None,
            });
            let elapsed = now.saturating_duration_since(state.start);
            let (delta, done) = if paused {
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
                arm(&mut state.armed_until, now, self.step, window, cx);
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

/// The one origin every step line in the app counts from.
struct StepEpoch(Instant);

impl gpui::Global for StepEpoch {}

/// Each frame redraws the whole window, so loops on private grids would add
/// up (three marks started at different moments = 90 redraws a second, not
/// 30; a 33 ms pulse, a 90 ms sprite and a 100 ms label = ~50, #LOGS-3).
/// Every repaint lands on a tick of ONE [`SMOOTH_STEP`] grid counted from
/// this epoch, so all loops wake together and share a single frame.
pub(super) fn step_epoch(now: Instant, cx: &mut App) -> Instant {
    if !cx.has_global::<StepEpoch>() {
        cx.set_global(StepEpoch(now));
    }
    cx.global::<StepEpoch>().0
}

/// How long after `now` the first shared-grid tick at or after `due` falls:
/// the grid's ticks are the multiples of [`SMOOTH_STEP`] since `epoch`.
pub fn until_shared_tick(now: Instant, due: Instant, epoch: Instant) -> Duration {
    let tick = SMOOTH_STEP.as_nanos();
    let since = due.saturating_duration_since(epoch).as_nanos();
    let snapped = since.div_ceil(tick) * tick;
    (epoch + Duration::from_nanos(snapped as u64)).saturating_duration_since(now)
}

/// A loop's start moved back onto its step's line at or before `now`, so
/// its frames change exactly on the lines every loop of that step shares:
/// three sparks started at different moments wake together, and the first
/// tick after each line shows that line's frame (none is skipped). The
/// phase moves by less than one step.
pub(super) fn aligned_start(now: Instant, step: Duration, epoch: Instant) -> Instant {
    let since = now.saturating_duration_since(epoch);
    now - (step - until_next_step(since, step)).min(since)
}

/// Schedule the repaint for the next step line, unless one is already
/// pending (a repaint for another reason between two lines must not add a
/// second timer). The timer notifies the view the element renders in; a
/// view gone by then notifies nothing.
fn arm(
    armed_until: &mut Option<Instant>,
    now: Instant,
    step: Duration,
    window: &Window,
    cx: &mut App,
) {
    if armed_until.is_some_and(|until| now < until) || super::glyph_layer::animation_paused(window, cx) {
        return;
    }
    let epoch = step_epoch(now, cx);
    let due = now + until_next_step(now.saturating_duration_since(epoch), step);
    let wait = until_shared_tick(now, due, epoch);
    *armed_until = Some(now + wait);
    let view = window.current_view();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(wait).await;
        cx.update(|cx| cx.notify(view));
    })
    .detach();
}

/// The same budget for a loop painted by hand: call it while painting a frame
/// that needs a successor, in place of `window.request_animation_frame()`.
/// `armed_until` is the caller's per-view memory of the pending timer.
pub fn repaint_on_next_step(
    armed_until: &Cell<Option<Instant>>,
    step: Duration,
    window: &Window,
    cx: &mut App,
) {
    let now = cx.background_executor().now();
    let mut until = armed_until.get();
    arm(&mut until, now, step, window, cx);
    armed_until.set(until);
}

#[cfg(test)]
mod tests {
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
        // An unfocused window paints the static face (#LOGS-4): focus it,
        // and count from the frame its focus drew.
        window.update(cx, |_, window, _| window.activate_window()).unwrap();
        cx.run_until_parked();
        renders.set(1);
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

    #[test]
    fn a_due_repaint_waits_for_the_next_shared_tick() {
        let epoch = Instant::now();
        let at = |ms: u64| epoch + Duration::from_millis(ms);
        // A 90 ms frame lands on the third tick (99.999 ms), a 100 ms label
        // on the fourth: 99.999 ms is a hair early, so it rounds up.
        assert_eq!(until_shared_tick(epoch, at(90), epoch), SMOOTH_STEP * 3);
        assert_eq!(until_shared_tick(epoch, at(100), epoch), SMOOTH_STEP * 4);
        // A tick is its own snap; the wait counts from `now`.
        assert_eq!(until_shared_tick(at(10), epoch + SMOOTH_STEP, epoch), SMOOTH_STEP - Duration::from_millis(10));
    }

    /// The heart of EXP-1325: a repeating stepped animation requests NO
    /// animation frame — the next frame runs no callbacks and repaints
    /// nothing — and repaints on the first shared tick after its step
    /// line (#LOGS-3).
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
        assert_eq!(renders.get(), 1, "the frame is due, the shared tick is not");

        cx.executor().advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
        assert_eq!(renders.get(), 2, "the tick after the step line: one repaint");
        assert!((deltas.get() - 0.1389).abs() < 1e-3, "99.999 ms into a 720 ms loop");

        // Lines at 180…630 ms land on ticks 6, 9, 11, 14, 17, 19: one repaint
        // per frame of the sprite, never more.
        cx.executor().advance_clock(Duration::from_millis(620));
        cx.run_until_parked();
        assert_eq!(renders.get(), 8, "one repaint per step line passed, never more");
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

        cx.executor().advance_clock(Duration::from_millis(70));
        cx.run_until_parked();
        assert_eq!(renders.get(), 3, "the original step line's tick");

        cx.executor().advance_clock(SMOOTH_STEP);
        cx.run_until_parked();
        assert_eq!(renders.get(), 3, "no second timer from the unrelated repaint");
    }

    struct Staggered {
        draws: Rc<std::cell::RefCell<Vec<Instant>>>,
        steps: Vec<Duration>,
    }

    impl Render for Staggered {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.draws.borrow_mut().push(cx.background_executor().now());
            div().size_full().children(self.steps.iter().enumerate().map(|(i, step)| {
                div().with_stepped_animation(
                    ElementId::Integer(i as u64),
                    Animation::new(Duration::from_millis(720)).repeat(),
                    *step,
                    |this, _| this,
                )
            }))
        }
    }

    /// #LOGS-3: loops at different steps (a 33 ms pulse, Claude's 90 ms
    /// spark, a 100 ms elapsed label), started at different moments, only
    /// ever repaint on the shared grid's ticks, so the window redraws at
    /// most 30 times a second whatever animates. (The test platform draws
    /// per notify, so this pins WHEN redraws happen, not how many.)
    #[gpui::test]
    async fn loops_at_any_step_only_redraw_on_the_shared_ticks(cx: &mut TestAppContext) {
        let draws = Rc::new(std::cell::RefCell::new(Vec::new()));
        let epoch = cx.executor().now();
        let window = cx.open_window(size(px(100.), px(100.)), {
            let draws = draws.clone();
            move |_, _| Staggered { draws, steps: vec![SMOOTH_STEP] }
        });
        window.update(cx, |_, window, _| window.activate_window()).unwrap();
        cx.run_until_parked();

        // Two more loops join between ticks, each at its own moment.
        for step in [STEP, Duration::from_millis(100)] {
            cx.executor().advance_clock(Duration::from_millis(11));
            window
                .update(cx, |view, _, cx| {
                    view.steps.push(step);
                    cx.notify();
                })
                .unwrap();
            cx.run_until_parked();
        }
        draws.borrow_mut().clear();

        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        let draws = draws.borrow();
        let mut ticks: Vec<u128> = Vec::new();
        for at in draws.iter() {
            let since = at.saturating_duration_since(epoch).as_nanos();
            assert_eq!(since % SMOOTH_STEP.as_nanos(), 0, "a redraw off the shared grid at {since} ns");
            ticks.push(since / SMOOTH_STEP.as_nanos());
        }
        ticks.dedup();
        assert!(ticks.len() <= 31, "at most one redraw per tick, {} ticks", ticks.len());
        assert!(ticks.len() >= 29, "the pulse still repaints every tick, {} ticks", ticks.len());
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
