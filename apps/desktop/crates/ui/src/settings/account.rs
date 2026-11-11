//! Settings → Account: profile identity + timezone (masterplan v5 §8.9, L25).
//!
//! Web parity: the settings `account` section (EXP-238 folded the old
//! Account screen into the settings nav; the email prefs are their own
//! Notifications section now — see [`super::notifications_prefs`]). The old
//! Integrations pane is gone (L25): GitHub App install/manage lives solely in
//! **team settings → Repositories**, and there is no calendar UI anywhere on
//! the desktop. EXP-311: the rail's account button shows only the avatar +
//! first name, so the full name + email live HERE. EXP-369 adds the account's
//! timezone row — the clock the daily digest's send hour is read in.
//! EXP-1126 adds the Sign-in methods + Passkeys bands under the timezone
//! picker row ([`super::sign_in_methods`]). Web parity closes the page with
//! the Danger zone; deleting an account stays a web errand (EXP-69), so its
//! Delete account hands off to the web account page.
//!
//! #EXP-1629: the pane is ONE document on eUI's native host binding: the
//! pane keeps its state, [`AccountPane::project`] is its data model at
//! `/pane` (the sign-in section's projection merged in), [`document`] the
//! tree (the identity's genUI `Avatar`, the timezone's `row` `Select`, the
//! sign-in bands, the Privacy switch, the Danger zone) and every press or
//! pick an action back to the pane.

use std::rc::Rc;

