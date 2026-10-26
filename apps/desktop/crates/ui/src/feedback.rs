//! EXP-1275 / EXP-1271 (EXP-1259 D8) — the IDE's feedback states, the
//! desktop half of `packages/domain-contract/fixtures/feedback-states.json`.
//! Six kinds and nothing else draws a wait or an error:
//!
//! 1. INLINE — [`spinner`] / [`inline_status`]: a 10px spinner + a
//!    progressive word in a band, header or caption. The EXP-1274 list-head
//!    spinner (`band_loading_glyph`) is this glyph.
//! 2. PENDING — [`Pending`]: a button that started something swaps its icon
//!    slot for the spinner and its label for the progressive word, keeps its
//!    idle width as a minimum, and changes nothing for the first 150 ms.
//! 3. PLACEHOLDER — [`skeleton_rows`]: rows in the real row geometry, a
//!    dot and bars pulsing opacity.
//! 4. THE MARK — [`mark`]: the logo disc whose three cut stripes sweep in
//!    turn ([`mark_stripe_span`], a port of `src/feedback-mark.ts`), painted
//!    with gpui paths, the cut in the caller's ground colour.
//! 5. BLOCKING — [`blocking_overlay`] + [`Blocking`]: window backdrop + the
//!    mark + one line, only for switching team, switching account and
//!    signing out.
//! 6. ERRORS — toasts (`crate::toast`), per the fixture's `errors.rule`.
//!
//! gpui has no reduce-motion signal (see `theme::motion`), so the animated
//! kinds always animate; [`mark_static`] is the reduce-motion face the
//! styleguide shows beside them.
//!
//! The constants below are the fixture's WHOLE vocabulary (locked by the
//! tests); the IDE's feedback sweep (FEED-79 wave) consumes the words this
//! lane's surfaces do not use yet.
#![cfg_attr(not(test), allow(dead_code))]

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, point, prelude::FluentBuilder as _, px, Animation, AnyElement, App, Bounds,
    ContentMask, Div, ElementId, Global, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, Styled, Window,
};
use exponential_ui_gpui::controls::{SteppedAnimationExt as _, SMOOTH_STEP};
use gpui_component::button::Button;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, ElementExt as _, Icon};

use exponential_ui_gpui::chrome::Chrome;

// ---------------------------------------------------------------------------
// The fixture's numbers and copy (locked by the tests below)
// ---------------------------------------------------------------------------

/// `inline.spinner` / `inline.gap` (px).
pub(crate) const INLINE_SPINNER_PX: f32 = 10.;
pub(crate) const INLINE_GAP_PX: f32 = 6.;
pub(crate) const WORD_SAVING: &str = "Saving";
pub(crate) const WORD_SAVED: &str = "Saved";
pub(crate) const WORD_LOADING: &str = "Loading";

/// `pending.delayMs` / `pending.spinner`.
pub(crate) const PENDING_DELAY: Duration = Duration::from_millis(150);
pub(crate) const PENDING_SPINNER_PX: f32 = 12.;
pub(crate) const PENDING_START: &str = "Starting";
pub(crate) const PENDING_CONNECT: &str = "Connecting";
pub(crate) const PENDING_SAVE: &str = "Saving";
pub(crate) const PENDING_DELETE: &str = "Deleting";
pub(crate) const PENDING_SEND: &str = "Sending";
pub(crate) const PENDING_MERGE: &str = "Merging";
pub(crate) const PENDING_INVITE: &str = "Inviting";

/// `placeholder.*`.
pub(crate) const SKELETON_ROWS: usize = 3;
const SKELETON_BAR_H: f32 = 8.;
const SKELETON_RADIUS: f32 = 4.;
const SKELETON_DOT: f32 = 12.;
const SKELETON_PULSE: Duration = Duration::from_millis(1600);
const SKELETON_OPACITY: (f32, f32) = (0.45, 0.9);

/// `mark.*`.
pub(crate) const MARK_LOOP: Duration = Duration::from_millis(1200);
const MARK_SAMPLES: usize = 24;
const MARK_STRIPE_WIDTH: f32 = 6.;
pub(crate) const MARK_INLINE_PX: f32 = 16.;
pub(crate) const MARK_STATUS_PX: f32 = 16.;
pub(crate) const MARK_STYLEGUIDE_PX: f32 = 40.;

struct Stripe {
    cubic: [[f64; 2]; 4],
    draw: [f64; 2],
    erase: [f64; 2],
}

