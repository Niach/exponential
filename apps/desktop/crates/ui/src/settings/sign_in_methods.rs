//! Settings → Account → Sign-in methods + Passkeys (EXP-1126).
//!
//! Web parity: `components/account/sign-in-methods-section.tsx` + the
//! passkeys band under it. ONE list of every way into the account: the code
//! to the primary email (changeable, [`super::change_email_dialog`]), the
//! instance's providers (Apple, Google, OIDC) with Link/Unlink, and a
//! Password row while one is set; then the account's passkeys. Everything
//! reads `users.signInMethods` (server-only, so tRPC — never a shape) and
//! every removal goes through tRPC so the server's last-way-in rule answers
//! with its own sentence.
//!
//! Linking runs the browser handoff in LINK mode: a short-lived ticket
//! (`users.mintSignInLinkTicket`) → `api::login::link_start_url` →
//! [`crate::oauth::start_link`]; the `exponential://oauth-return?linked=…`
//! hand-back lands on the App and reaches this section through the
//! [`SignInLinkOutcome`] global. The desktop registers no passkeys (no native
//! WebAuthn ceremony): adding one is a web errand, removing one works here.

use gpui::{
    div, prelude::FluentBuilder as _, AnyElement, App, ClipboardItem, Entity, FontWeight,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant},
    h_flex,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::toast::Toast;
use api::users::{SignInMethods, SignInPasskey, SignInProvider};

use crate::controls::WebControl as _;
use crate::icons::{registry, ExpIcon};
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::Navigation;
use crate::oauth::SignInLinkOutcome;
use crate::queries;

use super::storage::format_created_date;
use super::{error_notice, form_error, open_url, section};

enum Load {
    Idle,
    Loading,
    Ready(SignInMethods),
    Error(String),
}

pub struct SignInMethodsSection {
    nav: Entity<Navigation>,
    load: Load,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// The account the loaded methods belong to — a re-login must re-fetch.
    account_id: Option<String>,
    /// An unlink / passkey removal in flight.
    busy: bool,
    /// The provider whose link attempt is waiting on the browser.
    pending_link: Option<String>,
    /// Opener-degradation: the link start URL to copy by hand.
    copy_url: Option<SharedString>,
    /// The last [`SignInLinkOutcome::seq`] adopted.
    outcome_seq: u64,
    _subscriptions: Vec<Subscription>,
}

