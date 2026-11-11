use super::*;
use crate::settings::change_email_dialog::{ChangeEmailDialogView, EmailShot};

fn email_shot() -> EmailShot {
    EmailShot { code_for: None, email: "", code: "", busy: false, error: None }
}

fn change_email(name: &str, shot: EmailShot) {
    shoot_pane_acted(name, ChangeEmailDialogView::for_shots, |_, _| {}, None, &[], move |view, window, cx| view.show(shot, window, cx));
}

#[test]
#[ignore]
fn settings_change_email_empty() {
    change_email("change-email-empty", email_shot());
}

#[test]
#[ignore]
fn settings_change_email_typed() {
    change_email("change-email-typed", EmailShot { email: "dana@newmail.example", ..email_shot() });
}

#[test]
#[ignore]
fn settings_change_email_sending() {
    change_email("change-email-sending", EmailShot { email: "dana@newmail.example", busy: true, ..email_shot() });
}

#[test]
#[ignore]
fn settings_change_email_code() {
    change_email("change-email-code", EmailShot { code_for: Some("dana@newmail.example"), code: "1234", ..email_shot() });
}

#[test]
#[ignore]
fn settings_change_email_code_error() {
    change_email("change-email-code-error", EmailShot { code_for: Some("dana@newmail.example"), code: "123456", error: Some("That code is not valid. Check it or send a new one."), ..email_shot() });
}