const STRIPES: [Stripe; 3] = [
    Stripe {
        cubic: [[-5.87, 62.01], [39.09, 65.44], [48.72, 28.71], [49.03, -6.21]],
        draw: [0., 0.3],
        erase: [0.55, 0.8],
    },
    Stripe {
        cubic: [[-5.07, 86.], [53.78, 84.42], [71.13, 37.29], [73., -5.09]],
        draw: [0.12, 0.42],
        erase: [0.65, 0.9],
    },
    Stripe {
        cubic: [[-4.27, 109.99], [68.46, 103.4], [93.55, 45.86], [96.98, -3.98]],
        draw: [0.24, 0.54],
        erase: [0.75, 1.],
    },
];

/// `blocking.*`.
const BLOCKING_BACKDROP_OPACITY: f32 = 0.85;
pub(crate) const BLOCKING_MARK_PX: f32 = 40.;
const BLOCKING_GAP: f32 = 16.;
const BLOCKING_SWITCH_TEAM: &str = "Switching to {team}";
const BLOCKING_SWITCH_ACCOUNT: &str = "Switching to {account}";
pub(crate) const BLOCKING_SIGN_OUT: &str = "Signing out";

/// `starting.*`: the composer after Start, until the session row lands.
const STARTING_LABEL: &str = "Starting on {device}";
pub(crate) const STARTING_TIMEOUT: Duration = Duration::from_millis(30_000);
const STARTING_TIMEOUT_TITLE: &str = "The session did not start";
const STARTING_TIMEOUT_DESCRIPTION: &str = "{device} did not answer.";
pub(crate) const RETRY: &str = "Retry";
/// `errors.mergeRefusal.force`: the check gate's override (EXP-1378).
pub(crate) const MERGE_ANYWAY: &str = "Merge anyway";

/// `autosave.*`.
pub(crate) const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(800);
pub(crate) const AUTOSAVE_SAVED_HOLD: Duration = Duration::from_millis(2000);
pub(crate) const AUTOSAVE_SAVING: &str = "Saving";
pub(crate) const AUTOSAVE_SAVED: &str = "Saved";
const AUTOSAVE_EDITED: &str = "Edited {ago}";

/// `remoteStart.*`.
pub(crate) const REMOTE_START_DURATION: Duration = Duration::from_millis(4000);
const REMOTE_START_TITLE: &str = "{name} started a session on {thisDevice}";
pub(crate) const REMOTE_START_AGENT: &str = "An agent";
const REMOTE_START_THIS_MAC: &str = "this Mac";
const REMOTE_START_THIS_COMPUTER: &str = "this computer";
pub(crate) const REMOTE_START_CHAT: &str = "Chat";
pub(crate) const REMOTE_START_OPEN: &str = "Open";

/// `testIds.*` (the IDE has no DOM; the styleguide labels its specimens
/// with them so a capture names the same states as the web).
pub(crate) const TEST_ID_STARTING: &str = "composer-starting";

pub(crate) fn blocking_switch_team(team: &str) -> String {
    BLOCKING_SWITCH_TEAM.replace("{team}", team)
}

pub(crate) fn blocking_switch_account(account: &str) -> String {
    BLOCKING_SWITCH_ACCOUNT.replace("{account}", account)
}

pub(crate) fn starting_label(device: &str) -> String {
    STARTING_LABEL.replace("{device}", device)
}

/// The 30 s starting timeout's toast copy: (title, description).
pub(crate) fn starting_timeout_copy(device: &str) -> (&'static str, String) {
    (STARTING_TIMEOUT_TITLE, STARTING_TIMEOUT_DESCRIPTION.replace("{device}", device))
}

/// "Edited 2m ago" — `ago` is the IDE's own relative-time string.
pub(crate) fn autosave_edited(ago: &str) -> String {
    AUTOSAVE_EDITED.replace("{ago}", ago)
}

/// `remoteStart.toastWhen` = `person` (review #9): a start lands with the
/// remote-start toast only when a PERSON made it from another device (no
/// `started_reason`, own or a teammate's). Trigger starts (`schedule`,
/// `event`), agent starts (`agent`) and account-rotation resumes stay
/// quiet: they only appear in Running.
pub(crate) fn remote_start_toasts(started_reason: Option<&str>, rotation: bool) -> bool {
    started_reason.is_none() && !rotation
}

/// "Danny started a session on this Mac".
pub(crate) fn remote_start_title(name: &str) -> String {
    let this_device = if cfg!(target_os = "macos") {
        REMOTE_START_THIS_MAC
    } else {
        REMOTE_START_THIS_COMPUTER
    };
    REMOTE_START_TITLE
        .replace("{name}", name)
        .replace("{thisDevice}", this_device)
}

