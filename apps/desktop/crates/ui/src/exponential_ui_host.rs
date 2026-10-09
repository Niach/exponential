//! VAPP-91: the IDE as an Exponential UI HOST, the desktop twin of the web
//! app's `lib/exponential-ui-host.tsx`. Everything goes through the SDK's
//! PUBLIC gpui host API (`exponential_ui_gpui::runtime::ExponentialHost`),
//! like a third party's host would; no private hook:
//!
//! - functions: `harness.toast` (the IDE toast stack), `harness.openIssue`
//!   (the synced issue's detail, else an error toast), `harness.openDevice`
//!   (the device settings dialog for an OWN row, else a toast with its
//!   name), `harness.mcp` (a `tools/call` on the instance's own `/api/mcp`
//!   with the signed-in account's credentials, `api::TrpcClient::post_json`);
//! - bindings: the `exp:` scheme over the synced store (`exp:devices` →
//!   `{rows, count, online}`, derived exactly as the web derives it;
//!   `exp:issues?board=…&limit=…` → `{rows, count}`);
//! - extensions: the app extension (`packages/ui/exponential-ui/extension.json`)
//!   with gpui painters for its `IconDisc` and `LiveDot` natives, and the
//!   app's declarative package (Devices);
//! - policy: `harness.mcp` ASKS first, through a native confirm window
//!   (`crate::native_dialog`), Deny on Return.
//!
//! The SDK's `SourceResolver` gets no `App`, so the store is observed HERE
//! (one observer per collection + a liveness tick) into a snapshot cache the
//! resolver reads synchronously; every subscription is re-emitted when its
//! value moves.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use exponential_ui::extension::parse_extension;
use exponential_ui::host::{FunctionPolicy, MediaOptions, ParsedSource, UrlPolicy};
use exponential_ui::measure::LeafRequest;
use exponential_ui::surface::PlacedNode;
use exponential_ui::theme::{Mode, ResolvedTheme};
use exponential_ui_gpui::extension::{ExtensionPainter, PaintContext};
use exponential_ui_gpui::host::HostPlugin;
use exponential_ui_gpui::runtime::{
    ConsentHook, Emit, ExponentialHost, FunctionCallInfo, HostFunction, HostOptions, HostPolicy,
    SourceResolver,
};
use gpui::{
    div, px, svg, AnyElement, App, AppContext as _, Entity, Hsla, IntoElement, ParentElement as _,
    SharedString, Styled as _, Subscription, Task, Window,
};
use gpui_component::IconNamed as _;
use serde_json::{json, Value};

use crate::toast::Toast;

/// The app's declarative Devices package (VAPP-83's template), byte for
/// byte what the web host installs.
pub(crate) const DEVICES_PACKAGE_JSON: &str =
    include_str!("../../../../../packages/ui/exponential-ui/templates/devices.json");

/// The app extension catalog (`IconDisc`, `LiveDot`, …).
pub(crate) const APP_EXTENSION_JSON: &str =
    include_str!("../../../../../packages/ui/exponential-ui/extension.json");

/// The theme every surface of this host paints with (the web's
/// `DEFAULT_THEME_ID`).
pub(crate) const THEME_ID: &str = "exponential";

/// Online-ness ages out on a clock, not on a delta (the Devices page's tick).
const LIVENESS_TICK: Duration = Duration::from_secs(30);

/// What the MCP transport requires (406 without both).
const MCP_ACCEPT: &str = "application/json, text/event-stream";
const MCP_TIMEOUT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// The `exp:devices` derivation (pure, tested)
// ---------------------------------------------------------------------------

/// The web's `readinessAgo` (Last seen): floored, `just now` under a minute,
/// then `N min ago`, `N h ago`, `N d ago`.
pub(crate) fn format_ago(seen_ms: i64, now_ms: i64) -> String {
    domain::coding_readiness::ago(now_ms, seen_ms)
}

