//! One member's connection to one upstream
//! (`packages/emcp/fixtures/upstream.json`): the states, the transitions,
//! what is callable and the consent page's missing upstreams; VAPP-230:
//! the `upstream:<server>/<tool>?<args>` read bindings (`binding`); VAPP-239:
//! device-routed upstreams (`device`).

use std::collections::{BTreeSet, HashMap};

use serde::Deserialize;
use serde_json::{Map, Value};

use super::fixture;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpstreamState {
    Disconnected,
    Connecting,
    Connected,
    Expired,
    Reconnect,
    Failed,
}

impl UpstreamState {
    pub fn as_str(self) -> &'static str {
        match self {
            UpstreamState::Disconnected => "disconnected",
            UpstreamState::Connecting => "connecting",
            UpstreamState::Connected => "connected",
            UpstreamState::Expired => "expired",
            UpstreamState::Reconnect => "reconnect",
            UpstreamState::Failed => "failed",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        serde_json::from_value(Value::String(text.to_string())).ok()
    }

    /// The words every surface shows (`labels`).
    pub fn label(self) -> &'static str {
        match self {
            UpstreamState::Disconnected => "Not connected",
            UpstreamState::Connecting => "Connecting…",
            UpstreamState::Connected | UpstreamState::Expired => "Connected",
            UpstreamState::Reconnect => "Reconnect needed",
            UpstreamState::Failed => "Connection failed",
        }
    }
}

/// The binding errors (`errors`).
pub const ERROR_DISCONNECTED: &str = "upstreamDisconnected";
pub const ERROR_MISSING: &str = "upstreamMissing";
pub const ERROR_FAILED: &str = "upstreamFailed";

/// The next state; an event a state does not list changes nothing (`*`
/// wins). Events are the fixture's words (`connect`, `callbackOk`, …).
pub fn upstream_step(state: UpstreamState, event: &str) -> UpstreamState {
    let transitions = &fixture::upstream()["transitions"];
    let next = transitions["*"][event].as_str().or_else(|| transitions[state.as_str()][event].as_str());
    next.and_then(UpstreamState::parse).unwrap_or(state)
}

/// May the broker call through it (an expired token is refreshed first)?
pub fn upstream_callable(state: UpstreamState) -> bool {
    matches!(state, UpstreamState::Connected | UpstreamState::Expired)
}

/// The servers the chosen apps declare that exist and are not callable for
/// this member (no state = disconnected), sorted, each once.
pub fn missing_upstreams(
    declared: &[Vec<String>],
    states: &HashMap<String, UpstreamState>,
    servers: &[String],
) -> Vec<String> {
    let names: BTreeSet<&String> = declared.iter().flatten().filter(|name| servers.contains(name)).collect();
    names
        .into_iter()
        .filter(|name| !upstream_callable(states.get(*name).copied().unwrap_or(UpstreamState::Disconnected)))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// VAPP-230 (eApps M1): `upstream:<server>/<tool>?<args>` read bindings
// (`binding` in the fixture: its `$comment` is the spec).
// ---------------------------------------------------------------------------

/// The binding scheme (`binding.scheme`).
pub const BINDING_SCHEME: &str = "upstream";
/// A good answer is cached this long; hosts re-read on the same beat.
pub const CACHE_SECONDS: u64 = 60;
/// The host banner of a disconnected upstream (`{server}` = its name).
pub const BANNER: &str = "Connect {server}";
/// The banner's one action (opens Settings › MCP).
pub const CONNECT: &str = "Connect";
/// The binding's own refusals (`binding.errors`).
pub const ERROR_NOT_READ_ONLY: &str = "notReadOnly";
pub const ERROR_DESTRUCTIVE: &str = "destructive";
pub const ERROR_NOT_DECLARED: &str = "notDeclared";
pub const ERROR_INVALID: &str = "invalid";

/// One parsed `upstream:` source: the server, its tool and the arguments
/// (keys sorted).
#[derive(Clone, Debug, PartialEq)]
pub struct UpstreamBinding {
    pub server: String,
    pub tool: String,
    pub args: Map<String, Value>,
}

/// JavaScript's `decodeURIComponent(part.replace(/\+/g, " "))`, the RAW part
/// when it throws (a bad escape, invalid UTF-8).
fn decode(part: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        (b as char).to_digit(16).map(|d| d as u8)
    }
    let spaced = part.replace('+', " ");
    let bytes = spaced.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            match (bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => return part.to_string(),
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| part.to_string())
}

