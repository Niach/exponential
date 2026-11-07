//! VAPP-262 (contract E0 VAPP-260): consent. The sheet's answer, who may
//! write a grant (only the ConsentUi, only after a gesture), the Vault record
//! and its reconcile on every package load, and the two genUI surfaces (the
//! ask sheet, the Permissions sheet) made of existing components only.
//! Locked by `packages/eengine/fixtures/consent.json`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::broker::{scope_def, ConsentKind, GrantValue, Grants, ManifestEntry, VerdictKind};
use crate::contract::CONSENT;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsentAnswer {
    Allow,
    Once,
    Deny,
    Dismiss,
}

/// Who asks to change a grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GrantVia {
    ConsentUi,
    Agent,
    HubTool,
    Package,
    HostConfig,
    Sync,
}

/// The grants record's version (`recordVersion`).
pub fn grants_record_version() -> u64 {
    CONSENT["recordVersion"].as_u64().unwrap_or(1)
}

fn copy(key: &str) -> String {
    CONSENT["copy"][key].as_str().unwrap_or_default().to_string()
}

/// The sheet's answer → the call's verdict, its reason and the grants to store.
pub fn consent_answer(grants: &Grants, scope: &str, answer: ConsentAnswer) -> (VerdictKind, Option<&'static str>, Grants) {
    let mut next = grants.clone();
    match answer {
        ConsentAnswer::Allow => {
            next.insert(scope.to_string(), GrantValue::Granted);
            (VerdictKind::Call, None, next)
        }
        ConsentAnswer::Once => (VerdictKind::Call, None, next),
        ConsentAnswer::Deny => {
            next.insert(scope.to_string(), GrantValue::Denied);
            (VerdictKind::Denied, Some("consentDenied"), next)
        }
        ConsentAnswer::Dismiss => (VerdictKind::Denied, Some("consentDismissed"), next),
    }
}

/// Is `id` a declared `ask` scope of the manifest (one a grant is kept for)?
fn declared_ask(manifest: &[ManifestEntry], id: &str) -> bool {
    scope_def(id).is_some_and(|(_, s)| !s.host_only && s.consent == ConsentKind::Ask) && manifest.iter().any(|m| m.id == id)
}

/// A grant write: only the ConsentUi, only after a gesture, only an ask scope.
/// `value` `None` = revoke. Answers the grants and the refusal, if any.
pub fn grant_write(manifest: &[ManifestEntry], grants: &Grants, via: GrantVia, gesture: bool, scope: &str, value: Option<GrantValue>) -> (Grants, Option<&'static str>) {
    if via != GrantVia::ConsentUi {
        return (grants.clone(), Some("onlyConsentUi"));
    }
    if !gesture {
        return (grants.clone(), Some("noGesture"));
    }
    if !declared_ask(manifest, scope) {
        return (grants.clone(), Some("notAsk"));
    }
    let mut next = grants.clone();
    match value {
        Some(v) => {
            next.insert(scope.to_string(), v);
        }
        None => {
            next.remove(scope);
        }
    }
    (next, None)
}

fn http_origins(manifest: &[ManifestEntry]) -> Vec<String> {
    let mut origins = manifest.iter().find(|m| m.id == "http.fetch").and_then(|m| m.origins.clone()).unwrap_or_default();
    origins.sort();
    origins
}

/// The Vault key and canonical JSON value of an app's grants.
pub fn grants_record(app: &str, signer: &str, manifest: &[ManifestEntry], grants: &Grants) -> (String, String) {
    let key = CONSENT["vaultKey"].as_str().unwrap_or_default().replace("{app}", app);
    let value = json!({"grants": grants, "origins": http_origins(manifest), "signer": signer, "v": grants_record_version()});
    (key, eui_core::host::canonical_json(&value))
}

/// The grants that survive a package load (`stored` = the Vault value).
pub fn grants_on_package(stored: Option<&str>, signer: &str, manifest: &[ManifestEntry]) -> Grants {
    let Some(record) = stored.and_then(|s| serde_json::from_str::<Value>(s).ok()) else { return Grants::new() };
    if record.get("v").and_then(Value::as_u64) != Some(grants_record_version()) || record.get("signer").and_then(Value::as_str) != Some(signer) {
        return Grants::new();
    }
    let Some(stored) = record.get("grants").and_then(Value::as_object) else { return Grants::new() };
    let granted: Vec<&str> = record.get("origins").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let declared_origins = http_origins(manifest);
    let mut out = Grants::new();
    for (id, value) in stored {
        let value = match value.as_str() {
            Some("granted") => GrantValue::Granted,
            Some("denied") => GrantValue::Denied,
            _ => continue,
        };
        if !declared_ask(manifest, id) {
            continue;
        }
        if scope_def(id).is_some_and(|(_, s)| s.origins) && !declared_origins.iter().all(|o| granted.contains(&o.as_str())) {
            continue;
        }
        out.insert(id.clone(), value);
    }
    out
}

/// The ask sheet for one manifest entry: the surface's components and its data.
pub fn consent_sheet(app: &str, entry: &ManifestEntry) -> (Vec<Value>, Value) {
    let verb = scope_def(&entry.id).map_or(entry.id.clone(), |(_, s)| s.verb.clone());
    let verb = verb.replace("{origins}", &entry.origins.clone().unwrap_or_default().join(", "));
    let title = copy("title").replace("{app}", app).replace("{verb}", &verb);
    let components = vec![
        json!({"id": "root", "component": "Drawer", "title": title, "open": true, "side": "bottom", "children": ["why", "answer"], "slots": {"footer": "confirm"}, "on": {"change": {"event": {"name": "consent.answer", "context": {"answer": "dismiss"}}}}}),
        json!({"id": "why", "component": "Text", "text": entry.why}),
        json!({"id": "answer", "component": "ChoiceTiles", "name": "answer", "columns": 1, "value": {"@path": "/answer"}, "options": CONSENT["copy"]["options"].clone()}),
        json!({"id": "confirm", "component": "Button", "label": copy("confirm"), "on": {"press": {"event": {"name": "consent.answer", "context": {"answer": {"@path": "/answer"}}}}}}),
    ];
    (components, json!({"answer": "allow"}))
}

/// The Permissions sheet: one Switch per declared ask scope, in manifest order.
pub fn manage_sheet(manifest: &[ManifestEntry], grants: &Grants) -> Vec<Value> {
    let rows: Vec<&ManifestEntry> = manifest.iter().filter(|m| declared_ask(manifest, &m.id)).collect();
    let mut out = vec![json!({"id": "root", "component": "Drawer", "title": copy("manageTitle"), "open": true, "side": "bottom", "children": (0..rows.len()).map(|i| format!("scope_{i}")).collect::<Vec<_>>()})];
    for (i, m) in rows.iter().enumerate() {
        let label = scope_def(&m.id).map(|(c, _)| c.label.clone()).unwrap_or_default();
        out.push(json!({"id": format!("scope_{i}"), "component": "Switch", "label": label, "description": m.why, "checked": grants.get(&m.id) == Some(&GrantValue::Granted), "on": {"change": {"event": {"name": "consent.toggle", "context": {"scope": m.id}}}}}));
    }
    out
}
