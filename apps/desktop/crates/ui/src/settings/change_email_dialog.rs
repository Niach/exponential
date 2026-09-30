//! Settings → Account → Sign-in methods → "Change" (EXP-1126).
//!
//! Web parity: `components/account/change-email-dialog.tsx`. Two steps
//! against Better Auth's email-otp plugin (bearer + Origin, `AuthClient`):
//! the NEW address gets a 6-digit code (`/email-otp/request-email-change`),
//! the code swaps the address (`/email-otp/change-email`). The current
//! mailbox is not re-proven — the signed-in session is the ownership proof.
//! On success the session is re-read and the new identity lands in
//! accounts.json ([`api::AuthStore::update_identity`]), so the Account
//! header and the rail follow at once.
//!
//! Its own WINDOW (like [`super::resend_invite_dialog`]): the steps re-render
//! on their own state, and alert content is built once.

use gpui::{
    div, px, size, App, AppContext as _, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _,
};

use crate::controls::{glass_input, WebControl as _};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;
use crate::session::AuthContext;

use super::sign_in_methods::SignInMethodsSection;

/// Open the dialog over the Account pane. `current_email` guards the
/// same-address case locally (web parity); `section` refetches on success.
pub(super) fn open(
    window: &mut Window,
    cx: &mut App,
    current_email: String,
    section: WeakEntity<SignInMethodsSection>,
) {
    let spec = DialogSpec::new("Change your email", size(px(420.), px(300.)))
        .resizable(size(px(360.), px(260.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| ChangeEmailDialogView::new(current_email, section, window, cx));
        let busy = view.clone();
        let submit = view.clone();
        DialogContent::new(view)
            .can_close(move |cx| !busy.read(cx).busy)
            .on_enter(move |window, cx| {
                submit.update(cx, |view, cx| view.submit(window, cx));
            })
    });
}

#[derive(Clone, PartialEq)]
enum Step {
    Address,
    Code { new_email: String },
}

