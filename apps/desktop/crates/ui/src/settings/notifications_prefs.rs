//! Account → Notifications: email-notification prefs (masterplan-v3 §4.2).
//!
//! Web parity: `routes/_authenticated/account/notifications.tsx` — a master
//! email `Switch`, the per-type rows (labels + hints verbatim), the
//! EXP-801 "messages from teammates' agents" block, the delivery cadence
//! select and (EXP-369) the daily send-time hour.
//! **Desktop-only, no web counterpart:** the "Desktop notifications" row
//! at the top of the list (EXP-638) — the per-MACHINE switch for real OS
//! notifications, persisted in the local settings.json
//! (`Settings::os_notifications`), never synced; the web card
//! (`components/account/email-notifications-card.tsx`) has nothing to
//! mirror it. The per-type switches below gate it too. Email is
//! the catch-up channel: nothing is emailed per-event — still-unread
//! notifications are bundled into one digest, so the server's `digestValues`
//! are `off` = Hourly digest / `daily` = Daily digest at the account's own
//! `digestHour`, read in its timezone (`lib/notification-email-policy.ts`).
//! `user_notification_prefs` is server-only — read + written via
//! `notifications.emailPrefs` / `updateEmailPrefs`, never synced.
//!
//! #EXP-1629: the pane is a document on eUI's native host binding
//! (`eui_gpui::binding`): [`Prefs`] is the state, [`project`] its data model
//! at `/pane`, [`document`] the tree (the glass rows' geometry and colours,
//! the IDE's chrome written in), and each switch or picker an action whose
//! [`Mutation`] is the old optimistic write: the local change at once, the
//! mutation carrying only the changed field, a failure reverted and toasted.
//! When the email transport is unconfigured (self-host without SES/SMTP)
//! the email controls disable and the notice shows. EXP-369: the per-type
//! switches gate PUSH as well as email, so they stay live with the master
//! email switch off — only the digest controls follow it.

use std::rc::Rc;

