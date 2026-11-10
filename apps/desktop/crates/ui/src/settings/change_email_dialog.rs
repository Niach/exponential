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

use eui_gpui::{binding::Binding, view::SurfaceView};
use gpui::{div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, WeakEntity, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;
use crate::session::AuthContext;

use super::doc::{hex, path, IdeToasts, Ink, SM, XS};
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

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct EmailShot {
    /// `Some` = the code step for that address.
    pub code_for: Option<&'static str>,
    pub email: &'static str,
    pub code: &'static str,
    pub busy: bool,
    pub error: Option<&'static str>,
}

pub(crate) struct ChangeEmailDialogView {
    current_email: String,
    section: WeakEntity<SignInMethodsSection>,
    step: Step,
    /// The fields as typed, and the text each was last given (its bound
    /// value: a programmatic write, e.g. the code's digit filter).
    email: String,
    code: String,
    shown_email: String,
    shown_code: String,
    busy: bool,
    error: Option<SharedString>,
    /// The field to focus once the document has drawn it (an `Input`'s
    /// text field is its `<id>.field` part node): a field exists
    /// only after the paint that first shows it, so a focus asked for
    /// before then is retried after every render until it lands.
    want_focus: Option<&'static str>,
    /// Renders left to retry [`Self::want_focus`] (a field that never
    /// shows must not keep the window rendering).
    focus_tries: u8,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
}

impl ChangeEmailDialogView {
    fn new(
        current_email: String,
        section: WeakEntity<SignInMethodsSection>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // Every action leaves the binding for the view (the steps' writes
        // and the window's close need its window).
        let me = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/pane", Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(std::rc::Rc::new(IdeToasts));
            for name in ["edit", "submit", "cancel", "resend", "different"] {
                let me = me.clone();
                binding.on_action(name, move |_, event, cx| {
                    let field = event.context.get("field").and_then(Value::as_str).unwrap_or_default().to_string();
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let me = me.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = me.update(cx, |view, cx| view.action(name, &field, value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let theme = cx.theme();
        let chrome = Chrome {
            radius: f32::from(theme.radius),
            danger: hex(theme.danger),
            button_fg: hex(theme.button_foreground),
            // The outline `Button`: the input wash (premultiplied: its few
            // percent alpha has no exact byte) and the input stroke.
            button_fill: premultiplied_hex(theme.input_background()),
            button_stroke: hex(theme.input),
            primary: hex(theme.button_primary),
            primary_fg: hex(theme.button_primary_foreground),
            primary_hover: hex(theme.button_primary_hover),
        };
        let view = super::doc::mount_with("change-email", document(&Ink::of(cx), &chrome), &binding, std::rc::Rc::new(super::doc::ide_plugin()), true, window, cx);
        let mut this = Self {
            current_email,
            section,
            step: Step::Address,
            email: String::new(),
            code: String::new(),
            shown_email: String::new(),
            shown_code: String::new(),
            busy: false,
            error: None,
            want_focus: Some("email.field"),
            focus_tries: FOCUS_TRIES,
            binding,
            view,
        };
        this.push(cx);
        this
    }

    /// A document action: a field's edit (the code keeps six digits only:
    /// web `replace(/\D/g, "")` + maxLength 6), Enter, Cancel, Resend code,
    /// Use a different email.
    fn action(&mut self, name: &str, field: &str, value: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "edit" => match field {
                "email" => self.email = value,
                "code" => {
                    let digits = normalize_code(&value);
                    if digits != value {
                        self.shown_code = digits.clone();
                    }
                    self.code = digits;
                }
                _ => return,
            },
            "submit" => self.submit(window, cx),
            "cancel" => native_dialog::close_dialog_window(window, cx),
            "resend" => {
                if let Step::Code { new_email } = self.step.clone() {
                    self.send_code(new_email, window, cx);
                }
            }
            "different" => self.back_to_address(cx),
            _ => {}
        }
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new("dana@example.com".into(), WeakEntity::new_invalid(), window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: EmailShot, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.step = match shot.code_for {
            Some(address) => Step::Code { new_email: address.to_string() },
            None => Step::Address,
        };
        // Today's dialog: the address field holds the focus it opened
        // with; a code step shot directly has none.
        // The field lets the shot's text in only while unfocused (a
        // focused field keeps what the user typed): blur, write, refocus.
        self.view.update(cx, |view, cx| view.blur_field("email.field", window, cx));
        (self.want_focus, self.focus_tries) = ((shot.code_for.is_none()).then_some("email.field"), FOCUS_TRIES);
        (self.email, self.shown_email) = (shot.email.to_string(), shot.email.to_string());
        (self.code, self.shown_code) = (shot.code.to_string(), shot.code.to_string());
        self.busy = shot.busy;
        self.error = shot.error.map(SharedString::from);
        cx.notify();
    }

    /// Enter / the primary button: whichever submit the visible step means.
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match self.step.clone() {
            Step::Address => {
                let address = self.email.clone();
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
            crate::action_error::failed_action("send", "the code", None, cx);
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
            let _ = this.update_in(cx, |this, _window, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        // A fresh code invalidates whatever is in the field.
                        // A fresh code invalidates whatever is in the field
                        // (a programmatic write: it wins over the focus).
                        this.code.clear();
                        this.shown_code.clear();
                        this.step = Step::Code { new_email: target };
                        (this.want_focus, this.focus_tries) = (Some("code.field"), FOCUS_TRIES);
                        this.push(cx);
                    }
                    // FEED-79: the server refusing the ADDRESS stays under the
                    // field; a request that failed otherwise is a toast.
                    Err(err @ api::ApiError::Http { .. }) => {
                        log::warn!("[ui] request-email-change failed: {err}");
                        this.error = Some(error_copy(&err, "Couldn't send the code.").into());
                    }
                    Err(err) => {
                        log::warn!("[ui] request-email-change failed: {err}");
                        crate::action_error::failed_action_err("send", "the code", &err, cx);
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
        let otp = self.code.trim().to_string();
        if otp.len() < 6 {
            return;
        }
        let Some((instance_url, token, client)) = credentials(cx) else {
            crate::action_error::failed_action("change", "the email", None, cx);
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
                    // The CODE the server refused stays under the field.
                    Err(err @ api::ApiError::Http { .. }) => {
                        log::warn!("[ui] change-email failed: {err}");
                        this.error = Some(error_copy(&err, "Couldn't change the email.").into());
                        cx.notify();
                    }
                    Err(err) => {
                        log::warn!("[ui] change-email failed: {err}");
                        crate::action_error::failed_action_err("change", "the email", &err, cx);
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

impl ChangeEmailDialogView {
    /// The state as the document binds it.
    fn project(&self) -> Value {
        let (code_step, new_email) = match &self.step {
            Step::Address => (false, String::new()),
            Step::Code { new_email } => (true, new_email.clone()),
        };
        let _ = new_email;
        let off = self.busy || if code_step { self.code.trim().len() < 6 } else { self.email.trim().is_empty() };
        json!({
            "description": step_description(&self.step),
            "address": !code_step,
            "codeStep": code_step,
            "fields": {"email": self.shown_email, "code": self.shown_code},
            "hasError": self.error.is_some(),
            "error": self.error.clone().map(|e| e.to_string()).unwrap_or_default(),
            "primary": match (code_step, self.busy) {
                (false, false) => "Send code",
                (false, true) => "Sending…",
                (true, false) => "Confirm",
                (true, true) => "Checking…",
            },
            "on": !off,
            "off": off && !self.busy,
            "busy": self.busy,
        })
    }

    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        let projection = self.project();
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
    }
}

/// Renders a wanted focus is retried for: the field appears on the paint
/// after the one that first shows it.
const FOCUS_TRIES: u8 = 4;

impl Render for ChangeEmailDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ChangeEmailDialogView").entered();
        self.push(cx);
        if let Some(id) = self.want_focus.filter(|_| self.focus_tries > 0) {
            self.focus_tries -= 1;
            let view = self.view.clone();
            cx.defer_in(window, move |this, window, cx| {
                // Focus once the field holds its bound text: a focused field
                // keeps its own text over a later write.
                let bound = if id == "code.field" { &this.shown_code } else { &this.shown_email };
                let ready = view.read(cx).field_text(id, cx).as_ref() == Some(bound);
                if ready && view.update(cx, |view, cx| view.focus_field(id, window, cx)) {
                    this.want_focus = None;
                } else if this.focus_tries > 0 {
                    cx.notify();
                }
            });
        }
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// `#rrggbbaa` of a faint translucent colour that paints like the exact one
/// (the alpha rounds to its byte, the channels absorb the rounding).
fn premultiplied_hex(color: gpui::Hsla) -> String {
    let c = color.to_rgb();
    let alpha = (c.a.clamp(0., 1.) * 255.).round().max(1.);
    let channel = |v: f32| ((v * c.a * 255. / alpha) * 255.).round().clamp(0., 255.) as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", channel(c.r), channel(c.g), channel(c.b), alpha as u8)
}

/// The colours the document needs beyond [`Ink`]: the error ink and the
/// outline / primary `Button`s' paints.
struct Chrome {
    radius: f32,
    danger: String,
    button_fg: String,
    button_fill: String,
    button_stroke: String,
    primary: String,
    primary_fg: String,
    primary_hover: String,
}

/// A `web_sm` `Button` (32 px, `px 12`, `text_sm` on a line of 1, the theme
/// radius; `gap_1` before a glyph).
#[allow(clippy::too_many_arguments)]
fn sm_button(ink: &Ink, id: &str, glyph: Option<Value>, label: Value, fill: &str, stroke: Option<&str>, fg: &str, event: &str, radius: f32) -> Value {
    let size = ink.r(SM);
    let text = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": label.clone()}, "style": {"fontSize": size, "lineHeight": size, "color": fg, "flexShrink": 0}});
    let mut button = json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": {"name": event}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": radius, "backgroundColor": fill, "cursor": "pointer"},
        "children": glyph.into_iter().chain([text]).collect::<Vec<_>>()});
    if let Some(stroke) = stroke {
        button["style"]["borderWidth"] = json!(1);
        button["style"]["borderColor"] = json!(stroke);
    }
    button
}

/// A field (`gap_1`): the `text_xs` muted label over the boxed Input
/// (`variant: box`: `glass_input` at `web_input`).
fn field(ink: &Ink, id: &str, label: &str, placeholder: &str, visible: Value) -> Value {
    json!({"id": format!("{id}_field"), "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25), "width": "100%"},
        "children": [
            ink.text(&format!("{id}_label"), json!(label), XS, &ink.muted),
            {"id": id, "component": "Input", "props": {"label": label, "variant": "box", "name": id, "placeholder": placeholder, "value": path(&format!("/pane/fields/{id}"))},
                "style": {"width": "100%"},
                "on": {"change": {"event": {"name": "edit", "context": {"field": id}}}, "submit": {"event": {"name": "submit"}}}},
        ]})
}

/// A `text_xs` muted text link (its hover underline).
fn text_link(ink: &Ink, id: &str, label: &str, event: &str) -> Value {
    let mut text = ink.text(&format!("{id}_text"), json!(label), XS, &ink.muted);
    text["style"][":hover"] = json!({"textDecoration": "underline"});
    json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": {"name": event}}}, "style": {"cursor": "pointer"}, "children": [text]})
}

/// The dialog's tree (`gap_4`): the body (`gap_3`: the step's description,
/// its field, the code step's links, the inline error), then the footer
/// (Cancel, the step's primary).
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let description = ink.text("description", path("/pane/description"), SM, &ink.muted);
    let links = json!({"id": "links", "component": "Box", "visible": path("/pane/codeStep"), "style": {"display": "flex", "flexDirection": "row", "gap": ink.r(1.)},
        "children": [text_link(ink, "resend", "Resend code", "resend"), text_link(ink, "different", "Use a different email", "different")]});
    let error = json!({"id": "error", "component": "Text", "visible": path("/pane/hasError"), "props": {"text": path("/pane/error")},
        "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": chrome.danger}});
    let body = json!({"id": "body", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "width": "100%"},
        "children": [description, field(ink, "email", "New email", "you@example.com", path("/pane/address")), field(ink, "code", "Code", "123456", path("/pane/codeStep")), links, error]});
    let cancel = sm_button(ink, "cancel", None, json!("Cancel"), &chrome.button_fill, Some(&chrome.button_stroke), &chrome.button_fg, "cancel", chrome.radius);
    let mut primary = sm_button(ink, "primary", None, path("/pane/primary"), &chrome.primary, None, &chrome.primary_fg, "submit", chrome.radius);
    primary["visible"] = path("/pane/on");
    primary["style"][":hover"] = json!({"backgroundColor": chrome.primary_hover});
    // Disabled: the fill at half under the solid label; busy: web's pending
    // Button, the spinner before the progressive word.
    let off = |id: &str, glyph: Option<Value>, visible: Value| {
        let mut button = sm_button(ink, id, glyph, path("/pane/primary"), "#00000000", None, &chrome.primary_fg, "submit", chrome.radius);
        button["visible"] = visible;
        button["props"]["pressable"] = json!(false);
        button["style"]["cursor"] = json!("default");
        button["style"]["position"] = json!("relative");
        button["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": format!("{id}_fill"), "component": "Box",
            "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": chrome.radius, "backgroundColor": chrome.primary, "opacity": 0.5}}));
        button
    };
    let spinner = json!({"id": "busy_spinner", "component": "Spinner", "style": {"width": ink.r(SM), "height": ink.r(SM), "color": chrome.primary_fg, "flexShrink": 0}});
    let footer = json!({"id": "footer", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "gap": ink.r(0.5)},
        "children": [cancel, primary, off("primary_off", None, path("/pane/off")), off("primary_busy", Some(spinner), path("/pane/busy"))]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.), "width": "100%", "minWidth": 0}, "children": [body, footer]})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let ink = Ink {
            rem: 14.,
            primary: "#fafafaff".into(),
            foreground: "#ffffffff".into(),
            hint: "#ffffffff".into(),
            value: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0f".into(),
            stroke_card: "#ffffff1a".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 8.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff1a".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
            primary_button_fill: "#f4f4f5ff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#f4f4f580".into(),
            primary_button_fg: "#18181bff".into(),
        };
        let chrome = Chrome {
            radius: 6.,
            danger: "#f87171ff".into(),
            button_fg: "#fafafaff".into(),
            button_fill: "#ffffff08".into(),
            button_stroke: "#ffffff26".into(),
            primary: "#f4f4f5ff".into(),
            primary_fg: "#18181bff".into(),
            primary_hover: "#e4e4e7ff".into(),
        };
        let tree = document(&ink, &chrome);
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-change-email", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

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