use eui_gpui::{
    binding::Binding,
    host::HostPlugin,
    view::SurfaceView,
};
use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use super::doc::{faded, hex, path, IdeToasts, Ink, SM, VALUE_ALPHA, XS};
use super::sign_in_methods::SignInMethodsSection;
use super::spawn_trpc;
use crate::navigation::Navigation;
use crate::queries;

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// The account's stored IANA timezone (`users.timezone` — a server-only
/// column, so it comes over tRPC, not the users shape).
enum Timezone {
    Idle,
    Loading,
    /// `None` = never captured; the digest sweep reads UTC for those.
    Ready(Option<String>),
    Error,
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct AccountShot {
    pub user_id: String,
    pub name: Option<String>,
    pub email: String,
    /// The picture's URL and PNG bytes.
    pub picture: Option<(String, Vec<u8>)>,
    /// `None` = loading, `Some(None)` = failed, else the stored zone.
    pub timezone: Option<Option<String>>,
    pub timezone_open: bool,
    pub send_diagnostics: bool,
    pub manage_url: Option<String>,
}

/// The Account section pane — identity, timezone, sign-in, privacy.
pub struct AccountPane {
    timezone: Timezone,
    timezone_generation: u64,
    /// The account the timezone belongs to — a re-login must not show the
    /// previous account's value (same guard as the prefs pane).
    account_id: Option<String>,
    nav: Entity<Navigation>,
    /// EXP-1126: Sign-in methods + Passkeys (server-only, tRPC): its state
    /// and writes; its bands are part of this pane's document.
    sign_in_methods: Entity<SignInMethodsSection>,
    /// FEED-79: the timezone read in flight is the person's Retry press (its
    /// failure toasts); a read on entry fails quietly.
    timezone_retry_pressed: bool,
    /// #LOGS-2: the local "Send diagnostics" switch (never synced).
    send_diagnostics: bool,
    /// The bench's state (headless has no account, team or picture).
    #[cfg(test)]
    shot: Option<AccountShot>,
    /// The bench's open Select: frames left before its trigger's press.
    #[cfg(test)]
    shot_open: Option<u8>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

impl AccountPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let users = sync::Store::global(cx).collections().users.clone();
        let sign_in_methods = cx.new(|cx| SignInMethodsSection::new(nav.clone(), cx));
        let subscriptions = vec![
            // The identity rides the users shape (profile image URL).
            cx.observe(&users, |_, _, cx| cx.notify()),
            cx.observe(&sign_in_methods, |_, _, cx| cx.notify()),
        ];
        // Every action leaves the binding for the pane: the confirms and the
        // change-email dialog open on its window.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["row", "addPasskey", "copy", "retry", "timezone", "retryTimezone", "diagnostics", "deleteAccount"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = |key: &str| event.context.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
                    let (kind, key) = (context("kind"), context("key"));
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).cloned().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &kind, &key, value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let plugin: Rc<dyn HostPlugin> = Rc::new(PicturePlugin(super::sign_in_methods::marks_plugin()));
        let view = super::doc::mount_with("settings-account", {
            let ink = Ink::of(cx);
            let sign_in = super::sign_in_methods::document(&ink, cx);
            document(&ink, &Chrome::of(cx), sign_in)
        }, &binding, plugin, true, window, cx);
        Self {
            nav,
            timezone: Timezone::Idle,
            timezone_generation: 0,
            account_id: None,
            sign_in_methods,
            timezone_retry_pressed: false,
            send_diagnostics: crate::diagnostics::enabled(),
            #[cfg(test)]
            shot: None,
            #[cfg(test)]
            shot_open: None,
            binding,
            view,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(send_diagnostics: bool, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut pane = Self::new(crate::navigation::nav_for_window(window, cx), window, cx);
        // Built in the shot's state: the switch never eases into it.
        pane.send_diagnostics = send_diagnostics;
        pane
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: AccountShot, sign_in: super::sign_in_methods::SignInShot, cx: &mut gpui::Context<Self>) {
        self.account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        self.timezone = match &shot.timezone {
            None => Timezone::Loading,
            Some(None) => Timezone::Error,
            Some(Some(zone)) => Timezone::Ready(Some(zone.clone())),
        };
        self.send_diagnostics = shot.send_diagnostics;
        self.shot_open = shot.timezone_open.then_some(2);
        self.shot = Some(shot);
        self.sign_in_methods.update(cx, |section, cx| section.show(sign_in, cx));
        cx.notify();
    }

    /// EXP-369: this pane is built once per window and outlives every visit,
    /// so its server-only read is dropped whenever the pane is (re-)entered —
    /// see [`super::SettingsView::mark_personal_stale`].
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.timezone, Timezone::Ready(_) | Timezone::Error) {
            self.timezone = Timezone::Idle;
        }
        self.sign_in_methods
            .update(cx, |section, cx| section.mark_stale(cx));
        cx.notify();
    }

    fn ensure_timezone_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
        let account_id = sync::Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.timezone = Timezone::Idle;
        }
        if !matches!(self.timezone, Timezone::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.timezone = Timezone::Loading;
        self.timezone_generation += 1;
        let generation = self.timezone_generation;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::users::users_timezone(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.timezone_generation != generation {
                    return;
                }
                let attended = std::mem::take(&mut this.timezone_retry_pressed);
                this.timezone = match result {
                    Ok(timezone) => Timezone::Ready(timezone),
                    // FEED-79: the row reads "Unavailable" + Retry, no
                    // toast; only a Retry press that fails again toasts.
                    Err(err) => {
                        log::warn!("[ui] users.timezone failed: {err}");
                        crate::action_error::failed_load(
                            "your timezone",
                            attended,
                            crate::action_error::action_error_description(&err),
                            cx,
                        );
                        Timezone::Error
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// The failed read's Retry: the same read, attended.
    fn retry_timezone(&mut self, cx: &mut gpui::Context<Self>) {
        self.timezone_retry_pressed = true;
        self.timezone = Timezone::Idle;
        cx.notify();
    }

    /// A picked zone, written explicitly (not `onlyIfUnset` — this is the
    /// user asking for it), with the shown value updated optimistically.
    fn set_timezone(&mut self, timezone: String, cx: &mut gpui::Context<Self>) {
        self.timezone = Timezone::Ready(Some(timezone.clone()));
        cx.notify();
        spawn_trpc(cx, "users.setTimezone", move |trpc| {
            api::users::users_set_timezone(trpc, &timezone, false)
        });
    }

    /// A document action: the sign-in section's (`row`, `addPasskey`,
    /// `copy`, `retry`), or the pane's own.
    fn action(&mut self, name: &str, kind: &str, key: &str, value: Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "timezone" => {
                let current = match &self.timezone {
                    Timezone::Ready(zone) => Some(zone.clone().unwrap_or_else(|| "UTC".to_string())),
                    _ => None,
                };
                if let Some(zone) = value.as_str().filter(|zone| !zone.is_empty() && current.as_deref() != Some(*zone)) {
                    if current.is_some() {
                        self.set_timezone(zone.to_string(), cx);
                    }
                }
            }
            "retryTimezone" => self.retry_timezone(cx),
            "diagnostics" => {
                let on = value.as_bool().unwrap_or(!self.send_diagnostics);
                self.send_diagnostics = on;
                crate::diagnostics::set_enabled(on, cx);
                cx.notify();
            }
            "deleteAccount" => {
                if let Some(url) = self.manage_url(cx) {
                    super::open_url(cx, url);
                }
            }
            _ => self.sign_in_methods.update(cx, |section, cx| section.action(name, kind, key, window, cx)),
        }
    }

    /// The web account page (the Danger zone's hand-off).
    fn manage_url(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.manage_url.clone();
        }
        super::sign_in_methods::manage_on_web(cx, &self.nav)
    }

    /// Avatar + full name + email — the one place the full identity shows
    /// (the rail's chrome is first-name-only, EXP-311).
    fn project_identity(&self, cx: &App) -> Value {
        let account = queries::active_account(cx);
        let name = account.as_ref().and_then(|account| account.name.clone());
        let email = account.as_ref().map(|account| account.email.clone());
        let image = queries::active_user(cx).and_then(|user| user.image);
        let user_id = account.as_ref().map(|account| account.user_id.clone()).unwrap_or_default();
        #[cfg(test)]
        let (name, email, image, user_id) = match &self.shot {
            Some(shot) => {
                use base64::Engine as _;
                let picture = shot.picture.as_ref().map(|(_, bytes)| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
                (shot.name.clone(), Some(shot.email.clone()), picture, shot.user_id.clone())
            }
            None => (name, email, image, user_id),
        };
        let name = name.filter(|name| !name.trim().is_empty());
        // EXP-698: the hue key is the USER ID, so a picture-less account wears
        // the same colour here as in every member list; #EXP-1754: no
        // account = no key = the muted disc.
        let seed = crate::user_avatar::avatar_hue_key(Some(user_id.as_str()), name.as_deref(), email.as_deref()).unwrap_or_default().to_string();
        let title = name.clone().or_else(|| email.clone()).unwrap_or_else(|| "Not signed in".to_string());
        // The email sub-line is dropped when it already IS the title
        // (name-less Apple sign-in accounts).
        let sub_email = name.is_some().then(|| email.clone()).flatten();
        // Only absolute http(s) URLs are pictures (Better Auth `image`).
        let picture = image.filter(|url| picture_src(url));
        json!({
            "title": title,
            "seed": seed,
            "picture": picture.unwrap_or_default(),
            "hasEmail": sub_email.is_some(),
            "email": sub_email.unwrap_or_default(),
        })
    }

    /// The timezone row: the stored zone (no stored zone IS UTC, EXP-771)
    /// and the zones the picker offers.
    fn project_timezone(&self) -> Value {
        let stored = match &self.timezone {
            Timezone::Ready(zone) => Some(zone.clone().unwrap_or_else(|| "UTC".to_string())),
            _ => None,
        };
        let options: Vec<Value> = stored
            .as_deref()
            .map(|current| timezone_options(Some(current)).into_iter().map(|zone| json!({"label": zone, "value": zone})).collect())
            .unwrap_or_default();
        json!({
            "ready": stored.is_some(),
            "loading": matches!(self.timezone, Timezone::Idle | Timezone::Loading),
            "failed": matches!(self.timezone, Timezone::Error),
            "value": stored.unwrap_or_default(),
            "options": options,
        })
    }

    /// The state as the document binds it.
    fn project(&self, cx: &App) -> Value {
        let mut projection = self.sign_in_methods.read(cx).project(cx);
        projection["account"] = self.project_identity(cx);
        projection["tz"] = self.project_timezone();
        projection["diagnostics"] = json!(self.send_diagnostics);
        projection["hasDanger"] = json!(self.manage_url(cx).is_some());
        projection
    }
}

impl Render for AccountPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AccountPane").entered();
        self.ensure_timezone_loaded(cx);
        self.sign_in_methods.update(cx, |section, cx| section.ensure_loaded(cx));
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        // The bench's open Select: its trigger's press once it is laid out
        // (two painted frames in).
        #[cfg(test)]
        match self.shot_open {
            Some(0) => {
                self.shot_open = None;
                self.view.update(cx, |view, cx| {
                    if let Some(trigger) = view.index_of("timezone.trigger") {
                        view.fire(trigger, "press", None, cx);
                    }
                });
            }
            Some(n) => {
                self.shot_open = Some(n - 1);
                cx.notify();
            }
            // The headless clock stands still: the open popup's enter fade
            // runs out on the frames after the press.
            None if self.view.read(cx).layers().iter().any(|layer| layer.owner == "timezone") => {
                self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            }
            None => {}
        }
        #[cfg(not(test))]
        let _ = window;
        // An ordinary section pane since EXP-238 — `SettingsView` owns the
        // scroll container and the detail column (`shell::DetailShell`).
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// A profile picture's src: an absolute http(s) URL.
fn picture_src(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("https://") || url.starts_with("http://") || cfg!(test) && url.starts_with("data:image/png;base64,")
}

/// The IDE's plugin (with the sign-in marks), plus the account's picture:
/// `users.image` is an absolute third-party URL (Google/GitHub), fetched
/// without the instance's headers.
struct PicturePlugin(Rc<dyn HostPlugin>);

impl HostPlugin for PicturePlugin {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        self.0.icon(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        self.0.icon_element(name, size, color, cx)
    }

    fn media_allowed(&self, src: &str) -> bool {
        picture_src(src) || self.0.media_allowed(src)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.0.on_unknown(node)
    }
}

/// The colours the document needs beyond [`Ink`]: the danger tint and the
/// danger `Button`'s paints.
struct Chrome {
    radius: f32,
    danger: String,
    button: String,
    button_fg: String,
    button_hover: String,
    /// `feedback::placeholder_bar`'s fill.
    skeleton: String,
}

impl Chrome {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            radius: f32::from(theme.radius),
            danger: hex(theme.danger),
            button: hex(theme.button_danger),
            button_fg: hex(theme.button_danger_foreground),
            button_hover: hex(theme.button_danger_hover),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
        }
    }
}