impl SignInMethodsSection {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let outcome_seq = cx
            .try_global::<SignInLinkOutcome>()
            .map(|outcome| outcome.seq)
            .unwrap_or(0);
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // The browser hand-back lands on the App — the settings window
            // may even have been closed and reopened since the click.
            cx.observe_global::<SignInLinkOutcome>(|this, cx| this.adopt_link_outcome(cx)),
        ];
        Self {
            nav,
            load: Load::Idle,
            generation: 0,
            account_id: None,
            busy: false,
            pending_link: None,
            copy_url: None,
            outcome_seq,
            _subscriptions: subscriptions,
        }
    }

    /// Server read — refetch on every pane entry.
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.load, Load::Ready(_) | Load::Error(_)) {
            self.load = Load::Idle;
        }
        self.copy_url = None;
        cx.notify();
    }

    /// Drop the loaded state and read again (after a mutation / a link).
    pub(crate) fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }

    /// The change-email dialog's success: say so, re-read.
    pub(crate) fn email_changed(&mut self, email: &str, cx: &mut gpui::Context<Self>) {
        crate::toast::show_in_active_window(
            Toast::success(format!("Your email is now {email}.")),
            cx,
        );
        self.refetch(cx);
    }

    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let account_id = sync::Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.load = Load::Idle;
            self.pending_link = None;
            self.copy_url = None;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::users::sign_in_methods(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = match result {
                    Ok(methods) => Load::Ready(methods),
                    Err(err) => {
                        log::warn!("[ui] users.signInMethods failed: {err}");
                        Load::Error(form_error(&err, "Couldn't load your sign-in methods."))
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn adopt_link_outcome(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(outcome) = cx.try_global::<SignInLinkOutcome>() else {
            return;
        };
        if outcome.seq == self.outcome_seq {
            return;
        }
        self.outcome_seq = outcome.seq;
        // An attempt another account started is not ours to report.
        if let (Some(owner), Some(mine)) = (&outcome.account_id, &self.account_id) {
            if owner != mine {
                return;
            }
        }
        let Some(result) = outcome.result.clone() else {
            return;
        };
        self.pending_link = None;
        self.copy_url = None;
        let toast = match result {
            Ok(provider_id) => {
                let name = self
                    .methods()
                    .and_then(|methods| methods.providers.iter().find(|p| p.id == provider_id))
                    .map(|provider| provider.name.clone())
                    .unwrap_or(provider_id);
                Toast::success(format!("{name} linked to your account."))
            }
            Err(message) => Toast::error(message),
        };
        crate::toast::show_in_active_window(toast, cx);
        self.refetch(cx);
    }

    fn methods(&self) -> Option<&SignInMethods> {
        match &self.load {
            Load::Ready(methods) => Some(methods),
            _ => None,
        }
    }

    /// Link: mint a ticket, then open the browser handoff in LINK mode.
    fn link(&mut self, provider: &SignInProvider, cx: &mut gpui::Context<Self>) {
        let (Some(trpc), Some(account)) = (queries::trpc_client(cx), queries::active_account(cx))
        else {
            return;
        };
        let provider_id = provider.id.clone();
        self.pending_link = Some(provider_id.clone());
        self.copy_url = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mint_id = provider_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::users::mint_sign_in_link_ticket(&trpc, &mint_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.pending_link.as_deref() != Some(provider_id.as_str()) {
                    return; // superseded
                }
                let ticket = match result {
                    Ok(ticket) => ticket,
                    Err(err) => {
                        log::warn!("[ui] users.mintSignInLinkTicket failed: {err}");
                        this.pending_link = None;
                        crate::toast::show_in_active_window(
                            Toast::error(form_error(&err, "Couldn't start linking. Try again.")),
                            cx,
                        );
                        cx.notify();
                        return;
                    }
                };
                let pkce = api::login::generate_pkce();
                let url = api::login::link_start_url(
                    &account.instance_url,
                    &ticket.ticket,
                    &provider_id,
                    &pkce.challenge,
                );
                if let Err(url) = crate::oauth::start_link(
                    account.id.clone(),
                    provider_id.clone(),
                    url,
                    pkce.verifier,
                    cx,
                ) {
                    // The whole opener chain failed — degrade to a copyable
                    // URL; the pending link is recorded, so the copied link
                    // still completes in this process.
                    this.pending_link = None;
                    this.copy_url = Some(url.into());
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Unlink / Remove password — confirmed, then `users.unlinkSignInMethod`.
    fn confirm_unlink(
        &mut self,
        provider: &SignInProvider,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let password = provider.kind == "password";
        let (title, description, ok_label) = if password {
            (
                "Remove your password?".to_string(),
                "You will no longer be able to sign in with a password. Your other \
                 sign-in methods keep working."
                    .to_string(),
                "Remove",
            )
        } else {
            (
                format!("Unlink {}?", provider.name),
                format!(
                    "{} will no longer sign you in. You can link it again any time; \
                     your other sign-in methods keep working.",
                    provider.name
                ),
                "Unlink",
            )
        };
        let provider_id = provider.id.clone();
        self.confirm_removal(
            title,
            description,
            ok_label,
            "Couldn't remove that sign-in method.",
            move |trpc| api::users::unlink_sign_in_method(trpc, &provider_id),
            window,
            cx,
        );
    }

    fn confirm_remove_passkey(
        &mut self,
        passkey: &SignInPasskey,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let id = passkey.id.clone();
        self.confirm_removal(
            "Remove passkey?".to_string(),
            format!(
                "{} will no longer sign you in. The copy on your device stays until \
                 you delete it there.",
                passkey_name(passkey)
            ),
            "Remove",
            "Couldn't remove the passkey.",
            move |trpc| api::users::delete_passkey(trpc, &id),
            window,
            cx,
        );
    }

    /// The shared danger confirm → background mutation → refetch, with the
    /// server's refusal (the last-way-in sentence) as an error notification.
    #[allow(clippy::too_many_arguments)]
    fn confirm_removal(
        &mut self,
        title: String,
        description: String,
        ok_label: &'static str,
        fallback: &'static str,
        call: impl Fn(&api::TrpcClient) -> Result<(), api::ApiError> + Send + Sync + 'static,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let section = cx.entity().downgrade();
        let handle = window.window_handle();
        let call = std::sync::Arc::new(call);
        let spec = AlertSpec::new(title, description, ok_label)
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, cx| {
                let Some(trpc) = queries::trpc_client(cx) else {
                    return true;
                };
                let _ = section.update(cx, |this, cx| {
                    this.busy = true;
                    cx.notify();
                });
                let section = section.clone();
                let call = call.clone();
                cx.spawn(async move |cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move { call(&trpc) })
                        .await;
                    let _ = section.update(cx, |this, cx| {
                        this.busy = false;
                        if result.is_ok() {
                            this.refetch(cx);
                        }
                        cx.notify();
                    });
                    if let Err(err) = result {
                        log::warn!("[ui] sign-in method removal failed: {err}");
                        let note = Toast::error(form_error(&err, fallback));
                        let _ = handle.update(cx, |_, window, cx| {
                            crate::toast::show(note, window, cx);
                        });
                    }
                })
                .detach();
                true
            });
        open_alert(window, cx, spec);
    }

    fn open_change_email(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(methods) = self.methods() else {
            return;
        };
        let current = methods.email.clone();
        let section = cx.entity().downgrade();
        super::change_email_dialog::open(window, cx, current, section);
    }

    // ---- rendering ----

    fn render_email_row(&self, methods: &SignInMethods, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let subtitle: SharedString = if methods.email_otp_enabled {
            methods.email.clone().into()
        } else {
            format!("{} · sign-in codes are off on this instance", methods.email).into()
        };
        let trailing = methods.email_otp_enabled.then(|| {
            Button::new("sign-in-email-change")
                .outline()
                .cursor_pointer()
                .web_sm()
                .label("Change")
                .on_click(cx.listener(|this, _, window, cx| this.open_change_email(window, cx)))
                .into_any_element()
        });
        method_row(
            Some(glyph(Icon::from(registry::UI_MAIL), cx)),
            "Email code".into(),
            subtitle,
            trailing,
            cx,
        )
    }

    fn render_provider_row(
        &self,
        methods: &SignInMethods,
        provider: &SignInProvider,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let blocked = provider.linked && !api::users::can_unlink(methods, provider);
        let subtitle: SharedString = provider_subtitle(provider, blocked).into();
        let leading = match provider.kind.as_str() {
            // Apple's mark is monochrome — the tinted Icon pipeline (login.rs).
            "apple" => Some(glyph(Icon::from(ExpIcon::Apple), cx)),
            // The full-colour "G" goes through `img()` (login.rs, EXP-9).
            "google" => Some(
                gpui::img("icons/google.svg")
                    .size_4()
                    .flex_none()
                    .into_any_element(),
            ),
            _ => None,
        };
        let id = SharedString::from(format!("sign-in-provider-{}", provider.id));
        let trailing = if provider.linked {
            let target = provider.clone();
            Button::new(id)
                .outline()
                .cursor_pointer()
                .web_sm()
                .text_color(cx.theme().danger)
                .label(if provider.kind == "password" {
                    "Remove"
                } else {
                    "Unlink"
                })
                .disabled(blocked || self.busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.confirm_unlink(&target, window, cx);
                }))
                .into_any_element()
        } else {
            let target = provider.clone();
            let waiting = self.pending_link.as_deref() == Some(provider.id.as_str());
            Button::new(id)
                .outline()
                .cursor_pointer()
                .web_sm()
                .label(if waiting {
                    "Waiting for your browser…"
                } else {
                    "Link"
                })
                .disabled(!provider.available)
                .on_click(cx.listener(move |this, _, _, cx| this.link(&target, cx)))
                .into_any_element()
        };
        method_row(
            leading,
            provider.name.clone().into(),
            subtitle,
            Some(trailing),
            cx,
        )
    }

    fn render_passkey_row(
        &self,
        methods: &SignInMethods,
        passkey: &SignInPasskey,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        // A passkey counts as a way in only while passkeys are on.
        let blocked = methods.passkey_enabled && methods.ways_in <= 1;
        let target = passkey.clone();
        let trailing = Button::new(SharedString::from(format!("passkey-remove-{}", passkey.id)))
            .outline()
            .cursor_pointer()
            .web_sm()
            .text_color(cx.theme().danger)
            .label("Remove")
            .disabled(blocked || self.busy)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.confirm_remove_passkey(&target, window, cx);
            }))
            .into_any_element();
        method_row(
            Some(glyph(Icon::from(registry::AUTH_PASSKEY), cx)),
            passkey_name(passkey).into(),
            passkey_subtitle(passkey).into(),
            Some(trailing),
            cx,
        )
    }

    fn render_copy_url(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let url = self.copy_url.clone()?;
        let url_for_copy = url.clone();
        // The caption stays with the URL it explains; the transient messages
        // of this section are toasts (EXP-1031).
        let row = h_flex()
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
                    .border_color(super::row_stroke(cx))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .overflow_x_hidden()
                    .child(url),
            )
            .child(
                crate::surface::glass_pill_button(
                    "sign-in-copy-link-url",
                    crate::surface::PillSize::Sm,
                    cx,
                )
                .label("Copy")
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(url_for_copy.to_string()));
                }),
            );
        Some(
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Couldn't open your browser. Open this link manually:"),
                )
                .child(row)
                .into_any_element(),
        )
    }
}