// ---------------------------------------------------------------------------
// 1. Inline
// ---------------------------------------------------------------------------

/// THE spinner glyph: the SDK's one spinner (`controls::spinner`, the
/// chrome's refresh glyph spinning) in `color`. The EXP-1274 band spinner
/// (`controls::band_loading_glyph`) IS the inline one below.
pub(crate) fn spinner_colored(size: f32, color: Hsla, cx: &App) -> AnyElement {
    exponential_ui_gpui::controls::spinner(size, color, cx)
}

/// [`spinner_colored`] in the muted foreground.
pub(crate) fn spinner(size: f32, cx: &App) -> AnyElement {
    spinner_colored(size, cx.theme().muted_foreground, cx)
}

/// The INLINE spinner (10px, muted).
pub(crate) fn inline_spinner(cx: &App) -> AnyElement {
    spinner(INLINE_SPINNER_PX, cx)
}

/// The inline status: an optional spinner + the progressive word, muted
/// and small (`Saving` with the spinner, `Saved` without).
pub(crate) fn inline_status(word: impl Into<SharedString>, spinning: bool, cx: &App) -> Div {
    h_flex()
        .flex_none()
        .items_center()
        .gap(px(INLINE_GAP_PX))
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .when(spinning, |row| row.child(inline_spinner(cx)))
        .child(word.into())
}

// ---------------------------------------------------------------------------
// 2. Pending
// ---------------------------------------------------------------------------

/// A pending button's state, held by the view that owns the button.
/// [`Pending::start`] when the action fires, [`Pending::finish`] when it
/// settles; [`Pending::apply`] dresses the button. Nothing changes for the
/// first [`PENDING_DELAY`] so a fast action never flashes; the button is
/// inert from the first moment either way.
#[derive(Clone, Default)]
pub(crate) struct Pending {
    since: Option<Instant>,
    idle_w: Rc<Cell<Pixels>>,
}

impl Pending {
    /// Mark the action in flight and repaint once the delay has passed.
    pub(crate) fn start<V: 'static>(&mut self, cx: &mut gpui::Context<V>) {
        self.since = Some(Instant::now());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PENDING_DELAY).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// A pending face in flight since `since` (the styleguide's specimen).
    pub(crate) fn shown_since(since: Instant) -> Self {
        Self {
            since: Some(since),
            idle_w: Rc::default(),
        }
    }

    pub(crate) fn finish(&mut self) {
        self.since = None;
    }

    /// The action is in flight (the button must not fire again).
    pub(crate) fn active(&self) -> bool {
        self.since.is_some()
    }

    /// The pending face shows (in flight for at least the delay).
    pub(crate) fn shown(&self) -> bool {
        pending_shown(self.since, Instant::now())
    }

    /// Dress `button` (already carrying its idle icon + label): while shown
    /// its icon slot is the spinner and its label `word`, never narrower
    /// than it measured idle. Wrap the result where the button went.
    pub(crate) fn apply(&self, button: Button, word: &str, cx: &App) -> AnyElement {
        let shown = self.shown();
        let idle_w = self.idle_w.clone();
        let min_w = self.idle_w.get();
        let icon = Icon::empty().path(Chrome::global(cx).icons.refresh.clone());
        let button = if shown {
            button
                .loading(true)
                .loading_icon(icon)
                .label(SharedString::from(word.to_string()))
                .min_w(min_w)
        } else if self.active() {
            button.loading(true).loading_icon(Icon::empty())
        } else {
            button
        };
        let wrap = div().flex_none().child(button);
        if self.active() {
            wrap.into_any_element()
        } else {
            wrap.on_prepaint(move |bounds, _, _| idle_w.set(bounds.size.width))
                .into_any_element()
        }
    }
}

/// Whether a pending face that began at `since` shows at `now`.
fn pending_shown(since: Option<Instant>, now: Instant) -> bool {
    since.is_some_and(|since| now.saturating_duration_since(since) >= PENDING_DELAY)
}

// ---------------------------------------------------------------------------
// 3. Placeholder
// ---------------------------------------------------------------------------

/// The skeleton's opacity at loop `delta` (0..1): ease-in-out between the
/// fixture's two values, low at the loop's ends.
fn skeleton_opacity(delta: f32) -> f32 {
    let (lo, hi) = SKELETON_OPACITY;
    let wave = 0.5 - 0.5 * (delta * std::f32::consts::TAU).cos();
    lo + (hi - lo) * wave
}

