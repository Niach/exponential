//! Step 1 (`prepareCases`): the package file and bundle.toml → what every
//! target bundles, or the first refusal.

use std::collections::BTreeMap;

use eengine::package::{package_of, validate, verify_package_file, Verification};
use eengine::runner::{runner_config, RunnerConfig};
use serde_json::{json, Map, Value};

use crate::contract::strings;

/// What every target bundles.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The package file as given: `app/package.app.json` byte for byte.
    pub file_bytes: Vec<u8>,
    pub package: Value,
    /// bundle.toml's runner keys = `app/runner.json`.
    pub config: RunnerConfig,
    /// The declared capability scopes, sorted, de-duplicated.
    pub scopes: Vec<String>,
    /// Each scope's `why` (the purpose strings).
    pub whys: BTreeMap<String, String>,
    /// bundle.toml's `[signing]` table (the author's keys), never written into the bundle.
    pub signing: Map<String, Value>,
    /// The verified author's name, when signed.
    pub author: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let why = match self.code {
            "notJson" => "the package file is not JSON",
            "signature" => "the package's signature does not verify",
            "unsigned" => "the package is unsigned (sign it, or pass --allow-unsigned)",
            "catalog" => "the package uses the Exponential-app catalog, which runs only inside Exponential",
            "invalid" => "genUI refuses the package",
            "capability" => "the package declares a capability scope an exported app cannot have",
            "config" => "bundle.toml has issues",
            _ => "refused",
        };
        if self.detail.is_empty() { write!(f, "{why}") } else { write!(f, "{why}: {}", self.detail) }
    }
}

impl std::error::Error for Refusal {}

fn refuse(code: &'static str, detail: impl Into<String>) -> Refusal {
    Refusal { code, detail: detail.into() }
}

/// The run's `runner.json` value: `{id, name, version, network}`.
pub fn config_value(config: &RunnerConfig) -> Value {
    serde_json::to_value(config).unwrap_or(Value::Null)
}

/// `prepare(file, bundleToml, options)`. `bundle_toml` is bundle.toml as JSON.
pub fn prepare(file_bytes: &[u8], bundle_toml: &Value, allow_unsigned: bool) -> Result<Prepared, Refusal> {
    let file: Value = serde_json::from_slice(file_bytes).map_err(|e| refuse("notJson", e.to_string()))?;
    let author = match verify_package_file(&file) {
        Verification::Valid { author } => Some(author.name),
        Verification::Invalid { issues } => return Err(refuse("signature", issues.iter().map(|i| format!("{} {}", i.path, i.message)).collect::<Vec<_>>().join("; "))),
        Verification::Unsigned if allow_unsigned => None,
        Verification::Unsigned => return Err(refuse("unsigned", "")),
    };
    let package = package_of(&file).clone();
    let catalog = package.get("catalogId").and_then(Value::as_str).unwrap_or_default();
    if strings("/refuse/catalogs").iter().any(|c| c == catalog) {
        return Err(refuse("catalog", catalog));
    }
    let mut scopes = Vec::new();
    let mut whys = BTreeMap::new();
    for entry in package.get("capabilities").and_then(Value::as_array).into_iter().flatten() {
        let id = entry.get("id").and_then(Value::as_str).unwrap_or_default();
        match eengine::broker::scope_def(id) {
            Some((_, scope)) if !scope.host_only => {}
            _ => return Err(refuse("capability", id)),
        }
        if let Some(why) = entry.get("why").and_then(Value::as_str).filter(|w| !w.trim().is_empty()) {
            whys.entry(id.to_string()).or_insert_with(|| why.to_string());
        }
        scopes.push(id.to_string());
    }
    let issues = validate(&package);
    if !issues.is_empty() {
        return Err(refuse("invalid", issues.iter().map(|i| format!("{} {}", i.path, i.message)).collect::<Vec<_>>().join("; ")));
    }
    scopes.sort();
    scopes.dedup();
    let (config, signing) = read_config(bundle_toml)?;
    Ok(Prepared { file_bytes: file_bytes.to_vec(), package, config, scopes, whys, signing, author })
}

/// bundle.toml → the runner config (runner.json's `runnerConfig`) + the `[signing]` table.
fn read_config(bundle_toml: &Value) -> Result<(RunnerConfig, Map<String, Value>), Refusal> {
    let Some(table) = bundle_toml.as_object() else { return Err(refuse("config", ":notAnObject")) };
    let sections = strings("/config/sections");
    let runner_keys: Map<String, Value> = table.iter().filter(|(k, _)| !sections.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
    let (config, issues) = runner_config(&Value::Object(runner_keys));
    if !issues.is_empty() {
        return Err(refuse("config", issues.iter().map(|i| format!("{}:{}", i.key, i.code)).collect::<Vec<_>>().join(",")));
    }
    let signing = table.get("signing").and_then(Value::as_object).cloned().unwrap_or_default();
    Ok((config.expect("no issues = a config"), signing))
}

/// The fixture's view of a run: `{ok: {config, scopes}}` or `{refused: {code, detail?}}`.
pub fn prepare_value(file_bytes: &[u8], bundle_toml: &Value, allow_unsigned: bool) -> Value {
    match prepare(file_bytes, bundle_toml, allow_unsigned) {
        Ok(p) => json!({"ok": {"config": config_value(&p.config), "scopes": p.scopes}}),
        Err(r) => json!({"refused": {"code": r.code, "detail": r.detail}}),
    }
}

/// `BUNDLE.targets`.
pub fn targets() -> Vec<String> {
    strings("/targets")
}
