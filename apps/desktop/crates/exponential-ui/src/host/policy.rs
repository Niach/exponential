//! The policy rules every host applies before a client function, a URL open
//! or a media load (`catalog/host.json` functions / urls / media). Pure;
//! `fixtures/host-policy.json` locks them on every platform.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use url::Url;

use super::contract::DEFAULT_URL_SCHEMES;
use super::js_trim;

/// The catalog's built-in client functions: the basic 14 + round 1's 15
/// core ones (`FUNCTION_NAMES`).
pub const BUILTIN_FUNCTIONS: &[&str] = crate::generated::catalog::FUNCTION_NAMES;

pub fn is_builtin_function(name: &str) -> bool {
    BUILTIN_FUNCTIONS.contains(&name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionDecision {
    Allow,
    Ask,
    Deny,
    NotFound,
}

impl FunctionDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            FunctionDecision::Allow => "allow",
            FunctionDecision::Ask => "ask",
            FunctionDecision::Deny => "deny",
            FunctionDecision::NotFound => "not_found",
        }
    }

    pub fn parse(s: &str) -> Option<FunctionDecision> {
        Some(match s {
            "allow" => FunctionDecision::Allow,
            "ask" => FunctionDecision::Ask,
            "deny" => FunctionDecision::Deny,
            "not_found" => FunctionDecision::NotFound,
            _ => return None,
        })
    }

    fn rank(self) -> u8 {
        match self {
            FunctionDecision::Allow => 0,
            FunctionDecision::Ask => 1,
            FunctionDecision::Deny => 2,
            FunctionDecision::NotFound => 3,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
    /// For a registered name no list matches; default `allow`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<FunctionDecision>,
}

/// `harness.*` matches `harness.toast`; anything else is exact.
pub fn matches_pattern(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// The gate: built-ins always run; an unregistered name is `not_found`; then
/// deny wins, then allow, then ask, then the policy's default.
pub fn decide_function(policy: Option<&FunctionPolicy>, name: &str, registered: bool) -> FunctionDecision {
    if is_builtin_function(name) {
        return FunctionDecision::Allow;
    }
    if !registered {
        return FunctionDecision::NotFound;
    }
    let empty = FunctionPolicy::default();
    let p = policy.unwrap_or(&empty);
    let any = |list: &Option<Vec<String>>| list.iter().flatten().any(|pattern| matches_pattern(pattern, name));
    if any(&p.deny) {
        return FunctionDecision::Deny;
    }
    if any(&p.allow) {
        return FunctionDecision::Allow;
    }
    if any(&p.ask) {
        return FunctionDecision::Ask;
    }
    p.default.unwrap_or(FunctionDecision::Allow)
}

/// Two policies stacked (a package's allowlist under the host's own): the
/// stricter decision wins.
pub fn combine_decisions(a: FunctionDecision, b: FunctionDecision) -> FunctionDecision {
    if a.rank() >= b.rank() {
        a
    } else {
        b
    }
}

/// A declarative package's `functions` list as a policy: listed = allow,
/// anything else = deny.
pub fn package_policy(functions: Option<&[String]>) -> FunctionPolicy {
    FunctionPolicy { allow: Some(functions.map(<[String]>::to_vec).unwrap_or_default()), ask: None, deny: None, default: Some(FunctionDecision::Deny) }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UrlPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schemes: Option<Vec<String>>,
    /// http(s) hosts allowed: exact or `*.example.com`; unset = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosts: Option<Vec<String>>,
    #[serde(default, rename = "baseUrl", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlDecision {
    pub allowed: bool,
    /// The absolute url when it parses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// `invalid` | `scheme` | `host`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `new URL(url)`, else `new URL(url, baseUrl)` (WHATWG both).
fn absolute(url: &str, base_url: Option<&str>) -> Option<Url> {
    if let Ok(u) = Url::parse(url) {
        return Some(u);
    }
    let base = Url::parse(base_url?).ok()?;
    base.join(url).ok()
}

fn host_matches(pattern: &str, host: &str) -> bool {
    let p = pattern.to_lowercase();
    match p.strip_prefix('*') {
        Some(suffix) if p.starts_with("*.") => host.ends_with(suffix) && host.len() > suffix.len(),
        _ => host == p,
    }
}

pub fn decide_url(policy: Option<&UrlPolicy>, url: &str) -> UrlDecision {
    let Some(parsed) = absolute(js_trim(url), policy.and_then(|p| p.base_url.as_deref())) else {
        return UrlDecision { allowed: false, url: None, reason: Some("invalid".into()) };
    };
    let scheme = parsed.scheme().to_lowercase();
    let href = parsed.as_str().to_string();
    let allowed_scheme = match policy.and_then(|p| p.schemes.as_ref()) {
        Some(list) => list.contains(&scheme),
        None => DEFAULT_URL_SCHEMES.contains(&scheme.as_str()),
    };
    if !allowed_scheme {
        return UrlDecision { allowed: false, url: Some(href), reason: Some("scheme".into()) };
    }
    if let Some(hosts) = policy.and_then(|p| p.hosts.as_ref()) {
        if scheme == "http" || scheme == "https" {
            let host = parsed.host_str().unwrap_or("").to_lowercase();
            if !hosts.iter().any(|h| host_matches(h, &host)) {
                return UrlDecision { allowed: false, url: Some(href), reason: Some("host".into()) };
            }
        }
    }
    UrlDecision { allowed: true, url: Some(href), reason: None }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaRule {
    pub prefix: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaOptions {
    #[serde(default, rename = "baseUrl", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<MediaRule>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRequest {
    pub url: String,
    /// Sorted by name.
    pub headers: BTreeMap<String, String>,
}

/// The image / media loader's request: the absolute url plus the headers of
/// every rule whose prefix it starts with (later rules win per header).
/// `None` when the url does not resolve.
pub fn media_request(url: &str, options: &MediaOptions) -> Option<MediaRequest> {
    let parsed = absolute(js_trim(url), options.base_url.as_deref())?;
    let href = parsed.as_str().to_string();
    let mut headers = BTreeMap::new();
    for rule in options.rules.iter().flatten() {
        if href.starts_with(&rule.prefix) {
            headers.extend(rule.headers.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }
    Some(MediaRequest { url: href, headers })
}