use eui_gpui::{
    binding::{Binding, BindingHost, Mutation, Toasts},
    host::{ActionEvent, HostPlugin},
    view::{SurfaceView, SurfaceViewOptions},
};
use gpui::{div, App, AppContext as _, Entity, Hsla, IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use api::notifications::{EmailPrefs, UpdateEmailPrefsInput};
use domain::contract::{
    NOTIFICATION_TYPE_AGENT_SIGNED_OUT, NOTIFICATION_TYPE_ISSUE_ASSIGNED, NOTIFICATION_TYPE_ISSUE_COMMENT,
    NOTIFICATION_TYPE_ISSUE_CREATED, NOTIFICATION_TYPE_ISSUE_MENTION, NOTIFICATION_TYPE_ISSUE_STATUS_CHANGED,
    NOTIFICATION_TYPE_PR_MERGED, NOTIFICATION_TYPE_PR_OPENED, NOTIFICATION_TYPE_REPORTER_REPLY,
    NOTIFICATION_TYPE_SESSION_BLOCKED,
};

use crate::coding_flow::CodingHub;
use crate::queries;

/// Web `TYPE_ROWS` — verbatim labels + hints, contract-locked type values.
const TYPE_ROWS: [(&str, &str, &str); 10] = [
    (NOTIFICATION_TYPE_ISSUE_CREATED, "New feedback", "A new issue is filed in your team via the feedback widget."),
    (NOTIFICATION_TYPE_ISSUE_ASSIGNED, "Assigned to you", "Someone assigns an issue to you."),
    (NOTIFICATION_TYPE_ISSUE_COMMENT, "Comments", "New comments on issues you're subscribed to."),
    (NOTIFICATION_TYPE_ISSUE_MENTION, "Mentions", "Someone @mentions you in a description or comment."),
    (NOTIFICATION_TYPE_ISSUE_STATUS_CHANGED, "Status changes", "An issue you're subscribed to changes status."),
    (NOTIFICATION_TYPE_PR_OPENED, "Pull request opened", "A PR is opened for an issue you follow."),
    (NOTIFICATION_TYPE_PR_MERGED, "Pull request merged", "A PR for an issue you follow is merged."),
    (NOTIFICATION_TYPE_REPORTER_REPLY, "Reporter replies", "A widget reporter answered on an issue in your teams."),
    (NOTIFICATION_TYPE_SESSION_BLOCKED, "Blocked sessions", "One of your coding sessions hits a rate limit."),
    // #EXP-1534: one inbox row (and push) when a device loses a login.
    (NOTIFICATION_TYPE_AGENT_SIGNED_OUT, "Signed-out agents", "One of your devices loses an agent login."),
];

/// Server `digestValues` (`lib/notification-email-policy.ts`): `off` is the
/// default HOURLY digest of still-unread notifications, `daily` batches at
/// most one digest email per day. There is no per-event email anymore.
const DIGEST_OFF: &str = "off";
const DIGEST_DAILY: &str = "daily";
const DIGESTS: [(&str, &str); 2] = [(DIGEST_OFF, "Hourly digest"), (DIGEST_DAILY, "Daily digest")];

/// The row keys that are not notification types.
const ROW_OS: &str = "os";
const ROW_EMAIL: &str = "email";
const ROW_AGENTS: &str = "agents";

/// The prefs band when their load failed (FEED-79: the empty state + Retry).
const NO_PREFERENCES: &str = "No notification preferences";

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

enum Load {
    Idle,
    Loading,
    Ready(EmailPrefs),
    /// The load failed: the controls stay inert; the failure was a toast.
    Error,
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) enum ShotLoad {
    Ready(EmailPrefs),
    Failed,
}

/// The pane's state: the server's prefs and this machine's switch.
pub(crate) struct Prefs {
    load: Load,
    generation: u64,
    /// The account the loaded prefs belong to — a re-login must not show
    /// (or write!) the previous account's preferences.
    account_id: Option<String>,
    /// FEED-79: the load in flight is the person's Retry press (its failure
    /// toasts); a load on entry fails quietly into the empty state.
    retry_pressed: bool,
    /// EXP-638: the per-machine OS-notification switch (`CodingHub`).
    os_notifications: bool,
}

impl Prefs {
    fn ready(&self) -> Option<&EmailPrefs> {
        match &self.load {
            Load::Ready(prefs) => Some(prefs),
            _ => None,
        }
    }

    fn ready_mut(&mut self) -> Option<&mut EmailPrefs> {
        match &mut self.load {
            Load::Ready(prefs) => Some(prefs),
            _ => None,
        }
    }
}

fn row(key: &str, label: &str, hint: &str, checked: bool, disabled: bool) -> Value {
    json!({"key": key, "label": label, "hint": hint, "checked": checked, "disabled": disabled})
}

/// The state as the document binds it.
fn project(state: &Prefs) -> Value {
    let prefs = state.ready();
    let transport = prefs.is_some_and(|p| p.transport_configured);
    // EXP-1054: ONE list — the per-machine OS switch, the master email
    // switch, the per-type switches and the agents block; only the digest
    // pickers keep a group of their own (the web card fuses the same way).
    let mut rows = vec![
        row(ROW_OS, "Desktop notifications", "This computer only.", state.os_notifications, false),
        row(ROW_EMAIL, "Email notifications", "Notifications still unread are bundled into one digest email.", prefs.is_some_and(|p| p.email_enabled), !transport || prefs.is_none()),
    ];
    let mut digest = Value::Null;
    if let Some(prefs) = prefs {
        // EXP-369: the per-type switches gate PUSH too, so only the mail
        // transport disables them.
        for (kind, label, hint) in TYPE_ROWS {
            // Web: `typePrefs[type] !== false` — missing means ON.
            rows.push(row(kind, label, hint, prefs.type_prefs.get(kind).copied() != Some(false), !transport));
        }
        // EXP-801: a BLOCK, not a delivery mute: live whatever the transport.
        rows.push(row(ROW_AGENTS, "Messages from teammates' agents", "Let other members' agents send you a notification over MCP. Your own agents can always reach you.", prefs.allow_agent_messages, false));
        let value = prefs.digest.clone().unwrap_or_else(|| DIGEST_OFF.to_string());
        let label = DIGESTS.iter().find(|(v, _)| *v == value).map_or("Hourly digest", |(_, l)| l);
        digest = json!({
            "value": value,
            "label": label,
            "daily": value == DIGEST_DAILY,
            "hour": prefs.digest_hour,
            "hourLabel": format!("{}:00", prefs.digest_hour),
            "disabled": !transport || !prefs.email_enabled,
        });
    }
    for (ix, row) in rows.iter_mut().enumerate() {
        row["first"] = json!(ix == 0);
    }
    json!({
        "loading": matches!(state.load, Load::Idle | Load::Loading),
        "failed": matches!(state.load, Load::Error),
        "ready": prefs.is_some(),
        // Only once loaded: `transport` is false while they are in flight.
        "noTransport": prefs.is_some() && !transport,
        "rows": rows,
        "digest": digest,
    })
}

/// `#rrggbbaa` of a chrome colour (the document's literal styles).
fn hex(color: Hsla) -> String {
    let c = color.to_rgb();
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b), byte(c.a))
}

