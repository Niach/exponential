//! The host router. Server messages in, ops out (`catalog/host.json` ops);
//! the platform host performs them on its surfaces. Pure and synchronous, so
//! `fixtures/host-router.json` locks the same behaviour on every platform.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::contract::{error_message, INVALID_MESSAGE, MESSAGE_KINDS, SURFACE_NOT_FOUND, TEMPLATE_NOT_FOUND, UNSUPPORTED_CATALOG};
use super::package::{supported_catalog_ids, template_messages, validate_package, PackageIssue};
use super::sources::parse_source;
use crate::catalog::A2UI_VERSION;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInfo {
    pub surface_id: String,
    pub catalog_id: String,
    /// The package whose template created it (its function policy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
}

/// One router per connection: the surfaces it created, the packages
/// installed, the extension catalogs negotiated.
#[derive(Debug, Clone, Default)]
pub struct HostRouter {
    extension_ids: Vec<String>,
    surfaces: IndexMap<String, SurfaceInfo>,
    packages: IndexMap<String, Value>,
}

fn send(message: Value) -> Value {
    json!({"op": "send", "message": message})
}

fn invalid(surface_id: &str, message: &str) -> Value {
    send(error_message(INVALID_MESSAGE, surface_id, message, None))
}

fn missing(surface_id: &str) -> Value {
    send(error_message(SURFACE_NOT_FOUND, surface_id, &format!("no surface {surface_id}; send createSurface first"), None))
}

fn surface_id_of(message: &Map<String, Value>) -> String {
    message.values().find_map(|v| v.as_object().and_then(|o| o.get("surfaceId")).and_then(Value::as_str)).unwrap_or("").to_string()
}

/// A2UI: an omitted path or `/` is the whole data model (`""` in JSON pointer
/// terms, which every surface's setPointer takes).
pub fn normalize_path(path: Option<&str>) -> String {
    match path {
        None | Some("/") => String::new(),
        Some(p) => p.to_string(),
    }
}

impl HostRouter {
    /// `extension_ids` = the extension catalog ids the host registered.
    pub fn new<S: AsRef<str>>(extension_ids: &[S]) -> Self {
        HostRouter { extension_ids: extension_ids.iter().map(|s| s.as_ref().to_string()).collect(), ..Default::default() }
    }

    pub fn supported_catalog_ids(&self) -> Vec<String> {
        supported_catalog_ids(&self.extension_ids)
    }

    pub fn register_extension(&mut self, id: &str) {
        if !self.extension_ids.iter().any(|x| x == id) {
            self.extension_ids.push(id.to_string());
        }
    }

    /// Install a declarative package: its templates become `applyTemplate`
    /// targets. Returns the validation issues (installed only when none).
    pub fn install_package(&mut self, pkg: &Value) -> Vec<PackageIssue> {
        let issues = validate_package(pkg, &self.supported_catalog_ids());
        if issues.is_empty() {
            let id = pkg["id"].as_str().unwrap_or("").to_string();
            self.packages.insert(id, pkg.clone());
        }
        issues
    }

    pub fn package(&self, id: &str) -> Option<&Value> {
        self.packages.get(id)
    }

    pub fn surface(&self, id: &str) -> Option<&SurfaceInfo> {
        self.surfaces.get(id)
    }

    /// In creation order.
    pub fn surface_ids(&self) -> Vec<String> {
        self.surfaces.keys().cloned().collect()
    }

    /// The package whose template created the surface.
    pub fn package_id_of(&self, surface_id: &str) -> Option<String> {
        self.surfaces.get(surface_id).and_then(|s| s.package_id.clone())
    }