/// One `exp:devices` row, exactly as the web host derives it:
/// `{id, label, detail, icon, discTone, tone, status}`.
pub(crate) fn device_row(row: &domain::rows::DeviceRow, now_ms: i64) -> Value {
    let online = crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms);
    let seen_ms = row
        .last_seen_at
        .as_deref()
        .and_then(crate::comments::parse_epoch)
        .map(|s| s * 1_000);
    let kind = if row.is_server() { "Server" } else { "Desktop" };
    let detail = [
        Some(kind.to_string()),
        row.platform.clone(),
        row.version.as_ref().map(|v| format!("v{v}")),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    let status = if online {
        "Online".to_string()
    } else {
        match seen_ms {
            Some(seen) => format!("Last seen {}", format_ago(seen, now_ms)),
            None => "Offline".to_string(),
        }
    };
    json!({
        "id": row.id,
        "label": row.label.clone().unwrap_or_default(),
        "detail": detail,
        "icon": crate::icons::device_icon_name(row.icon.as_deref(), row.is_server()),
        "discTone": if online { "success" } else { "muted" },
        "tone": if online { "live" } else { "idle" },
        "status": status,
    })
}

/// `exp:devices` → `{rows, count, online}`, rows by label.
pub(crate) fn devices_value<'a>(
    rows: impl IntoIterator<Item = &'a domain::rows::DeviceRow>,
    now_ms: i64,
) -> Value {
    let mut sorted: Vec<&domain::rows::DeviceRow> = rows.into_iter().collect();
    // `localeCompare` ≈ case-insensitive first, then the raw label.
    sorted.sort_by(|a, b| {
        let (a, b) = (a.label.as_deref().unwrap_or(""), b.label.as_deref().unwrap_or(""));
        a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b))
    });
    let rows: Vec<Value> = sorted.into_iter().map(|row| device_row(row, now_ms)).collect();
    let online = rows.iter().filter(|r| r["tone"] == "live").count();
    json!({ "count": rows.len(), "online": online, "rows": rows })
}

/// One issue as `exp:issues` needs it (cached off the store).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IssueLite {
    pub(crate) id: String,
    pub(crate) identifier: String,
    pub(crate) title: String,
    pub(crate) status: Value,
    pub(crate) board_id: String,
    pub(crate) sort_order: f64,
}

/// `exp:issues?board=<id>&limit=<n>` → `{rows, count}` (sort order, the
/// web's default limit 50).
pub(crate) fn issues_value(issues: &[IssueLite], board: Option<&str>, limit: usize) -> Value {
    let mut picked: Vec<&IssueLite> = issues
        .iter()
        .filter(|i| board.is_none_or(|b| i.board_id == b))
        .collect();
    picked.sort_by(|a, b| a.sort_order.total_cmp(&b.sort_order));
    let rows: Vec<Value> = picked
        .into_iter()
        .take(limit)
        .map(|i| json!({"id": i.id, "identifier": i.identifier, "title": i.title, "status": i.status}))
        .collect();
    json!({ "count": rows.len(), "rows": rows })
}

// ---------------------------------------------------------------------------
// The `exp:` source over the store
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Query {
    Devices,
    Issues { board: Option<String>, limit: usize },
}

struct Sub {
    id: u64,
    query: Query,
    emit: Emit,
    last: Option<Value>,
}

/// The snapshot cache the resolver reads (it gets no `App`).
struct Feeds {
    devices: Value,
    issues: Vec<IssueLite>,
    subs: Vec<Sub>,
    next: u64,
}

impl Default for Feeds {
    /// The template's own empty value until the store says otherwise.
    fn default() -> Self {
        Feeds {
            devices: json!({"rows": [], "count": 0, "online": 0}),
            issues: Vec::new(),
            subs: Vec::new(),
            next: 0,
        }
    }
}

impl Feeds {
    fn value_for(&self, query: &Query) -> Value {
        match query {
            Query::Devices => self.devices.clone(),
            Query::Issues { board, limit } => issues_value(&self.issues, board.as_deref(), *limit),
        }
    }

    /// Re-emit every subscription whose value moved.
    fn publish(&mut self) {
        let values: Vec<Value> = self.subs.iter().map(|s| self.value_for(&s.query)).collect();
        for (sub, value) in self.subs.iter_mut().zip(values) {
            if sub.last.as_ref() != Some(&value) {
                (sub.emit)(value.clone());
                sub.last = Some(value);
            }
        }
    }
}

fn query_of(source: &ParsedSource) -> Option<Query> {
    match source.name.as_str() {
        "devices" => Some(Query::Devices),
        "issues" => Some(Query::Issues {
            board: source.params.get("board").cloned().filter(|b| !b.is_empty()),
            limit: source
                .params
                .get("limit")
                .and_then(|l| l.parse::<usize>().ok())
                .unwrap_or(50),
        }),
        _ => None,
    }
}