impl Render for SignInMethodsSection {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);

        let mut methods_section = section(cx).child(crate::surface::glass_section_header(
            "Sign-in methods",
            None,
            cx,
        ));
        let mut passkeys_section =
            section(cx).child(crate::surface::glass_section_header("Passkeys", None, cx));

        let skeleton = || {
            v_flex()
                .gap_2()
                .child(crate::controls::skeleton().h_4().w_full())
                .child(crate::controls::skeleton().h_4().w_64())
        };

        let methods = match &self.load {
            Load::Ready(methods) => Some(methods.clone()),
            _ => None,
        };
        match (&self.load, methods) {
            (_, Some(methods)) => {
                let mut rows = vec![self.render_email_row(&methods, cx)];
                for provider in &methods.providers {
                    rows.push(self.render_provider_row(&methods, provider, cx));
                }
                let list = rows
                    .into_iter()
                    .enumerate()
                    .map(|(ix, row)| crate::surface::list_row(row, ix));
                methods_section = methods_section
                    .child(v_flex().w_full().min_w_0().children(list))
                    .children(self.render_copy_url(cx));

                if methods.passkeys.is_empty() {
                    passkeys_section = passkeys_section.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No passkeys yet. Add one from the web app."),
                    );
                } else {
                    let list = methods
                        .passkeys
                        .iter()
                        .map(|passkey| self.render_passkey_row(&methods, passkey, cx))
                        .collect::<Vec<_>>()
                        .into_iter()
                        .enumerate()
                        .map(|(ix, row)| crate::surface::list_row(row, ix));
                    passkeys_section =
                        passkeys_section.child(v_flex().w_full().min_w_0().children(list));
                }
            }
            (Load::Error(message), None) => {
                methods_section =
                    methods_section.child(error_notice(SharedString::from(message.clone()), cx));
                passkeys_section = passkeys_section.child(skeleton());
            }
            _ => {
                methods_section = methods_section.child(skeleton());
                passkeys_section = passkeys_section.child(skeleton());
            }
        }

        // Passkeys are added on the web (no native WebAuthn ceremony here) —
        // the same hand-off recipe as the widget pane (`widget.rs`).
        let slug = super::active_team(cx, &self.nav).and_then(|team| team.slug);
        if let (Some(slug), Some(account)) = (slug, queries::active_account(cx)) {
            let url = manage_on_web_url(&account.instance_url, &slug);
            passkeys_section = passkeys_section.child(
                h_flex().child(
                    Button::new("passkeys-manage")
                        .outline()
                        .cursor_pointer()
                        .web_sm()
                        .icon(registry::UI_EXTERNAL_LINK)
                        .label("Manage on the web")
                        .on_click(cx.listener(move |_, _, _, cx| {
                            open_url(cx, url.clone());
                        })),
                ),
            );
        }

        v_flex()
            .gap_6()
            .child(methods_section)
            .child(passkeys_section)
    }
}

