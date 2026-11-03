//! One member's connection to one upstream
//! (`packages/emcp/fixtures/upstream.json`): the states, the transitions,
//! what is callable and the consent page's missing upstreams; VAPP-230:
//! the `upstream:<server>/<tool>?<args>` read bindings (`binding`).

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