fn path(p: &str) -> Value {
    json!({"@path": p})
}

fn not(value: Value) -> Value {
    json!({"@call": "not", "args": {"value": value}})
}

fn eq(a: Value, b: Value) -> Value {
    json!({"@call": "eq", "args": {"a": a, "b": b}})
}

fn and(a: Value, b: Value) -> Value {
    json!({"@call": "and", "args": {"values": [a, b]}})
}

/// The colours and metrics the document writes in: the IDE's glass chrome
/// (`surface::glass_group_rows`, `glass_toggle_row`, `glass_picker_row`) in
/// px. gpui's sizes are rems and the IDE's root sets the rem to the theme's
/// font size, so `px_4` / `text_sm` are read through [`Ink::rem`]; a gpui
/// text line is `phi` × its size ([`LINE`]).
struct Ink {
    rem: f32,
    foreground: String,
    hint: String,
    value: String,
    muted: String,
    muted_glyph: String,
    primary: String,
    fill_row: String,
    stroke_row: String,
    fill_card: String,
    stroke_card: String,
    code: String,
    radius_lg: f32,
    radius_md: f32,
}

/// The IDE's translucent inks (`foreground.opacity(…)`) as the node's
/// opacity over the solid colour: a `#rrggbbaa` byte rounds 0.5 to 0.502.
const HINT_ALPHA: f32 = 0.5;
const VALUE_ALPHA: f32 = 0.7;
const GLYPH_ALPHA: f32 = 0.6;

/// `node` drawn at `alpha` (see [`HINT_ALPHA`]).
fn faded(mut node: Value, alpha: f32) -> Value {
    node["style"]["opacity"] = json!(alpha);
    node
}

/// gpui's default line height (`phi()`), relative to the font size.
const LINE: f32 = 1.618_034;
/// gpui `text_sm` / `text_xs`, in rems.
const SM: f32 = 0.875;
const XS: f32 = 0.75;

impl Ink {
    fn of(cx: &App) -> Self {
        let chrome = crate::controls::ide_chrome();
        let theme = cx.theme();
        Self {
            rem: f32::from(theme.font_size),
            foreground: hex(theme.foreground),
            hint: hex(theme.foreground),
            value: hex(theme.foreground),
            muted: hex(theme.muted_foreground),
            muted_glyph: hex(theme.muted_foreground),
            primary: hex(theme.primary),
            fill_row: hex(chrome.fill_row),
            stroke_row: hex(chrome.stroke_row),
            fill_card: hex(chrome.fill_card),
            stroke_card: hex(chrome.stroke_card),
            code: hex(theme.muted),
            radius_lg: chrome.radius_lg,
            radius_md: chrome.radius_md,
        }
    }

    /// `rems` in px.
    fn r(&self, rems: f32) -> f32 {
        self.rem * rems
    }

    /// A text run at `rems` (`SM`, `XS`) in gpui's line height.
    fn text(&self, id: &str, text: Value, rems: f32, color: &str) -> Value {
        let size = self.r(rems);
        json!({"id": id, "component": "Text", "props": {"text": text}, "style": {"fontSize": size, "lineHeight": size * LINE, "color": color}})
    }

    /// `glass_group`: the radius-12 `fill_row` block.
    fn group(&self, id: &str, children: Vec<Value>) -> Value {
        json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": self.radius_lg, "backgroundColor": self.fill_row, "overflow": "hidden"}, "children": children})
    }

    /// The hairline a group row draws above itself (`glass_row_divider`).
    fn divider(&self, id: &str, visible: Value) -> Value {
        json!({"id": id, "component": "Box", "visible": visible, "style": {"height": 1, "flexShrink": 0, "backgroundColor": self.stroke_row}})
    }