fn skeleton_bar(width: f32, fill: Hsla) -> Div {
    div()
        .flex_none()
        .w(px(width))
        .h(px(SKELETON_BAR_H))
        .rounded(px(SKELETON_RADIUS))
        .bg(fill)
}

/// The placeholder fill (the skeleton's muted bars).
fn skeleton_fill(cx: &App) -> Hsla {
    cx.theme().muted_foreground.opacity(0.18)
}

/// The PLACEHOLDER for ONE value still loading (a title, a picker's value,
/// a field's text, a tab chip's label): a single bar `width` px wide in the
/// skeleton geometry, never the text "Loading…". Static: the caller pulses
/// it ([`skeleton_value`]) where it sits on a page.
pub(crate) fn placeholder_bar(width: f32, cx: &App) -> Div {
    skeleton_bar(width, skeleton_fill(cx))
}

/// [`placeholder_bar`], pulsing like the skeleton rows.
pub(crate) fn skeleton_value(id: impl Into<ElementId>, width: f32, cx: &App) -> AnyElement {
    placeholder_bar(width, cx)
        .with_stepped_animation(id, Animation::new(SKELETON_PULSE).repeat(), SMOOTH_STEP, |bar, delta| {
            bar.opacity(skeleton_opacity(delta))
        })
        .into_any_element()
}

/// The PLACEHOLDER for a picture still loading: a `height` px block in the
/// skeleton fill (radius `placeholder.radius`), pulsing, never "Loading
/// image…".
pub(crate) fn skeleton_block(id: impl Into<ElementId>, height: f32, cx: &App) -> AnyElement {
    div()
        .h(px(height))
        .rounded(px(SKELETON_RADIUS))
        .bg(skeleton_fill(cx))
        .with_stepped_animation(id, Animation::new(SKELETON_PULSE).repeat(), SMOOTH_STEP, |block, delta| {
            block.opacity(skeleton_opacity(delta))
        })
        .into_any_element()
}

/// The PLACEHOLDER for a long text still loading (a prompt, a document):
/// [`SKELETON_ROWS`] bars where its lines sit, the block pulsing.
pub(crate) fn skeleton_text(id: impl Into<ElementId>, cx: &App) -> AnyElement {
    const LINES: [f32; SKELETON_ROWS] = [280., 220., 160.];
    let fill = skeleton_fill(cx);
    v_flex()
        .w_full()
        .gap_3()
        .children(LINES.iter().map(|width| skeleton_bar(*width, fill)))
        .with_stepped_animation(id, Animation::new(SKELETON_PULSE).repeat(), SMOOTH_STEP, |block, delta| {
            block.opacity(skeleton_opacity(delta))
        })
        .into_any_element()
}

/// ONE skeleton row `row_h` px tall: a dot where the row's glyph sits and
/// two bars where its text sits (`ix` varies the second bar's length).
pub(crate) fn skeleton_row(ix: usize, row_h: f32, cx: &App) -> Div {
    let fill = skeleton_fill(cx);
    const TAILS: [f32; 3] = [200., 150., 240.];
    h_flex()
        .w_full()
        .h(px(row_h))
        .flex_none()
        .items_center()
        .gap_2()
        .px_3()
        .child(
            div()
                .flex_none()
                .size(px(SKELETON_DOT))
                .rounded_full()
                .bg(fill),
        )
        .child(skeleton_bar(72., fill))
        .child(skeleton_bar(TAILS[ix % TAILS.len()], fill))
}

/// The PLACEHOLDER: `rows` skeleton rows in a list's row geometry, the
/// whole block pulsing.
pub(crate) fn skeleton_rows(
    id: impl Into<ElementId>,
    rows: usize,
    row_h: f32,
    cx: &App,
) -> AnyElement {
    let list = v_flex()
        .w_full()
        .children((0..rows).map(|ix| crate::surface::list_row(skeleton_row(ix, row_h, cx))));
    list.with_stepped_animation(
        id,
        Animation::new(SKELETON_PULSE).repeat(),
        SMOOTH_STEP,
        |list, delta| list.opacity(skeleton_opacity(delta)),
    )
    .into_any_element()
}

/// The PLACEHOLDER inside a dropdown menu (a lazily-fetched list's first
/// load): ONE inert item holding the skeleton rows at the menu row height.
pub(crate) fn menu_skeleton(
    menu: gpui_component::menu::PopupMenu,
    id: &'static str,
) -> gpui_component::menu::PopupMenu {
    menu.item(
        gpui_component::menu::PopupMenuItem::element(move |_, cx| {
            div()
                .w(px(MENU_SKELETON_W))
                .child(skeleton_rows(id, SKELETON_ROWS, crate::controls::MENU_ROW_H, cx))
        })
        .disabled(true),
    )
}

