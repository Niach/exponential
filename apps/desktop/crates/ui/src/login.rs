//! Minimal functional login surface (masterplan-v3 §5.7 mechanics; Phase-2
//! placeholder — the §4.2/EXP-5 pixel-parity auth screen with the leading
//! cloud button + OAuth rows replaces this in Phase 3).
//!
//! Server URL + email/password over gpui-component `Input`/`Button`, calling
//! `api::AuthClient::sign_in_with_password` on a background task, persisting
//! the token via `api::AuthStore` (0600 file store), then starting sync
//! through [`crate::session::connect_account`]. The workspace renders this
//! view whenever the session machine is not `Synced` — including
//! `AuthExpired`, the EXP-1 #13(b) dead-token routing (never an empty board).

use gpui::{
    div, App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};
use sync::{SessionPhase, Store};

use crate::session::{connect_account, AuthContext};

/// Default instance for the server-URL field when no account is remembered.
const DEFAULT_INSTANCE: &str = "https://app.exponential.at";

pub struct LoginView {
    server: Entity<InputState>,
    email: Entity<InputState>,
    password: Entity<InputState>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl LoginView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Prefill from the last remembered account (login after sign-out /
        // session expiry should not retype the instance + email).
        let remembered = cx
            .try_global::<AuthContext>()
            .and_then(|auth| auth.auth.accounts().into_iter().next_back());
        let (server_prefill, email_prefill) = remembered
            .map(|account| (account.instance_url, account.email))
            .unwrap_or_else(|| (DEFAULT_INSTANCE.to_string(), String::new()));

        let server = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(DEFAULT_INSTANCE)
                .default_value(server_prefill)
        });
        let email = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("you@example.com")
                .default_value(email_prefill)
        });
        let password = cx.new(|cx| InputState::new(window, cx).placeholder("Password").masked(true));

        let mut subscriptions = Vec::new();
        // Enter in any field submits (functional-placeholder ergonomics).
        for input in [&server, &email, &password] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.submit(window, cx);
                    }
                },
            ));
        }
        // Re-render on session-phase changes (SigningIn busy state,
        // AuthExpired banner).
        let state = Store::global(cx).state();
        subscriptions.push(cx.observe(&state, |_, _, cx| cx.notify()));

        Self {
            server,
            email,
            password,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    fn submit(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let store = Store::global(cx).clone();
        if store.session(cx) == SessionPhase::SigningIn {
            return;
        }

        let server = self.server.read(cx).value().trim().to_string();
        let email = self.email.read(cx).value().trim().to_string();
        let password = self.password.read(cx).value().to_string();
        if server.is_empty() || email.is_empty() || password.is_empty() {
            self.error = Some("Server, email and password are required.".into());
            cx.notify();
            return;
        }

        self.error = None;
        store.begin_sign_in(cx);
        cx.notify();

        let auth = cx.global::<AuthContext>().clone();
        // App-level task (NOT spawn_in): the session transition must complete
        // even if the initiating window/view is closed mid-flight — a
        // view-bound update would silently drop the result and wedge the §5
        // state machine in `SigningIn` forever (every window's login surface
        // renders disabled). The error message is a best-effort view update.
        cx.spawn(async move |this, cx| {
            // Blocking HTTP on a background thread (§3.5 — never on the
            // foreground executor).
            let client = auth.client.clone();
            let (server_bg, email_bg) = (server.clone(), email.clone());
            let result = cx
                .background_executor()
                .spawn(async move { client.sign_in_with_password(&server_bg, &email_bg, &password) })
                .await;

            let error: Option<String> = cx.update(|cx| {
                let store = Store::global(cx).clone();
                match result {
                    Ok(success) => {
                        match auth.auth.sign_in(&server, &success.token, &success.user) {
                            Ok(account) => {
                                if connect_account(&account, cx) {
                                    None
                                } else {
                                    Some(
                                        "Signed in, but the local sync store failed to open."
                                            .to_string(),
                                    )
                                }
                            }
                            Err(err) => {
                                store.abort_sign_in(cx);
                                Some(format!("Could not store the session: {err}"))
                            }
                        }
                    }
                    Err(err) => {
                        store.abort_sign_in(cx);
                        Some(sign_in_error_message(&err))
                    }
                }
            });

            if let Some(message) = error {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(message.into());
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

/// Human-readable sign-in failure (a bad credential is an HTTP 401 from the
/// sign-in endpoint — see `AuthClient::sign_in_with_password`).
fn sign_in_error_message(err: &api::ApiError) -> String {
    match err {
        api::ApiError::Http { status: 401, .. } | api::ApiError::Unauthorized => {
            "Invalid email or password.".to_string()
        }
        other => format!("Sign-in failed: {other}"),
    }
}

impl Render for LoginView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let session = Store::global(cx).session(cx);
        let signing_in = session == SessionPhase::SigningIn;
        let expired = matches!(session, SessionPhase::AuthExpired { .. });

        let mut form = v_flex()
            .w(gpui::px(360.))
            .gap_3()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Sign in to Exponential"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Enter your instance URL and credentials."),
            );

        if expired {
            // The EXP-1 #13(b) surface: the dead token routed HERE, with the
            // reason visible — not to an empty board.
            form = form.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().warning_foreground)
                    .bg(cx.theme().warning)
                    .px_3()
                    .py_2()
                    .rounded(cx.theme().radius)
                    .child("Your session expired. Please sign in again."),
            );
        }

        form = form
            .child(labeled(cx, "Server", Input::new(&self.server).small()))
            .child(labeled(cx, "Email", Input::new(&self.email).small()))
            .child(labeled(cx, "Password", Input::new(&self.password).small()));

        if let Some(error) = &self.error {
            form = form.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }

        form = form.child(
            Button::new("login-submit")
                .primary()
                .label(if signing_in { "Signing in…" } else { "Sign in" })
                .loading(signing_in)
                .disabled(signing_in)
                .w_full()
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
        );

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                form.p_6()
                    .rounded(cx.theme().radius_lg)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().popover),
            )
    }
}

fn labeled(cx: &App, label: &'static str, input: Input) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(input)
}