struct ChangeEmailDialogView {
    current_email: String,
    section: WeakEntity<SignInMethodsSection>,
    step: Step,
    email: Entity<InputState>,
    code: Entity<InputState>,
    busy: bool,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl ChangeEmailDialogView {
    fn new(
        current_email: String,
        section: WeakEntity<SignInMethodsSection>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("you@example.com"));
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("123456"));
        let subscriptions = vec![
            cx.subscribe_in(&email, window, |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.submit(window, cx);
                }
            }),
            cx.subscribe_in(&code, window, |this, _, event: &InputEvent, window, cx| {
                match event {
                    // Digits only, six at most (web `replace(/\D/g, "")` +
                    // maxLength 6).
                    InputEvent::Change => {
                        let value = this.code.read(cx).value().to_string();
                        let digits = normalize_code(&value);
                        if digits != value {
                            this.code
                                .update(cx, |state, cx| state.set_value(digits, window, cx));
                        }
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    _ => {}
                }
            }),
            cx.observe(&email, |_, _, cx| cx.notify()),
        ];
        email.update(cx, |state, cx| state.focus(window, cx));
        Self {
            current_email,
            section,
            step: Step::Address,
            email,
            code,
            busy: false,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Enter / the primary button: whichever submit the visible step means.
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match self.step.clone() {
            Step::Address => {
                let address = self.email.read(cx).value().to_string();
                self.send_code(address, window, cx);
            }
            Step::Code { new_email } => self.confirm(new_email, window, cx),
        }
    }

    fn send_code(&mut self, address: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        let target = address.trim().to_lowercase();
        if target.is_empty() {
            return;
        }
        if target == self.current_email.to_lowercase() {
            self.error = Some("That is already your email.".into());
            cx.notify();
            return;
        }
        let Some((instance_url, token, client)) = credentials(cx) else {
            self.error = Some("Couldn't send the code.".into());
            cx.notify();
            return;
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let target_bg = target.clone();
            let result = cx
                .background_executor()
                .spawn(
                    async move { client.request_email_change(&instance_url, &token, &target_bg) },
                )
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        // A fresh code invalidates whatever is in the field.
                        this.code
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        this.code.update(cx, |state, cx| state.focus(window, cx));
                        this.step = Step::Code { new_email: target };
                    }
                    Err(err) => {
                        log::warn!("[ui] request-email-change failed: {err}");
                        this.error = Some(error_copy(&err, "Couldn't send the code.").into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn confirm(&mut self, new_email: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        let otp = self.code.read(cx).value().trim().to_string();
        if otp.len() < 6 {
            return;
        }
        let Some((instance_url, token, client)) = credentials(cx) else {
            self.error = Some("Couldn't change the email.".into());
            cx.notify();
            return;
        };
        let Some(account_id) = queries::active_account(cx).map(|account| account.id) else {
            return;
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let email_bg = new_email.clone();
            // Change, then re-read the session for the identity to persist —
            // a failed re-read still counts as changed (the server did it).
            let result = cx
                .background_executor()
                .spawn(async move {
                    client.change_email(&instance_url, &token, &email_bg, &otp)?;
                    let user = client.fetch_session(&instance_url, &token).ok().flatten();
                    Ok::<_, api::ApiError>(user)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(user) => {
                        let (email, name) = match &user {
                            Some(user) => (user.email.clone(), user.name.clone()),
                            None => (new_email.clone(), None),
                        };
                        if let Some(auth) = cx.try_global::<AuthContext>() {
                            auth.auth
                                .update_identity(&account_id, &email, name.as_deref());
                        }
                        let _ = this.section.update(cx, |section, cx| {
                            section.email_changed(&email, cx);
                        });
                        // The Account header reads accounts.json on render.
                        cx.refresh_windows();
                        native_dialog::close_dialog_window(window, cx);
                    }
                    Err(err) => {
                        log::warn!("[ui] change-email failed: {err}");
                        this.error = Some(error_copy(&err, "Couldn't change the email.").into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn back_to_address(&mut self, cx: &mut gpui::Context<Self>) {
        self.error = None;
        self.step = Step::Address;
        cx.notify();
    }

    fn text_link(
        id: &'static str,
        label: &'static str,
        cx: &mut gpui::Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut gpui::Context<Self>) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .cursor_pointer()
            .hover(|style| style.text_decoration_1())
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| handler(this, window, cx)))
    }
}

/// The active account's instance URL + session token + the auth client.
fn credentials(cx: &App) -> Option<(String, String, std::sync::Arc<api::AuthClient>)> {
    let auth = cx.try_global::<AuthContext>()?;
    let account = queries::active_account(cx)?;
    let token = auth.auth.token(&account.id)?;
    Some((account.instance_url, token, auth.client.clone()))
}

/// The server's (already contract-mapped) sentence, else the step's own.
fn error_copy(err: &api::ApiError, fallback: &str) -> String {
    match err {
        api::ApiError::Http { message, .. } if !message.trim().is_empty() => message.clone(),
        api::ApiError::Transport { .. } => err.user_message(),
        _ => fallback.to_string(),
    }
}

/// Digits only, at most six (the code field's filter).
fn normalize_code(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).take(6).collect()
}

/// Web `DialogDescription`, word for word.
fn step_description(step: &Step) -> String {
    match step {
        Step::Address => "We'll send a code to the new address. Sign-in codes, notifications \
                          and @mentions use it once confirmed; your old address stops working \
                          for sign-in."
            .to_string(),
        Step::Code { new_email } => {
            format!("Enter the 6-digit code we sent to {new_email}. It expires in 10 minutes.")
        }
    }
}

impl Render for ChangeEmailDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let label = |text: &'static str| div().text_xs().text_color(muted).child(text);
        let mut body = v_flex().gap_3().child(
            div()
                .text_sm()
                .text_color(muted)
                .child(step_description(&self.step)),
        );
        let primary = match &self.step {
            Step::Address => {
                body = body.child(
                    v_flex()
                        .gap_1()
                        .child(label("New email"))
                        .child(glass_input(&self.email, window, cx).web_input()),
                );
                let empty = self.email.read(cx).value().trim().is_empty();
                Button::new("change-email-send")
                    .primary()
                    .cursor_pointer()
                    .web_sm()
                    .label(if self.busy { "Sending…" } else { "Send code" })
                    .loading(self.busy)
                    .disabled(self.busy || empty)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
            }
            Step::Code { .. } => {
                body = body
                    .child(
                        v_flex()
                            .gap_1()
                            .child(label("Code"))
                            .child(glass_input(&self.code, window, cx).web_input()),
                    )
                    .child(
                        h_flex()
                            .gap_4()
                            .child(Self::text_link(
                                "change-email-resend",
                                "Resend code",
                                cx,
                                |this, window, cx| {
                                    if let Step::Code { new_email } = this.step.clone() {
                                        this.send_code(new_email, window, cx);
                                    }
                                },
                            ))
                            .child(Self::text_link(
                                "change-email-different",
                                "Use a different email",
                                cx,
                                |this, _window, cx| this.back_to_address(cx),
                            )),
                    );
                let short = self.code.read(cx).value().trim().len() < 6;
                Button::new("change-email-confirm")
                    .primary()
                    .cursor_pointer()
                    .web_sm()
                    .label(if self.busy { "Checking…" } else { "Confirm" })
                    .loading(self.busy)
                    .disabled(self.busy || short)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
            }
        };
        if let Some(error) = self.error.clone() {
            body = body.child(div().text_sm().text_color(cx.theme().danger).child(error));
        }
        let footer = h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("change-email-cancel")
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label("Cancel")
                    .disabled(self.busy)
                    .on_click(|_, window, cx| native_dialog::close_dialog_window(window, cx)),
            )
            .child(primary);
        v_flex().gap_4().child(body).child(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_field_keeps_six_digits_only() {
        assert_eq!(normalize_code("12 34-56"), "123456");
        assert_eq!(normalize_code("1234567"), "123456");
        assert_eq!(normalize_code("abc"), "");
    }

    #[test]
    fn descriptions_follow_the_web_dialog() {
        assert_eq!(
            step_description(&Step::Address),
            "We'll send a code to the new address. Sign-in codes, notifications and \
             @mentions use it once confirmed; your old address stops working for sign-in."
        );
        assert_eq!(
            step_description(&Step::Code {
                new_email: "new@example.com".to_string()
            }),
            "Enter the 6-digit code we sent to new@example.com. It expires in 10 minutes."
        );
    }

    #[test]
    fn transport_faults_never_leak_raw() {
        let err = api::ApiError::Decode("boom".to_string());
        assert_eq!(
            error_copy(&err, "Couldn't send the code."),
            "Couldn't send the code."
        );
        let err = api::ApiError::Http {
            status: 400,
            message: "That code is not right. Check the email and try again.".to_string(),
        };
        assert_eq!(
            error_copy(&err, "x"),
            "That code is not right. Check the email and try again."
        );
    }
}
