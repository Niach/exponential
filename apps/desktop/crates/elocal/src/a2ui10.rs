//! Round 9 §14.1 (#VAPP-289): a Local desktop's own one-shot A2UI 1.0 pass.
//! The server's migration never sees this store, and format 5 stops loading
//! here too, so opening a store rewrites what it holds with `upgrade09` (the
//! Rust mirror): `apps.package`, `apps.draft_package` (→ format 6),
//! `apps.granted_tools` (flat names) and every clear `messages.card` surface
//! (→ `version: "1.0"`). Sealed cards are left to the display reader.
//! Idempotent; the marker file next to the store skips it from then on. This
//! and the migration script are the only callers of `upgrade09`.

use std::path::Path;

use eui_core::catalog::A2UI_VERSION;
use eui_core::host::{PACKAGE_FORMAT_VERSION, PACKAGE_FORMAT_VERSION_09};
use eui_core::upgrade09::{upgrade_card09, upgrade_package_body09};
use serde_json::{json, Map, Value};

use crate::store::LocalStore;

/// The marker's file name, beside `local.sqlite`.
pub const MARKER: &str = "a2ui10.done";

fn format_is(pkg: &Value, version: u32) -> bool {
    pkg.get("formatVersion").and_then(Value::as_u64) == Some(u64::from(version))
}

/// A stored package at format 6: `Some` = rewritten.
pub fn upgrade_package(pkg: &Value) -> Option<Value> {
    if !format_is(pkg, PACKAGE_FORMAT_VERSION_09) {
        return None;
    }
    let upgraded = upgrade_package_body09(pkg);
    format_is(&upgraded, PACKAGE_FORMAT_VERSION).then_some(upgraded)
}

/// `apps.granted_tools` under flat names, order kept: `Some` = rewritten.
pub fn upgrade_grants(grants: &Value) -> Option<Value> {
    let names: Vec<&str> = grants.as_array()?.iter().map(Value::as_str).collect::<Option<_>>()?;
    let mut flat: Vec<String> = Vec::with_capacity(names.len());
    for name in &names {
        let name = if name.starts_with("exponential.") { name.replace('.', "_") } else { name.to_string() };
        if !flat.contains(&name) {
            flat.push(name);
        }
    }
    (flat.len() != names.len() || flat.iter().zip(&names).any(|(a, b)| a != b)).then(|| json!(flat))
}

/// A `messages.card` with its clear surface at 1.0: `Some` = rewritten.
pub fn upgrade_card(card: &Value) -> Option<Value> {
    let object = card.as_object()?;
    if object.contains_key("sealed") || object.contains_key("sealedCard") {
        return None;
    }
    let surface = object.get("surface")?;
    if !surface.is_object() || surface.get("version").is_some() {
        return None;
    }
    let mut next = object.clone();
    next.insert("surface".into(), upgrade_card09(surface));
    Some(Value::Object(next))
}

fn upgraded_app(row: &Map<String, Value>) -> Option<Map<String, Value>> {
    let mut next = row.clone();
    let mut changed = false;
    for column in ["package", "draft_package"] {
        if let Some(pkg) = row.get(column).and_then(upgrade_package) {
            next.insert(column.into(), pkg);
            changed = true;
        }
    }
    if let Some(grants) = row.get("granted_tools").and_then(upgrade_grants) {
        next.insert("granted_tools".into(), grants);
        changed = true;
    }
    changed.then_some(next)
}

fn upgraded_message(row: &Map<String, Value>) -> Option<Map<String, Value>> {
    let card = row.get("card").and_then(upgrade_card)?;
    let mut next = row.clone();
    next.insert("card".into(), card);
    Some(next)
}

/// The pass over the store: how many rows it rewrote.
pub fn upgrade_store(store: &LocalStore) -> Result<usize, String> {
    let apps: Vec<_> = store.rows("apps")?.iter().filter_map(upgraded_app).collect();
    let messages: Vec<_> = store.rows("messages")?.iter().filter_map(upgraded_message).collect();
    let rewritten = apps.len() + messages.len();
    if !apps.is_empty() {
        store.write_many("apps", apps)?;
    }
    if !messages.is_empty() {
        store.write_many("messages", messages)?;
    }
    Ok(rewritten)
}

/// [`upgrade_store`] once per store (`dir` holds the marker). A store that
/// fails stays as it is and is tried again on the next open.
pub fn upgrade_once(store: &LocalStore, dir: &Path) -> Result<(), String> {
    let marker = dir.join(MARKER);
    if marker.exists() {
        return Ok(());
    }
    let rewritten = upgrade_store(store)?;
    if rewritten > 0 {
        log::info!("[elocal] A2UI 1.0: rewrote {rewritten} rows of {}", dir.display());
    }
    std::fs::write(&marker, A2UI_VERSION).map_err(|e| e.to_string())
}
