//! EXP-1212: `draft-leave-dialog` (3 Special components): what leaving a New
//! issue page with content asks, Discard · Save draft · Create issue, plus
//! the `×`'s "Discard this draft and its files?" confirm.
//!
//! The IDE's dialogs are native alert windows (`issue_draft_screen`'s
//! `prompt_leave` and `discard`). A window cannot be embedded, so the demo
//! draws the alert's REAL view (`native_dialog::alert_specimen`) over the
//! SAME spec builders the page opens (`issue_draft_screen::leave_alert`,
//! `issue_draft_screen::discard_confirm_alert`), the answers inert.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::issue_draft_screen::{discard_confirm_alert, leave_alert};

pub(crate) const ID: &str = "draft-leave-dialog";
pub(crate) const OWNER: &str = "EXP-1212";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let leave = leave_alert(|_, _| true, |_, _| true, |_, _| true);
    let discard = discard_confirm_alert(|_, _| true);
    div()
        .flex()
        .flex_wrap()
        .gap_4()
        .child(crate::native_dialog::alert_specimen(
            "sg-draft-leave",
            leave,
            window,
            cx,
        ))
        .child(crate::native_dialog::alert_specimen(
            "sg-draft-discard-confirm",
            discard,
            window,
            cx,
        ))
}