/// The menu skeleton's width: a dropdown sizes to its widest item.
const MENU_SKELETON_W: f32 = 220.;

// ---------------------------------------------------------------------------
// 4. The mark
// ---------------------------------------------------------------------------

fn ease_in_out_cubic(x: f64) -> f64 {
    if x < 0.5 {
        4. * x * x * x
    } else {
        1. - (-2. * x + 2.).powi(3) / 2.
    }
}

fn round4(value: f64) -> f64 {
    (value * 10_000.).round() / 10_000.
}

/// The visible span `[t0, t1]` of stripe `index` (0 = lower-left) at loop
/// `phase`: the cut draws in from t=0 during its draw window, stays whole,
/// then erases from t=0 during its erase window, and is absent outside them.
/// Rounded to 4 decimals (`markStripeSpan`).
pub(crate) fn mark_stripe_span(index: usize, phase: f64) -> Option<(f64, f64)> {
    let stripe = STRIPES.get(index)?;
    let p = ((phase % 1.) + 1.) % 1.;
    let [draw_from, draw_to] = stripe.draw;
    let [erase_from, erase_to] = stripe.erase;
    if p < draw_from || p >= erase_to {
        return None;
    }
    if p < draw_to {
        return Some((0., round4(ease_in_out_cubic((p - draw_from) / (draw_to - draw_from)))));
    }
    if p < erase_from {
        return Some((0., 1.));
    }
    Some((round4(ease_in_out_cubic((p - erase_from) / (erase_to - erase_from))), 1.))
}

/// A point on stripe `index`'s cubic at `t`, in the logo's 100×100 grid
/// (`markStripePoint`).
pub(crate) fn mark_stripe_point(index: usize, t: f64) -> (f64, f64) {
    let [p0, p1, p2, p3] = STRIPES[index].cubic;
    let u = 1. - t;
    let (a, b, c, d) = (u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t);
    (
        a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
        a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
    )
}

/// Paint the mark into `bounds`: the disc in `ink`, each stripe's visible
/// span (`None` phase = the static logo, every span whole) stroked in
/// `ground`, clipped to the square.
fn paint_mark(bounds: Bounds<Pixels>, ink: Hsla, ground: Hsla, phase: Option<f64>, window: &mut Window) {
    let side = bounds.size.width.min(bounds.size.height);
    let scale = f32::from(side) / 100.;
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_quad(gpui::fill(bounds, ink).corner_radii(side / 2.));
        for index in 0..STRIPES.len() {
            let span = match phase {
                Some(phase) => mark_stripe_span(index, phase),
                None => Some((0., 1.)),
            };
            let Some((t0, t1)) = span else { continue };
            if t1 <= t0 {
                continue;
            }
            let steps = ((MARK_SAMPLES as f64) * (t1 - t0)).ceil().max(1.) as usize;
            let at = |t: f64| {
                let (x, y) = mark_stripe_point(index, t);
                point(
                    bounds.origin.x + px(x as f32 * scale),
                    bounds.origin.y + px(y as f32 * scale),
                )
            };
            let mut path = gpui::PathBuilder::stroke(px(MARK_STRIPE_WIDTH * scale));
            path.move_to(at(t0));
            for step in 1..=steps {
                path.line_to(at(t0 + (t1 - t0) * step as f64 / steps as f64));
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, ground);
            }
        }
    });
}

fn mark_canvas(size: f32, ink: Hsla, ground: Hsla, phase: Option<f64>) -> impl IntoElement {
    canvas(|_, _, _| (), move |bounds, _, window, _| paint_mark(bounds, ink, ground, phase, window))
        .size(px(size))
        .flex_none()
}

/// THE MARK, animated: the disc in the foreground, its three stripes
/// sweeping on a [`MARK_LOOP`] linear loop, cut in `ground` (the surface
/// it sits on).
pub(crate) fn mark(id: impl Into<ElementId>, size: f32, ground: Hsla, cx: &App) -> AnyElement {
    let ink = cx.theme().foreground;
    div()
        .flex_none()
        .size(px(size))
        .with_stepped_animation(id, Animation::new(MARK_LOOP).repeat(), SMOOTH_STEP, move |slot, delta| {
            slot.child(mark_canvas(size, ink, ground, Some(delta as f64)))
        })
        .into_any_element()
}