fn exp_resolver(feeds: Weak<RefCell<Feeds>>) -> SourceResolver {
    Box::new(move |source: ParsedSource, emit: Emit| {
        let (Some(query), Some(shared)) = (query_of(&source), feeds.upgrade()) else {
            emit(Value::Null);
            return Box::new(|| {});
        };
        let id = {
            let mut f = shared.borrow_mut();
            f.next += 1;
            let id = f.next;
            let value = f.value_for(&query);
            emit(value.clone());
            f.subs.push(Sub { id, query, emit, last: Some(value) });
            id
        };
        let feeds = feeds.clone();
        Box::new(move || {
            if let Some(shared) = feeds.upgrade() {
                shared.borrow_mut().subs.retain(|s| s.id != id);
            }
        })
    })
}

fn refresh_devices(feeds: &Rc<RefCell<Feeds>>, cx: &App) {
    let Some(store) = sync::Store::try_global(cx) else { return };
    let devices = store.collections().devices.read(cx);
    let value = devices_value(devices.iter(), chrono::Utc::now().timestamp_millis());
    let mut f = feeds.borrow_mut();
    f.devices = value;
    f.publish();
}

fn refresh_issues(feeds: &Rc<RefCell<Feeds>>, cx: &App) {
    let Some(store) = sync::Store::try_global(cx) else { return };
    let issues: Vec<IssueLite> = store
        .collections()
        .issues
        .read(cx)
        .iter()
        .map(|i| IssueLite {
            id: i.id.clone(),
            identifier: i.identifier.clone(),
            title: i.title.clone(),
            status: serde_json::to_value(&i.status).unwrap_or(Value::Null),
            board_id: i.board_id.clone(),
            sort_order: i.sort_order.unwrap_or(0.),
        })
        .collect();
    let mut f = feeds.borrow_mut();
    if f.issues != issues {
        f.issues = issues;
        f.publish();
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

fn arg_str(args: &Value, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn toast_fn() -> HostFunction {
    exponential_ui_gpui::runtime::sync_function(|args, _, cx| {
        let text = arg_str(&args, "message");
        let toast = if args.get("tone").and_then(Value::as_str) == Some("error") {
            Toast::error(text)
        } else {
            Toast::info(text)
        };
        crate::toast::show_in_active_window(toast, cx);
        Ok(Value::Null)
    })
}

fn open_issue_fn() -> HostFunction {
    exponential_ui_gpui::runtime::sync_function(|args, _, cx| {
        let identifier = arg_str(&args, "identifier");
        let hit = sync::Store::try_global(cx).and_then(|store| {
            store
                .collections()
                .issues
                .read(cx)
                .iter()
                .find(|i| i.identifier == identifier)
                .map(|i| (i.id.clone(), i.board_id.clone()))
        });
        match hit {
            Some((issue_id, board_id)) => crate::navigation::on_active_window(cx, move |window, cx| {
                crate::navigation::open_issue_scoped(window, cx, issue_id, board_id, None)
            }),
            None => crate::toast::show_in_active_window(
                Toast::error(format!("{identifier} is not synced here")),
                cx,
            ),
        }
        Ok(Value::Null)
    })
}

fn open_device_fn() -> HostFunction {
    exponential_ui_gpui::runtime::sync_function(|args, _, cx| {
        let id = arg_str(&args, "id");
        let me = crate::queries::active_account(cx).map(|a| a.user_id);
        // The settings dialog edits OWN rows only (the machines menu never
        // offers it on a teammate's); anything else gets the web's toast.
        let own = sync::Store::try_global(cx).is_some_and(|store| {
            store
                .collections()
                .devices
                .read(cx)
                .get(&id)
                .is_some_and(|row| row.user_id.is_some() && row.user_id == me)
        });
        if own {
            crate::navigation::on_active_window(cx, move |window, cx| {
                crate::device_settings::open(window, cx, id)
            });
        } else {
            let label = arg_str(&args, "label");
            crate::toast::show_in_active_window(Toast::info(if label.is_empty() { id } else { label }), cx);
        }
        Ok(Value::Null)
    })
}

/// The id `api::mcp_tools::tool_call_body` stamps on every request.
const MCP_REQUEST_ID: i64 = 1;

/// The message with `id` in one JSON text (a message or a batch), like the
/// web's `parseJsonRpcMessage`; not JSON / no match = `None`.
fn json_rpc_message(text: &str, id: i64) -> Option<Value> {
    let parsed: Value = serde_json::from_str(text).ok()?;
    let messages = match parsed {
        Value::Array(items) => items,
        one => vec![one],
    };
    messages.into_iter().find(|m| m.get("id").and_then(Value::as_i64) == Some(id))
}

/// The answer to request `id` in a JSON or SSE body (the web's
/// `readJsonRpcAnswer`): SSE events are read in order and notifications or
/// foreign ids skipped.
fn json_rpc_answer(body: &str, id: i64) -> Option<Value> {
    if body.trim_start().starts_with(['{', '[']) {
        return json_rpc_message(body, id);
    }
    let mut data: Vec<&str> = Vec::new();
    for line in body.lines().map(|l| l.strip_suffix('\r').unwrap_or(l)) {
        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest));
        } else if line.is_empty() && !data.is_empty() {
            if let Some(message) = json_rpc_message(&data.join("\n"), id) {
                return Some(message);
            }
            data.clear();
        }
    }
    (!data.is_empty()).then(|| json_rpc_message(&data.join("\n"), id)).flatten()
}