/// The identity: no card, the 48 px picture leading (`gap_3`), the name
/// (MEDIUM, the rem) over the `text_sm` muted address.
fn identity(ink: &Ink) -> Value {
    let mut title = ink.text("identity_title", path("/pane/account/title"), 1., &ink.foreground);
    title["props"]["lines"] = json!(1);
    title["style"]["fontWeight"] = json!(500);
    // The rem's gpui line (`phi`), on the half point gpui lays it out at.
    title["style"]["lineHeight"] = json!(22.5);
    let mut email = ink.text("identity_email", path("/pane/account/email"), SM, &ink.muted);
    email["props"]["lines"] = json!(1);
    email["visible"] = path("/pane/account/hasEmail");
    json!({"id": "identity", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.75), "width": "100%", "minWidth": 0},
        "children": [
            {"id": "identity_avatar", "component": "Avatar", "props": {"name": path("/pane/account/title"), "src": path("/pane/account/picture"), "seed": path("/pane/account/seed"), "size": "xl"}, "style": {"flexShrink": 0}},
            {"id": "identity_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "minWidth": 0}, "children": [title, email]},
        ]})
}

/// The Timezone row: the `row` Select over the tz database once the zone
/// is read; while it loads the value is the placeholder bar, a failed read
/// says "Unavailable" + Retry (FEED-79).
fn timezone(ink: &Ink, chrome: &Chrome) -> Value {
    let select = json!({"id": "timezone", "component": "Select", "visible": path("/pane/tz/ready"),
        "props": {"label": "Timezone", "name": "timezone", "triggerVariant": "row", "searchable": true,
            "emptyText": "No matching timezone.", "value": path("/pane/tz/value"), "options": path("/pane/tz/options")},
        "on": {"change": {"event": {"name": "timezone"}}}});
    // The value column of the read's two other states (the IDE's ghost
    // `Button`: the value, `gap_2`, the chevron).
    let glyph = ink.r(1.);
    let chevron = |id: &str| json!({"id": id, "component": "Icon", "props": {"name": "ui-chevron-right"}, "style": {"width": glyph, "height": glyph, "color": ink.value, "opacity": VALUE_ALPHA, "flexShrink": 0}});
    // `feedback::skeleton_value`: the 96 px pulsing bar.
    let bar = json!({"id": "timezone_bar", "component": "Skeleton", "props": {"width": "96", "height": "8"}, "style": {"backgroundColor": chrome.skeleton, "borderRadius": 4}});
    let mut retry = ink.text("timezone_retry_text", json!(crate::feedback::RETRY), XS, &ink.primary);
    retry["style"]["cursor"] = json!("pointer");
    let pending = |id: &str, visible: Value, value: Value, extra: Option<Value>| {
        let mut trailing = vec![json!({"id": format!("{id}_trigger"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5)}, "children": [value, chevron(&format!("{id}_chevron"))]})];
        trailing.extend(extra);
        let control = json!({"id": format!("{id}_control"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "flex-end", "gap": ink.r(0.5), "flexGrow": 1, "minWidth": 200}, "children": trailing});
        let label = json!({"id": format!("{id}_text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexShrink": 0}, "children": [ink.text(&format!("{id}_label"), json!("Timezone"), SM, &ink.foreground)]});
        ink.shell(id, false, visible, vec![label, control])
    };
    let loading = pending("timezone_loading", path("/pane/tz/loading"), bar, None);
    // The ghost `Button`'s run: `text_sm`'s line in a 1 px (clear) border.
    let mut unavailable = faded(ink.text("timezone_unavailable", json!("Unavailable"), SM, &ink.value), VALUE_ALPHA);
    unavailable["style"]["lineHeight"] = json!(ink.r(1.25));
    unavailable["style"]["paddingVertical"] = json!(1);
    let failed = pending("timezone_failed", path("/pane/tz/failed"), unavailable,
        Some(json!({"id": "timezone_retry", "component": "Box", "props": {"pressable": true}, "on": {"press": {"event": {"name": "retryTimezone"}}}, "children": [retry]})));
    ink.group("timezone_group", vec![select, loading, failed])
}