/// `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`, so `007` stays text.
fn json_number(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - start
    };
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(c) if c.is_ascii_digit() => {
            digits(&mut i);
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if digits(&mut i) == 0 {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// `Number(text)` as JSON: an integral value in the safe range is an
/// integer (so `-1.5e2` equals `-150`), a non-finite one is null.
fn number_value(text: &str) -> Value {
    let f: f64 = text.parse().unwrap_or(f64::NAN);
    if f.is_finite() && f.fract() == 0.0 && f.abs() <= 9_007_199_254_740_991.0 {
        Value::from(f as i64)
    } else {
        serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

/// A param's text as an argument: JSON literals, numbers, objects and arrays
/// read as JSON; anything else stays the string.
pub fn upstream_arg(text: &str) -> Value {
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ if json_number(text) => number_value(text),
        _ if text.starts_with('{') || text.starts_with('[') => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
        }
        _ => Value::String(text.to_string()),
    }
}

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `(scheme, raw name, query)` of a source (`^([a-zA-Z][a-zA-Z0-9+.-]*):([^?]+)(?:\?(.*))?$`).
fn split_source(source: &str) -> Option<(&str, &str, Option<&str>)> {
    let (scheme, rest) = source.trim().split_once(':')?;
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    {
        return None;
    }
    let (name, query) = match rest.split_once('?') {
        Some((n, q)) => (n, Some(q)),
        None => (rest, None),
    };
    if name.is_empty() || query.is_some_and(|q| q.chars().any(is_line_terminator)) {
        return None;
    }
    Some((scheme, name, query))
}

/// `upstream:crm/search?q=acme` → `{server: "crm", tool: "search", args:
/// {q: "acme"}}` (eUI's source grammar; the name splits at its LAST `/`),
/// `None` when it is not an upstream source.
pub fn parse_upstream_source(source: &str) -> Option<UpstreamBinding> {
    let (scheme, raw_name, query) = split_source(source)?;
    if !scheme.eq_ignore_ascii_case(BINDING_SCHEME) {
        return None;
    }
    let name = decode(raw_name);
    let slash = name.rfind('/')?;
    if slash == 0 || slash == name.len() - 1 {
        return None;
    }
    let mut params = std::collections::BTreeMap::new();
    for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        let (k, v) = match pair.find('=') {
            Some(eq) => (&pair[..eq], &pair[eq + 1..]),
            None => (pair, ""),
        };
        let key = decode(k);
        if !key.is_empty() {
            params.insert(key, decode(v));
        }
    }
    let args = params.into_iter().map(|(k, v)| (k, upstream_arg(&v))).collect();
    Some(UpstreamBinding { server: name[..slash].to_string(), tool: name[slash + 1..].to_string(), args })
}

/// One refused binding of the install check: the source's index and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclareIssue {
    pub index: usize,
    pub code: &'static str,
}

/// Exponential's install check: every `upstream:` source of a package's
/// template bindings parses and names one of its `upstreams`.
pub fn upstream_declare_issues(sources: &[String], upstreams: &[String]) -> Vec<DeclareIssue> {
    let prefix = format!("{BINDING_SCHEME}:");
    sources
        .iter()
        .enumerate()
        .filter(|(_, source)| source.trim().get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(&prefix)))
        .filter_map(|(index, source)| match parse_upstream_source(source) {
            None => Some(DeclareIssue { index, code: ERROR_INVALID }),
            Some(b) if !upstreams.contains(&b.server) => Some(DeclareIssue { index, code: ERROR_NOT_DECLARED }),
            Some(_) => None,
        })
        .collect()
}

/// The broker's call-time check against the tool's live annotations and the
/// package's `sources` (an exact `upstream:<server>/<tool>` vouches for a tool
/// that sets no readOnlyHint): `None` = it reads, else the error.
pub fn upstream_read_error(server: &str, tool: &str, annotations: Option<&Value>, sources: &[String]) -> Option<&'static str> {
    let annotations = annotations.filter(|a| !a.is_null());
    if super::exposes::upstream_destructive(tool, annotations) {
        return Some(ERROR_DESTRUCTIVE);
    }
    match annotations.and_then(|a| a.get("readOnlyHint")) {
        Some(Value::Bool(true)) => return None,
        Some(Value::Bool(false)) => return Some(ERROR_NOT_READ_ONLY),
        _ => {}
    }
    let vouch = format!("{BINDING_SCHEME}:{server}/{tool}");
    if sources.iter().any(|s| *s == vouch) {
        None
    } else {
        Some(ERROR_NOT_READ_ONLY)
    }
}

/// The cache key of one read: member, server id, tool and canonical args.
pub fn upstream_cache_key(member: &str, server_id: &str, tool: &str, args: &Map<String, Value>) -> String {
    let args = super::audit::canonical_json(&Value::Object(args.clone()));
    [member, server_id, tool, args.as_str()].join("\n")
}