/// A JSON-RPC `tools/call` response (plain JSON or SSE) → the `result` of
/// the message answering request `id`, like the web host's `callMcp`. A
/// JSON-RPC `error` rejects with its message and code (the web's
/// `McpCallError`); no answer with `id` rejects too.
pub(crate) fn mcp_result(body: &str, id: i64) -> Result<Value, String> {
    let message = json_rpc_answer(body, id).ok_or_else(|| "MCP: no answer".to_string())?;
    if let Some(error) = message.get("error") {
        let text = error["message"].as_str().filter(|m| !m.is_empty()).unwrap_or("MCP error");
        return Err(match error.get("code").and_then(Value::as_i64) {
            Some(code) => format!("{text} (code {code})"),
            None => text.to_string(),
        });
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

fn mcp_fn() -> HostFunction {
    Rc::new(|args: Value, _: &FunctionCallInfo, cx: &mut App| -> Task<Result<Value, String>> {
        let tool = arg_str(&args, "tool");
        let arguments = args.get("arguments").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return Task::ready(Err("not signed in".to_string()));
        };
        cx.background_spawn(async move {
            let body = api::mcp_tools::tool_call_body(&tool, &arguments);
            let response = trpc
                .post_json("/api/mcp", &body, MCP_ACCEPT, MCP_TIMEOUT)
                .map_err(|e| format!("MCP {e}"))?;
            mcp_result(&response, MCP_REQUEST_ID)
        })
    })
}

/// What the consent window asks (`prompts.json` `exponential-ui-consent`,
/// Deny focused so Return denies): the tool a `harness.mcp` call runs, else
/// the function's own name (the web's `ConsentCard`).
pub(crate) fn consent_prompt(call: &FunctionCallInfo) -> domain::prompts::Prompt {
    let tool = if call.name == "harness.mcp" {
        arg_str(&call.args, "tool")
    } else {
        call.name.clone()
    };
    domain::prompts::exponential_ui_consent(&tool)
}

/// `ask` decisions: a native confirm over the active window. No window, a
/// dismissal or Deny = `false`.
fn consent_hook() -> ConsentHook {
    Rc::new(|call: &FunctionCallInfo, cx: &mut App| -> Task<bool> {
        let (tx, rx) = flume::bounded::<bool>(1);
        let prompt = consent_prompt(call);
        crate::navigation::on_active_window(cx, move |window, cx| {
            let (ok, closed) = (tx.clone(), tx);
            let spec = crate::native_dialog::AlertSpec::from_prompt("Exponential UI", &prompt)
                .on_ok(move |_, _| {
                    let _ = ok.try_send(true);
                    true
                })
                .on_closed(move |_| {
                    let _ = closed.try_send(false);
                });
            crate::native_dialog::open_alert(window, cx, spec);
        });
        cx.background_spawn(async move { rx.recv_async().await.unwrap_or(false) })
    })
}

// ---------------------------------------------------------------------------
// The painter plugin + the app-extension painters
// ---------------------------------------------------------------------------

/// The rest of the painter callbacks: icons by CONCEPT from the generated
/// registry (then raw Lucide names, the web host's map), fonts as the
/// kitchen sink (the painter default).
pub(crate) struct IdePlugin;

pub(crate) fn icon_path(name: &str) -> Option<SharedString> {
    crate::icons::registry::concept_by_name(name)
        .or_else(|| crate::icons::registry::icon_by_name(name))
        .map(|icon| icon.path())
}

impl HostPlugin for IdePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        icon_path(name)
    }

    fn on_unknown(&self, node: &PlacedNode) {
        log::warn!("[exponential-ui host] unknown component {} ({})", node.component, node.id);
    }
}