/// #LOGS-2 web parity: the Privacy band, one switch row.
fn privacy(ink: &Ink) -> Value {
    let row = ink.shell("diagnostics", false, json!(true), vec![
        json!({"id": "diagnostics_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "minWidth": 0},
            "children": [ink.text("diagnostics_label", json!("Send diagnostics"), SM, &ink.foreground)]}),
        json!({"id": "diagnostics_switch", "component": "Switch", "props": {"label": "", "name": "diagnostics", "checked": path("/pane/diagnostics")},
            "on": {"change": {"event": {"name": "diagnostics"}}}}),
    ]);
    json!({"id": "privacy", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [ink.band("privacy_band", "Privacy", None), ink.group("privacy_group", vec![row])]})
}

/// Web `DeleteAccountSection`'s band + row (`settings::danger_zone`): the
/// danger-tinted band label, then one row: the muted sentence and the
/// danger `Button` at `web_xs` (24 px, `px 8`, the `size_3` glyph); the
/// deletion itself runs on the web account page (EXP-69).
fn danger_zone(ink: &Ink, chrome: &Chrome) -> Value {
    let mut label = ink.text("danger_band_label", json!("Danger zone"), SM, &chrome.danger);
    label["style"]["fontWeight"] = json!(500);
    let mut sentence = ink.text("danger_text", json!("Permanently delete your account and every team where you are the only member."), SM, &ink.muted);
    sentence["style"]["flexGrow"] = json!(1);
    sentence["style"]["flexShrink"] = json!(1);
    sentence["style"]["minWidth"] = json!(0);
    let size = ink.r(XS);
    let button = json!({"id": "danger_delete", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Delete account"}, "on": {"press": {"event": {"name": "deleteAccount"}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 24, "paddingHorizontal": 8, "borderRadius": chrome.radius, "backgroundColor": chrome.button, "cursor": "pointer", ":hover": {"backgroundColor": chrome.button_hover}},
        "children": [
            {"id": "danger_delete_icon", "component": "Icon", "props": {"name": "ui-delete"}, "style": {"width": ink.r(0.75), "height": ink.r(0.75), "color": chrome.button_fg, "flexShrink": 0}},
            {"id": "danger_delete_text", "component": "Text", "props": {"text": "Delete account"}, "style": {"fontSize": size, "lineHeight": size, "paddingTop": 1, "color": chrome.button_fg, "flexShrink": 0}},
        ]});
    let mut row = ink.shell("danger_row", false, json!(true), vec![sentence, button]);
    row["style"]["justifyContent"] = json!("space-between");
    json!({"id": "danger", "component": "Box", "visible": path("/pane/hasDanger"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [ink.band_with("danger_band", Some(label), "", None, None), ink.group("danger_group", vec![row])]})
}

/// The pane's tree: `gap_6` between the identity, the Timezone row, the
/// sign-in bands, Privacy and the Danger zone.
fn document(ink: &Ink, chrome: &Chrome, sign_in: [Value; 2]) -> Value {
    let mut children = vec![identity(ink), timezone(ink, chrome)];
    children.extend(sign_in);
    children.push(privacy(ink));
    children.push(danger_zone(ink, chrome));
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.5), "width": "100%"}, "children": children})
}