    /// The row's hairline where `shown`: the IDE draws it as the row's top
    /// border, so a disabled row's dims with it (two nodes, `disabled`
    /// picks).
    fn row_dividers(&self, id: &str, shown: Value, disabled: Value) -> [Value; 2] {
        [self.divider(id, and(shown.clone(), not(disabled.clone()))), faded(self.divider(&format!("{id}_dim"), and(shown, disabled)), 0.5)]
    }

    /// `glass_row_shell`: `px_4` / `py_3`, centred, `gap_3`. A disabled row
    /// dims WHOLE (EXP-1557), so each row is drawn twice and `dim` picks.
    fn shell(&self, id: &str, dim: bool, visible: Value, children: Vec<Value>) -> Value {
        json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": self.r(0.75), "paddingHorizontal": self.r(1.), "paddingVertical": self.r(0.75), "opacity": if dim { 0.5 } else { 1.0 }}, "children": children})
    }

    /// The label column (`gap_0p5`): the label over its muted hint.
    fn label_column(&self, id: &str, label: Value, hint: Value, grow: bool) -> Value {
        let mut hint = faded(self.text(&format!("{id}_hint"), hint, XS, &self.hint), HINT_ALPHA);
        hint["style"]["maxWidth"] = json!(eui_gpui::controls::ROW_DESCRIPTION_MAX_W);
        json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": self.r(0.125), "minWidth": 0, "flexGrow": if grow { 1 } else { 0 }, "flexShrink": 1},
            "children": [self.text(&format!("{id}_label"), label, SM, &self.foreground), hint]})
    }
}

/// One switch row of the list (a template instance over `/pane/rows`).
fn toggle_row(ink: &Ink, dim: bool) -> Value {
    let id = if dim { "row_dim" } else { "row_live" };
    let visible = if dim { path("disabled") } else { not(path("disabled")) };
    ink.shell(id, dim, visible, vec![
        ink.label_column(&format!("{id}_text"), path("label"), path("hint"), true),
        json!({"id": format!("{id}_switch"), "component": "Switch", "props": {"label": "", "name": "pref", "checked": path("checked"), "disabled": path("disabled")},
            "on": {"change": {"event": {"name": "toggle", "context": {"key": path("key")}}}}}),
    ])
}

/// A picker row (`glass_picker_row`): the value at 70% with its chevron
/// (a small ghost `Button`: `text_sm`, `gap_1`, a `size_4` glyph), opening
/// the single-select menu.
#[allow(clippy::too_many_arguments)]
fn picker_row(ink: &Ink, id: &str, dim: bool, visible: Value, label: &str, hint: &str, value: Value, action: &str, items: Vec<Value>) -> Value {
    let glyph = ink.r(1.);
    let trigger = json!({"id": format!("{id}_trigger"), "component": "Box", "props": {"pressable": !dim},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.25), "cursor": "pointer"},
        "children": [
            faded(ink.text(&format!("{id}_value"), value, SM, &ink.value), VALUE_ALPHA),
            {"id": format!("{id}_chevron"), "component": "Icon", "props": {"name": "ui-chevron-right"}, "style": {"width": glyph, "height": glyph, "color": ink.value, "opacity": VALUE_ALPHA}},
        ]});
    let control = if dim {
        trigger
    } else {
        json!({"id": format!("{id}_menu"), "component": "Menu", "props": {"items": items, "align": "end"}, "on": {"select": {"event": {"name": action}}}, "children": [trigger]})
    };
    let control = json!({"id": format!("{id}_control"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "flexGrow": 1, "minWidth": 200}, "children": [control]});
    ink.shell(id, dim, visible, vec![ink.label_column(&format!("{id}_text"), json!(label), json!(hint), false), control])
}

