//! #EXP-1629: the "Resend invite" / "Send invite" dialog's content in every
//! state, at the dialog window's content width (420 less its 16 px padding).
use super::*;
use domain::placeholder_status::PlaceholderStatus;
use gpui::Entity;

use crate::settings::resend_invite_dialog::{ResendInviteDialogView, ResendShot, SHOT_EMAIL, SHOT_NAME};

const LINK: &str = "https://exponential.at/invite/9f2c4e7a1b3d5f6e8a0c2e4b6d8f0a1c3e5b7d9f";

struct DialogColumn {
    content: Entity<ResendInviteDialogView>,
}

impl Render for DialogColumn {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(388.)).child(self.content.clone())
    }
}

fn resend() -> ResendShot {
    ResendShot { status: PlaceholderStatus::Pending, name: SHOT_NAME, email: SHOT_EMAIL, sending: false, error: None, out_of_seats: false, link: None }
}

fn shoot(name: &str, shot: ResendShot) {
    shoot_pane_acted(
        name,
        |window, cx| DialogColumn { content: cx.new(|cx| ResendInviteDialogView::for_shots(window, cx)) },
        |_, _| {},
        None,
        &[],
        move |column, window, cx| column.content.update(cx, |content, cx| content.show(shot, window, cx)),
    );
}

#[test]
#[ignore]
fn settings_resend_invite_resend() {
    shoot("resend-invite-resend", resend());
}

#[test]
#[ignore]
fn settings_resend_invite_first_send() {
    shoot("resend-invite-first-send", ResendShot { status: PlaceholderStatus::Unsent, ..resend() });
}

#[test]
#[ignore]
fn settings_resend_invite_empty() {
    shoot("resend-invite-empty", ResendShot { name: "", email: "", ..resend() });
}

#[test]
#[ignore]
fn settings_resend_invite_typed() {
    shoot("resend-invite-typed", ResendShot { name: "Ada King", email: "ada.king@newmail.example", ..resend() });
}

#[test]
#[ignore]
fn settings_resend_invite_sending() {
    shoot("resend-invite-sending", ResendShot { sending: true, ..resend() });
}

#[test]
#[ignore]
fn settings_resend_invite_error() {
    shoot("resend-invite-error", ResendShot { error: Some("Enter a full email address"), ..resend() });
}

#[test]
#[ignore]
fn settings_resend_invite_out_of_seats() {
    shoot("resend-invite-out-of-seats", ResendShot { out_of_seats: true, ..resend() });
}

#[test]
#[ignore]
fn settings_resend_invite_link() {
    shoot("resend-invite-link", ResendShot { link: Some(LINK), ..resend() });
}

/// The dialog opens on the address (web `autoFocus="email"`).
#[test]
#[ignore]
fn settings_resend_invite_focuses_the_email() {
    let (focused, field) = super::focus::focus_after_open("email.field", ResendInviteDialogView::for_shots, |view| view.surface());
    assert!(field.is_some(), "the dialog has an email field");
    assert_eq!(focused, field);
}