/// The zones the picker offers (web `timezoneOptions`): the system tz
/// database's zones (`zone.tab`, read once), with UTC and the device's own
/// zone always present; a stored zone the list lacks leads it, so the
/// current value is always pickable. Without a tz database (Windows) the
/// list is the short fallback, like the web's.
fn timezone_options(current: Option<&str>) -> Vec<String> {
    static ZONES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let base = ZONES.get_or_init(|| {
        let tab = ["/usr/share/zoneinfo/zone1970.tab", "/usr/share/zoneinfo/zone.tab"]
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let mut zones = zones_from_tab(&tab);
        zones.push("UTC".to_string());
        if let Ok(local) = iana_time_zone::get_timezone() {
            zones.push(local);
        }
        zones.sort();
        zones.dedup();
        zones
    });
    let mut zones = base.clone();
    if let Some(current) = current.filter(|zone| !zone.is_empty()) {
        if !zones.iter().any(|zone| zone == current) {
            zones.insert(0, current.to_string());
        }
    }
    zones
}

/// The zone names of a `zone.tab` / `zone1970.tab` (third tab column;
/// `#` lines are comments).
fn zones_from_tab(tab: &str) -> Vec<String> {
    tab.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split('\t').nth(2))
        .map(|zone| zone.trim().to_string())
        .filter(|zone| !zone.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_tab_reads_the_third_column() {
        let tab = "# comment\nAT\t+4813+01620\tEurope/Vienna\nUS\t+404251-0740023\tAmerica/New_York\tEastern\n";
        assert_eq!(zones_from_tab(tab), ["Europe/Vienna", "America/New_York"]);
    }

    #[test]
    fn a_stored_zone_is_always_offered() {
        let zones = timezone_options(Some("Mars/Olympus_Mons"));
        assert_eq!(zones[0], "Mars/Olympus_Mons");
        assert!(zones.iter().any(|zone| zone == "UTC"));
    }
}