/// The pane's tree.
fn document(ink: &Ink) -> Value {
    // `no_transport_alert`: the sentence word by word (`gap_x 4`), the env
    // names as `text_xs` mono chips (`px_1`, radius 4, `muted`).
    let code = |id: &str, name: &str| {
        let mut text = ink.text(&format!("{id}_text"), json!(name), XS, &ink.foreground);
        text["style"]["fontFamily"] = json!(theme::terminal::FONT_FAMILY);
        json!({"id": id, "component": "Box", "style": {"borderRadius": 4, "paddingHorizontal": ink.r(0.25), "backgroundColor": ink.code}, "children": [text]})
    };
    let words = |id: &str, text: &str| -> Vec<Value> {
        text.split(' ').enumerate().map(|(ix, word)| ink.text(&format!("{id}_{ix}"), json!(word), SM, &ink.foreground)).collect()
    };
    let mut line = words("alert_lead", "Email sending is not configured on this server. Set");
    line.push(code("alert_ses", "AWS_SES_REGION"));
    line.push(ink.text("alert_or", json!("or"), SM, &ink.foreground));
    line.push(code("alert_smtp", "SMTP_HOST"));
    line.extend(words("alert_tail", "to enable it."));
    // `controls::alert(Default)`: the card fill and stroke, `px_3` / `py_2`.
    let alert = json!({"id": "alert", "component": "Box", "visible": path("/pane/noTransport"),
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start", "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md, "borderWidth": 1, "borderColor": ink.stroke_card, "backgroundColor": ink.fill_card},
        "children": [{"id": "alert_line", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "columnGap": 4, "flexGrow": 1, "minWidth": 0}, "children": line}]});

    let row_children: Vec<Value> = ink.row_dividers("row_divider", not(path("first")), path("disabled")).into_iter().chain([toggle_row(ink, false), toggle_row(ink, true)]).collect();
    let rows = ink.group("rows", vec![json!({"id": "rows_list", "component": "List", "props": {"gap": "none"},
        "template": {"component": "row", "path": "/pane/rows", "key": "key"},
        "children": [{"id": "row", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
            "children": row_children}]})]);

    let skeleton = json!({"id": "skeleton", "component": "Skeleton", "visible": path("/pane/loading"), "props": {"rows": crate::feedback::SKELETON_ROWS, "rowHeight": super::SETTINGS_ROW_H}});
    // `action_error::load_failed_state`: `controls::empty_state_list` (a
    // 20px muted glyph over the `text_sm` title, `gap_1p5`, `px_3` / `py_6`)
    // with the `text_xs` primary Retry link under it.
    let mut retry = ink.text("failed_retry_text", json!(crate::feedback::RETRY), XS, &ink.primary);
    retry["style"]["cursor"] = json!("pointer");
    // `text_center` on the column: each run centres inside its box, which
    // gpui rounds up to a whole pixel.
    retry["style"]["textAlign"] = json!("center");
    let mut title = ink.text("failed_title", json!(NO_PREFERENCES), SM, &ink.muted);
    title["style"]["textAlign"] = json!("center");
    let failed = json!({"id": "failed", "component": "Box", "visible": path("/pane/failed"),
        "style": {"display": "flex", "flexDirection": "column", "alignItems": "center", "gap": ink.r(0.375), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(1.5), "width": "100%"},
        "children": [
            {"id": "failed_icon", "component": "Icon", "props": {"name": "settings-notifications"}, "style": {"width": 20, "height": 20, "color": ink.muted_glyph, "opacity": GLYPH_ALPHA}},
            title,
            {"id": "failed_retry", "component": "Box", "props": {"pressable": true}, "on": {"press": {"event": {"name": "retry"}}}, "children": [retry]},
        ]});

    let digest_items: Vec<Value> = DIGESTS.iter().map(|(value, label)| json!({"label": label, "value": value, "kind": "checkbox", "checked": eq(path("/pane/digest/value"), json!(value))})).collect();
    let hour_items: Vec<Value> = (0..24).map(|hour| json!({"label": format!("{hour}:00"), "value": hour.to_string(), "kind": "checkbox", "checked": eq(path("/pane/digest/hour"), json!(hour))})).collect();
    let disabled = path("/pane/digest/disabled");
    let daily = path("/pane/digest/daily");
    let delivery = |dim: bool| picker_row(ink, if dim { "delivery_dim" } else { "delivery" }, dim, if dim { disabled.clone() } else { not(disabled.clone()) }, "Delivery", "How often the digest goes out.", path("/pane/digest/label"), "digest", digest_items.clone());
    let send_time = |dim: bool| picker_row(ink, if dim { "hour_dim" } else { "hour" }, dim, and(daily.clone(), if dim { disabled.clone() } else { not(disabled.clone()) }), "Send time", "Full hours only, in your timezone.", path("/pane/digest/hourLabel"), "hour", hour_items.clone());
    let mut digest = ink.group("digest", [vec![delivery(false), delivery(true)], ink.row_dividers("hour_divider", daily.clone(), disabled.clone()).to_vec(), vec![send_time(false), send_time(true)]].concat());
    digest["visible"] = path("/pane/ready");

    // `settings::section`: `gap_2`.
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [alert, rows, skeleton, failed, digest]})
}

