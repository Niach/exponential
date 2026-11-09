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
//!
//! #EXP-1629: the section's bands are part of the Account pane's document
//! ([`super::account`]): the section keeps its state and writes,
//! [`SignInMethodsSection::project`] is its share of the data model at
//! `/pane`, [`document`] its nodes (the IDE's bands, flat rows and outline
//! buttons written in), and every press an action the pane hands back to
//! [`SignInMethodsSection::action`] (the confirms and the change-email
//! dialog need its window). Apple's monochrome mark and Google's
//! full-colour "G" are the IDE's own assets, painted through the host's
//! icon hooks ([`marks_plugin`]).

use std::rc::Rc;

use eui_gpui::host::HostPlugin;
use gpui::{App, ClipboardItem, Entity, IntoElement as _, Styled as _, Subscription, Window};
use gpui_component::{ActiveTheme as _, IconNamed as _};
use serde_json::{json, Value};

use crate::toast::Toast;
use api::users::{SignInMethods, SignInPasskey, SignInProvider};

use crate::icons::ExpIcon;
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::Navigation;
use crate::oauth::SignInLinkOutcome;
use crate::queries;

use super::doc::{and, empty_state, failed_state, hex, not, path, skeleton, Ink, SM, XS};
use super::open_url;
use super::storage::format_created_date;

/// The methods band when its load failed (FEED-79: the empty state + Retry).
const NO_SIGN_IN_METHODS: &str = "No sign-in methods";

/// The host icon names of the two marks the IDE paints itself.
const MARK_APPLE: &str = "sign-in-apple";
const MARK_GOOGLE: &str = "sign-in-google";

enum Load {
    Idle,
    Loading,
    Ready(SignInMethods),
    Error,
}

