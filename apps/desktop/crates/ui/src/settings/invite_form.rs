//! What an invite by email shares (EXP-630).
//!
//! Web parity: `components/team/invite-member-form.tsx`. Sending puts the
//! person on the roster AT ONCE as a PLACEHOLDER member (name + email,
//! assignable and attributable before they ever sign in), so the form is a
//! member-creating control, not just a mailer. A [`ResendTarget`] turns it
//! into a RE-INVITE of an existing placeholder whose address stays editable —
//! a resend is usually there to fix a typo.
//!
//! #EXP-1629: the Members pane and the "Resend invite" dialog each draw the
//! form as document nodes (the pane with FEED-108's OUTLINE "Add without
//! inviting", the dialog in web `layout="stack"`); both read a send's answer
//! through [`invite_outcome`].

use gpui::{App, Window};

/// EXP-771 (web copy wins): the server accepted the invite but could not mail
/// it — the generated link renders right under this, so the wording points at
/// it. Kept byte-identical to `invite-member-form.tsx`'s toast — a toast
/// here too (EXP-1031).
/// FEED-79: the invite exists but its mail did not go out — the toast's
/// description under `Couldn't email the invite`.
const EMAIL_FALLBACK_MESSAGE: &str = "Copy the link below and share it instead.";

/// EXP-771: the seat cap is not "some plan limit" — it is the web's
/// `UpgradeDialog` on the invite controls, rendered inline here (the desktop
/// never shows pricing, §4.9). Title + body byte-identical to the web.
/// feedback-states.json's invalid-email input case: the field's line when
/// the server refused the address without a sentence of its own.
pub(super) const INVALID_EMAIL: &str = "Enter a full email address";

pub(super) const OUT_OF_SEATS_TITLE: &str = "Out of seats";
pub(super) const OUT_OF_SEATS_MESSAGE: &str =
    "Everyone on your plan's seats is already in this team. Add seats to \
     invite more teammates.";
/// The desktop's one billing hand-off under the notice (§4.9).
pub(super) const OUT_OF_SEATS_NOTE: &str = "Add seats on the web.";

/// The placeholder member a "Resend invite" is bound to (web `resendTarget`).
#[derive(Clone, Debug)]
pub(super) struct ResendTarget {
    pub user_id: String,
    /// Prefill for the Name field (the placeholder's current display name).
    pub name: String,
    /// Prefill for the address — editable, since the reason for a resend is
    /// often that it was wrong.
    pub email: String,
}

/// What `teamInvites.create` with an address came to — the form and the
/// Members pane read the same answer, each into its own fields.
pub(super) enum InviteOutcome {
    /// The mail went out; the link (`{base}/invite/{token}`) stays as a
    /// manual fallback.
    Delivered(String),
    /// The invite exists but its mail did not go out: the link to share.
    Undelivered(String),
    /// The seat cap refused it — the web's "Out of seats" notice.
    OutOfSeats,
    /// The address the server refused: the line under the field
    /// (feedback-states.json's invalid-email case).
    Refused(String),
    /// Anything else (already toasted).
    Failed,
}

/// Reads a send's `result` and raises its toasts: "Invite sent to …"
/// (EXP-1031: landing in the window that outlives a resend dialog closing on
/// it), the failed-delivery toast pointing at the link (FEED-79), the
/// failed-action toast for anything but a seat cap or a refused address.
pub(super) fn invite_outcome(result: Result<api::teams::InviteCreateOutput, api::ApiError>, base: &str, email: &str, window: &mut Window, cx: &mut App) -> InviteOutcome {
    match result {
        Ok(out) => {
            let url = format!("{base}/invite/{}", out.token);
            if out.email_delivered == Some(true) {
                let sent = format!("Invite sent to {email}");
                crate::navigation::defer_in_result_window(window, cx, move |w, cx| {
                    crate::toast::success(sent, w, cx);
                });
                InviteOutcome::Delivered(url)
            } else {
                // Requested but not delivered (transport down or
                // unconfigured) — fall back to the link.
                crate::toast::show(
                    crate::action_error::failed_toast("email", "the invite", Some(EMAIL_FALLBACK_MESSAGE.into())),
                    window,
                    cx,
                );
                InviteOutcome::Undelivered(url)
            }
        }
        Err(err) if super::is_plan_limit(&err) => InviteOutcome::OutOfSeats,
        Err(api::ApiError::Http { status: 400, message }) => {
            InviteOutcome::Refused(crate::action_error::description_of(&message).unwrap_or_else(|| INVALID_EMAIL.to_string()))
        }
        Err(err) => {
            crate::action_error::failed_action_err("send", "the invite", &err, cx);
            InviteOutcome::Failed
        }
    }
}