/// The theme the pane paints in: the Exponential theme, its Switch the
/// IDE's own (`controls::web_switch`, 36×20, painted natively), not the
/// theme's 32px track, until #EXP-1745 gives genUI's Switch that look.
// drop with EXP-1745
fn pane_theme() -> Option<std::sync::Arc<eui_core::theme::ResolvedTheme>> {
    let mut theme = (*eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID)?).clone();
    let rule = eui_core::theme::RecipeRule { when: None, style: serde_json::from_value(json!({"width": 36, "height": 20, "native": true})).ok()? };
    theme.recipes.entry("Switch".into()).or_default().entry("track".into()).or_default().push(rule);
    Some(std::sync::Arc::new(theme))
}

/// The IDE's toasts for a failed write.
struct IdeToasts;

impl Toasts for IdeToasts {
    fn failed(&self, title: &str, detail: Option<&str>, cx: &mut App) {
        let toast = crate::toast::Toast::error(title.to_string());
        let toast = match detail.and_then(crate::action_error::description_of) {
            Some(detail) => toast.description(detail),
            None => toast,
        };
        crate::toast::show_in_active_window(toast, cx);
    }
}

/// The optimistic write of `edit` with the mutation carrying only what
/// changed (the web handlers' shape); `None` before the prefs loaded.
fn write(state: &Prefs, key: &str, edit: impl Fn(&mut EmailPrefs) + 'static, input: impl FnOnce(&EmailPrefs) -> UpdateEmailPrefsInput, cx: &App) -> Option<Mutation<Prefs>> {
    let before = state.ready()?.clone();
    let mut after = before.clone();
    edit(&mut after);
    let input = input(&after);
    let trpc = queries::trpc_client(cx)?;
    let thing = "your notification preferences";
    Some(
        Mutation::new(
            move |state: &mut Prefs| {
                if let Some(prefs) = state.ready_mut() {
                    edit(prefs);
                }
            },
            move || api::notifications::notifications_update_email_prefs(&trpc, &input).map(|_| ()).map_err(|err| err.user_message()),
        )
        .revert(move |state: &mut Prefs| {
            if let Some(prefs) = state.ready_mut() {
                *prefs = before;
            }
        })
        .key(key.to_string())
        .on_failure(crate::action_error::action_error_title("update", thing)),
    )
}

fn toggle(state: &Prefs, event: &ActionEvent, cx: &mut App) -> Option<Mutation<Prefs>> {
    let key = event.context.get("key").and_then(Value::as_str)?.to_string();
    let next = event.payload.as_ref().and_then(|p| p.get("checked")).and_then(Value::as_bool)?;
    match key.as_str() {
        // EXP-638: the per-machine switch is local settings.json (the coding
        // hub's ui-prefs save — no doctor rerun), never the server.
        ROW_OS => {
            let hub = CodingHub::global(cx);
            let mut settings = hub.read(cx).settings.clone();
            if settings.os_notifications != next {
                settings.os_notifications = next;
                CodingHub::save_ui_prefs(&hub, settings, cx);
            }
            Some(Mutation::local(move |state: &mut Prefs| state.os_notifications = next))
        }
        ROW_EMAIL => write(state, &key, move |p| p.email_enabled = next, |_| UpdateEmailPrefsInput { email_enabled: Some(next), ..Default::default() }, cx),
        ROW_AGENTS => write(state, &key, move |p| p.allow_agent_messages = next, |_| UpdateEmailPrefsInput { allow_agent_messages: Some(next), ..Default::default() }, cx),
        kind => {
            // EXP-638: the OS-notification gate caches these; refetch next time.
            crate::os_notifications::invalidate_type_prefs(cx);
            let kind = kind.to_string();
            write(
                state,
                "types",
                move |p| {
                    p.type_prefs.insert(kind.clone(), next);
                },
                |p| UpdateEmailPrefsInput { type_prefs: Some(p.type_prefs.clone()), ..Default::default() },
                cx,
            )
        }
    }
}

fn selected(event: &ActionEvent) -> Option<String> {
    event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).map(str::to_string)
}

