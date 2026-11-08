//! The host router. Agent messages in, ops out (`catalog/host.json` ops);
//! the platform host performs them on its surfaces. Pure and synchronous, so
//! `fixtures/host-router.json` locks the same behaviour on every platform.
//!
//! Round 9 (A2UI 1.0, `docs/round-9-contract.md` §1–§2): the check order of
//! §1.1 (size, object, version through [`check_envelope`] with the 0.9
//! bridge, one known kind, host-internal kinds only TRUSTED, closed bodies,
//! the kind's own checks), lifetime-unique surface ids (INTEGRITY_ERROR,
//! [`HostRouter::mint_surface_id`]), `createSurface{components, dataModel,
//! metadata}`, the per-entry check (§5.2: a malformed entry is dropped with
//! ONE INVALID_MESSAGE, the rest applies), `callRendererFunction` (§6.9) and
//! `agentFunctionResponse`.

use std::collections::HashSet;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::contract::{
    error_message, error_message_to, renderer_function_response, ErrorTarget, EXECUTION_ERROR, HOST_MESSAGE_KINDS, INTEGRITY_ERROR, INVALID_FUNCTION_CALL, INVALID_MESSAGE,
    MESSAGE_KINDS, SOURCE_DENIED, SURFACE_NOT_FOUND, TEMPLATE_NOT_FOUND, UNSUPPORTED_CATALOG,
};
use super::package::{package_sources, source_allowed, supported_catalog_ids, template_messages, validate_package_with, AvailableExtension, PackageIssue};
use super::sources::parse_source;
use crate::catalog::{ACCEPT_V09, A2UI_VERSION, A2UI_VERSION_09};
use crate::limits;
use crate::upgrade09::Upgrade09State;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInfo {
    pub surface_id: String,
    /// The surface's default catalog (`""` = none, §5.5).
    pub catalog_id: String,
    /// The package whose template created it (its function policy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
    /// §6.5: created by an UNTRUSTED `createSurface` (an agent's): its
    /// unknown functions may route to the agent when the host has a channel.
    #[serde(default)]
    pub agent: bool,
}

/// `route`'s options (§1.1): `trusted` = host code (applyTemplate,
/// bindDataModel allowed); `accept_v09` = run the 0.9 bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteOptions {
    pub trusted: bool,
    pub accept_v09: bool,
}

impl Default for RouteOptions {
    fn default() -> Self {
        RouteOptions { trusted: false, accept_v09: ACCEPT_V09 }
    }
}

impl RouteOptions {
    /// Host code's route.
    pub fn trusted() -> Self {
        RouteOptions { trusted: true, ..Default::default() }
    }
}

/// One router per renderer lifetime: the surfaces it created (and every id
/// it EVER created), the packages installed, the extension catalogs
/// negotiated.
#[derive(Debug, Clone, Default)]
pub struct HostRouter {
    extension_ids: Vec<String>,
    /// VAPP-161: their versions and components (a package's `extensions`
    /// are checked against them; `None` = only that the id is registered).
    extensions: Option<Vec<AvailableExtension>>,
    /// §10: inline catalog ids (they join the available set).
    inline_catalog_ids: Vec<String>,
    surfaces: IndexMap<String, SurfaceInfo>,
    /// §1.4: every surface id ever created by this router.
    seen: HashSet<String>,
    /// §1.4: the mint counter (starts at 1).
    mint_next: u64,
    packages: IndexMap<String, Value>,
    /// §14: the 0.9 bridge's id maps, threaded across the stream.
    upgrade_state: Upgrade09State,
}

fn send(message: Value) -> Value {
    json!({"op": "send", "message": message})
}

fn invalid(surface_id: &str, message: &str) -> Value {
    send(error_message(INVALID_MESSAGE, surface_id, message, None))
}

fn invalid_at(surface_id: &str, message: &str, path: &str) -> Value {
    send(error_message(INVALID_MESSAGE, surface_id, message, Some(path)))
}

fn missing(surface_id: &str) -> Value {
    send(error_message(SURFACE_NOT_FOUND, surface_id, &format!("no surface {surface_id}; send createSurface first"), None))
}

/// Is the message's JSON past `maxMessageBytes`? (Counted while
/// serializing; the count stops at the limit.)
fn oversized(message: &Value) -> bool {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            if self.0 > limits::MAX_MESSAGE_BYTES {
                return Err(std::io::Error::other("over the limit"));
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, message).is_err()
}