/// An MCP tools/call result as the binding's value: `Ok(value)`, or
/// `Err(upstreamFailed)` for an `isError` answer.
pub fn upstream_binding_value(result: &Value) -> Result<Value, &'static str> {
    if result.get("isError") == Some(&Value::Bool(true)) {
        return Err(ERROR_FAILED);
    }
    if let Some(structured) = result.get("structuredContent").filter(|v| v.is_object()) {
        return Ok(structured.clone());
    }
    let text = result.get("content").and_then(Value::as_array).and_then(|content| {
        content
            .iter()
            .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
            .find_map(|c| c.get("text").and_then(Value::as_str))
    });
    Ok(match text {
        None => Value::Null,
        Some(text) => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string())),
    })
}

/// One host answer of tRPC `apps.upstreamRead`: `{ok: true, value, meta}` |
/// `{ok: false, error, server, message}`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct UpstreamReadAnswer {
    pub ok: bool,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub meta: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    /// VAPP-239: the device a device-routed read runs on, when it is offline.
    #[serde(default)]
    pub offline: Option<String>,
}

/// The servers whose read answered upstreamDisconnected, sorted, each once.
pub fn upstream_banner_servers(answers: &[UpstreamReadAnswer]) -> Vec<String> {
    let servers: BTreeSet<&String> = answers
        .iter()
        .filter(|a| !a.ok && a.error.as_deref() == Some(ERROR_DISCONNECTED))
        .filter_map(|a| a.server.as_ref())
        .collect();
    servers.into_iter().cloned().collect()
}

/// The host's banners: one per server whose read answered
/// upstreamDisconnected, sorted, each once (`Connect <server>`).
pub fn upstream_banners(answers: &[UpstreamReadAnswer]) -> Vec<String> {
    upstream_banner_servers(answers).iter().map(|server| BANNER.replace("{server}", server)).collect()
}

// ---------------------------------------------------------------- device (VAPP-239)

/// How long a device call lives (`device.ttlSeconds`).
pub const DEVICE_CALL_TTL_SECONDS: u64 = 30;
/// How long the relay holds a call for the device's answer.
pub const DEVICE_ANSWER_TIMEOUT_MS: u64 = 25_000;
/// The largest answer (JSON bytes) a device seals.
pub const DEVICE_MAX_ANSWER_BYTES: usize = 262_144;
/// The refusal of a call to another member's device.
pub const ERROR_DEVICE_NOT_OWNED: &str = "deviceNotOwned";

/// The device copy (`device.copy`), `{server}` / `{device}` filled.
pub const COPY_NO_DEVICE: &str = "{server} runs on a device (stdio): its owner names the device in Settings › MCP";
pub const COPY_NOT_OWNER: &str = "{server} runs on {device}: only its owner's calls reach it";
pub const COPY_OFFLINE: &str = "{server} runs on {device}, offline";
pub const COPY_NO_ANSWER: &str = "{server}: {device} did not answer";

fn fill(template: &str, server: &str, device: &str) -> String {
    template.replace("{server}", server).replace("{device}", device)
}

/// The device a server row names: its id, its owner and its label.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutedDevice {
    pub id: String,
    pub owner_id: String,
    pub label: String,
}

/// A server row as the router reads it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RoutedServer {
    pub name: String,
    pub transport: String,
    pub device: Option<RoutedDevice>,
}

/// Where one call runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpstreamRoute {
    Server,
    Device { device_id: String },
    Refused { error: &'static str, message: String },
}

/// Where one call to `server` by `caller_id` (the member, or an automation's
/// sponsor) runs: the server, the owner's device, or a refusal.
pub fn upstream_route(server: &RoutedServer, caller_id: &str) -> UpstreamRoute {
    match &server.device {
        None if server.transport == "http" => UpstreamRoute::Server,
        None => UpstreamRoute::Refused { error: ERROR_FAILED, message: fill(COPY_NO_DEVICE, &server.name, "") },
        Some(device) if device.owner_id != caller_id => {
            UpstreamRoute::Refused { error: ERROR_DEVICE_NOT_OWNED, message: fill(COPY_NOT_OWNER, &server.name, &device.label) }
        }
        Some(device) => UpstreamRoute::Device { device_id: device.id.clone() },
    }
}

/// The device's answer, sealed to the call's reply key: the tool's result,
/// or why it did not run (`reason` = `vetoed` | `refused` | `failed`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, Deserialize)]
#[serde(untagged)]
pub enum DeviceAnswer {
    Ok { ok: bool, result: Value },
    Err { ok: bool, reason: String, error: String },
}