fn set_digest(state: &Prefs, event: &ActionEvent, cx: &mut App) -> Option<Mutation<Prefs>> {
    let digest = selected(event).filter(|v| DIGESTS.iter().any(|(d, _)| d == v))?;
    let value = digest.clone();
    write(state, "digest", move |p| p.digest = Some(value.clone()), |_| UpdateEmailPrefsInput { digest: Some(digest), ..Default::default() }, cx)
}

fn set_hour(state: &Prefs, event: &ActionEvent, cx: &mut App) -> Option<Mutation<Prefs>> {
    // EXP-369: full hours only — the server's sweep resolves send points at
    // hour resolution.
    let hour = selected(event)?.parse::<i64>().ok().filter(|h| (0..24).contains(h))?;
    write(state, "digestHour", move |p| p.digest_hour = hour, |_| UpdateEmailPrefsInput { digest_hour: Some(hour), ..Default::default() }, cx)
}

pub struct NotificationsPrefsPane {
    view: Entity<SurfaceView>,
    binding: Entity<Binding<Prefs>>,
    _subscriptions: Vec<Subscription>,
}

impl NotificationsPrefsPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let pane = cx.weak_entity();
        let hub = CodingHub::global(cx);
        let os_notifications = hub.read(cx).settings.os_notifications;
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Prefs { load: Load::Idle, generation: 0, account_id: None, retry_pressed: false, os_notifications }, |state, _| project(state));
            binding.set_toasts(Rc::new(IdeToasts));
            binding.on_action("toggle", toggle);
            binding.on_action("digest", set_digest);
            binding.on_action("hour", set_hour);
            binding.on_action("retry", move |_, _, cx| {
                // FEED-79: the failed load's Retry: the same read, attended.
                // Deferred: the binding is mid-update and the retry writes it.
                let pane = pane.clone();
                cx.defer(move |cx| {
                    let _ = pane.update(cx, |pane, cx| pane.retry_load(cx));
                });
                None
            });
            binding
        });
        let inner: Rc<dyn HostPlugin> = Rc::new(crate::exponential_ui_host::IdePlugin { media_origin: String::new(), uploads: None });
        let host: Rc<dyn HostPlugin> = Rc::new(BindingHost::new(&binding, inner));
        let options = SurfaceViewOptions {
            surface_id: "settings-notifications".into(),
            theme: pane_theme(),
            mode: eui_core::theme::Mode::Dark,
            host,
            settings: Some(eui_core::surface::SurfaceSettings { mode: eui_core::theme::ModeSetting::Dark, reduced_motion: theme::motion::reduce_motion(), ..Default::default() }),
            ..Default::default()
        };
        let tree: eui_core::NestedNode = serde_json::from_value(document(&Ink::of(cx))).expect("the notifications document parses");
        let view = cx.new(|cx| {
            let mut view = SurfaceView::new(options, window, cx);
            let outcome = view.set_nested(tree, cx);
            debug_assert!(outcome.issues.is_empty(), "the notifications document: {:?}", outcome.issues);
            view
        });
        binding.update(cx, |binding, cx| binding.attach(&view, cx));
        let subscriptions = vec![cx.observe(&hub, |this, hub, cx| {
            let on = hub.read(cx).settings.os_notifications;
            this.binding.update(cx, |binding, cx| binding.update_state(|state| state.os_notifications = on, cx));
        })];
        Self { view, binding, _subscriptions: subscriptions }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, load: ShotLoad, cx: &mut gpui::Context<Self>) {
        let account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        self.binding.update(cx, |binding, cx| {
            binding.update_state(
                |state| {
                    state.account_id = account_id;
                    state.load = match load {
                        ShotLoad::Ready(prefs) => Load::Ready(prefs),
                        ShotLoad::Failed => Load::Error,
                    };
                },
                cx,
            )
        });
    }

    /// The failed load's Retry: the same read, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.binding.update(cx, |binding, cx| {
            binding.update_state(
                |state| {
                    state.retry_pressed = true;
                    state.load = Load::Idle;
                },
                cx,
            )
        });
        cx.notify();
    }

    /// EXP-369: drop cached prefs so the next render refetches. The pane is
    /// built once per window and lives as long as it, so without this the
    /// first visit's snapshot would show forever (a change made on the web,
    /// or on another device, never landed). A load already in flight is left
    /// alone — it is about to write the fresh value anyway.
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.binding.update(cx, |binding, cx| {
            if matches!(binding.state().load, Load::Ready(_)) {
                binding.update_state(|state| state.load = Load::Idle, cx);
            }
        });
        cx.notify();
    }

    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        let binding = self.binding.clone();
        let trpc = queries::trpc_client(cx);
        binding.update(cx, |binding, cx| {
            if binding.state().account_id != account_id {
                binding.update_state(
                    |state| {
                        state.account_id = account_id;
                        state.load = Load::Idle;
                    },
                    cx,
                );
            }
            if !matches!(binding.state().load, Load::Idle) {
                return;
            }
            let Some(trpc) = trpc else { return };
            let mut generation = 0;
            binding.update_state(
                |state| {
                    state.load = Load::Loading;
                    state.generation += 1;
                    generation = state.generation;
                },
                cx,
            );
            cx.spawn(async move |this, cx| {
                let result = cx.background_executor().spawn(async move { api::notifications::notifications_email_prefs(&trpc) }).await;
                let _ = this.update(cx, |binding, cx| {
                    if binding.state().generation != generation {
                        return;
                    }
                    let attended = binding.state().retry_pressed;
                    let load = match result {
                        Ok(prefs) => Load::Ready(prefs),
                        Err(err) => {
                            // FEED-79: the empty state + Retry; only a Retry
                            // press that fails again toasts.
                            crate::action_error::failed_load("your notification preferences", attended, crate::action_error::action_error_description(&err), cx);
                            Load::Error
                        }
                    };
                    binding.update_state(
                        |state| {
                            state.retry_pressed = false;
                            state.load = load;
                        },
                        cx,
                    );
                });
            })
            .detach();
        });
    }
}

