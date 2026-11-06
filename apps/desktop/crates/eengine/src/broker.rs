//! VAPP-262 (contract E0 VAPP-260): the capability catalog and the ONE broker
//! for app UI and agent calls: the package's manifest is the ceiling, the
//! person's consent comes next, then one audit row. Pure; locked by
//! `packages/eengine/fixtures/capabilities.json` (the TS twin:
//! `@exp/eengine/capabilities`).

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::contract::{strings, CAPABILITIES};

/// How a scope asks: the Engine's sheet once (`ask`), or the platform picker every call (`picker`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsentKind {
    Ask,
    Picker,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityScope {
    pub id: String,
    pub verb: String,
    pub consent: ConsentKind,
    pub tools: Vec<String>,
    #[serde(default)]
    pub host_only: bool,
    #[serde(default)]
    pub origins: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDef {
    pub name: String,
    pub label: String,
    pub scopes: Vec<CapabilityScope>,
}

/// One entry of a package's capability manifest (format 5 `capabilities`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origins: Option<Vec<String>>,
}

impl ManifestEntry {
    /// The manifest of a package (`capabilities`; absent or malformed = none).
    pub fn of_package(pkg: &Value) -> Vec<ManifestEntry> {
        pkg.get("capabilities").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
    }
}

pub static CATALOG: LazyLock<Vec<CapabilityDef>> =
    LazyLock::new(|| serde_json::from_value(CAPABILITIES["capabilities"].clone()).expect("capabilities.json capabilities parse"));
pub static RESERVED_NAMES: LazyLock<Vec<String>> = LazyLock::new(|| strings(&CAPABILITIES, "/reservedNames"));

pub fn capability_def(name: &str) -> Option<&'static CapabilityDef> {
    CATALOG.iter().find(|c| c.name == name)
}

/// The catalog scope with `id`, with its capability.
pub fn scope_def(id: &str) -> Option<(&'static CapabilityDef, &'static CapabilityScope)> {
    CATALOG.iter().find_map(|c| c.scopes.iter().find(|s| s.id == id).map(|s| (c, s)))
}

fn name_part(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Is `id` a scope id of the right form (`<capability>.<scope>`)?
pub fn scope_id_valid(id: &str) -> bool {
    id.split_once('.').is_some_and(|(a, b)| name_part(a) && name_part(b))
}

/// May a capability be mounted under `name`? A hub server name (hub.json:
/// lowercase letters, digits and `-`, at most 32) that is not reserved.
pub fn capability_name_valid(name: &str) -> bool {
    let mut chars = name.chars();
    let hub_name = name.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    hub_name && !RESERVED_NAMES.iter().any(|r| r == name)
}

/// Where a call comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallFrom {
    Ui,
    Agent,
}