fn theme_color(theme: Option<&Arc<ResolvedTheme>>, mode: Mode, key: &str) -> Option<Hsla> {
    theme?
        .modes
        .get(mode)
        .color
        .get(key)
        .and_then(|hex| exponential_ui_gpui::paint::color::parse_hex(hex))
}

fn prop_str<'a>(node: &'a PlacedNode, key: &str) -> Option<&'a str> {
    node.props.get(key).and_then(Value::as_str)
}

/// `IconDisc`: a 32 px circle in the tone colour at 15 % with the icon
/// concept centred in the tone colour (the web's `IconDiscPainter`).
pub(crate) struct IconDiscPainter;

/// The disc tone → the theme colour key.
pub(crate) fn disc_color_key(tone: Option<&str>) -> &'static str {
    match tone {
        Some("success") => "success",
        Some("danger") => "destructive",
        Some("muted") => "mutedForeground",
        _ => "primary",
    }
}

impl ExtensionPainter for IconDiscPainter {
    fn measure(&self, _: &LeafRequest, _: Option<f32>, _: &mut Window, _: &mut App) -> Option<(f32, f32)> {
        Some((32., 32.))
    }

    fn paint(&self, ctx: PaintContext, _: &mut Window, _: &mut App) -> AnyElement {
        let color = theme_color(ctx.theme, ctx.mode, disc_color_key(prop_str(ctx.node, "tone")))
            .or_else(|| theme_color(ctx.theme, ctx.mode, "primary"))
            .unwrap_or_else(|| theme::tokens::BLUE.to_hsla());
        let disc = div()
            .size(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(color.opacity(0.15));
        match prop_str(ctx.node, "icon").and_then(icon_path) {
            Some(path) => disc.child(svg().path(path).size(px(16.)).text_color(color)),
            None => disc,
        }
        .into_any_element()
    }
}

/// `LiveDot`: the 8 px status dot (the SDK's `controls::live_dot`), in the
/// colours the IDE paints its device and session dots with.
pub(crate) struct LiveDotPainter;

impl ExtensionPainter for LiveDotPainter {
    fn measure(&self, _: &LeafRequest, _: Option<f32>, _: &mut Window, _: &mut App) -> Option<(f32, f32)> {
        let size = exponential_ui_gpui::controls::LIVE_DOT_PX;
        Some((size, size))
    }