/// §1.1 `surfaceIdOf`: the first string `surfaceId` among the message's
/// object values, else `""`.
pub fn surface_id_of(message: &Map<String, Value>) -> String {
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

/// What §1.1 steps 3–4 decide about a message's `version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Envelope {
    /// `"1.0"`: go on.
    V10,
    /// 0.9 (absent or `"v0.9"`) and the bridge is on: upgrade, then go on.
    Upgrade,
    /// Refused: INVALID_MESSAGE with this message.
    Refused(String),
}

/// Round 9 §1.5: the ONE version check every core entry runs (the router,
/// `Surface::apply`, cards, packages).
pub fn check_envelope(message: &Map<String, Value>, accept_v09: bool) -> Envelope {
    match message.get("version") {
        None if accept_v09 => Envelope::Upgrade,
        None => Envelope::Refused("version is required".into()),
        Some(Value::String(v)) if v == A2UI_VERSION_09 => {
            if accept_v09 {
                Envelope::Upgrade
            } else {
                Envelope::Refused(format!("unsupported version {A2UI_VERSION_09}"))
            }
        }
        Some(Value::String(v)) if v == A2UI_VERSION => Envelope::V10,
        Some(Value::String(v)) => Envelope::Refused(format!("unsupported version {v}")),
        Some(other) => Envelope::Refused(format!("unsupported version {}", serde_json::to_string(other).unwrap_or_default())),
    }
}

/// The body keys each kind allows (§1.2; the host kinds also carry the
/// internal `hostParams`).
fn allowed_keys(kind: &str) -> &'static [&'static str] {
    match kind {
        "createSurface" => &["surfaceId", "catalogId", "components", "dataModel", "sendDataModel", "metadata"],
        "updateComponents" => &["surfaceId", "components"],
        "updateDataModel" => &["surfaceId", "path", "value"],
        "deleteSurface" => &["surfaceId"],
        "callRendererFunction" => &["surfaceId", "functionCallId", "callFunction"],
        "agentFunctionResponse" => &["functionCallId", "value", "error"],
        "applyTemplate" => &["surfaceId", "templateId", "packageId", "data", "hostParams"],
        "bindDataModel" => &["surfaceId", "path", "source", "hostParams"],
        _ => &[],
    }
}