/// A state the pictures show (`settings_bench`): no methods = the failed
/// load, or the first load with `loading`.
#[cfg(test)]
pub(crate) struct SignInShot {
    pub methods: Option<SignInMethods>,
    pub pending_link: Option<String>,
    pub copy_url: Option<String>,
    pub manage_url: Option<String>,
    pub loading: bool,
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
    copy_url: Option<String>,
    /// The last [`SignInLinkOutcome::seq`] adopted.
    outcome_seq: u64,
    /// FEED-79: the load in flight is the person's Retry press (its failure
    /// toasts); a load on entry or after a write fails quietly.
    retry_pressed: bool,
    /// The bench's web account page (headless has no team or account).
    #[cfg(test)]
    shot_manage_url: Option<String>,
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
            retry_pressed: false,
            #[cfg(test)]
            shot_manage_url: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: SignInShot, cx: &mut gpui::Context<Self>) {
        self.account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        self.load = match shot.methods {
            Some(methods) => Load::Ready(methods),
            None if shot.loading => Load::Loading,
            None => Load::Error,
        };
        self.pending_link = shot.pending_link;
        self.copy_url = shot.copy_url;
        self.shot_manage_url = shot.manage_url;
        cx.notify();
    }

    /// Server read — refetch on every pane entry.
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.load, Load::Ready(_) | Load::Error) {
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

    /// The failed load's Retry: the same read, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.refetch(cx);
    }

    /// The change-email dialog's success: say so, re-read.
    pub(crate) fn email_changed(&mut self, email: &str, cx: &mut gpui::Context<Self>) {
        crate::toast::show_in_active_window(
            Toast::success(format!("Your email is now {email}.")),
            cx,
        );
        self.refetch(cx);
    }

    pub(super) fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
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
                let attended = std::mem::take(&mut this.retry_pressed);
                this.load = match result {
                    Ok(methods) => Load::Ready(methods),
                    Err(err) => {
                        log::warn!("[ui] users.signInMethods failed: {err}");
                        // FEED-79: the empty state + Retry; only a Retry
                        // press that fails again toasts.
                        crate::action_error::failed_load(
                            "your sign-in methods",
                            attended,
                            crate::action_error::action_error_description(&err),
                            cx,
                        );
                        Load::Error
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
        // SLOP-25: the GitHub connection (Settings → Repositories) rides the
        // same link hop; unless this section started it or lists the
        // provider, the outcome is that surface's to report.
        let provider_id = outcome
            .provider_id
            .clone()
            .or_else(|| result.as_ref().ok().cloned());
        let mine = provider_id.as_deref().is_some_and(|provider_id| {
            self.pending_link.as_deref() == Some(provider_id)
                || self
                    .methods()
                    .is_some_and(|methods| methods.providers.iter().any(|p| p.id == provider_id))
        });
        if !mine {
            return;
        }
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
            Err(message) => crate::action_error::failed_toast("link", "the account", Some(message.to_string())),
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

    /// A document action: `row` (the `kind` of the row's button on the row
    /// `key`), `addPasskey`, `copy`, `retry`.
    pub(super) fn action(&mut self, name: &str, kind: &str, key: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match (name, kind) {
            ("row", "change") => self.open_change_email(window, cx),
            ("row", "link") => {
                if let Some(provider) = self.provider(key).filter(|p| !p.linked && p.available) {
                    self.link(&provider, cx);
                }
            }
            ("row", "unlink") if !self.busy => {
                if let Some(provider) = self.provider(key).filter(|p| p.linked) {
                    self.confirm_unlink(&provider, window, cx);
                }
            }
            ("row", "passkey") if !self.busy => {
                let passkey = self.methods().and_then(|methods| methods.passkeys.iter().find(|p| p.id == key).cloned());
                if let Some(passkey) = passkey {
                    self.confirm_remove_passkey(&passkey, window, cx);
                }
            }
            ("addPasskey", _) => {
                if let Some(url) = self.manage_url(cx) {
                    open_url(cx, url);
                }
            }
            ("copy", _) => {
                if let Some(url) = self.copy_url.clone() {
                    cx.write_to_clipboard(ClipboardItem::new_string(url));
                }
            }
            ("retry", _) => self.retry_load(cx),
            _ => {}
        }
    }

    fn provider(&self, id: &str) -> Option<SignInProvider> {
        self.methods()?.providers.iter().find(|p| p.id == id).cloned()
    }

    /// Link: mint a ticket, then open the browser handoff in LINK mode.
    fn link(&mut self, provider: &SignInProvider, cx: &mut gpui::Context<Self>) {
        let (Some(trpc), Some(account)) = (queries::trpc_client(cx), queries::active_account(cx))
        else {
            return;
        };
        let provider_id = provider.id.clone();
        let provider = provider.clone();
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
                        let section = cx.entity().downgrade();
                        let retry_provider = provider.clone();
                        crate::action_error::failed_action_with_retry(
                            "link",
                            &provider.name,
                            crate::action_error::action_error_description(&err),
                            move |_, cx| {
                                let provider = retry_provider.clone();
                                let _ = section.update(cx, |this, cx| this.link(&provider, cx));
                            },
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
                    this.copy_url = Some(url);
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
        let (window_title, prompt) = if provider.kind == "password" {
            ("Remove password", domain::prompts::remove_password())
        } else {
            (
                "Unlink sign-in method",
                domain::prompts::unlink_sign_in_method(&provider.name),
            )
        };
        let provider_id = provider.id.clone();
        let (verb, thing) = if provider.kind == "password" {
            ("remove", "the password".to_string())
        } else {
            ("unlink", provider.name.clone())
        };
        self.confirm_removal(
            window_title,
            prompt,
            (verb, thing),
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
            "Remove passkey",
            domain::prompts::remove_passkey(passkey.name.as_deref()),
            ("remove", "the passkey".to_string()),
            move |trpc| api::users::delete_passkey(trpc, &id),
            window,
            cx,
        );
    }

    /// The shared danger confirm (a prompts-fixture entry) → background
    /// mutation → refetch, with the server's refusal (the last-way-in
    /// sentence) as an error notification.
    fn confirm_removal(
        &mut self,
        window_title: &'static str,
        prompt: domain::prompts::Prompt,
        failure: (&'static str, String),
        call: impl Fn(&api::TrpcClient) -> Result<(), api::ApiError> + Send + Sync + 'static,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let section = cx.entity().downgrade();
        let handle = window.window_handle();
        let call = std::sync::Arc::new(call);
        let spec = AlertSpec::from_prompt(window_title, &prompt)
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
                let failure = failure.clone();
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
                        let note = crate::action_error::failed_toast(
                            failure.0,
                            &failure.1,
                            crate::action_error::action_error_description(&err),
                        );
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

    /// The web account page the band's "Add passkey" hands the errand to.
    fn manage_url(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if self.shot_manage_url.is_some() {
            return self.shot_manage_url.clone();
        }
        manage_on_web(cx, &self.nav)
    }

    /// The state as the document binds it.
    pub(super) fn project(&self, cx: &App) -> Value {
        let ready = self.methods();
        let mut methods = Vec::new();
        let mut passkeys = Vec::new();
        if let Some(m) = ready {
            methods.push(email_row(m));
            for provider in &m.providers {
                methods.push(provider_row(m, provider, self.pending_link.as_deref(), self.busy));
            }
            passkeys = m.passkeys.iter().map(|passkey| passkey_row(m, passkey, self.busy)).collect();
        }
        let failed = matches!(self.load, Load::Error);
        json!({
            "loading": matches!(self.load, Load::Idle | Load::Loading),
            "failed": failed,
            "ready": ready.is_some(),
            "methods": methods,
            // A failed load reads the passkey list as empty under it.
            "noPasskeys": failed || (ready.is_some() && passkeys.is_empty()),
            "hasPasskeys": !passkeys.is_empty(),
            "passkeys": passkeys,
            "hasCopyUrl": ready.is_some() && self.copy_url.is_some(),
            "copyUrl": self.copy_url.clone().unwrap_or_default(),
            "addPasskey": self.manage_url(cx).is_some(),
        })
    }
}

/// One row of a list as the document binds it: the leading glyph (an
/// `icons.json` concept, or one of the IDE's marks), title over subtitle,
/// the trailing outline button (`kind` = what its press does).
#[allow(clippy::too_many_arguments)]
fn row(key: &str, glyph: Option<&str>, mark: Option<&str>, title: &str, subtitle: &str, action: Option<(&str, &str)>, danger: bool, disabled: bool) -> Value {
    let (label, kind) = action.unwrap_or_default();
    json!({
        "key": key,
        "hasGlyph": glyph.is_some(),
        "glyph": glyph.unwrap_or_default(),
        "hasMark": mark.is_some(),
        "mark": mark.unwrap_or_default(),
        "title": title,
        "subtitle": subtitle,
        "hasAction": action.is_some(),
        "action": label,
        "kind": kind,
        "danger": danger,
        "disabled": disabled,
    })
}

fn email_row(methods: &SignInMethods) -> Value {
    let subtitle = if methods.email_otp_enabled {
        methods.email.clone()
    } else {
        // ONE wording ×4: "{email} · Off on this server".
        format!("{} · Off on this server", methods.email)
    };
    let action = methods.email_otp_enabled.then_some(("Change", "change"));
    row("email", Some("ui-mail"), None, "Email code", &subtitle, action, false, false)
}

fn provider_row(methods: &SignInMethods, provider: &SignInProvider, pending_link: Option<&str>, busy: bool) -> Value {
    let blocked = provider.linked && !api::users::can_unlink(methods, provider);
    let (glyph, mark) = match provider.kind.as_str() {
        // Apple's mark is monochrome — the tinted Icon pipeline (login.rs).
        "apple" => (Some(MARK_APPLE), None),
        // SLOP-7: GitHub as a sign-in method (GITHUB_LOGIN_ENABLED).
        "github" => (Some("ui-github"), None),
        // The full-colour "G" goes through `img()` (login.rs, EXP-9).
        "google" => (None, Some(MARK_GOOGLE)),
        _ => (None, None),
    };
    let (action, danger, disabled) = if provider.linked {
        let label = if provider.kind == "password" { "Remove" } else { "Unlink" };
        ((label, "unlink"), true, blocked || busy)
    } else {
        let waiting = pending_link == Some(provider.id.as_str());
        let label = if waiting { "Waiting for your browser…" } else { "Link" };
        ((label, "link"), false, !provider.available)
    };
    row(&provider.id, glyph, mark, &provider.name, &provider_subtitle(provider, blocked), Some(action), danger, disabled)
}

fn passkey_row(methods: &SignInMethods, passkey: &SignInPasskey, busy: bool) -> Value {
    // A passkey counts as a way in only while passkeys are on.
    let blocked = methods.passkey_enabled && methods.ways_in <= 1;
    row(&passkey.id, Some("auth-passkey"), None, &passkey_name(passkey), &passkey_subtitle(passkey, blocked), Some(("Remove", "passkey")), true, blocked || busy)
}

/// The colours and radii the document needs beyond [`Ink`]: gpui-component's
/// `theme.radius`, the rows' hairline and the outline `Button`'s paints.
struct Chrome {
    radius: f32,
    stroke: String,
    danger: String,
    button_fg: String,
    button_fill: String,
    button_hover: String,
    button_stroke: String,
    button_fill_disabled: String,
    button_stroke_disabled: String,
    button_fg_disabled: String,
}

impl Chrome {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            radius: f32::from(theme.radius),
            stroke: hex(super::row_stroke(cx)),
            danger: hex(theme.danger),
            button_fg: hex(theme.button_foreground),
            // The washes' few-percent alphas have no exact byte:
            // [`premultiplied_hex`]; the stroke is the token's own byte.
            button_fill: premultiplied_hex(theme.input_background()),
            button_hover: premultiplied_hex(gpui_component::Colorize::mix_oklab(&theme.input, theme.transparent, 0.5)),
            button_stroke: hex(theme.input),
            button_fill_disabled: premultiplied_hex(theme.input_background().opacity(0.5)),
            button_stroke_disabled: hex(theme.input.opacity(0.5)),
            button_fg_disabled: hex(theme.muted_foreground),
        }
    }
}

/// `#rrggbbaa` of a faint translucent colour that paints like the exact
/// one: the alpha rounds to its byte and the channels absorb the rounding,
/// so colour × alpha (what the quad adds over its ground) stays exact; at
/// a few percent alpha one channel step is a thirtieth of a pixel level.
fn premultiplied_hex(color: gpui::Hsla) -> String {
    let c = color.to_rgb();
    let alpha = (c.a.clamp(0., 1.) * 255.).round().max(1.);
    let channel = |v: f32| ((v * c.a * 255. / alpha) * 255.).round().clamp(0., 255.) as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", channel(c.r), channel(c.g), channel(c.b), alpha as u8)
}

/// A run of text cut to one line with an ellipsis.
fn one_line(ink: &Ink, id: &str, text: Value, rems: f32, color: &str) -> Value {
    let mut t = ink.text(id, text, rems, color);
    t["props"]["lines"] = json!(1);
    t["style"]["minWidth"] = json!(0);
    t["style"]["flexShrink"] = json!(1);
    t
}

/// The outline `Button` at `web_sm` (gpui-component's outline Default: the
/// input wash and stroke, 32px tall, `px 12`, its `text_sm` label at line
/// height 1, which gpui centres half a point lower than the flex centre:
/// `paddingTop: 1`), in the danger tint where `danger`; disabled = the half
/// wash, the half stroke, the faded muted label (the tint stays: it is the
/// caller's own style). Drawn twice, `disabled` picks.
fn outline_button(ink: &Ink, chrome: &Chrome, id: &str) -> [Value; 2] {
    let disabled = path("disabled");
    let draw = |dim: bool| {
        let id = if dim { format!("{id}_dim") } else { id.to_string() };
        let size = ink.r(SM);
        let label = |color: &str, suffix: &str, visible: Value| json!({"id": format!("{id}_label{suffix}"), "component": "Text", "visible": visible, "props": {"text": path("action")}, "style": {"fontSize": size, "lineHeight": size, "paddingTop": 1, "color": color, "flexShrink": 0}});
        let plain = if dim { &chrome.button_fg_disabled } else { &chrome.button_fg };
        // The disabled ink is `muted_foreground` at half: the solid colour
        // at the node's opacity (a `#rrggbbaa` byte rounds 0.5 to 0.502).
        let faded_label = |color: &str, suffix: &str, visible: Value| if dim && suffix.is_empty() { super::doc::faded(label(color, suffix, visible), 0.5) } else { label(color, suffix, visible) };
        let visible = and(path("hasAction"), if dim { disabled.clone() } else { not(disabled.clone()) });
        let mut button = json!({"id": id, "component": "Box", "visible": visible,
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "height": 32, "paddingHorizontal": 12,
                "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": if dim { &chrome.button_stroke_disabled } else { &chrome.button_stroke }, "backgroundColor": if dim { &chrome.button_fill_disabled } else { &chrome.button_fill }},
            "children": [label(&chrome.danger, "_danger", path("danger")), faded_label(plain, "", not(path("danger")))]});
        if !dim {
            button["props"] = json!({"pressable": true});
            button["accessibility"] = json!({"label": path("action")});
            button["on"] = json!({"press": {"event": {"name": "row", "context": {"key": path("key"), "kind": path("kind")}}}});
            button["style"]["cursor"] = json!("pointer");
            button["style"][":hover"] = json!({"backgroundColor": chrome.button_hover});
        }
        button
    };
    [draw(false), draw(true)]
}

/// One list (`methods` or `passkeys`): flat rows (`px_3` / `py_2`, `gap_3`)
/// with no hairline between them (EXP-1320): the `size_8` leading slot (kept
/// even when empty so titles align; a `small` glyph or the 16px mark, muted),
/// the `text_sm` MEDIUM title over the `text_xs` muted subtitle, the button.
fn list(ink: &Ink, chrome: &Chrome, list: &str, visible: Value) -> Value {
    let glyph = ink.r(0.875);
    let mark = ink.r(1.);
    let mut title = one_line(ink, &format!("{list}_title"), path("title"), SM, &ink.foreground);
    title["style"]["fontWeight"] = json!(500);
    let [button, button_dim] = outline_button(ink, chrome, &format!("{list}_action"));
    let row = json!({"id": format!("{list}_row"), "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": [
            {"id": format!("{list}_leading"), "component": "Box", "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": ink.r(2.), "height": ink.r(2.)},
                "children": [
                    {"id": format!("{list}_glyph"), "component": "Icon", "visible": path("hasGlyph"), "props": {"name": path("glyph")}, "style": {"width": glyph, "height": glyph, "color": ink.muted, "flexShrink": 0}},
                    {"id": format!("{list}_mark"), "component": "Icon", "visible": path("hasMark"), "props": {"name": path("mark")}, "style": {"width": mark, "height": mark, "color": ink.muted, "flexShrink": 0}},
                ]},
            {"id": format!("{list}_text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0},
                "children": [title, one_line(ink, &format!("{list}_subtitle"), path("subtitle"), XS, &ink.muted)]},
            button,
            button_dim,
        ]});
    json!({"id": format!("{list}_rows"), "component": "List", "visible": visible, "props": {"gap": "none"},
        "template": {"component": format!("{list}_wrap"), "path": format!("/pane/{list}"), "key": "key"},
        "children": [{"id": format!("{list}_wrap"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row]}]})
}

/// The section's two bands (methods, passkeys), each a node of the Account
/// pane's column.
pub(super) fn document(ink: &Ink, cx: &App) -> [Value; 2] {
    let chrome = &Chrome::of(cx);
    // Opener degradation: the caption over the URL in its hairline box
    // (`px_2` / `py_1`, `text_xs` muted, clipped) and the Copy pill.
    let mut caption = ink.text("copy_caption", json!("Couldn't open your browser. Open this link manually:"), SM, &ink.muted);
    caption["style"]["width"] = json!("100%");
    let [copy, _] = ink.pill_button("copy_button", None, json!("Copy"), json!(false), json!({"name": "copy"}));
    let copy_band = json!({"id": "copy", "component": "Box", "visible": path("/pane/hasCopyUrl"),
        "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25), "width": "100%"},
        "children": [
            caption,
            {"id": "copy_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "width": "100%"},
                "children": [
                    {"id": "copy_url", "component": "Box", "style": {"flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "paddingHorizontal": ink.r(0.5), "paddingVertical": ink.r(0.25), "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.stroke, "overflow": "hidden"},
                        "children": [ink.text("copy_url_text", path("/pane/copyUrl"), XS, &ink.muted)]},
                    copy,
                ]},
        ]});

    // `settings::section`: `gap_2` under the band.
    let section = |id: &str, children: Vec<Value>| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": children});
    let methods = section("methods", vec![
        ink.band("methods_band", "Sign-in methods", None),
        list(ink, chrome, "methods", path("/pane/ready")),
        copy_band,
        failed_state(ink, "methods_failed", "settings-account", NO_SIGN_IN_METHODS, path("/pane/failed")),
        skeleton("methods_skeleton", path("/pane/loading")),
    ]);
    // Web parity: the band's trailing "Add passkey" pill. The desktop has no
    // native WebAuthn ceremony, so the pill hands the errand to the web
    // account page.
    let [mut add, _] = ink.pill_button("passkeys_add", None, json!("Add passkey"), json!(false), json!({"name": "addPasskey"}));
    add["visible"] = path("/pane/addPasskey");
    let passkeys = section("passkeys", vec![
        ink.band("passkeys_band", "Passkeys", Some(add)),
        list(ink, chrome, "passkeys", path("/pane/hasPasskeys")),
        empty_state(ink, "passkeys_empty", "auth-passkey", crate::empty_state_copy::PASSKEYS, path("/pane/noPasskeys")),
        skeleton("passkeys_skeleton", path("/pane/loading")),
    ]);
    [methods, passkeys]
}

/// The IDE's plugin, plus the two sign-in marks it paints itself: Apple's
/// monochrome SVG tinted like any glyph, Google's full-colour "G" as an
/// image (a glyph would flatten it to one tint).
struct MarksPlugin(Rc<dyn HostPlugin>);

impl HostPlugin for MarksPlugin {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        match name {
            MARK_APPLE => Some(ExpIcon::Apple.path()),
            _ => self.0.icon(name),
        }
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        match name {
            MARK_GOOGLE => Some(gpui::img("icons/google.svg").size(gpui::px(size)).flex_none().into_any_element()),
            _ => self.0.icon_element(name, size, color, cx),
        }
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.0.on_unknown(node)
    }
}