impl Render for NotificationsPrefsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "NotificationsPrefsPane").entered();
        self.ensure_loaded(cx);
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn prefs(transport: bool) -> EmailPrefs {
        EmailPrefs { email_enabled: true, type_prefs: HashMap::from([(NOTIFICATION_TYPE_ISSUE_COMMENT.to_string(), false)]), digest: Some(DIGEST_DAILY.into()), digest_hour: 9, allow_agent_messages: false, transport_configured: transport }
    }

    fn state(load: Load) -> Prefs {
        Prefs { load, generation: 0, account_id: None, retry_pressed: false, os_notifications: true }
    }

    #[test]
    fn loading_shows_the_two_local_rows_over_the_skeleton() {
        let value = project(&state(Load::Loading));
        assert_eq!(value["loading"], json!(true));
        let rows = value["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["checked"], json!(true));
        assert_eq!(rows[1]["disabled"], json!(true), "email waits for the prefs");
        assert!(value["digest"].is_null());
    }

    #[test]
    fn ready_projects_every_type_with_missing_meaning_on() {
        let value = project(&state(Load::Ready(prefs(true))));
        let rows = value["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2 + TYPE_ROWS.len() + 1);
        let comment = rows.iter().find(|r| r["key"] == NOTIFICATION_TYPE_ISSUE_COMMENT).unwrap();
        assert_eq!(comment["checked"], json!(false));
        let mention = rows.iter().find(|r| r["key"] == NOTIFICATION_TYPE_ISSUE_MENTION).unwrap();
        assert_eq!(mention["checked"], json!(true));
        assert_eq!(value["digest"]["hourLabel"], json!("9:00"));
        assert_eq!(value["digest"]["daily"], json!(true));
        assert_eq!(rows.iter().filter(|r| r["first"] == json!(true)).count(), 1);
    }

    #[test]
    fn no_transport_disables_email_and_types_but_never_the_agents_block() {
        let value = project(&state(Load::Ready(prefs(false))));
        assert_eq!(value["noTransport"], json!(true));
        let rows = value["rows"].as_array().unwrap();
        let disabled = |key: &str| rows.iter().find(|r| r["key"] == key).unwrap()["disabled"].clone();
        assert_eq!(disabled(ROW_EMAIL), json!(true));
        assert_eq!(disabled(NOTIFICATION_TYPE_PR_OPENED), json!(true));
        assert_eq!(disabled(ROW_AGENTS), json!(false));
        assert_eq!(disabled(ROW_OS), json!(false));
        assert_eq!(value["digest"]["disabled"], json!(true));
    }

    #[test]
    fn the_document_parses() {
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
        };
        let tree: eui_core::NestedNode = serde_json::from_value(document(&ink)).expect("parses");
        assert_eq!(tree.id, "root");
    }
}
