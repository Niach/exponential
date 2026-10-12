//! EXP-1275 — `feedback-states` (2 General components): the SIX kinds of
//! `fixtures/feedback-states.json`, each the REAL primitive of
//! [`crate::feedback`] (or the toast layer's own face for errors), at rest
//! where a live one would need a window root.

use std::time::{Duration, Instant};

use gpui::{div, px, App, Div, IntoElement, ParentElement as _, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::feedback::{self, Pending};
use crate::icons::registry;
use crate::toast::{self, Toast};

pub(crate) const ID: &str = "feedback-states";
pub(crate) const OWNER: &str = "EXP-1275";

fn labelled(caption: &'static str, body: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(caption),
        )
        .child(body.into_any_element())
}

fn start_button(id: &'static str) -> Button {
    Button::new(id).primary().icon(registry::UI_SUBMIT).label("Start")
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let ground = cx.theme().background;
    // 1. Inline: the progressive word, and the list-head band spinner.
    let inline = h_flex()
        .gap_6()
        .items_center()
        .child(feedback::inline_status(feedback::AUTOSAVE_SAVING, true, cx))
        .child(feedback::inline_status(feedback::AUTOSAVE_SAVED, false, cx))
        .child(feedback::inline_status(feedback::autosave_edited("2m ago"), false, cx))
        .child(
            div()
                .w(px(240.))
                .child(crate::surface::glass_section_header_loading("Devices", true, None, cx)),
        );
    // 2. Pending: idle, then past the delay (spinner in the icon slot).
    let pending = h_flex()
        .gap_4()
        .items_center()
        .child(Pending::default().apply(start_button("sg-pending-idle"), feedback::PENDING_START, cx))
        .child(Pending::shown_since(Instant::now() - feedback::PENDING_DELAY).apply(
            start_button("sg-pending-shown"),
            feedback::PENDING_START,
            cx,
        ));
    // 3. Placeholder: skeleton rows in a list's 32px row geometry.
    let placeholder = div().w(px(420.)).child(feedback::skeleton_rows(
        "sg-feedback-skeleton",
        feedback::SKELETON_ROWS,
        32.,
        cx,
    ));
    // 4. The mark: animated, at rest (reduce motion), and the starting row.
    let mark = v_flex()
        .gap_3()
        .child(
            h_flex()
                .gap_6()
                .items_center()
                .child(feedback::mark("sg-feedback-mark", feedback::MARK_STYLEGUIDE_PX, ground, cx))
                .child(feedback::mark_static(feedback::MARK_STYLEGUIDE_PX, ground, cx)),
        )
        .child(div().w(px(420.)).child(feedback::starting_row(
            feedback::TEST_ID_STARTING,
            "macbook",
            Instant::now() - Duration::from_millis(1400),
            ground,
            cx,
        )));
    // 5. Blocking: the overlay over a stand-in window.
    let blocking = div()
        .relative()
        .w(px(420.))
        .h(px(200.))
        .rounded(px(theme::tokens::radius::LG))
        .border_1()
        .border_color(cx.theme().border)
        .overflow_hidden()
        .child(
            v_flex()
                .p_4()
                .gap_2()
                .child(div().text_sm().child("Agent"))
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Inbox")),
        )
        .child(feedback::blocking_overlay(feedback::blocking_switch_team("Personal"), cx));
    // 6. Errors: a failed action is a toast with Retry; invalid input stays
    // inline under its field. Plus the remote-start toast (D11).
    let failed = Toast::error("Couldn't save the team prompt")
        .description("The server didn't answer.")
        .action(feedback::RETRY, |_, _| {});
    let remote = Toast::info(feedback::remote_start_title("Danny"))
        .description("EXP-1271")
        .action(feedback::REMOTE_START_OPEN, |_, _| {});
    let errors = v_flex()
        .gap_3()
        .child(toast::card(&failed, cx).w(px(toast::constants::WIDTH)))
        .child(
            div()
                .text_size(px(13.))
                .text_color(cx.theme().danger)
                .child(crate::settings::team_general::prompt_too_long(
                    domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES + 1,
                )),
        )
        .child(toast::card(&remote, cx).w(px(toast::constants::WIDTH)));
    v_flex()
        .gap_6()
        .child(labelled("Inline", inline, cx))
        .child(labelled("Pending", pending, cx))
        .child(labelled("Placeholder", placeholder, cx))
        .child(labelled("The mark", mark, cx))
        .child(labelled("Blocking", blocking, cx))
        .child(labelled("Errors", errors, cx))
}
