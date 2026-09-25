//! Settings → Members → "Resend invite" (EXP-630).
//!
//! Web parity: the `Dialog` half of `components/team/members-section.tsx` — a
//! placeholder member is on the roster but has not joined, so the row's
//! overflow menu offers a FRESH link, at a corrected address if the first one
//! was wrong. The form is the shared [`super::invite_form::InviteForm`],
//! prefilled and bound to that member (`placeholder_user_id`), so a resend
//! keeps the row's attributions instead of minting a second person.
//!
//! EXP-1076: for a row the Linear import seated without ever mailing
//! ([`PlaceholderStatus::Unsent`]) this is a FIRST send — the title and the
//! copy say "Send invite" and "has never been invited"; the form and the call
//! underneath are the same.
//!
//! Its own WINDOW rather than an [`crate::native_dialog::AlertSpec`]: the form
//! re-renders on its own state (spinner, notices, the fallback link) and alert
//! content is built once.

use gpui::{
    div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Window,
};
use gpui_component::{v_flex, ActiveTheme as _};

use domain::placeholder_status::PlaceholderStatus;

use crate::native_dialog::{self, DialogContent, DialogSpec};

use super::invite_form::{InviteDelivered, InviteForm, InviteFormLayout, ResendTarget};

/// Open the dialog over the Members pane. `display_name` leads the
/// description (web `resendTarget.displayName` / `.name`); `status` picks the
/// title and the copy (web `resendTarget.status`).
pub(super) fn open(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    display_name: String,
    status: PlaceholderStatus,
    target: ResendTarget,
) {
    let spec = DialogSpec::new(status.invite_verb(), size(px(420.), px(260.)))
        .resizable(size(px(360.), px(220.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            ResendInviteDialogView::new(team_id, display_name, status, target, window, cx)
        });
        let busy = view.clone();
        DialogContent::new(view).can_close(move |cx| !busy.read(cx).form.read(cx).sending())
    });
}

struct ResendInviteDialogView {
    description: String,
    form: Entity<InviteForm>,
    _subscriptions: Vec<Subscription>,
}

impl ResendInviteDialogView {
    fn new(
        team_id: String,
        display_name: String,
        status: PlaceholderStatus,
        target: ResendTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let form = cx
            .new(|cx| InviteForm::new(team_id, Some(target), InviteFormLayout::Stack, window, cx));
        let subscriptions = vec![
            // A delivered link is the whole errand — the dialog closes on it
            // (web `onInvited`); a failed delivery keeps it open around the
            // link to copy.
            cx.subscribe_in(&form, window, |_, _, _: &InviteDelivered, window, cx| {
                native_dialog::close_dialog_window(window, cx);
            }),
        ];
        Self {
            // Web copy, word for word.
            description: resend_description(display_name.as_str(), status),
            form,
            _subscriptions: subscriptions,
        }
    }
}

/// Web `DialogDescription`, word for word: a first send for a never-invited
/// row, a fresh link else.
fn resend_description(display_name: &str, status: PlaceholderStatus) -> String {
    match status {
        PlaceholderStatus::Unsent => format!(
            "{display_name} is on the team but has never been invited. Send them a link."
        ),
        PlaceholderStatus::Pending | PlaceholderStatus::Expired => format!(
            "{display_name} is on the team but has not joined yet. Send a \
             fresh link — fix the address first if it was wrong."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_branches_on_the_placeholder_state_like_the_web_dialog() {
        assert_eq!(
            resend_description("Ada", PlaceholderStatus::Unsent),
            "Ada is on the team but has never been invited. Send them a link."
        );
        assert_eq!(
            resend_description("Ada", PlaceholderStatus::Expired),
            "Ada is on the team but has not joined yet. Send a fresh link — fix the \
             address first if it was wrong."
        );
        assert_eq!(
            resend_description("Ada", PlaceholderStatus::Pending),
            resend_description("Ada", PlaceholderStatus::Expired)
        );
    }
}

impl Render for ResendInviteDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.description.clone()),
            )
            .child(self.form.clone())
    }
}