    fn paint(&self, ctx: PaintContext, _: &mut Window, _: &mut App) -> AnyElement {
        let muted = theme_color(ctx.theme, ctx.mode, "mutedForeground")
            .unwrap_or_else(|| theme::tokens::NEUTRAL.to_hsla());
        let tone = match prop_str(ctx.node, "tone") {
            Some("live") => theme::tokens::GREEN.to_hsla(),
            Some("attention") => theme::tokens::YELLOW.to_hsla(),
            Some("done") => theme::tokens::BLUE.to_hsla(),
            Some("unread") => theme_color(ctx.theme, ctx.mode, "primary").unwrap_or(muted),
            Some("idle") => muted.opacity(0.4),
            _ => muted,
        };
        let ping = ctx.node.props.get("ping").and_then(Value::as_bool).unwrap_or(false);
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(exponential_ui_gpui::controls::live_dot(tone, ping))
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// The host
// ---------------------------------------------------------------------------

/// The IDE's host: the SDK's [`ExponentialHost`] entity plus the store
/// observers feeding its `exp:` source. Keep it alive as long as its
/// surfaces are on screen.
pub(crate) struct IdeHost {
    pub(crate) host: Entity<ExponentialHost>,
    _feeds: Rc<RefCell<Feeds>>,
    _subscriptions: Vec<Subscription>,
    _tick: Task<()>,
}

impl IdeHost {
    pub(crate) fn new(mode: Mode, cx: &mut App) -> Self {
        let feeds = Rc::new(RefCell::new(Feeds::default()));
        refresh_devices(&feeds, cx);
        refresh_issues(&feeds, cx);
        let mut subscriptions = Vec::new();
        if let Some(store) = sync::Store::try_global(cx) {
            let (devices, issues) = (
                store.collections().devices.clone(),
                store.collections().issues.clone(),
            );
            let f = feeds.clone();
            subscriptions.push(cx.observe(&devices, move |_, cx| refresh_devices(&f, cx)));
            let f = feeds.clone();
            subscriptions.push(cx.observe(&issues, move |_, cx| refresh_issues(&f, cx)));
        }
        let weak = Rc::downgrade(&feeds);
        let tick = cx.spawn(async move |cx| loop {
            cx.background_executor().timer(LIVENESS_TICK).await;
            let Some(feeds) = weak.upgrade() else { return };
            cx.update(|cx| refresh_devices(&feeds, cx));
        });

        let instance = sync::Store::try_global(cx)
            .and_then(|_| crate::queries::active_account(cx))
            .map(|a| a.instance_url);
        let mut functions: HashMap<String, HostFunction> = HashMap::new();
        functions.insert("harness.toast".into(), toast_fn());
        functions.insert("harness.openIssue".into(), open_issue_fn());
        functions.insert("harness.openDevice".into(), open_device_fn());
        functions.insert("harness.mcp".into(), mcp_fn());
        let mut sources: HashMap<String, SourceResolver> = HashMap::new();
        sources.insert("exp".into(), exp_resolver(Rc::downgrade(&feeds)));
        let extensions = match parse_extension(APP_EXTENSION_JSON) {
            Ok(ext) => vec![ext],
            Err(e) => {
                log::error!("[exponential-ui host] the app extension does not parse: {e}");
                Vec::new()
            }
        };
        let options = HostOptions {
            functions,
            sources,
            extensions,
            painters: vec![
                ("IconDisc".into(), Rc::new(IconDiscPainter) as Rc<dyn ExtensionPainter>),
                ("LiveDot".into(), Rc::new(LiveDotPainter) as Rc<dyn ExtensionPainter>),
            ],
            policy: HostPolicy {
                functions: Some(FunctionPolicy {
                    ask: Some(vec!["harness.mcp".into()]),
                    ..Default::default()
                }),
                on_function_call: Some(consent_hook()),
                urls: Some(UrlPolicy { base_url: instance.clone(), ..Default::default() }),
                open_url: None,
                media: Some(MediaOptions { base_url: instance, ..Default::default() }),
            },
            theme: exponential_ui::themes::builtin_theme(THEME_ID),
            mode,
            plugin: Rc::new(IdePlugin),
            ..HostOptions::default()
        };
        let host = cx.new(|cx| {
            let mut host = ExponentialHost::new(options, cx);
            match serde_json::from_str::<Value>(DEVICES_PACKAGE_JSON) {
                Ok(pkg) => {
                    for issue in host.install_package(&pkg) {
                        log::warn!("[exponential-ui host] Devices package: {issue:?}");
                    }
                }
                Err(e) => log::error!("[exponential-ui host] the Devices package does not parse: {e}"),
            }
            host
        });
        Self { host, _feeds: feeds, _subscriptions: subscriptions, _tick: tick }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(value: Value) -> domain::rows::DeviceRow {
        serde_json::from_value(value).expect("a devices row")
    }

    const NOW: i64 = 1_791_460_800_000; // 2026-10-08T12:00:00Z

    #[test]
    fn format_ago_matches_the_web() {
        // The web's `readinessAgo`: floored, never rounded up.
        assert_eq!(format_ago(NOW, NOW), "just now");
        assert_eq!(format_ago(NOW - 59_000, NOW), "just now");
        assert_eq!(format_ago(NOW - 60_000, NOW), "1 min ago");
        assert_eq!(format_ago(NOW - 119_000, NOW), "1 min ago");
        assert_eq!(format_ago(NOW - 59 * 60_000, NOW), "59 min ago");
        assert_eq!(format_ago(NOW - 119 * 60_000, NOW), "1 h ago");
        assert_eq!(format_ago(NOW - 3 * 3_600_000, NOW), "3 h ago");
        assert_eq!(format_ago(NOW - 47 * 3_600_000, NOW), "1 d ago");
        assert_eq!(format_ago(NOW - 72 * 3_600_000, NOW), "3 d ago");
    }

    #[test]
    fn device_rows_derive_like_the_web_host() {
        let online = device(json!({
            "id": "d1", "label": "MacBook Pro", "kind": "desktop", "platform": "macos",
            "version": "0.18.80", "last_seen_at": "2026-10-08T11:59:50Z"
        }));
        let server = device(json!({
            "id": "d2", "label": "homeserver", "kind": "server", "platform": "linux",
            "icon": "server", "last_seen_at": "2026-10-08 09:00:00+00"
        }));
        let never = device(json!({ "id": "d3", "label": "Old box", "kind": "desktop" }));
        let value = devices_value([&server, &never, &online], NOW);
        assert_eq!(value["count"], 3);
        assert_eq!(value["online"], 1);
        let rows = value["rows"].as_array().unwrap();
        let labels: Vec<&str> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["homeserver", "MacBook Pro", "Old box"]);
        assert_eq!(
            rows[1],
            json!({
                "id": "d1", "label": "MacBook Pro", "detail": "Desktop · macos · v0.18.80",
                "icon": "monitor", "discTone": "success", "tone": "live", "status": "Online"
            })
        );
        assert_eq!(rows[0]["detail"], "Server · linux");
        assert_eq!(rows[0]["icon"], "server");
        assert_eq!(rows[0]["tone"], "idle");
        assert_eq!(rows[0]["discTone"], "muted");
        assert_eq!(rows[0]["status"], "Last seen 3 h ago");
        assert_eq!(rows[2]["detail"], "Desktop");
        assert_eq!(rows[2]["status"], "Offline");
    }

    #[test]
    fn issues_filter_sort_and_limit() {
        let issue = |id: &str, board: &str, order: f64| IssueLite {
            id: id.into(),
            identifier: format!("EXP-{id}"),
            title: id.into(),
            status: json!("backlog"),
            board_id: board.into(),
            sort_order: order,
        };
        let all = vec![issue("3", "b1", 3.), issue("1", "b1", 1.), issue("2", "b2", 2.)];
        let v = issues_value(&all, Some("b1"), 50);
        assert_eq!(v["count"], 2);
        assert_eq!(v["rows"][0]["identifier"], "EXP-1");
        let v = issues_value(&all, None, 2);
        assert_eq!(v["count"], 2);
        assert_eq!(v["rows"][1]["id"], "2");
    }

    #[test]
    fn the_exp_source_emits_its_snapshot_and_cancels() {
        let feeds = Rc::new(RefCell::new(Feeds::default()));
        feeds.borrow_mut().devices = json!({"rows": [], "count": 0, "online": 0});
        let resolver = exp_resolver(Rc::downgrade(&feeds));
        let got = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let sink = got.clone();
        let emit: Emit = Arc::new(move |v| sink.lock().unwrap().push(v));
        let parse = |s: &str| exponential_ui::host::parse_source(s).unwrap();
        let cancel = resolver(parse("exp:devices"), emit.clone());
        assert_eq!(got.lock().unwrap().len(), 1);
        // A move re-emits; an unchanged publish does not.
        feeds.borrow_mut().devices = json!({"rows": [], "count": 1, "online": 0});
        feeds.borrow_mut().publish();
        feeds.borrow_mut().publish();
        assert_eq!(got.lock().unwrap().len(), 2);
        cancel();
        assert!(feeds.borrow().subs.is_empty());
        // An unknown name emits null at once.
        let _ = resolver(parse("exp:nope"), emit);
        assert_eq!(got.lock().unwrap().last(), Some(&Value::Null));
    }

    struct Holder(Entity<exponential_ui_gpui::view::SurfaceView>);
    impl gpui::Render for Holder {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            div().w(px(600.)).child(self.0.clone())
        }
    }

    /// A row press (a list TEMPLATE instance, `row.0`) reaches
    /// `harness.openDevice` through the host.
    #[gpui::test]
    fn a_row_press_reaches_the_host_function(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let calls = Rc::new(RefCell::new(Vec::<Value>::new()));
        let c = calls.clone();
        let host = cx.update(|cx| {
            let ide = IdeHost::new(Mode::Dark, cx);
            ide.host.update(cx, |h, _| {
                h.register_function(
                    "harness.openDevice",
                    exponential_ui_gpui::runtime::sync_function(move |args, _, _| {
                        c.borrow_mut().push(args);
                        Ok(Value::Null)
                    }),
                )
            });
            let host = ide.host.clone();
            std::mem::forget(ide);
            host
        });
        host.update(cx, |h, cx| {
            h.receive(&json!({"version": "v0.9", "applyTemplate": {"surfaceId": "devices", "templateId": "devices"}}), cx);
        });
        cx.run_until_parked();
        let view = host.read_with(cx, |h, _| h.surface("devices")).expect("the surface");
        let row = device(json!({"id": "d1", "label": "Mac", "kind": "desktop"}));
        view.update(cx, |v, cx| v.set_data("/devices", Some(devices_value([&row], NOW)), cx));
        let v2 = view.clone();
        let (_, vcx) = cx.add_window_view(move |_, _| Holder(v2));
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
        let pressable: Vec<(u32, String)> = view.read_with(vcx, |v, _| {
            v.placed_nodes().iter().filter(|n| n.pressable).map(|n| (n.index, n.id.clone())).collect()
        });
        assert!(!pressable.is_empty(), "a pressable row");
        let index = pressable[0].0;
        vcx.update(|window, cx| view.update(cx, |v, cx| v.press(index, window, cx)));
        vcx.run_until_parked();
        assert_eq!(*calls.borrow(), vec![json!({"id": "d1", "label": "Mac"})], "{pressable:?}");
    }

    #[test]
    fn mcp_results_decode_json_and_sse() {
        let json_body = r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok"}]}}"#;
        assert_eq!(mcp_result(json_body, 1).unwrap()["content"][0]["text"], "ok");
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"x\":1}}\n\n";
        assert_eq!(mcp_result(sse, 1).unwrap(), json!({"x": 1}));
        assert_eq!(mcp_result(json_body, 2).unwrap_err(), "MCP: no answer");
    }

    #[test]
    fn mcp_results_read_only_the_answer_with_our_id() {
        // A notification, a foreign id, then ours (multi-line data, CRLF).
        let sse = concat!(
            ": keep-alive\n\n",
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{}}\n\n",
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{\"x\":\"foreign\"}}\n\n",
            "event: message\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\r\ndata: \"result\":{\"x\":\"ours\"}}\r\n\r\n",
        );
        assert_eq!(mcp_result(sse, 7).unwrap(), json!({"x": "ours"}));
        assert_eq!(mcp_result(sse, 3).unwrap_err(), "MCP: no answer");
        // A batch body answers by id too.
        let batch = r#"[{"jsonrpc":"2.0","id":2,"result":{}},{"jsonrpc":"2.0","id":1,"result":{"y":1}}]"#;
        assert_eq!(mcp_result(batch, 1).unwrap(), json!({"y": 1}));
        // An error reply rejects with its message and code.
        let error = "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32602,\"message\":\"Unknown tool\"}}\n\n";
        assert_eq!(mcp_result(error, 1).unwrap_err(), "Unknown tool (code -32602)");
        assert_eq!(
            mcp_result(r#"{"jsonrpc":"2.0","id":1,"error":{"message":""}}"#, 1).unwrap_err(),
            "MCP error"
        );
    }

    #[test]
    fn consent_names_the_tool() {
        let call = FunctionCallInfo {
            surface_id: "s".into(),
            component_id: "c".into(),
            name: "harness.mcp".into(),
            args: json!({"tool": "exponential_issues_list"}),
        };
        let prompt = consent_prompt(&call);
        assert_eq!(prompt, domain::prompts::exponential_ui_consent("exponential_issues_list"));
        assert_eq!(prompt.title, "Allow this surface to run exponential_issues_list?");
        assert_eq!(prompt.body.as_deref(), Some("It acts as you, with your access to this team."));
        // Return denies: Deny = cancel + focused, Allow = a plain default.
        assert_eq!(prompt.focused().label, "Deny");
        assert_eq!(prompt.focused().role, domain::prompts::Role::Cancel);
        assert_eq!(prompt.action("allow").role, domain::prompts::Role::Default);
        let named = FunctionCallInfo { name: "harness.openIssue".into(), args: json!({}), ..call };
        assert_eq!(consent_prompt(&named).title, "Allow this surface to run harness.openIssue?");
    }

    #[test]
    fn the_package_and_extension_parse_and_name_the_painted_natives() {
        let pkg: Value = serde_json::from_str(DEVICES_PACKAGE_JSON).unwrap();
        assert_eq!(pkg["id"], "exponential.devices");
        let ext = parse_extension(APP_EXTENSION_JSON).expect("the app extension parses");
        assert_eq!(ext.id, pkg["catalogId"].as_str().unwrap());
        assert_eq!(disc_color_key(Some("danger")), "destructive");
        assert_eq!(disc_color_key(None), "primary");
        for name in ["nav-devices", "monitor", "server"] {
            assert!(icon_path(name).is_some(), "{name} resolves");
        }
    }
}