/// The 32px leading slot's glyph, muted like the web's `text-muted-foreground`.
fn glyph(icon: Icon, cx: &App) -> AnyElement {
    icon.small()
        .text_color(cx.theme().muted_foreground)
        .into_any_element()
}

/// One flat row: the leading slot (kept even when empty so titles align),
/// title over a muted subtitle, the trailing control.
fn method_row(
    leading: Option<AnyElement>,
    title: SharedString,
    subtitle: SharedString,
    trailing: Option<AnyElement>,
    cx: &App,
) -> gpui::Div {
    crate::surface::flat_row()
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .child(
            div()
                .size_8()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .children(leading),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(subtitle),
                ),
        )
        .when_some(trailing, |row, trailing| {
            row.child(div().flex_shrink_0().child(trailing))
        })
}

/// Web copy, word for word (`sign-in-methods-section.tsx`).
fn provider_subtitle(provider: &SignInProvider, blocked: bool) -> String {
    if !provider.linked {
        return "Not linked".to_string();
    }
    if blocked {
        return "Linked · your only way to sign in".to_string();
    }
    if !provider.available {
        return "Linked · no longer offered on this instance".to_string();
    }
    match provider.linked_at.as_deref() {
        Some(at) => format!("Linked {}", format_created_date(at)),
        None => "Linked".to_string(),
    }
}

