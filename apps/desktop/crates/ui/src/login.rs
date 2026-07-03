//! The auth screen (masterplan-v3 §4.2 "Auth" / §5.7 / EXP-5 — pixel parity
//! with `apps/web/src/routes/auth/login.tsx` + `auth-form-shell.tsx` +
//! `oauth-provider-buttons.tsx` at compact density).
//!
//! Layout, top to bottom, mirroring the web card with the native
//! instance-picker addition (§4.2):
//!
//! - the **logo + "Exponential" wordmark** centered above the card
//!   (web `AuthFormShell`: `ExponentialLogo size=32` + text-xl semibold),
//! - a centered `Card`: title "Sign in" + description,
//! - the **native instance picker the web does not need** — the
//!   **Exponential Cloud choice comes FIRST** (EXP-5: the Linux login was
//!   missing the leading cloud button), then Self-hosted with a base-URL
//!   `Input`,
//! - `OAuthProviderButtons`: the configured OIDC providers then Google
//!   (web order), each `outline` full-width, gated on `GET /api/auth-config`
//!   of the chosen instance; an "or" divider when a password form follows,
//! - the email/password form (labels + inputs — the password `Input` carries
//!   the web `PasswordInput` show/hide **eye toggle** — + full-width submit)
//!   and the web footer **"Don't have an account? Register"** link (opens
//!   the instance's `/auth/register` in the browser), both shown only when
//!   the instance has password auth enabled.
//!
//! OAuth opens the system browser through the `api::opener` chain (§5.7 —
//! never a raw `xdg-open`); when the ENTIRE chain fails the URL surfaces in
//! a **copyable row** (EXP-5: degrade to copy-paste, never a dead end). The
//! callback lands via `on_open_urls` → [`crate::oauth::handle_open_urls`].
//!
//! The workspace renders this view whenever the session machine is not
//! `Synced` — including `AuthExpired`, the EXP-1 #13(b) dead-token routing
//! (never an empty board).

