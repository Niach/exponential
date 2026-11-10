//! EXP-1275 — `feedback-states` (2 General components): the SIX kinds of
//! `fixtures/feedback-states.json`, each the REAL primitive of
//! [`crate::feedback`] (or the toast layer's own face for errors), at rest
//! where a live one would need a window root. EXP-1554: ONE layout ×4 =
//! the web `FeedbackStatesSpecimen` (`packages/ui/src/feedback-specimen.tsx`):
//! six numbered cards ('1 · Inline' …) in its order, with its copy and
//! samples and nothing else.

use std::time::{Duration, Instant};

use gpui::{div, px, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::InputState;
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::controls::{glass_input, WebControl as _};
use crate::feedback::{self, Pending};
use crate::icons::registry;
use crate::toast::{self, Toast};

pub(crate) const ID: &str = "feedback-states";
pub(crate) const OWNER: &str = "EXP-1275";

/// The web specimen's four frozen phases of the mark's loop.
const MARK_PHASES: [f64; 4] = [0.08, 0.2, 0.32, 0.5];
/// The starting row's mark, frozen like the web specimen's (`phase={0.3}`).
const STARTING_PHASE: f64 = 0.3;
// 5 · Blocking draws the static logo (web `BlockingSurface still`).
/// feedback-states.json `errors.cases[4].message` (the invalid email).
const INVALID_EMAIL: &str = "Enter a full email address";

/// One kind: a bordered card headed `{index} · {title}` (web `Kind`).
fn kind(index: usize, title: &'static str, body: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .w(px(340.))
        .gap_3()
        .p_4()
        .rounded_xl()
        .border_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(SharedString::from(format!("{index} · {title}"))),
        )
        .child(body.into_any_element())
}

/// A 40px muted band with a label and a trailing status (web `Band`).
fn band(label: &'static str, status: impl IntoElement, cx: &App) -> Div {
    h_flex()
        .h(px(40.))
        .items_center()
        .justify_between()
        .rounded_md()
        .bg(cx.theme().muted.opacity(0.4))
        .px_3()
        .text_sm()
        .child(label)
        .child(status.into_any_element())
}

fn start_button(id: &'static str) -> Button {
    Button::new(id).primary().icon(registry::UI_SUBMIT).label("Start")
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let ground = cx.theme().background;
    let invalid = window.use_keyed_state("sg-feedback-invalid", cx, |window, cx| {
        InputState::new(window, cx).default_value("dennis@")
    });
    // 1. Inline: Saving / Saved in a band, and the list-head band spinner.
    let inline = v_flex()
        .gap_3()
        .child(band("Team prompt", feedback::inline_status(feedback::AUTOSAVE_SAVING, true, cx), cx))
        .child(band("Team prompt", feedback::inline_status(feedback::AUTOSAVE_SAVED, false, cx), cx))
        .child(crate::surface::glass_section_header_loading(
            "MCP servers",
            true,
            Some(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("3")
                    .into_any_element(),
            ),
            cx,
        ));
    // 2. Pending action: idle, then past the delay (spinner in the icon
    // slot), and the outline twin.
    let shown = Instant::now() - feedback::PENDING_DELAY;
    let pending = h_flex()
        .flex_wrap()
        .gap_2()
        .items_center()
        .child(Pending::default().apply(start_button("sg-pending-idle"), feedback::PENDING_START, cx))
        .child(Pending::shown_since(shown).apply(
            start_button("sg-pending-shown"),
            feedback::PENDING_START,
            cx,
        ))
        .child(Pending::shown_since(shown).apply(
            Button::new("sg-pending-connect").outline().label("Connect"),
            feedback::PENDING_CONNECT,
            cx,
        ));
    // 3. Placeholder: skeleton rows in a list's 32px row geometry.
    let placeholder = feedback::skeleton_rows("sg-feedback-skeleton", feedback::SKELETON_ROWS, 32., cx);
    // 4. The mark: four frozen phases, then the starting row.
    let mark = v_flex()
        .gap_3()
        .child(h_flex().gap_4().items_center().children(
            MARK_PHASES.map(|phase| feedback::mark_at(feedback::MARK_STYLEGUIDE_PX, phase, ground, cx)),
        ))
        .child(
            div()
                .rounded_xl()
                .border_1()
                .border_color(cx.theme().border)
                .px_3()
                .py(px(10.))
                .child(feedback::starting_row_frozen(
                    feedback::TEST_ID_STARTING,
                    "macbook",
                    Instant::now() - Duration::from_millis(1400),
                    ground,
                    Some(STARTING_PHASE),
                    cx,
                )),
        );
    // 5. Blocking: the overlay contained in its card.
    let blocking = div()
        .relative()
        .w_full()
        .h(px(160.))
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .overflow_hidden()
        .child(feedback::blocking_overlay_face(feedback::blocking_switch_team("Personal"), true, cx));
    // 6. Errors: a failed action is a toast with Retry; invalid input stays
    // inline under its field (FEED-79).
    let failed = Toast::error("Couldn't save the team prompt")
        .description("Network error")
        .action(feedback::RETRY, |_, _| {});
    let errors = v_flex()
        .gap_3()
        .child(toast::card(&failed, cx).w_full())
        .child(
            v_flex()
                .gap(px(6.))
                .child(glass_input(&invalid, window, cx).web_input().border_color(cx.theme().danger))
                .child(crate::action_error::field_error(INVALID_EMAIL, cx)),
        );
    // The web grid's auto-fill columns: cards wrap, three to a row at the
    // styleguide's 1440 window.
    h_flex()
        .flex_wrap()
        .items_start()
        .gap_4()
        .child(kind(1, "Inline", inline, cx))
        .child(kind(2, "Pending action", pending, cx))
        .child(kind(3, "Placeholder", placeholder, cx))
        .child(kind(4, "The mark", mark, cx))
        .child(kind(5, "Blocking", blocking, cx))
        .child(kind(6, "Errors", errors, cx))
}
