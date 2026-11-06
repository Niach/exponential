//! VAPP-260 (format version 5): a package's CAPABILITY MANIFEST, the mirror
//! of `packages/exponential-ui/src/host/capabilities.ts`;
//! `fixtures/host-router.json` (`acme.capabilities`, `acme.badcapabilities`)
//! locks both. The catalog's tools and consent are the Engine's
//! (`packages/eengine/fixtures/capabilities.json`).

use serde_json::{Map, Value};

use super::package::PackageIssue;

pub const MAX_CAPABILITIES: usize = 16;
pub const MAX_CAPABILITY_WHY: usize = 200;
/// The scope ids a manifest may name (`catalog/host.json` `package.capabilities.scopes`).
pub const CAPABILITY_SCOPES: &[&str] = &[
    "camera.capture",
    "clipboard.read",
    "clipboard.write",
    "files.read",
    "files.write",
    "http.fetch",
    "location.read",
    "notifications.post",
    "photos.read",
    "print.pdf",
    "share.send",
];
/// The scopes whose entry lists its `origins` (and only these may).
pub const CAPABILITY_ORIGIN_SCOPES: &[&str] = &["http.fetch"];
/// `catalog/host.json` `package.capabilities.originPattern`, checked by hand in [`capability_origin_valid`].
pub const CAPABILITY_ORIGIN_PATTERN: &str = "^https://[a-z0-9]([a-z0-9-]*[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*(:[0-9]{1,5})?$";
const ENTRY_KEYS: &[&str] = &["id", "why", "origins"];

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn label_valid(label: &str) -> bool {
    let bytes = label.as_bytes();
    !bytes.is_empty()
        && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
}

/// Is `origin` an `https://host[:port]` origin ([`CAPABILITY_ORIGIN_PATTERN`])?
pub fn capability_origin_valid(origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("https://") else { return false };
    let (host, port) = match rest.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (rest, None),
    };
    if let Some(port) = port {
        if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    host.split('.').all(label_valid)
}

fn show(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(other) => other.to_string(),
    }
}

/// Every problem with a package's `capabilities` (unset = none).
pub fn validate_package_capabilities(p: &Map<String, Value>) -> Vec<PackageIssue> {
    let Some(value) = p.get("capabilities") else { return Vec::new() };
    let Some(list) = value.as_array() else { return vec![issue("/capabilities", "capabilities is a list of {id, why, origins?}")] };
    let mut issues = Vec::new();
    if list.len() > MAX_CAPABILITIES {
        issues.push(issue("/capabilities", format!("at most {MAX_CAPABILITIES} capabilities")));
    }
    let mut seen: Vec<&str> = Vec::new();
    for (i, entry) in list.iter().enumerate() {
        let at = format!("/capabilities/{i}");
        let Some(obj) = entry.as_object() else {
            issues.push(issue(at, "a capability is {id, why, origins?}"));
            continue;
        };
        let mut keys: Vec<&String> = obj.keys().collect();
        keys.sort();
        for key in keys {
            if !ENTRY_KEYS.contains(&key.as_str()) {
                issues.push(issue(format!("{at}/{key}"), format!("unknown key {key}")));
            }
        }
        let id = obj.get("id").and_then(Value::as_str);
        match id {
            Some(id) if CAPABILITY_SCOPES.contains(&id) => {
                if seen.contains(&id) {
                    issues.push(issue(format!("{at}/id"), format!("duplicate capability {id}")));
                } else {
                    seen.push(id);
                }
            }
            Some(id) => issues.push(issue(format!("{at}/id"), format!("unknown capability {id}"))),
            None => issues.push(issue(format!("{at}/id"), format!("unknown capability {}", show(obj.get("id"))))),
        }
        match obj.get("why").and_then(Value::as_str) {
            Some(why) if !why.trim().is_empty() => {
                if why.chars().count() > MAX_CAPABILITY_WHY {
                    issues.push(issue(format!("{at}/why"), format!("why is at most {MAX_CAPABILITY_WHY} characters")));
                }
            }
            _ => issues.push(issue(format!("{at}/why"), "why is a required string")),
        }
        let wants_origins = id.is_some_and(|id| CAPABILITY_ORIGIN_SCOPES.contains(&id));
        match obj.get("origins") {
            None => {
                if wants_origins {
                    issues.push(issue(format!("{at}/origins"), format!("{} lists its origins", id.unwrap_or_default())));
                }
            }
            Some(_) if !wants_origins => issues.push(issue(format!("{at}/origins"), format!("origins are only for {}", CAPABILITY_ORIGIN_SCOPES.join(", ")))),
            Some(Value::Array(origins)) if !origins.is_empty() => {
                let mut distinct: Vec<&str> = Vec::new();
                for (j, origin) in origins.iter().enumerate() {
                    match origin.as_str() {
                        Some(o) if capability_origin_valid(o) => {
                            if distinct.contains(&o) {
                                issues.push(issue(format!("{at}/origins/{j}"), format!("duplicate origin {o}")));
                            } else {
                                distinct.push(o);
                            }
                        }
                        _ => issues.push(issue(format!("{at}/origins/{j}"), "an origin is https://host[:port]")),
                    }
                }
            }
            Some(_) => issues.push(issue(format!("{at}/origins"), "origins is a non-empty list")),
        }
    }
    issues
}