/// A call's name → `(capability, tool)`: `<capability>__<tool>` from an
/// agent, `<capability>.<tool>` from app UI; `None` for any other form or a
/// reserved name.
pub fn parse_capability_call(from: CallFrom, name: &str) -> Option<(String, String)> {
    let sep = match from {
        CallFrom::Agent => "__",
        CallFrom::Ui => ".",
    };
    let (capability, tool) = name.split_once(sep)?;
    if !name_part(capability) || !name_part(tool) || RESERVED_NAMES.iter().any(|r| r == capability) {
        return None;
    }
    Some((capability.to_string(), tool.to_string()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerdictKind {
    Call,
    Ask,
    Denied,
    Unknown,
    Unavailable,
}

/// The broker's answer to one call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub verdict: VerdictKind,
    pub capability: Option<String>,
    pub tool: Option<String>,
    pub scope: Option<String>,
    pub reason: Option<String>,
}

impl Verdict {
    fn unknown() -> Self {
        Verdict { verdict: VerdictKind::Unknown, capability: None, tool: None, scope: None, reason: Some("unknownTool".into()) }
    }
}

/// A stored grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrantValue {
    Granted,
    Denied,
}

/// An app's grants, scope id → value (sorted; absent = undecided).
pub type Grants = std::collections::BTreeMap<String, GrantValue>;

/// One call to decide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrokerCall {
    pub from: CallFrom,
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// The `https://host[:port]` origin of a URL (WHATWG), or `None`.
pub fn url_origin(url: Option<&str>) -> Option<String> {
    let parsed = url::Url::parse(url?).ok()?;
    match parsed.origin() {
        origin @ url::Origin::Tuple(..) => Some(origin.ascii_serialization()),
        url::Origin::Opaque(_) => None,
    }
}

/// The broker's verdict for one call (`decideCases`). `provided` = the
/// capability names this host mounts.
pub fn broker_decide(manifest: &[ManifestEntry], grants: &Grants, provided: &[String], foreground: bool, call: &BrokerCall) -> Verdict {
    let Some((cap, tool)) = parse_capability_call(call.from, &call.name) else { return Verdict::unknown() };
    let Some(def) = capability_def(&cap) else { return Verdict::unknown() };
    let Some(scope) = def.scopes.iter().find(|s| s.tools.iter().any(|t| *t == tool)) else { return Verdict::unknown() };
    let at = |verdict: VerdictKind, reason: Option<&str>| Verdict {
        verdict,
        capability: Some(def.name.clone()),
        tool: Some(tool.clone()),
        scope: Some(scope.id.clone()),
        reason: reason.map(str::to_string),
    };
    if scope.host_only {
        return at(VerdictKind::Denied, Some("hostOnly"));
    }
    let Some(entry) = manifest.iter().find(|m| m.id == scope.id) else { return at(VerdictKind::Denied, Some("notDeclared")) };
    if !provided.iter().any(|p| *p == def.name) {
        return at(VerdictKind::Unavailable, Some("unavailable"));
    }
    if scope.origins {
        let origin = url_origin(call.arguments.get("url").and_then(Value::as_str));
        let declared = entry.origins.as_deref().unwrap_or_default();
        if !origin.is_some_and(|o| declared.iter().any(|d| *d == o)) {
            return at(VerdictKind::Denied, Some("originNotDeclared"));
        }
    }
    if scope.consent == ConsentKind::Picker {
        return if foreground { at(VerdictKind::Call, None) } else { at(VerdictKind::Denied, Some("needsForeground")) };
    }
    match grants.get(&scope.id) {
        Some(GrantValue::Granted) => at(VerdictKind::Call, None),
        Some(GrantValue::Denied) => at(VerdictKind::Denied, Some("consentDenied")),
        None if foreground => at(VerdictKind::Ask, None),
        None => at(VerdictKind::Denied, Some("consentNeeded")),
    }
}

/// An agent's tools/list: every tool of a declared, provided, non-host-only scope, sorted.
pub fn agent_tool_list(manifest: &[ManifestEntry], provided: &[String]) -> Vec<String> {
    let mut names: Vec<String> = CATALOG
        .iter()
        .filter(|c| provided.iter().any(|p| *p == c.name))
        .flat_map(|c| {
            c.scopes
                .iter()
                .filter(|s| !s.host_only && manifest.iter().any(|m| m.id == s.id))
                .flat_map(move |s| s.tools.iter().map(move |t| format!("{}__{t}", c.name)))
        })
        .collect();
    names.sort();
    names
}

/// A call's outcome when it ran (hub.json `outcomes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallOutcome {
    Ok,
    IsError,
    InvalidParams,
    RpcError,
    Unreachable,
    Timeout,
}

impl CallOutcome {
    /// audit.json's status and hub.json's error code of the outcome.
    pub fn status(self) -> (&'static str, Option<&'static str>) {
        match self {
            CallOutcome::Ok => ("ok", None),
            CallOutcome::IsError => ("failed", None),
            CallOutcome::InvalidParams => ("invalid", Some("invalidArguments")),
            CallOutcome::RpcError | CallOutcome::Unreachable => ("upstream", Some("upstreamFailed")),
            CallOutcome::Timeout => ("upstream", Some("upstreamTimeout")),
        }
    }
}

/// The audit row of one brokered call (audit.json's columns the broker fills).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerAuditRow {
    pub app_id: String,
    pub client_id: String,
    pub tool: String,
    pub status: String,
    pub error: Option<String>,
    pub args_hash: String,
}

/// audit.json's args hash: `sha256:` + hex SHA-256 of the canonical JSON of the arguments (absent = `{}`).
pub fn args_hash(args: &Value) -> String {
    let empty = Value::Object(Default::default());
    let args = if args.is_null() { &empty } else { args };
    let digest = Sha256::digest(eui_core::host::canonical_json(args).as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// The audit row of one call. `client` = the agent run's id (`None` for UI);
/// `outcome` = what happened when the verdict was `call`.
pub fn broker_audit_row(app_id: &str, client: Option<&str>, call: &BrokerCall, verdict: &Verdict, outcome: Option<CallOutcome>) -> BrokerAuditRow {
    let tool = match (&verdict.capability, &verdict.tool) {
        (Some(c), Some(t)) => format!("{c}__{t}"),
        _ if call.from == CallFrom::Ui => call.name.replacen('.', "__", 1),
        _ => call.name.clone(),
    };
    let client_id = match call.from {
        CallFrom::Ui => "ui".to_string(),
        CallFrom::Agent => client.unwrap_or_default().to_string(),
    };
    let (status, error) = if verdict.verdict == VerdictKind::Call {
        let (status, error) = outcome.unwrap_or(CallOutcome::Ok).status();
        (status.to_string(), error.map(str::to_string))
    } else {
        ("denied".to_string(), verdict.reason.clone())
    };
    BrokerAuditRow { app_id: app_id.to_string(), client_id, tool, status, error, args_hash: args_hash(&call.arguments) }
}