/// JSON Pointer token escape.
fn esc(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn quoted(v: &str) -> String {
    Value::String(v.to_string()).to_string()
}

fn not_id(what: &str, v: &str) -> String {
    format!("{what}{} is not an identifier (UAX #31)", quoted(v))
}

/// §5.1: a `metadata` object (`{extensions?: {<identifier>: any}}`); the
/// first problem as `(pointer below metadata, message)`.
pub fn check_metadata(metadata: &Value) -> Option<(String, String)> {
    let Some(m) = metadata.as_object() else { return Some((String::new(), "metadata is an object".into())) };
    if let Some(k) = m.keys().find(|k| *k != "extensions") {
        return Some((format!("/{}", esc(k)), format!("metadata.{k} is not allowed")));
    }
    match m.get("extensions") {
        None => None,
        Some(Value::Object(ext)) => ext.keys().find(|k| !crate::ident::is_identifier(k)).map(|k| (format!("/extensions/{}", esc(k)), not_id("extension ", k))),
        Some(_) => Some(("/extensions".into(), "metadata.extensions is an object".into())),
    }
}

/// An Action (`on.<event>`, basic `action`): its event context and
/// userMessage are dynamic, its functionCall is a call.
fn check_action(value: &Value, at: &str) -> Option<(String, String)> {
    let dynamic = |v: &Value, at: &str| crate::expr::malformed_directive(v).map(|(below, message)| (format!("{at}{below}"), message));
    let Some(action) = value.as_object() else { return dynamic(value, at) };
    for (k, v) in action {
        let here = format!("{at}/{}", esc(k));
        let hit = if k == "functionCall" {
            match v.as_object().filter(|o| o.contains_key("@call")) {
                None => Some((here, "functionCall is a call ({\"@call\": …})".to_string())),
                Some(_) => dynamic(v, &here),
            }
        } else if let ("event", Value::Object(event)) = (k.as_str(), v) {
            event.iter().filter(|(ek, _)| *ek == "context" || *ek == "userMessage").find_map(|(ek, ev)| dynamic(ev, &format!("{here}/{ek}")))
        } else {
            dynamic(v, &here)
        };
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// The basic catalog's component-id fields (§14.3): `child`,
/// `Tabs.tabs[].child`, `Modal.trigger`/`content`, as `(pointer, value)`.
fn basic_id_refs<'a>(component: &str, entry: &'a Map<String, Value>) -> Vec<(String, &'a Value)> {
    let mut out = Vec::new();
    if let Some(v) = entry.get("child") {
        out.push(("/child".to_string(), v));
    }
    if component == "Modal" {
        for k in ["trigger", "content"] {
            if let Some(v) = entry.get(k) {
                out.push((format!("/{k}"), v));
            }
        }
    }
    if component == "Tabs" {
        for (i, t) in entry.get("tabs").and_then(Value::as_array).into_iter().flatten().enumerate() {
            if let Some(v) = t.as_object().and_then(|t| t.get("child")) {
                out.push((format!("/tabs/{i}/child"), v));
            }
        }
    }
    out
}

/// Round 9 §5.2 `check_entry`: why a component entry is refused, as
/// `(pointer below the entry, message)`; `None` = it passes. `catalog_id` =
/// the surface default (the entry's own wins), `view` = the catalogs that
/// decide its DATA positions and `componentId` props (basic = none). Check
/// order: the entry is an object, `/id`, `/component`, `/catalogId`,
/// `/metadata…`, then the other keys in key order. `src/entry.ts checkEntry`.
pub fn check_entry(entry: &Value, catalog_id: Option<&str>, view: &crate::catalog::CatalogView) -> Option<(String, String)> {
    let Some(o) = entry.as_object() else { return Some((String::new(), "a component is a JSON object".into())) };
    let Some(id_value) = o.get("id") else { return Some(("/id".into(), "id is required".into())) };
    let Some(id) = id_value.as_str() else { return Some(("/id".into(), "id is a string".into())) };
    if !crate::ident::is_identifier(id) {
        return Some(("/id".into(), not_id("id ", id)));
    }
    let Some(component_value) = o.get("component") else { return Some(("/component".into(), "component is required".into())) };
    let Some(component) = component_value.as_str() else { return Some(("/component".into(), "component is a string".into())) };
    if !crate::ident::is_identifier(component) {
        return Some(("/component".into(), not_id("component ", component)));
    }
    if component == crate::catalog::SURFACE_COMPONENT {
        return Some(("/component".into(), "Surface is reserved".into()));
    }
    if o.get("catalogId").is_some_and(|c| !c.is_string()) {
        return Some(("/catalogId".into(), "catalogId is a string".into()));
    }
    if let Some((at, message)) = o.get("metadata").and_then(check_metadata) {
        return Some((format!("/metadata{at}"), message));
    }
    let catalog = o.get("catalogId").and_then(Value::as_str).or(catalog_id).unwrap_or("");
    let basic = crate::functions::is_basic_catalog(catalog);
    let def = if basic { None } else { view.catalog_component(catalog, component) };
    let refs = if basic { basic_id_refs(component, o) } else { Vec::new() };
    // §4.2: an id reference is a string and an identifier.
    let id_ref = |v: &Value, at: String| -> Option<(String, String)> {
        match v.as_str() {
            None => Some((at, "a component id is a string".into())),
            Some(s) if !crate::ident::is_identifier(s) => Some((at, not_id("id ", s))),
            Some(_) => None,
        }
    };
    for (k, v) in o {
        if matches!(k.as_str(), "id" | "component" | "catalogId" | "metadata") {
            continue;
        }
        let at = format!("/{}", esc(k));
        if !crate::ident::is_identifier(k) {
            return Some((at, not_id("prop ", k)));
        }
        let dynamic = |v: &Value, at: &str| crate::expr::malformed_directive(v).map(|(below, message)| (format!("{at}{below}"), message));
        match k.as_str() {
            "accessibility" => {
                if let Some(a) = v.as_object() {
                    if let Some(hit) = ["label", "description", "hidden"].iter().find_map(|key| a.get(*key).and_then(|x| dynamic(x, &format!("{at}/{key}")))) {
                        return Some(hit);
                    }
                }
                continue;
            }
            "children" => {
                match v {
                    Value::Array(items) => {
                        if let Some(hit) = items.iter().enumerate().find_map(|(i, c)| id_ref(c, format!("{at}/{i}"))) {
                            return Some(hit);
                        }
                    }
                    Value::Object(t) => {
                        if let Some(hit) = t.get("componentId").and_then(|c| id_ref(c, format!("{at}/componentId"))) {
                            return Some(hit);
                        }
                    }
                    _ => {}
                }
                continue;
            }
            "slots" => {
                if let Some(slots) = v.as_object() {
                    if let Some(hit) = slots.iter().find_map(|(slot, c)| id_ref(c, format!("{at}/{}", esc(slot)))) {
                        return Some(hit);
                    }
                }
                continue;
            }
            "style" => continue,
            "on" => {
                if let Some(on) = v.as_object() {
                    if let Some(hit) = on.iter().find_map(|(event, action)| check_action(action, &format!("{at}/{}", esc(event)))) {
                        return Some(hit);
                    }
                }
                continue;
            }
            "action" if basic => {
                if let Some(hit) = check_action(v, &at) {
                    return Some(hit);
                }
                continue;
            }
            _ => {}
        }
        if basic {
            let mine: Vec<&(String, &Value)> = refs.iter().filter(|(p, _)| *p == at || p.starts_with(&format!("{at}/"))).collect();
            if let Some(hit) = mine.iter().find_map(|(p, c)| id_ref(c, p.clone())) {
                return Some(hit);
            }
            if mine.iter().any(|(p, _)| *p == at) {
                continue;
            }
        }
        let schema = def.and_then(|d| d.props.get(k.as_str()));
        if schema.is_some_and(|s| s.type_ == "componentId") {
            if let Some(hit) = id_ref(v, at) {
                return Some(hit);
            }
            continue;
        }
        if crate::data::is_data_schema(schema) {
            continue;
        }
        if let Some(hit) = dynamic(v, &at) {
            return Some(hit);
        }
    }
    None
}

impl HostRouter {
    /// `extension_ids` = the extension catalog ids the host registered.
    pub fn new<S: AsRef<str>>(extension_ids: &[S]) -> Self {
        HostRouter { extension_ids: extension_ids.iter().map(|s| s.as_ref().to_string()).collect(), ..Default::default() }
    }

    /// [`Self::new`] with the registered extensions' versions and components
    /// (the TS `HostRouterOptions.extensions`).
    pub fn with_extensions<S: AsRef<str>>(extension_ids: &[S], extensions: Option<Vec<AvailableExtension>>) -> Self {
        HostRouter { extensions, ..Self::new(extension_ids) }
    }

    pub fn supported_catalog_ids(&self) -> Vec<String> {
        supported_catalog_ids(&self.extension_ids)
    }

    pub fn register_extension(&mut self, id: &str) {
        self.register_extension_with(id, None);
    }

    /// VAPP-161: with its version and components (`None` = `{id}`), kept
    /// when the router checks extensions at all.
    pub fn register_extension_with(&mut self, id: &str, available: Option<AvailableExtension>) {
        if !self.extension_ids.iter().any(|x| x == id) {
            self.extension_ids.push(id.to_string());
        }
        if let Some(extensions) = &mut self.extensions {
            if !extensions.iter().any(|e| e.id == id) {
                extensions.push(available.unwrap_or_else(|| AvailableExtension { id: id.to_string(), ..Default::default() }));
            }
        }
    }

    /// Install a declarative package: its templates become `applyTemplate`
    /// targets. Returns the validation issues (installed only when none).
    pub fn install_package(&mut self, pkg: &Value) -> Vec<PackageIssue> {
        // Round 9 §12/§14: a format-5 package is upgraded while the bridge is on.
        let upgraded;
        let pkg = match upgrade_package(pkg) {
            Ok(Some(p)) => {
                upgraded = p;
                &upgraded
            }
            Ok(None) => pkg,
            Err(issue) => return vec![issue],
        };
        let issues = validate_package_with(pkg, &self.supported_catalog_ids(), self.extensions.as_deref());
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

    /// The catalogs an entry check reads: the core; the router knows
    /// extensions by id (their definitions stay with the surface).
    fn view(&self) -> std::sync::Arc<crate::catalog::CatalogView> {
        crate::catalog::CatalogView::core()
    }

    /// §5.5: the AVAILABLE set: the supported ids then the inline catalogs.
    pub fn available_catalog_ids(&self) -> Vec<String> {
        let mut out = self.supported_catalog_ids();
        for id in &self.inline_catalog_ids {
            if !out.contains(id) {
                out.push(id.clone());
            }
        }
        out
    }

    /// §10: an inline catalog the agent sent joins the available set.
    pub fn register_inline_catalog(&mut self, id: &str) {
        if !self.inline_catalog_ids.iter().any(|x| x == id) {
            self.inline_catalog_ids.push(id.to_string());
        }
    }

    fn available(&self, catalog_id: &str) -> bool {
        self.available_catalog_ids().iter().any(|c| c == catalog_id)
    }

    /// §1.4: a fresh id for a re-opened surface: `<base>#<n>`, skipping ids
    /// seen or live. Minting does not add to `seen`; creating does.
    pub fn mint_surface_id(&mut self, base: &str) -> String {
        if self.mint_next == 0 {
            self.mint_next = 1;
        }
        loop {
            let candidate = format!("{base}#{}", self.mint_next);
            self.mint_next += 1;
            if !self.seen.contains(&candidate) && !self.surfaces.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    /// Has this router ever created `surface_id`?
    pub fn was_created(&self, surface_id: &str) -> bool {
        self.seen.contains(surface_id)
    }

    /// One agent message → the ops to perform, in order. Never fails.
    /// Round 9 §1.1: the checks in order, the first failing one answers.
    pub fn route(&mut self, message: &Value, options: RouteOptions) -> Vec<Value> {
        if oversized(message) {
            let sid = message.as_object().map(surface_id_of).unwrap_or_default();
            return vec![invalid(&sid, &limits::message_bytes_issue())];
        }
        let Some(object) = message.as_object() else { return vec![invalid("", "a message is a JSON object")] };
        match check_envelope(object, options.accept_v09) {
            Envelope::V10 => {}
            Envelope::Refused(reason) => return vec![invalid(&surface_id_of(object), &reason)],
            Envelope::Upgrade => {
                let messages = crate::upgrade09::upgrade_message09_in(message, &mut self.upgrade_state);
                let mut ops = Vec::new();
                for m in messages {
                    // The upgraded form is 1.0: never bridged twice.
                    if m.get("version").and_then(Value::as_str) != Some(A2UI_VERSION) {
                        ops.push(invalid(&m.as_object().map(surface_id_of).unwrap_or_default(), "upgrade09 produced no 1.0 message"));
                        continue;
                    }
                    ops.extend(self.route(&m, options));
                }
                return ops;
            }
        }
        let kinds: Vec<&String> = object.keys().filter(|k| *k != "version").collect();
        if kinds.len() != 1 || !MESSAGE_KINDS.contains(&kinds[0].as_str()) {
            return vec![invalid(&surface_id_of(object), &format!("expected exactly one of {}", super::contract::A2UI_MESSAGE_KINDS.join(", ")))];
        }
        let kind = kinds[0].as_str();
        let sid_of = surface_id_of(object);
        if HOST_MESSAGE_KINDS.contains(&kind) && !options.trusted {
            return vec![invalid(&sid_of, &format!("{kind} is host-internal"))];
        }
        let Some(body) = object[kind].as_object() else {
            return vec![invalid(&sid_of, &format!("{kind} is an object"))];
        };
        if let Some(key) = body.keys().find(|k| !allowed_keys(kind).contains(&k.as_str())) {
            return vec![invalid(&sid_of, &format!("{kind}.{key} is not allowed"))];
        }
        match kind {
            "callRendererFunction" => return self.call_renderer_function(body),
            "agentFunctionResponse" => return agent_function_response(body),
            _ => {}
        }
        let Some(surface_id) = body.get("surfaceId").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string) else {
            return vec![invalid(&sid_of, &format!("{kind}.surfaceId is required"))];
        };
        let sid = surface_id.as_str();
        match kind {
            "createSurface" => self.create_surface(sid, body, options),
            "updateComponents" => {
                let Some(components) = body.get("components").and_then(Value::as_array).filter(|c| !c.is_empty()) else {
                    return vec![invalid(sid, "updateComponents.components needs at least one component")];
                };
                let Some(info) = self.surfaces.get(sid) else {
                    return vec![missing(sid)];
                };
                let default_catalog = info.catalog_id.clone();
                let default_catalog = (!default_catalog.is_empty()).then_some(default_catalog.as_str());
                components_ops(sid, "updateComponents", components, default_catalog, &self.view())
            }
            "updateDataModel" => {
                if !self.surfaces.contains_key(sid) {
                    return vec![missing(sid)];
                }
                let Some(value) = body.get("value") else {
                    return vec![invalid(sid, "updateDataModel.value is required (null deletes)")];
                };
                let path = match body.get("path") {
                    None => None,
                    Some(Value::String(p)) => Some(p.as_str()),
                    Some(_) => return vec![invalid(sid, "updateDataModel.path is a string")],
                };
                let path = normalize_path(path);
                match value {
                    Value::Null => vec![json!({"op": "data", "surfaceId": sid, "path": path})],
                    value => vec![json!({"op": "data", "surfaceId": sid, "path": path, "value": value})],
                }
            }
            "deleteSurface" => {
                if self.surfaces.shift_remove(sid).is_none() {
                    return vec![missing(sid)];
                }
                vec![json!({"op": "delete", "surfaceId": sid})]
            }
            "bindDataModel" => {
                let Some(info) = self.surfaces.get(sid) else {
                    return vec![missing(sid)];
                };
                match (body.get("path").and_then(Value::as_str), body.get("source").and_then(Value::as_str)) {
                    (Some(path), Some(source)) if parse_source(source).is_some() => {
                        // VAPP-107: a package surface follows only the sources its package lists.
                        if let Some(pkg) = info.package_id.as_deref().and_then(|id| self.packages.get(id)) {
                            if !source_allowed(package_sources(pkg).as_deref(), source) && !super::record_macros::is_record_macro_binding(&normalize_path(Some(path)), source) {
                                let pkg_id = pkg.get("id").and_then(Value::as_str).unwrap_or("");
                                let at = normalize_path(Some(path));
                                let at = if at.is_empty() { "/".to_string() } else { at };
                                return vec![send(error_message(SOURCE_DENIED, sid, &format!("source {source} is not in package {pkg_id}'s sources"), Some(&at)))];
                            }
                        }
                        // VAPP-190: `?query=<name>` follows the package's named query.
                        // VAPP-206: `${today}`, `${now}`, `${month}` from the host's params.
                        let source = match info.package_id.as_deref().and_then(|id| self.packages.get(id)) {
                            Some(pkg) => {
                                let host = body.get("hostParams");
                                super::query::expand_query_source_with(&super::host_params::fill_host_params(source, host), pkg.get("queries"), host)
                            }
                            None => source.to_string(),
                        };
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
                if body.get("data").is_some_and(|d| !d.is_object()) {
                    return vec![invalid(sid, "applyTemplate.data is an object")];
                }
                let mut ops = Vec::new();
                let pkg_id = pkg.get("id").and_then(Value::as_str).map(str::to_string);
                for mut m in template_messages(&pkg, template_id, sid, body.get("data")).unwrap_or_default() {
                    // VAPP-206: the template's own binds read the host's params too.
                    if let (Some(host), Some(bind)) = (body.get("hostParams").filter(|h| h.is_object()), m.get_mut("bindDataModel").and_then(Value::as_object_mut)) {
                        bind.insert("hostParams".into(), host.clone());
                    }
                    let created = m.get("createSurface").is_some();
                    let routed = self.route(&m, RouteOptions { trusted: true, ..options });
                    let refused = created && !routed.iter().any(|op| op.get("op").and_then(Value::as_str) == Some("create"));
                    ops.extend(routed);
                    if refused {
                        break;
                    }
                    // The surface belongs to the package from its creation on (its
                    // function policy and source allowlist apply to the template's own binds).
                    if let Some(info) = self.surfaces.get_mut(sid) {
                        info.package_id = pkg_id.clone();
                    }
                }
                ops
            }
            _ => vec![invalid(sid, "unknown message")],
        }
    }

    /// §1.2 createSurface.
    fn create_surface(&mut self, sid: &str, body: &Map<String, Value>, options: RouteOptions) -> Vec<Value> {
        let catalog_id = match body.get("catalogId") {
            None => None,
            Some(Value::String(c)) => Some(c.as_str()),
            Some(_) => return vec![invalid(sid, "createSurface.catalogId is a string")],
        };
        if let Some(catalog_id) = catalog_id {
            if !self.available(catalog_id) {
                return vec![send(error_message(UNSUPPORTED_CATALOG, sid, &format!("catalog {catalog_id} is not supported"), None))];
            }
        }
        if self.seen.contains(sid) {
            return vec![send(error_message(INTEGRITY_ERROR, sid, &format!("surface {sid} was already created; surface ids are unique for the renderer's lifetime"), None))];
        }
        let components = match body.get("components") {
            None => None,
            Some(Value::Array(c)) if !c.is_empty() => Some(c),
            Some(_) => return vec![invalid(sid, "createSurface.components needs at least one component")],
        };
        if body.get("dataModel").is_some_and(|d| !d.is_object()) {
            return vec![invalid(sid, "createSurface.dataModel is an object")];
        }
        if body.get("sendDataModel").is_some_and(|d| !d.is_boolean()) {
            return vec![invalid(sid, "createSurface.sendDataModel is a boolean")];
        }
        if let Some((at, message)) = body.get("metadata").and_then(check_metadata) {
            return vec![invalid_at(sid, &format!("createSurface.{message}"), &format!("/createSurface/metadata{at}"))];
        }
        self.seen.insert(sid.to_string());
        self.surfaces.insert(sid.to_string(), SurfaceInfo { surface_id: sid.to_string(), catalog_id: catalog_id.unwrap_or("").to_string(), package_id: None, agent: !options.trusted });
        let mut op = Map::new();
        op.insert("op".into(), json!("create"));
        op.insert("surfaceId".into(), json!(sid));
        if let Some(catalog_id) = catalog_id {
            op.insert("catalogId".into(), json!(catalog_id));
        }
        if body.get("sendDataModel") == Some(&Value::Bool(true)) {
            op.insert("sendDataModel".into(), Value::Bool(true));
        }
        if let Some(metadata) = body.get("metadata") {
            op.insert("metadata".into(), metadata.clone());
            // §8: the theme rides in metadata.extensions.
            if let Some(theme) = metadata.get("extensions").and_then(|e| e.get("at_exponential_theme")) {
                op.insert("theme".into(), theme.clone());
            }
        }
        let mut ops = vec![Value::Object(op)];
        if let Some(data) = body.get("dataModel") {
            ops.push(json!({"op": "data", "surfaceId": sid, "path": "", "value": data}));
        }
        if let Some(components) = components {
            ops.extend(components_ops(sid, "createSurface", components, catalog_id, &self.view()));
        }
        ops
    }

    /// §6.9 callRendererFunction.
    fn call_renderer_function(&self, body: &Map<String, Value>) -> Vec<Value> {
        let Some(call_id) = body.get("functionCallId").and_then(Value::as_str).filter(|s| !s.is_empty()) else {
            return vec![invalid("", "callRendererFunction.functionCallId is required")];
        };
        let bad = |message: &str| vec![send(error_message_to(INVALID_MESSAGE, ErrorTarget::FunctionCall(call_id), message, None))];
        let answer = |code: &str, message: &str| vec![send(renderer_function_response(call_id, Err((code, message))))];
        let Some(call) = body.get("callFunction").and_then(Value::as_object) else {
            return bad("callRendererFunction.callFunction is a function call");
        };
        let (Some(name), Some(catalog_id)) = (call.get("@call").and_then(Value::as_str), call.get("catalogId").and_then(Value::as_str)) else {
            return bad("callRendererFunction.callFunction needs a string @call and catalogId");
        };
        if let Some(k) = call.keys().find(|k| !matches!(k.as_str(), "@call" | "args" | "catalogId")) {
            return bad(&format!("callRendererFunction.callFunction.{k} is not allowed"));
        }
        if call.get("args").is_some_and(|a| !a.is_object()) {
            return bad("callRendererFunction.callFunction.args is an object");
        }
        let surface_id = match body.get("surfaceId") {
            None => None,
            Some(Value::String(s)) => Some(s.as_str()),
            Some(_) => return bad("callRendererFunction.surfaceId is a string"),
        };
        if !self.available(catalog_id) {
            return answer(INVALID_FUNCTION_CALL, &format!("catalog {catalog_id} is not available"));
        }
        if let Some(sid) = surface_id {
            if !self.surfaces.contains_key(sid) {
                return answer(INVALID_FUNCTION_CALL, &format!("no surface {sid}"));
            }
        }
        if name == crate::functions::INDEX_FUNCTION {
            return answer(EXECUTION_ERROR, "@index outside a template");
        }
        let Some(signature) = crate::functions::declared_function(catalog_id, name) else {
            return answer(INVALID_FUNCTION_CALL, &format!("unknown function {name}"));
        };
        if crate::functions::allowed_callers(name) == crate::functions::AllowedCallers::RendererOnly {
            return answer(INVALID_FUNCTION_CALL, &format!("{name} cannot be called by the agent"));
        }
        if crate::functions::requires_user_activation(name) {
            return answer(INVALID_FUNCTION_CALL, &format!("{name} needs a user gesture"));
        }
        let args = call.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
        if let Some(issue) = crate::functions::check_args(signature, &args) {
            return answer(INVALID_FUNCTION_CALL, &format!("args: {issue}"));
        }
        let mut op = Map::new();
        op.insert("op".into(), json!("call"));
        op.insert("functionCallId".into(), json!(call_id));
        if let Some(sid) = surface_id {
            op.insert("surfaceId".into(), json!(sid));
        }
        op.insert("catalogId".into(), json!(catalog_id));
        op.insert("name".into(), json!(name));
        op.insert("args".into(), Value::Object(args));
        vec![Value::Object(op)]
    }

    fn find_package(&self, template_id: &str, package_id: Option<&str>) -> Option<&Value> {
        let has = |pkg: &Value| pkg.get("templates").and_then(|t| t.get(template_id)).is_some_and(super::js_truthy);
        match package_id {
            Some(id) => self.packages.get(id).filter(|p| has(p)),
            None => self.packages.values().find(|p| has(p)),
        }
    }
}

/// §1.2 agentFunctionResponse.
fn agent_function_response(body: &Map<String, Value>) -> Vec<Value> {
    let Some(call_id) = body.get("functionCallId").and_then(Value::as_str).filter(|s| !s.is_empty()) else {
        return vec![invalid("", "agentFunctionResponse.functionCallId is required")];
    };
    let bad = |message: &str| vec![send(error_message_to(INVALID_MESSAGE, ErrorTarget::FunctionCall(call_id), message, None))];
    match (body.get("value"), body.get("error")) {
        (Some(value), None) => vec![json!({"op": "respond", "functionCallId": call_id, "value": value})],
        (None, Some(error)) => {
            let ok = error.as_object().is_some_and(|e| {
                e.get("code").is_some_and(Value::is_string) && e.get("message").is_some_and(Value::is_string) && e.keys().all(|k| k == "code" || k == "message")
            });
            if !ok {
                return bad("agentFunctionResponse.error is {code, message}");
            }
            vec![json!({"op": "respond", "functionCallId": call_id, "error": error})]
        }
        _ => bad("agentFunctionResponse needs exactly one of value and error"),
    }
}

/// §1.2 / §5.2: every entry through [`check_entry`]: one INVALID_MESSAGE per
/// refused entry (in order), then the `components` op of the accepted ones.
fn components_ops(sid: &str, kind: &str, components: &[Value], default_catalog: Option<&str>, view: &crate::catalog::CatalogView) -> Vec<Value> {
    let mut ops = Vec::new();
    let mut accepted = Vec::new();
    for (i, entry) in components.iter().enumerate() {
        match check_entry(entry, default_catalog, view) {
            Some((at, message)) => ops.push(invalid_at(sid, &message, &format!("/{kind}/components/{i}{at}"))),
            None => accepted.push(entry.clone()),
        }
    }
    if !accepted.is_empty() {
        ops.push(json!({"op": "components", "surfaceId": sid, "components": accepted}));
    }
    ops
}

/// Round 9 §12: a `formatVersion` 5 package through `upgrade_package09`
/// while `ACCEPT_V09` holds: `Ok(Some(v6))`; `Ok(None)` = not a v5 file (or
/// the bridge is off: validation refuses it); `Err` = its signature does
/// not verify.
pub fn upgrade_package(pkg: &Value) -> Result<Option<Value>, PackageIssue> {
    let v5 = pkg.get("formatVersion").and_then(Value::as_f64) == Some(f64::from(super::package::PACKAGE_FORMAT_VERSION_09));
    if !v5 || !ACCEPT_V09 {
        return Ok(None);
    }
    let up = crate::upgrade09::upgrade_package09(pkg, crate::upgrade09::structural_verification);
    match up.package {
        Some(p) => Ok(Some(p)),
        None => Err(PackageIssue { path: "/signature".into(), message: format!("the package signature is {}", up.status) }),
    }
}
