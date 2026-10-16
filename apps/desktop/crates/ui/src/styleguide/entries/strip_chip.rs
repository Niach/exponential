//! EXP-1334: `strip-chip` (2 General components): THE strip chip, the one
//! tab box of every strip of chips (the thread header's issue + session
//! chips, the work tab strip's tabs) and the "+N" of a folded zone, drawn
//! bare, case for case with the web entry: the REAL
//! [`crate::thread_screen::strip_chip`] (the
//! [`crate::surface::strip_chip_frame`] box with its hairline) at rest,
//! active, short (the identifier alone), folded (the open nested item as a
//! path and the chevron), a session chip (the agent mark, the open nested
//! session), with a trailing x (the work tab), then the "+N" overflow
//! ([`crate::surface::strip_overflow_button`]). The composed strips are the
//! `work-tabs-strip` and `thread-parts` entries.

use gpui::{
    div, px, AnyElement, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::session_tree::SessionState;
use domain::statuses::{constructed_default, ResolvedStatus};
use domain::IssueStatus;

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::surface::strip_overflow_button;
use crate::thread_screen::{strip_chip, strip_issue_body};

pub(crate) const ID: &str = "strip-chip";
pub(crate) const OWNER: &str = "EXP-1334";

/// A demo status (no store here): the builtin default of `status`.
fn status(status: IssueStatus) -> Option<ResolvedStatus> {
    Some(constructed_default(status))
}

/// The chip's chevron: it opens its subtree on a click, not itself.
fn chevron(cx: &App) -> AnyElement {
    Icon::new(registry::UI_CHEVRON_DOWN)
        .xsmall()
        .flex_shrink_0()
        .text_color(cx.theme().muted_foreground)
        .into_any_element()
}

/// The open nested item, `› <open one>`: a sub-issue's mono identifier
/// (`mono`), a nested session's name.
fn path(text: &'static str, mono: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let open = div().flex_shrink_0().child(SharedString::from(text));
    let open = if mono {
        open.text_xs().font_family(theme::terminal::FONT_FAMILY)
    } else {
        open
    };
    h_flex()
        .flex_shrink_0()
        .gap(px(6.))
        .items_center()
        .child(div().text_color(theme.muted_foreground).child("›"))
        .child(open)
        .into_any_element()
}

/// A short body: the status glyph and the mono identifier, no title.
fn short_body(status: Option<ResolvedStatus>, identifier: &str, active: bool, cx: &App) -> Vec<AnyElement> {
    let mut body = strip_issue_body(status, identifier, "", active, cx);
    body.truncate(2);
    body
}

/// One case: the chip, its caption under it (the web `Captioned`).
fn captioned(caption: &'static str, chip: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .gap_2()
        .items_start()
        .child(h_flex().items_center().child(chip))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(caption))
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    h_flex()
        .flex_wrap()
        .items_start()
        .gap_6()
        .child(captioned(
            "at rest",
            strip_chip("sg-strip-chip-rest", false, cx).children(strip_issue_body(
                status(IssueStatus::Backlog),
                "EXP-563",
                "google keywords",
                false,
                cx,
            )),
            cx,
        ))
        .child(captioned(
            "active",
            strip_chip("sg-strip-chip-active", true, cx).children(strip_issue_body(
                status(IssueStatus::InProgress),
                "EXP-1223",
                "ExponentialOS",
                true,
                cx,
            )),
            cx,
        ))
        .child(captioned(
            "short (the identifier alone)",
            strip_chip("sg-strip-chip-short", false, cx).children(short_body(
                status(IssueStatus::Done),
                "EXP-1334",
                false,
                cx,
            )),
            cx,
        ))
        .child(captioned(
            "folded: path and chevron (a click opens its tree)",
            strip_chip("sg-strip-chip-folded", true, cx)
                .children(strip_issue_body(status(IssueStatus::InProgress), "EXP-1334", "Thread header", true, cx))
                .child(path("EXP-1340", true, cx))
                .child(chevron(cx)),
            cx,
        ))
        .child(captioned(
            "a session chip: the agent mark, the open nested session",
            strip_chip("sg-strip-chip-session", false, cx)
                .child(div().flex_shrink_0().child(crate::run_rows::run_mark(
                    crate::run_rows::run_agent(None),
                    SessionState::Working,
                    cx,
                )))
                .child(div().min_w_0().truncate().child("Threads impl"))
                .child(path("Android lane", false, cx))
                .child(chevron(cx)),
            cx,
        ))
        .child(captioned(
            "with a trailing x (the work tab)",
            strip_chip("sg-strip-chip-closable", false, cx)
                // The work tab's `pr 4`: the 24px ghost x supplies the air.
                .pr(px(4.))
                .children(strip_issue_body(status(IssueStatus::Backlog), "EXP-1255", "Work tab strip", false, cx))
                .child(
                    Button::new("sg-strip-chip-close")
                        .ghost()
                        .web_icon_xs()
                        .icon(registry::UI_CLOSE)
                        .tooltip("Close tab"),
                ),
            cx,
        ))
        .child(captioned("the +N overflow", strip_overflow_button("sg-strip-chip-more", 3, cx), cx))
}