fn passkey_name(passkey: &SignInPasskey) -> String {
    passkey
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Passkey".to_string())
}

fn passkey_subtitle(passkey: &SignInPasskey) -> String {
    let mut subtitle = match passkey.created_at.as_deref() {
        Some(at) => format!("added {}", format_created_date(at)),
        None => "added".to_string(),
    };
    if passkey.backed_up {
        subtitle.push_str(" · synced across your devices");
    }
    subtitle
}

fn manage_on_web_url(instance_url: &str, team_slug: &str) -> String {
    format!(
        "{}/t/{team_slug}/settings/account",
        instance_url.trim_end_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(linked: bool, available: bool, linked_at: Option<&str>) -> SignInProvider {
        SignInProvider {
            id: "google".to_string(),
            name: "Google".to_string(),
            kind: "google".to_string(),
            available,
            linked,
            linked_at: linked_at.map(str::to_string),
        }
    }

    #[test]
    fn provider_subtitles_follow_the_web_copy() {
        assert_eq!(
            provider_subtitle(&provider(false, true, None), false),
            "Not linked"
        );
        assert_eq!(
            provider_subtitle(&provider(true, true, None), true),
            "Linked · your only way to sign in"
        );
        assert_eq!(
            provider_subtitle(&provider(true, false, None), false),
            "Linked · no longer offered on this instance"
        );
        assert_eq!(
            provider_subtitle(
                &provider(true, true, Some("2026-09-01T10:00:00.000Z")),
                false
            ),
            "Linked Sep 1, 2026"
        );
    }

    #[test]
    fn passkey_rows_name_and_describe_themselves() {
        let passkey = SignInPasskey {
            id: "pk".to_string(),
            name: None,
            created_at: Some("2026-09-02T00:00:00.000Z".to_string()),
            backed_up: true,
        };
        assert_eq!(passkey_name(&passkey), "Passkey");
        assert_eq!(
            passkey_subtitle(&passkey),
            "added Sep 2, 2026 · synced across your devices"
        );
        let named = SignInPasskey {
            name: Some("MacBook".to_string()),
            backed_up: false,
            ..passkey
        };
        assert_eq!(passkey_name(&named), "MacBook");
        assert_eq!(passkey_subtitle(&named), "added Sep 2, 2026");
    }

    #[test]
    fn manage_link_points_at_the_web_account_settings() {
        assert_eq!(
            manage_on_web_url("https://app.exponential.at/", "acme"),
            "https://app.exponential.at/t/acme/settings/account"
        );
    }
}