/// The mark at rest (the reduce-motion face, and the home's headline disc):
/// the logo, every stripe whole.
pub(crate) fn mark_static(size: f32, ground: Hsla, cx: &App) -> AnyElement {
    mark_canvas(size, cx.theme().foreground, ground, None).into_any_element()
}

/// The STARTING row (composer + run status before the first turn): the
/// mark, "Starting on <device>" and the elapsed seconds since `since`.
pub(crate) fn starting_row(
    id: impl Into<ElementId>,
    device: &str,
    since: Instant,
    ground: Hsla,
    cx: &App,
) -> Div {
    let id: ElementId = id.into();
    let muted = cx.theme().muted_foreground;
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(mark(
            ElementId::NamedChild(std::sync::Arc::new(id.clone()), "mark".into()),
            MARK_INLINE_PX,
            ground,
            cx,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .child(starting_label(device)),
        )
        .child(
            div()
                .flex_none()
                .text_xs()
                .text_color(muted)
                // EXP-1325: the label changes every 100 ms — repaint then,
                // not every frame.
                .with_stepped_animation(
                    ElementId::NamedChild(std::sync::Arc::new(id), "elapsed".into()),
                    Animation::new(Duration::from_millis(100)).repeat(),
                    Duration::from_millis(100),
                    move |slot, _| slot.child(elapsed_label(since.elapsed())),
                ),
        )
}

/// "1.4 s".
pub(crate) fn elapsed_label(elapsed: Duration) -> String {
    format!("{:.1} s", elapsed.as_secs_f32())
}

// ---------------------------------------------------------------------------
// 5. Blocking
// ---------------------------------------------------------------------------

/// The app-wide blocking wait: `Some(line)` while a team switch, an account
/// switch or a sign-out is in flight. Every window's root paints
/// [`blocking_overlay`] over itself while it is set.
#[derive(Default)]
pub(crate) struct Blocking {
    line: Option<SharedString>,
}

impl Global for Blocking {}

impl Blocking {
    pub(crate) fn begin(line: impl Into<SharedString>, cx: &mut App) {
        cx.default_global::<Blocking>().line = Some(line.into());
        cx.refresh_windows();
    }

    pub(crate) fn end(cx: &mut App) {
        if cx.default_global::<Blocking>().line.take().is_some() {
            cx.refresh_windows();
        }
    }

    pub(crate) fn line(cx: &App) -> Option<SharedString> {
        cx.try_global::<Blocking>().and_then(|b| b.line.clone())
    }

    /// The overlay to paint over a window root, if a wait is in flight.
    pub(crate) fn overlay(cx: &App) -> Option<AnyElement> {
        Self::line(cx).map(|line| blocking_overlay(line, cx))
    }
}

/// THE BLOCKING overlay: a full-window backdrop (the ground at 85 %) that
/// swallows the mouse, the mark at 40px and ONE muted line.
pub(crate) fn blocking_overlay(line: impl Into<SharedString>, cx: &App) -> AnyElement {
    let ground = cx.theme().background;
    v_flex()
        .id("feedback-blocking")
        .absolute()
        .inset_0()
        .occlude()
        .items_center()
        .justify_center()
        .gap(px(BLOCKING_GAP))
        .bg(ground.opacity(BLOCKING_BACKDROP_OPACITY))
        .child(mark("feedback-blocking-mark", BLOCKING_MARK_PX, ground, cx))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(line.into()),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/feedback-states.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("feedback-states.json parses")
    }

    fn num(v: &Value) -> f64 {
        v.as_f64().expect("a number")
    }

    fn s(v: &Value) -> &str {
        v.as_str().expect("a string")
    }

    #[test]
    fn the_mark_replays_every_fixture_case() {
        let f = fixture();
        for case in f["mark"]["cases"].as_array().expect("cases") {
            let phase = num(&case["phase"]);
            for (index, want) in case["spans"].as_array().unwrap().iter().enumerate() {
                let have = mark_stripe_span(index, phase);
                match (want.as_array(), have) {
                    (None, None) => {}
                    (Some(want), Some((t0, t1))) => {
                        assert!((num(&want[0]) - t0).abs() < 1e-9, "phase {phase} stripe {index}: {t0}");
                        assert!((num(&want[1]) - t1).abs() < 1e-9, "phase {phase} stripe {index}: {t1}");
                    }
                    (want, have) => panic!("phase {phase} stripe {index}: want {want:?}, have {have:?}"),
                }
            }
        }
        assert_eq!(mark_stripe_span(3, 0.5), None);
    }

    #[test]
    fn the_stripes_are_the_fixture_stripes() {
        let f = fixture();
        let mark = &f["mark"];
        assert_eq!(num(&mark["loopMs"]) as u128, MARK_LOOP.as_millis());
        assert_eq!(num(&mark["samples"]) as usize, MARK_SAMPLES);
        assert_eq!(num(&mark["stripeWidth"]) as f32, MARK_STRIPE_WIDTH);
        assert_eq!(num(&mark["sizes"]["inline"]) as f32, MARK_INLINE_PX);
        assert_eq!(num(&mark["sizes"]["status"]) as f32, MARK_STATUS_PX);
        assert_eq!(num(&mark["sizes"]["styleguide"]) as f32, MARK_STYLEGUIDE_PX);
        let stripes = mark["stripes"].as_array().unwrap();
        assert_eq!(stripes.len(), STRIPES.len());
        for (want, have) in stripes.iter().zip(STRIPES.iter()) {
            for (k, p) in want["cubic"].as_array().unwrap().iter().enumerate() {
                assert_eq!(num(&p[0]), have.cubic[k][0]);
                assert_eq!(num(&p[1]), have.cubic[k][1]);
            }
            assert_eq!([num(&want["draw"][0]), num(&want["draw"][1])], have.draw);
            assert_eq!([num(&want["erase"][0]), num(&want["erase"][1])], have.erase);
        }
        // The cubic's ends are its first and last control points.
        assert_eq!(mark_stripe_point(0, 0.), (-5.87, 62.01));
        assert_eq!(mark_stripe_point(2, 1.), (96.98, -3.98));
    }

    #[test]
    fn numbers_and_copy_are_the_fixture() {
        let f = fixture();
        assert_eq!(num(&f["inline"]["spinner"]) as f32, INLINE_SPINNER_PX);
        assert_eq!(INLINE_SPINNER_PX, exponential_ui_gpui::controls::INLINE_SPINNER_PX);
        assert_eq!(num(&f["inline"]["gap"]) as f32, INLINE_GAP_PX);
        assert_eq!(s(&f["inline"]["words"]["saving"]), WORD_SAVING);
        assert_eq!(s(&f["inline"]["words"]["saved"]), WORD_SAVED);
        assert_eq!(s(&f["inline"]["words"]["loading"]), WORD_LOADING);

        let p = &f["pending"];
        assert_eq!(num(&p["delayMs"]) as u128, PENDING_DELAY.as_millis());
        assert_eq!(num(&p["spinner"]) as f32, PENDING_SPINNER_PX);
        let words = &p["words"];
        assert_eq!(s(&words["start"]), PENDING_START);
        assert_eq!(s(&words["connect"]), PENDING_CONNECT);
        assert_eq!(s(&words["save"]), PENDING_SAVE);
        assert_eq!(s(&words["delete"]), PENDING_DELETE);
        assert_eq!(s(&words["send"]), PENDING_SEND);
        assert_eq!(s(&words["merge"]), PENDING_MERGE);
        assert_eq!(s(&words["invite"]), PENDING_INVITE);

        let ph = &f["placeholder"];
        assert_eq!(num(&ph["rows"]) as usize, SKELETON_ROWS);
        assert_eq!(num(&ph["barHeight"]) as f32, SKELETON_BAR_H);
        assert_eq!(num(&ph["radius"]) as f32, SKELETON_RADIUS);
        assert_eq!(num(&ph["dot"]) as f32, SKELETON_DOT);
        assert_eq!(num(&ph["pulseMs"]) as u128, SKELETON_PULSE.as_millis());
        assert_eq!(num(&ph["opacity"][0]) as f32, SKELETON_OPACITY.0);
        assert_eq!(num(&ph["opacity"][1]) as f32, SKELETON_OPACITY.1);

        let b = &f["blocking"];
        assert_eq!(num(&b["backdropOpacity"]) as f32, BLOCKING_BACKDROP_OPACITY);
        assert_eq!(num(&b["markSize"]) as f32, BLOCKING_MARK_PX);
        assert_eq!(num(&b["gap"]) as f32, BLOCKING_GAP);
        assert_eq!(s(&b["copy"]["switchTeam"]), BLOCKING_SWITCH_TEAM);
        assert_eq!(s(&b["copy"]["switchAccount"]), BLOCKING_SWITCH_ACCOUNT);
        assert_eq!(s(&b["copy"]["signOut"]), BLOCKING_SIGN_OUT);

        let st = &f["starting"];
        assert_eq!(s(&st["label"]), STARTING_LABEL);
        assert_eq!(num(&st["timeoutMs"]) as u128, STARTING_TIMEOUT.as_millis());
        assert_eq!(s(&st["timeoutToast"]["title"]), STARTING_TIMEOUT_TITLE);
        assert_eq!(s(&st["timeoutToast"]["description"]), STARTING_TIMEOUT_DESCRIPTION);
        assert_eq!(s(&st["timeoutToast"]["action"]), RETRY);
        // Review #3: the deadline counts from the SEND, remote starts only,
        // and every start carries its `startId`.
        assert_eq!(s(&st["deadlineFrom"]), "sent");
        assert_eq!(st["remoteOnly"].as_bool(), Some(true));
        assert_eq!(s(&st["startIdKey"]), "startId");
        assert_eq!(num(&st["dedupeMs"]), num(&st["watchMs"]));
        assert_eq!(s(&f["errors"]["retry"]), RETRY);
        assert_eq!(s(&f["errors"]["mergeRefusal"]["force"]), MERGE_ANYWAY);
        assert_eq!(s(&f["errors"]["mergeRefusal"]["checksCode"]), "PRECONDITION_REQUIRED");

        let a = &f["autosave"];
        assert_eq!(num(&a["debounceMs"]) as u128, AUTOSAVE_DEBOUNCE.as_millis());
        assert_eq!(num(&a["savedHoldMs"]) as u128, AUTOSAVE_SAVED_HOLD.as_millis());
        assert_eq!(s(&a["captions"]["saving"]), AUTOSAVE_SAVING);
        assert_eq!(s(&a["captions"]["saved"]), AUTOSAVE_SAVED);
        assert_eq!(s(&a["captions"]["edited"]), AUTOSAVE_EDITED);

        let r = &f["remoteStart"];
        assert_eq!(num(&r["durationMs"]) as u128, REMOTE_START_DURATION.as_millis());
        assert_eq!(s(&r["title"]), REMOTE_START_TITLE);
        assert_eq!(s(&r["agentName"]), REMOTE_START_AGENT);
        // Review #9: only a person's start toasts; every case replays.
        assert_eq!(s(&r["toastWhen"]), "person");
        let cases = r["cases"].as_array().expect("remoteStart.cases");
        assert!(cases.len() >= 5);
        for case in cases {
            let name = s(&case["name"]);
            assert_eq!(
                remote_start_toasts(
                    case["startedReason"].as_str(),
                    case["rotation"].as_bool().expect("rotation"),
                ),
                case["toast"].as_bool().expect("toast"),
                "{name}"
            );
        }
        assert_eq!(s(&r["thisDevice"]["macos"]), REMOTE_START_THIS_MAC);
        assert_eq!(s(&r["thisDevice"]["other"]), REMOTE_START_THIS_COMPUTER);
        assert_eq!(s(&r["description"]), "{subject}");
        assert_eq!(s(&r["chatSubject"]), REMOTE_START_CHAT);
        assert_eq!(s(&r["action"]), REMOTE_START_OPEN);
        // The toast stack's own clock is the remote start's 4 s.
        assert_eq!(REMOTE_START_DURATION, crate::toast::constants::DURATION);

        assert_eq!(s(&f["testIds"]["starting"]), TEST_ID_STARTING);
    }

    #[test]
    fn copy_fills_its_slots() {
        assert_eq!(blocking_switch_team("Personal"), "Switching to Personal");
        assert_eq!(blocking_switch_account("dennis@x.at"), "Switching to dennis@x.at");
        assert_eq!(starting_label("macbook"), "Starting on macbook");
        assert_eq!(
            starting_timeout_copy("macbook"),
            ("The session did not start", "macbook did not answer.".to_string())
        );
        assert_eq!(autosave_edited("2m ago"), "Edited 2m ago");
        assert!(remote_start_title("Danny").starts_with("Danny started a session on this "));
        assert_eq!(elapsed_label(Duration::from_millis(1440)), "1.4 s");
    }

    #[test]
    fn a_pending_face_waits_out_the_delay() {
        let t0 = Instant::now();
        assert!(!pending_shown(None, t0));
        assert!(!pending_shown(Some(t0), t0 + Duration::from_millis(149)));
        assert!(pending_shown(Some(t0), t0 + PENDING_DELAY));
    }

    #[test]
    fn the_skeleton_pulses_between_the_fixture_opacities() {
        assert!((skeleton_opacity(0.) - SKELETON_OPACITY.0).abs() < 1e-6);
        assert!((skeleton_opacity(0.5) - SKELETON_OPACITY.1).abs() < 1e-6);
        assert!((skeleton_opacity(1.) - SKELETON_OPACITY.0).abs() < 1e-6);
    }
}