impl DeviceAnswer {
    pub fn ok(result: Value) -> Self {
        DeviceAnswer::Ok { ok: true, result }
    }
    pub fn err(reason: &str, error: impl Into<String>) -> Self {
        DeviceAnswer::Err { ok: false, reason: reason.to_string(), error: error.into() }
    }
}

/// One device call's outcome for the broker.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceCallOutcome {
    Result(Value),
    Error { error: &'static str, message: String, offline: Option<String> },
}

/// `relay_reason` = None when the relay delivered, else its reason;
/// `answer` = None when what it delivered does not open.
pub fn device_call_outcome(server: &str, device: &str, relay_reason: Option<&str>, answer: Option<&DeviceAnswer>) -> DeviceCallOutcome {
    let error = |error: &'static str, message: String| DeviceCallOutcome::Error { error, message, offline: None };
    if relay_reason == Some("device_offline") {
        return DeviceCallOutcome::Error { error: ERROR_FAILED, message: fill(COPY_OFFLINE, server, device), offline: Some(device.to_string()) };
    }
    let Some(answer) = answer.filter(|_| relay_reason.is_none()) else {
        return error(ERROR_FAILED, fill(COPY_NO_ANSWER, server, device));
    };
    match answer {
        DeviceAnswer::Ok { result, .. } => DeviceCallOutcome::Result(result.clone()),
        DeviceAnswer::Err { reason, error: why, .. } => match reason.as_str() {
            "vetoed" if why == ERROR_DESTRUCTIVE => error(ERROR_DESTRUCTIVE, format!("{server}'s tool is destructive: it does not run here")),
            "vetoed" if why == "accessTooLow" => error("accessTooLow", format!("{server}'s tool is not read-only: it does not run here")),
            "vetoed" => error(ERROR_NOT_READ_ONLY, format!("{server}'s tool is not read-only: it does not run here")),
            "refused" => error(ERROR_DISCONNECTED, format!("{server} refused your connection; reconnect")),
            _ => error(ERROR_FAILED, format!("{server}: {why}")),
        },
    }
}

/// Whether a device may claim a call: the member's own, on that device,
/// unexpired and not claimed yet.
pub fn device_call_claimable(call_user: &str, call_device: &str, expires_in_ms: i64, claimed: bool, claim_user: &str, claim_device: &str) -> bool {
    !claimed && expires_in_ms > 0 && call_user == claim_user && call_device == claim_device
}

/// One host banner: its text and whether it carries the Connect action.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UpstreamHostBanner {
    pub server: String,
    pub text: String,
    pub connect: bool,
}

/// The host's banners ×4: `Connect <server>` for an upstreamDisconnected
/// read, the offline copy for a device that is offline; one per server,
/// sorted by server.
pub fn upstream_host_banners(answers: &[UpstreamReadAnswer]) -> Vec<UpstreamHostBanner> {
    let mut by_server: std::collections::BTreeMap<String, UpstreamHostBanner> = Default::default();
    for a in answers.iter().filter(|a| !a.ok) {
        let Some(server) = a.server.as_ref() else { continue };
        if by_server.contains_key(server) {
            continue;
        }
        if a.error.as_deref() == Some(ERROR_DISCONNECTED) {
            by_server.insert(server.clone(), UpstreamHostBanner { server: server.clone(), text: BANNER.replace("{server}", server), connect: true });
        } else if let (Some(ERROR_FAILED), Some(device)) = (a.error.as_deref(), a.offline.as_ref()) {
            by_server.insert(server.clone(), UpstreamHostBanner { server: server.clone(), text: fill(COPY_OFFLINE, server, device), connect: false });
        }
    }
    by_server.into_values().collect()
}

const LAN_SUFFIXES: &[&str] = &[".local", ".lan", ".home.arpa", ".internal"];

fn private_ipv4(host: &str) -> bool {
    match host.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => {
            let [a, b, ..] = ip.octets();
            a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168)
        }
        Err(_) => false,
    }
}

/// A server row's URL rule (`device.urlCases`): https anywhere; http on
/// loopback or a LAN host (a device-routed server's reach).
pub fn mcp_url_allowed(value: &str) -> bool {
    let value = value.trim();
    let (scheme, rest) = match value.split_once("://") {
        Some(parts) => parts,
        None => return false,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = match host_port.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => host_port,
    }
    .to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    match scheme.to_ascii_lowercase().as_str() {
        "https" => true,
        "http" => host == "localhost" || host == "127.0.0.1" || private_ipv4(&host) || LAN_SUFFIXES.iter().any(|s| host.ends_with(s)),
        _ => false,
    }
}