    /// One server message → the ops to perform, in order. Never fails.
    pub fn route(&mut self, message: &Value) -> Vec<Value> {
        let Some(message) = message.as_object() else { return vec![invalid("", "a message is a JSON object")] };
        if let Some(version) = message.get("version") {
            if version.as_str() != Some(A2UI_VERSION) {
                return vec![invalid(&surface_id_of(message), &format!("unsupported version {}", crate::json::to_js_string(version)))];
            }
        }
        let kinds: Vec<&String> = message.keys().filter(|k| *k != "version").collect();
        if kinds.len() != 1 || !MESSAGE_KINDS.contains(&kinds[0].as_str()) {
            return vec![invalid(&surface_id_of(message), &format!("expected exactly one of {}", MESSAGE_KINDS.join(", ")))];
        }
        let kind = kinds[0].as_str();
        let Some(body) = message[kind].as_object().filter(|b| b.get("surfaceId").and_then(Value::as_str).is_some_and(|s| !s.is_empty())) else {
            return vec![invalid("", &format!("{kind}.surfaceId is required"))];
        };
        let surface_id = body["surfaceId"].as_str().unwrap_or("").to_string();
        let sid = surface_id.as_str();
        match kind {
            "createSurface" => {
                let Some(catalog_id) = body.get("catalogId").and_then(Value::as_str) else {
                    return vec![invalid(sid, "createSurface.catalogId is required")];
                };
                if !self.supported_catalog_ids().iter().any(|c| c == catalog_id) {
                    return vec![send(error_message(UNSUPPORTED_CATALOG, sid, &format!("catalog {catalog_id} is not supported"), None))];
                }
                self.surfaces.insert(surface_id.clone(), SurfaceInfo { surface_id: surface_id.clone(), catalog_id: catalog_id.to_string(), package_id: None });
                let mut op = Map::new();
                op.insert("op".into(), json!("create"));
                op.insert("surfaceId".into(), json!(sid));
                op.insert("catalogId".into(), json!(catalog_id));
                if let Some(theme) = body.get("theme") {
                    op.insert("theme".into(), theme.clone());
                }
                if body.get("sendDataModel") == Some(&Value::Bool(true)) {
                    op.insert("sendDataModel".into(), Value::Bool(true));
                }
                vec![Value::Object(op)]
            }
            "updateComponents" => {
                let Some(components) = body.get("components").filter(|c| c.is_array()) else {
                    return vec![invalid(sid, "updateComponents.components is required")];
                };
                if !self.surfaces.contains_key(sid) {
                    return vec![missing(sid)];
                }
                vec![json!({"op": "components", "surfaceId": sid, "components": components})]
            }
            "updateDataModel" => {
                if !self.surfaces.contains_key(sid) {
                    return vec![missing(sid)];
                }
                let path = match body.get("path") {
                    None => None,
                    Some(Value::String(p)) => Some(p.as_str()),
                    Some(_) => return vec![invalid(sid, "updateDataModel.path is a string")],
                };
                let path = normalize_path(path);
                match body.get("value") {
                    Some(value) => vec![json!({"op": "data", "surfaceId": sid, "path": path, "value": value})],
                    None => vec![json!({"op": "data", "surfaceId": sid, "path": path})],
                }
            }
            "deleteSurface" => {
                if self.surfaces.shift_remove(sid).is_none() {
                    return vec![missing(sid)];
                }
                vec![json!({"op": "delete", "surfaceId": sid})]
            }
            "bindDataModel" => {
                if !self.surfaces.contains_key(sid) {
                    return vec![missing(sid)];
                }
                match (body.get("path").and_then(Value::as_str), body.get("source").and_then(Value::as_str)) {
                    (Some(path), Some(source)) if parse_source(source).is_some() => {
                        vec![json!({"op": "bind", "surfaceId": sid, "path": normalize_path(Some(path)), "source": source})]
                    }
                    _ => vec![invalid(sid, "bindDataModel needs a path and a scheme:name source")],
                }
            }
            "applyTemplate" => {
                let Some(template_id) = body.get("templateId").and_then(Value::as_str) else {
                    return vec![invalid(sid, "applyTemplate.templateId is required")];
                };
                let Some(pkg) = self.find_package(template_id, body.get("packageId").and_then(Value::as_str)).cloned() else {
                    return vec![send(error_message(TEMPLATE_NOT_FOUND, sid, &format!("no installed package has the template {template_id}"), None))];
                };
                let mut ops = Vec::new();
                for m in template_messages(&pkg, template_id, sid, body.get("data")).unwrap_or_default() {
                    ops.extend(self.route(&m));
                }
                if let Some(info) = self.surfaces.get_mut(sid) {
                    info.package_id = pkg.get("id").and_then(Value::as_str).map(str::to_string);
                }
                ops
            }
            _ => vec![invalid(sid, "unknown message")],
        }
    }

    fn find_package(&self, template_id: &str, package_id: Option<&str>) -> Option<&Value> {
        let has = |pkg: &Value| pkg.get("templates").and_then(|t| t.get(template_id)).is_some_and(super::js_truthy);
        match package_id {
            Some(id) => self.packages.get(id).filter(|p| has(p)),
            None => self.packages.values().find(|p| has(p)),
        }
    }
}