/// The IDE's plugin with the two sign-in marks.
pub(super) fn marks_plugin() -> Rc<dyn HostPlugin> {
    Rc::new(MarksPlugin(Rc::new(super::doc::ide_plugin())))
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

/// Web copy (`passkeys-section.tsx`); the last way in reads
/// `added <date> · your only way to sign in` (EXP-1209).
fn passkey_subtitle(passkey: &SignInPasskey, blocked: bool) -> String {
    let mut subtitle = match passkey.created_at.as_deref() {
        Some(at) => format!("added {}", format_created_date(at)),
        None => "added".to_string(),
    };
    if blocked {
        subtitle.push_str(" · your only way to sign in");
    } else if passkey.backed_up {
        subtitle.push_str(" · synced across your devices");
    }
    subtitle
}

/// The web account page for the active account + team, where the errands
/// the desktop cannot run itself (adding a passkey, deleting the account)
/// happen. `None` before a team or an account resolves.
pub(super) fn manage_on_web(cx: &App, nav: &Entity<crate::navigation::Navigation>) -> Option<String> {
    let slug = super::active_team(cx, nav).and_then(|team| team.slug)?;
    let account = queries::active_account(cx)?;
    Some(manage_on_web_url(&account.instance_url, &slug))
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
            created_at: Some("2026-09-02T09:00:00.000Z".to_string()),
            backed_up: true,
        };
        assert_eq!(passkey_name(&passkey), "Passkey");
        assert_eq!(
            passkey_subtitle(&passkey, false),
            "added Sep 2, 2026 · synced across your devices"
        );
        assert_eq!(
            passkey_subtitle(&passkey, true),
            "added Sep 2, 2026 · your only way to sign in"
        );
        let named = SignInPasskey {
            name: Some("MacBook".to_string()),
            backed_up: false,
            ..passkey
        };
        assert_eq!(passkey_name(&named), "MacBook");
        assert_eq!(passkey_subtitle(&named, false), "added Sep 2, 2026");
    }

    #[test]
    fn manage_link_points_at_the_web_account_settings() {
        assert_eq!(
            manage_on_web_url("https://app.exponential.at/", "acme"),
            "https://app.exponential.at/t/acme/settings/account"
        );
    }
}
