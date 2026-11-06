//! Step 4 (`permissions`, `permissionCases`): the declared scopes → each
//! platform's manifest pieces. Only what the scopes ask for, never more.

use std::collections::{BTreeMap, BTreeSet};

use eengine::providers::NetworkPolicy;
use eengine::runner::RunnerConfig;
use serde_json::{json, Map, Value};

use crate::contract::{strings, BUNDLE};

fn per_scope(pointer: &str, scopes: &[String]) -> Vec<String> {
    scopes.iter().flat_map(|s| strings(&format!("{pointer}/{s}"))).collect()
}

fn networked(config: &RunnerConfig) -> bool {
    config.network != NetworkPolicy::Off
}

/// Android: the `<uses-permission>` lines between the template's `E7:PERMISSIONS` markers.
pub fn android_permissions(scopes: &[String], config: &RunnerConfig) -> Vec<String> {
    let mut lines = per_scope("/permissions/android/scopes", scopes);
    if networked(config) {
        lines.extend(strings("/permissions/android/network"));
    }
    let mut seen = BTreeSet::new();
    lines.retain(|l| seen.insert(l.clone()));
    lines
}

/// iOS: the capability modules to link (sorted, unique).
pub fn ios_modules(scopes: &[String]) -> Vec<String> {
    let modules = &BUNDLE["permissions"]["ios"]["modules"];
    scopes.iter().filter_map(|s| modules.get(s).and_then(Value::as_str)).map(String::from).collect::<BTreeSet<_>>().into_iter().collect()
}

/// iOS: the Info.plist purpose strings, each the scope's `why`, else the fallback.
pub fn ios_purposes(scopes: &[String], config: &RunnerConfig, whys: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let ios = &BUNDLE["permissions"]["ios"];
    let mut out = BTreeMap::new();
    for s in scopes {
        let Some(key) = ios["purposes"].get(s).and_then(Value::as_str) else { continue };
        let text = whys.get(s).cloned().unwrap_or_else(|| ios["purposeFallback"][key].as_str().unwrap_or_default().replace("{name}", &config.name));
        out.insert(key.to_string(), text);
    }
    out
}

/// macOS: the sandbox entitlements (sorted, unique), each `true`.
pub fn macos_entitlements(scopes: &[String], config: &RunnerConfig) -> Vec<String> {
    let mut all: BTreeSet<String> = strings("/permissions/macos/base").into_iter().collect();
    all.extend(per_scope("/permissions/macos/scopes", scopes));
    if networked(config) {
        all.extend(strings("/permissions/macos/network"));
    }
    all.into_iter().collect()
}

/// The macOS entitlements as a plist.
pub fn macos_entitlements_plist(entitlements: &[String]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n");
    for e in entitlements {
        out.push_str(&format!("\t<key>{}</key>\n\t<true/>\n", xml_escape(e)));
    }
    out.push_str("</dict>\n</plist>\n");
    out
}

/// The URL scheme of deep links: the config id (runner.json).
pub fn scheme(config: &RunnerConfig) -> &str {
    &config.id
}

/// Linux: the `.desktop` entry; the installed binary is named after the config id.
pub fn linux_desktop_entry(config: &RunnerConfig) -> String {
    let linux = &BUNDLE["permissions"]["linux"];
    let exec = linux["exec"].as_str().unwrap_or("{bin} %u").replace("{bin}", &config.id);
    let mime = linux["mimeType"].as_str().unwrap_or("x-scheme-handler/{scheme};").replace("{scheme}", scheme(config));
    let categories = linux["categories"].as_str().unwrap_or("Utility;");
    format!("[Desktop Entry]\nType=Application\nName={}\nExec={exec}\nIcon={}\nTerminal=false\nCategories={categories}\nMimeType={mime}\n", config.name, config.id)
}

/// Windows: the registry key the installer writes for the URL protocol.
pub fn windows_protocol_key(config: &RunnerConfig) -> String {
    BUNDLE["permissions"]["windows"]["protocolKey"].as_str().unwrap_or_default().replace("{scheme}", scheme(config))
}

/// The web: `manifest.webmanifest`.
pub fn web_manifest(config: &RunnerConfig) -> Value {
    let web = &BUNDLE["permissions"]["web"];
    let icons: Vec<Value> = BUNDLE["icons"]["web"].as_array().into_iter().flatten().filter_map(Value::as_u64).map(|n| json!({"src": format!("icons/icon-{n}.png"), "sizes": format!("{n}x{n}"), "type": "image/png", "purpose": "any"})).collect();
    json!({
        "id": "./",
        "name": config.name,
        "short_name": config.name,
        "start_url": web["startUrl"],
        "scope": web["scope"],
        "display": web["display"],
        "background_color": web["color"],
        "theme_color": web["color"],
        "icons": icons,
    })
}

/// `platformManifest(scopes, config, whys)`: every target's pieces, the fixture's shape.
pub fn platform_manifest(scopes: &[String], config: &RunnerConfig, whys: &BTreeMap<String, String>) -> Value {
    let purposes: Map<String, Value> = ios_purposes(scopes, config, whys).into_iter().map(|(k, v)| (k, Value::String(v))).collect();
    json!({
        "android": {"permissions": android_permissions(scopes, config)},
        "ios": {"modules": ios_modules(scopes), "purposes": purposes},
        "macos": {"entitlements": macos_entitlements(scopes, config), "scheme": scheme(config)},
        "linux": {"desktopEntry": linux_desktop_entry(config)},
        "windows": {"protocolKey": windows_protocol_key(config)},
        "web": {"manifest": web_manifest(config)},
    })
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}