use gpui::{
    div, App, AppContext as _, ClipboardItem, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::{SessionPhase, Store};

use crate::icons::ExpIcon;
use crate::session::{connect_account, AuthContext};

/// The cloud instance (§4.2: "Exponential Cloud (`app.exponential.at`)").
const CLOUD_INSTANCE: &str = "https://app.exponential.at";

/// Which instance the user is signing in to (cloud FIRST — EXP-5).
#[derive(Clone, Copy, PartialEq, Eq)]
enum InstanceChoice {
    Cloud,
    SelfHosted,
}

pub struct LoginView {
    choice: InstanceChoice,
    server: Entity<InputState>,
    email: Entity<InputState>,
    password: Entity<InputState>,
    /// `GET /api/auth-config` of `config_for` (§5.7 step 1 — gates which
    /// methods render). `None` until the first fetch lands; the password
    /// form defaults to visible meanwhile (iOS-parity tolerance).
    auth_config: Option<api::AuthConfig>,
    config_for: Option<String>,
    /// OAuth attempt in flight (browser opened; label parity with web's
    /// "Redirecting…"). Buttons stay enabled so an abandoned browser tab
    /// never wedges the login.
    pending_provider: Option<String>,
    /// EXP-5 degradation: the OAuth URL to copy when the opener chain failed.
    copy_url: Option<SharedString>,
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
            .unwrap_or_else(|| (String::new(), String::new()));
        let choice = if server_prefill.is_empty() || server_prefill == CLOUD_INSTANCE {
            InstanceChoice::Cloud
        } else {
            InstanceChoice::SelfHosted
        };

        let server = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://exponential.example.com")
                .default_value(if server_prefill == CLOUD_INSTANCE {
                    String::new()
                } else {
                    server_prefill
                })
        });
        let email = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("you@example.com")
                .default_value(email_prefill)
        });
        let password =
            cx.new(|cx| InputState::new(window, cx).placeholder("Password").masked(true));

        let mut subscriptions = Vec::new();
        // Enter in email/password submits (web form submit).
        for input in [&email, &password] {
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
        // Self-hosted URL commit (Enter or blur) refreshes the auth-config.
        subscriptions.push(cx.subscribe_in(
            &server,
            window,
            |this, _, event: &InputEvent, _window, cx| match event {
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    this.fetch_auth_config(cx);
                }
                _ => {}
            },
        ));
        // Re-render on session-phase changes (SigningIn busy state,
        // AuthExpired banner, OAuth completion).
        let state = Store::global(cx).state();
        subscriptions.push(cx.observe(&state, |_, _, cx| cx.notify()));

        let mut this = Self {
            choice,
            server,
            email,
            password,
            auth_config: None,
            config_for: None,
            pending_provider: None,
            copy_url: None,
            error: None,
            _subscriptions: subscriptions,
        };
        this.fetch_auth_config(cx);
        this
    }

    /// The instance URL sign-in targets right now. `None` = self-hosted with
    /// an empty URL field.
    fn effective_instance(&self, cx: &App) -> Option<String> {
        match self.choice {
            InstanceChoice::Cloud => Some(CLOUD_INSTANCE.to_string()),
            InstanceChoice::SelfHosted => {
                let raw = self.server.read(cx).value().trim().to_string();
                (!raw.is_empty()).then(|| api::login::normalize_instance_url(&raw))
            }
        }
    }

    fn set_choice(&mut self, choice: InstanceChoice, cx: &mut gpui::Context<Self>) {
        if self.choice == choice {
            return;
        }
        self.choice = choice;
        self.copy_url = None;
        self.fetch_auth_config(cx);
        cx.notify();
    }

    /// §5.7 step 1: `GET /api/auth-config` for the chosen instance (stale
    /// responses are dropped by comparing the fetched-for URL).
    fn fetch_auth_config(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(instance) = self.effective_instance(cx) else {
            self.auth_config = None;
            self.config_for = None;
            return;
        };
        if self.config_for.as_deref() == Some(instance.as_str()) {
            return; // already have (or are fetching) this instance's config
        }
        self.config_for = Some(instance.clone());
        self.auth_config = None;

        let auth = cx.global::<AuthContext>().clone();
        cx.spawn(async move |this, cx| {
            let client = auth.client.clone();
            let fetch_for = instance.clone();
            let result = cx
                .background_executor()
                .spawn(async move { client.fetch_auth_config(&fetch_for) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.config_for.as_deref() != Some(instance.as_str()) {
                    return; // instance changed while in flight
                }
                match result {
                    Ok(config) => this.auth_config = Some(config),
                    Err(err) => {
                        // Tolerant: the form stays usable on its defaults.
                        log::warn!("[ui] auth-config fetch failed for {instance}: {err}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- OAuth (§5.7 / EXP-5) -------------------------------------------------

    fn sign_in_with_google(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(instance) = self.effective_instance(cx) else {
            self.error = Some("Enter your server URL first.".into());
            cx.notify();
            return;
        };
        let url = api::login::google_oauth_start_url(&instance);
        self.launch_oauth("google", instance, url, cx);
    }

    fn sign_in_with_oidc(&mut self, provider_id: String, cx: &mut gpui::Context<Self>) {
        let Some(instance) = self.effective_instance(cx) else {
            self.error = Some("Enter your server URL first.".into());
            cx.notify();
            return;
        };
        let url = api::login::oidc_oauth_start_url(&instance, &provider_id);
        self.launch_oauth(&provider_id, instance, url, cx);
    }

    fn launch_oauth(
        &mut self,
        provider: &str,
        instance: String,
        url: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.error = None;
        self.copy_url = None;
        self.pending_provider = Some(provider.to_string());
        match crate::oauth::start(instance, url, cx) {
            Ok(()) => {}
            Err(url) => {
                // EXP-5: the whole opener chain failed — degrade to a
                // copyable URL, never a dead end.
                self.pending_provider = None;
                self.copy_url = Some(url.into());
                self.error =
                    Some("Couldn't open your browser. Open this link manually:".into());
            }
        }
        cx.notify();
    }

    /// Web footer "Register" link — the desktop opens the instance's
    /// `/auth/register` in the system browser (registration is a web flow).
    fn open_register(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(instance) = self.effective_instance(cx) else {
            self.error = Some("Enter your server URL first.".into());
            cx.notify();
            return;
        };
        let url = format!("{}/auth/register", instance.trim_end_matches('/'));
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = api::opener::open_in_browser(&url) {
                    log::warn!("[ui] register: browser open failed: {err}");
                }
            })
            .detach();
    }

    // -- Password (§5.7 step 3) -------------------------------------------------

    fn submit(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let store = Store::global(cx).clone();
        if store.session(cx) == SessionPhase::SigningIn {
            return;
        }

        let Some(server) = self.effective_instance(cx) else {
            self.error = Some("Enter your server URL first.".into());
            cx.notify();
            return;
        };
        let email = self.email.read(cx).value().trim().to_string();
        let password = self.password.read(cx).value().to_string();
        if email.is_empty() || password.is_empty() {
            self.error = Some("Email and password are required.".into());
            cx.notify();
            return;
        }

        self.error = None;
        self.copy_url = None;
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
                .spawn(
                    async move { client.sign_in_with_password(&server_bg, &email_bg, &password) },
                )
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

    // -- render pieces ----------------------------------------------------------

    /// The native instance picker (§4.2): **cloud first** (EXP-5), then
    /// self-hosted with the URL input.
    fn render_instance_picker(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let cloud = self.choice == InstanceChoice::Cloud;
        let mut section = v_flex().gap_2().child(
            h_flex()
                .gap_2()
                .child(
                    // The CLOUD button comes FIRST (EXP-5).
                    Button::new("login-instance-cloud")
                        .small()
                        .flex_1()
                        .label("Exponential Cloud")
                        .map(|b| if cloud { b.primary() } else { b.outline() })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_choice(InstanceChoice::Cloud, cx);
                        })),
                )
                .child(
                    Button::new("login-instance-self-hosted")
                        .small()
                        .flex_1()
                        .label("Self-hosted")
                        .map(|b| if cloud { b.outline() } else { b.primary() })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_choice(InstanceChoice::SelfHosted, cx);
                        })),
                ),
        );
        if !cloud {
            section = section.child(labeled(cx, "Server URL", Input::new(&self.server).small()));
        }
        section
    }

    /// Web `OAuthProviderButtons`: OIDC providers then Google, each outline
    /// full-width; "or" divider when the password form follows.
    fn render_oauth_buttons(
        &self,
        config: &api::AuthConfig,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement> {
        if config.oidc_providers.is_empty() && !config.google_login_enabled {
            return None;
        }

        let mut section = v_flex().gap_3();
        for provider in &config.oidc_providers {
            let pending = self.pending_provider.as_deref() == Some(provider.id.as_str());
            let label = if pending {
                "Waiting for your browser…".to_string()
            } else {
                format!("Sign in with {}", provider.name)
            };
            let provider_id = provider.id.clone();
            section = section.child(
                Button::new(SharedString::from(format!("login-oidc-{}", provider.id)))
                    .outline()
                    .w_full()
                    .label(SharedString::from(label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sign_in_with_oidc(provider_id.clone(), cx);
                    })),
            );
        }
        if config.google_login_enabled {
            let pending = self.pending_provider.as_deref() == Some("google");
            section = section.child(
                Button::new("login-google")
                    .outline()
                    .w_full()
                    .label(if pending {
                        "Waiting for your browser…"
                    } else {
                        "Sign in with Google"
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.sign_in_with_google(cx))),
            );
        }

        if config.password_enabled {
            // The web "or" divider (auth-form-shell parity).
            section = section.child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(div().flex_1().h_px().bg(cx.theme().border))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("OR"),
                    )
                    .child(div().flex_1().h_px().bg(cx.theme().border)),
            );
        }
        Some(section)
    }

    /// EXP-5 degradation row: the OAuth URL with a Copy button.
    fn render_copy_url(&self, url: &SharedString, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let url_for_copy = url.clone();
        h_flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .py_1()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .overflow_x_hidden()
                    .child(url.clone()),
            )
            .child(
                Button::new("login-copy-oauth-url")
                    .outline()
                    .xsmall()
                    .label("Copy")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            url_for_copy.to_string(),
                        ));
                    })),
            )
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

        // Defaults (password on, no OAuth) until the instance's auth-config
        // lands — the form never blocks on the fetch.
        let config = self
            .auth_config
            .clone()
            .unwrap_or_else(default_auth_config);
        let password_enabled = config.password_enabled;

        // -- card header (web AuthFormShell: title + description) -----------
        let mut form = v_flex()
            .w(gpui::px(360.))
            .gap_4()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Sign in"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if password_enabled {
                                "Enter your email and password to continue"
                            } else {
                                "Sign in with your account"
                            }),
                    ),
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

        // -- instance picker (cloud FIRST — EXP-5) ---------------------------
        form = form.child(self.render_instance_picker(cx));

        // -- OAuth provider buttons (per auth-config) ------------------------
        if let Some(oauth) = self.render_oauth_buttons(&config, cx) {
            form = form.child(oauth);
        }

        // -- copyable-URL degradation (EXP-5) --------------------------------
        if let Some(error) = &self.error {
            form = form.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        if let Some(url) = self.copy_url.clone() {
            form = form.child(self.render_copy_url(&url, cx));
        }

        // -- password form ----------------------------------------------------
        if password_enabled {
            form = form
                .child(labeled(cx, "Email", Input::new(&self.email).small()))
                .child(labeled(
                    cx,
                    "Password",
                    // Web `PasswordInput`: show/hide eye toggle.
                    Input::new(&self.password).small().mask_toggle(),
                ))
                .child(
                    Button::new("login-submit")
                        .primary()
                        .label(if signing_in { "Signing in…" } else { "Sign in" })
                        .loading(signing_in)
                        .disabled(signing_in)
                        .w_full()
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                )
                .child(
                    // Web login footer: "Don't have an account? Register".
                    h_flex()
                        .gap_1()
                        .justify_center()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Don't have an account?")
                        .child(
                            div()
                                .id("login-register")
                                .text_color(cx.theme().primary)
                                .cursor_pointer()
                                .hover(|style| style.text_decoration_1())
                                .child("Register")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_register(cx);
                                })),
                        ),
                );
        }

        // Web `AuthFormShell`: logo + wordmark centered above the card.
        let brand = h_flex()
            .gap_2()
            .items_center()
            .justify_center()
            .child(
                Icon::from(ExpIcon::Logo)
                    .with_size(gpui::px(32.))
                    .text_color(cx.theme().foreground),
            )
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Exponential"),
            );

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .gap_6()
                    .items_center()
                    .child(brand)
                    .child(
                        form.p_6()
                            .rounded(cx.theme().radius_lg)
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().popover),
                    ),
            )
    }
}

/// The pre-fetch defaults (mirror `AuthConfig`'s serde defaults: password on,
/// nothing else).
fn default_auth_config() -> api::AuthConfig {
    serde_json::from_str("{}").expect("AuthConfig defaults decode")
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

use gpui::prelude::FluentBuilder as _;
