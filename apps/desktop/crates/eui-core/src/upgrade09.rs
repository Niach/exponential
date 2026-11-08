//! Round 9 §14: `upgrade09`, the PURE A2UI 0.9 → 1.0 converter. Mirrors
//! `packages/exponential-ui/src/upgrade09.ts` (same structure, same
//! decisions); both replay `fixtures/upgrade09.json`.
//!
//! - [`upgrade_message09`]: an agent → renderer (or host-internal) message,
//!   threading the per-surface id maps ([`Upgrade09State`]) across a stream;
//! - [`upgrade_client_message09`]: a renderer → agent `action` / `error`;
//! - [`upgrade_card09`]: a nested `exponential_show` card `{root, data}`;
//! - [`upgrade_package09`]: a formatVersion 5 package → 6;
//! - [`upgrade09_json`]: the one JSON entry the FFI exposes (`upgrade09Json`).
//!
//! Output key order follows the input's: a renamed key keeps its place, an
//! added key goes to the end of its object (`version` is the exception: it
//! is always first).

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::ident::{is_identifier, mangle};

type Obj = Map<String, Value>;

/// The 1.0 basic catalog id (§9.1).
pub const BASIC_CATALOG_ID_10: &str = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";
/// The two 0.9 basic catalog ids, both rewritten to [`BASIC_CATALOG_ID_10`].
pub const BASIC_CATALOG_IDS_09: &[&str] = &[
    "https://a2ui.org/specification/v0_9/basic_catalog.json",
    "https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json",
];

fn is_basic_id(id: Option<&Value>) -> bool {
    id.and_then(Value::as_str).is_some_and(|s| s == BASIC_CATALOG_ID_10 || BASIC_CATALOG_IDS_09.contains(&s))
}

fn upgrade_catalog_id(id: &Value) -> Value {
    match id.as_str() {
        Some(s) if BASIC_CATALOG_IDS_09.contains(&s) => Value::String(BASIC_CATALOG_ID_10.into()),
        _ => id.clone(),
    }
}

/// `surfaces`: per surface, every id seen so far → its 1.0 id (kept ids map
/// to themselves). `catalogs`: the catalog a `createSurface` named, so a
/// later `updateComponents` of a basic surface gets the basic rules.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Upgrade09State {
    #[serde(default)]
    pub surfaces: IndexMap<String, IndexMap<String, String>>,
    #[serde(default)]
    pub catalogs: IndexMap<String, String>,
    /// #EXP-1742: per surface, `surfaces`' inverse (target → original id),
    /// kept across messages so a streamed message costs O(message), not
    /// O(ids seen). A cache: never serialized or compared, rebuilt when its
    /// size disagrees with the map's (a deserialized state).
    #[serde(skip)]
    owners: HashMap<String, HashMap<String, String>>,
}

impl PartialEq for Upgrade09State {
    fn eq(&self, other: &Self) -> bool {
        self.surfaces == other.surfaces && self.catalogs == other.catalogs
    }
}

/// `version` absent or `"v0.9"`.
pub fn is_v09(message: &Value) -> bool {
    match message.get("version") {
        None => true,
        Some(v) => v.as_str() == Some("v0.9"),
    }
}

// ---------------------------------------------------------------------------
// Host function names (§6.3, §14.3)

const DOT_PREFIXES: &[&str] = &[
    "app", "records", "actions", "harness", "exponential", "camera", "clipboard", "files", "http", "location", "notifications", "photos",
    "print", "share",
];

/// Package tool ids re-keyed by the package upgrade (old → new).
type ToolMap = IndexMap<String, String>;

fn rename_with(name: &str, tools: &ToolMap) -> String {
    let Some((head, rest)) = name.split_once('.') else { return name.to_string() };
    if head == "tools" || head == "upstream" {
        let (id, tail) = match rest.split_once('.') {
            Some((id, tail)) => (id, Some(tail)),
            None => (rest, None),
        };
        let flat_id = match tools.get(id) {
            Some(t) if head == "tools" => t.clone(),
            _ => mangle(id),
        };
        return match tail {
            None => format!("{head}_{flat_id}"),
            Some(tail) => format!("{head}_{flat_id}__{}", mangle(tail)),
        };
    }
    if DOT_PREFIXES.contains(&head) {
        name.replace('.', "_")
    } else {
        name.to_string()
    }
}

fn rename_pattern_with(pattern: &str, tools: &ToolMap) -> String {
    let Some(base) = pattern.strip_suffix('*') else { return rename_with(pattern, tools) };
    let Some(stem) = base.strip_suffix('.') else { return format!("{}*", rename_with(base, tools)) };
    let segments: Vec<&str> = stem.split('.').collect();
    if segments.len() == 1 {
        return if DOT_PREFIXES.contains(&stem) || stem == "tools" || stem == "upstream" { format!("{stem}_*") } else { pattern.to_string() };
    }
    let flat = rename_with(stem, tools);
    if flat == stem {
        return pattern.to_string();
    }
    // `tools.<id>.*` / `upstream.<server>.*`: the tool name follows `__`.
    let sep = if (segments[0] == "tools" || segments[0] == "upstream") && segments.len() == 2 { "__" } else { "_" };
    format!("{flat}{sep}*")
}

/// §14.3 `rename`: a 0.9 host function name → its flat 1.0 name; any other
/// name unchanged. A pattern (ending in `*`) keeps its `*`:
/// `harness.*` → `harness_*`, `tools.crm.*` → `tools_crm__*`, `*` unchanged.
pub fn rename09(name: &str) -> String {
    rename_pattern_with(name, &ToolMap::new())
}

// ---------------------------------------------------------------------------
// Dynamic values (§14.3 "Dynamic conversion", "Presence wrap")

const VALIDATORS: &[&str] = &["required", "regex", "length", "numeric", "email"];

fn returns_of(name: &str) -> Option<String> {
    if VALIDATORS.contains(&name) {
        return Some("validationResult".into());
    }
    let fns = &crate::catalog::CORE.functions;
    fns.core.get(name).or_else(|| fns.basic.get(name)).and_then(|d| d.get("returns")).and_then(Value::as_str).map(str::to_string)
}

/// `present(v)`: the old JS truthiness (`0` is present).
fn present(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Visible,
    Check,
    Bool,
}

fn cond(x: &Value) -> Value {
    json!({"@call": "cond", "args": {"if": x.clone(), "then": true, "else": false}})
}

/// The presence wrap table: `x` is already 1.0.
fn as_presence(x: &Value, column: Column) -> Value {
    if let Some(o) = x.as_object() {
        if o.contains_key("@path") {
            return cond(x);
        }
        if let Some(call) = o.get("@call") {
            let r = call.as_str().and_then(returns_of);
            return if matches!(r.as_deref(), Some("boolean" | "validationResult")) { x.clone() } else { cond(x) };
        }
    }
    if column == Column::Check {
        return cond(x);
    }
    if x.is_boolean() {
        return x.clone();
    }
    Value::Bool(present(Some(x)))
}

struct Ctx {
    tools: ToolMap,
}

impl Ctx {
    fn plain() -> Self {
        Ctx { tools: ToolMap::new() }
    }
}

/// §14.3 `dyn`.
fn dyn_(v: &Value, ctx: &Ctx) -> Value {
    crate::deep(|| match v {
        Value::Array(items) => Value::Array(items.iter().map(|i| dyn_(i, ctx)).collect()),
        Value::Object(o) => {
            if o.len() == 1 && o.get("path").is_some_and(Value::is_string) {
                return json!({"@path": o["path"].clone()});
            }
            if let Some(Value::String(call)) = o.get("call") {
                let name = rename_with(call, &ctx.tools);
                let mut out = Obj::new();
                out.insert("@call".into(), Value::String(name.clone()));
                if let Some(Value::Object(args)) = o.get("args") {
                    out.insert("args".into(), Value::Object(dyn_args(&name, args, ctx)));
                }
                return Value::Object(out);
            }
            Value::Object(o.iter().map(|(k, item)| (if k.starts_with('@') { format!("@{k}") } else { k.clone() }, dyn_(item, ctx))).collect())
        }
        other => other.clone(),
    })
}

/// A call's args: every value `dyn`'d (the keys are arg names, kept), with
/// the presence wrap on `and.values[i]`, `or.values[i]`, `not.value`.
fn dyn_args(name: &str, args: &Obj, ctx: &Ctx) -> Obj {
    let mut out = Obj::new();
    for (k, item) in args {
        let mut value = dyn_(item, ctx);
        if (name == "and" || name == "or") && k == "values" {
            if let Value::Array(values) = &mut value {
                for x in values.iter_mut() {
                    *x = as_presence(x, Column::Bool);
                }
            }
        } else if name == "not" && k == "value" {
            value = as_presence(&value, Column::Bool);
        }
        out.insert(k.clone(), value);
    }
    out
}

/// An Action (`on.<event>` or basic `action`): event context values and
/// `userMessage` `dyn`'d, `functionCall` → the call object.
fn upgrade_action(action: &Value, ctx: &Ctx) -> Value {
    let Some(a) = action.as_object() else { return action.clone() };
    let mut out = Obj::new();
    for (k, v) in a {
        let value = match (k.as_str(), v) {
            ("event", Value::Object(event)) => {
                let mut e = Obj::new();
                for (ek, ev) in event {
                    let value = match (ek.as_str(), ev) {
                        ("context", Value::Object(ctx_obj)) => Value::Object(ctx_obj.iter().map(|(ck, cv)| (ck.clone(), dyn_(cv, ctx))).collect()),
                        ("userMessage", _) => dyn_(ev, ctx),
                        _ => ev.clone(),
                    };
                    e.insert(ek.clone(), value);
                }
                Value::Object(e)
            }
            ("functionCall", _) => dyn_(v, ctx),
            _ => v.clone(),
        };
        out.insert(k.clone(), value);
    }
    Value::Object(out)
}

fn upgrade_checks(checks: &Value, ctx: &Ctx) -> Value {
    let Some(items) = checks.as_array() else { return dyn_(checks, ctx) };
    Value::Array(
        items
            .iter()
            .map(|check| match check.as_object() {
                Some(o) => Value::Object(
                    o.iter().map(|(k, v)| (k.clone(), if k == "condition" { as_presence(&dyn_(v, ctx), Column::Check) } else { v.clone() })).collect(),
                ),
                None => dyn_(check, ctx),
            })
            .collect(),
    )
}

fn upgrade_accessibility(a11y: &Value, ctx: &Ctx) -> Value {
    match a11y.as_object() {
        Some(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), if k == "label" || k == "description" { dyn_(v, ctx) } else { v.clone() })).collect()),
        None => a11y.clone(),
    }
}

// ---------------------------------------------------------------------------
// Catalog lookups (core component prop schemas)

fn core_prop_schema(component: Option<&Value>, prop: &str) -> Option<&'static crate::types::PropSchema> {
    let name = component?.as_str()?;
    crate::catalog::CORE.components.get(name)?.props.get(prop)
}

/// The core props that hold a component id (§5.3).
fn is_core_id_prop(component: Option<&Value>, prop: &str) -> bool {
    let Some(name) = component.and_then(Value::as_str) else { return false };
    if matches!((name, prop), ("Label", "for") | ("Popover", "anchor") | ("Menu", "anchor")) {
        return true;
    }
    core_prop_schema(component, prop).is_some_and(|s| s.type_ == "componentId")
}

const BASIC_ID_PROPS: &[&str] = &["child", "trigger", "content"];

// ---------------------------------------------------------------------------
// Id re-key (§14.3)

struct Rekey<'a> {
    map: &'a mut IndexMap<String, String>,
    /// target → its original id (the map is injective).
    owners: &'a mut HashMap<String, String>,
}

impl<'a> Rekey<'a> {
    fn new(map: &'a mut IndexMap<String, String>, owners: &'a mut HashMap<String, String>) -> Self {
        if owners.len() != map.len() {
            *owners = map.iter().map(|(old, target)| (target.clone(), old.clone())).collect();
        }
        Rekey { map, owners }
    }

    fn target_of_other(&self, id: &str) -> bool {
        self.owners.get(id).is_some_and(|owner| owner != id)
    }

    fn assign(&mut self, old: &str, kept: &HashSet<String>) -> String {
        if let Some(t) = self.map.get(old) {
            return t.clone();
        }
        let target = if is_identifier(old) && !self.target_of_other(old) {
            old.to_string()
        } else {
            let base = mangle(old);
            let mut target = base.clone();
            let mut n = 2;
            while kept.contains(&target) || self.owners.contains_key(&target) {
                target = format!("{base}_{n}");
                n += 1;
            }
            target
        };
        self.map.insert(old.to_string(), target.clone());
        self.owners.insert(target.clone(), old.to_string());
        target
    }

    /// Steps 1–3: the defined ids, in definition order.
    fn define(&mut self, ids: &[String]) {
        let mut kept = HashSet::new();
        for id in ids {
            if !self.map.contains_key(id) && is_identifier(id) && !self.target_of_other(id) {
                kept.insert(id.clone());
            }
        }
        for id in ids {
            if kept.contains(id) {
                self.assign(id, &kept);
            }
        }
        for id in ids {
            self.assign(id, &kept);
        }
    }

    /// Step 4: a reference (assigned on first sight when undefined).
    fn reference(&mut self, id: &Value) -> Value {
        match id.as_str() {
            Some(s) => Value::String(self.assign(s, &HashSet::new())),
            None => id.clone(),
        }
    }
}

fn rekey_children(children: &Value, rekey: &mut Rekey) -> Value {
    match children {
        Value::Array(items) => Value::Array(items.iter().map(|c| rekey.reference(c)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), if k == "componentId" { rekey.reference(v) } else { v.clone() })).collect()),
        other => other.clone(),
    }
}

fn rekey_slots(slots: &Value, rekey: &mut Rekey) -> Value {
    match slots.as_object() {
        Some(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), rekey.reference(v))).collect()),
        None => slots.clone(),
    }
}

fn rekey_template(template: &Value, rekey: &mut Rekey) -> Value {
    match template.as_object() {
        Some(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), if k == "component" { rekey.reference(v) } else { v.clone() })).collect()),
        None => template.clone(),
    }
}

// ---------------------------------------------------------------------------
// Components

fn heading_level(variant: Option<&Value>) -> Option<usize> {
    match variant?.as_str()? {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        _ => None,
    }
}

/// The basic rules that run before `dyn` (Icon, Text, TextField): the entry
/// with them applied (key order kept), plus the extra check a
/// `validationRegexp` turns into.
fn basic_before(entry: &Obj, ctx: &Ctx) -> (Obj, Option<Value>) {
    let component = entry.get("component").and_then(Value::as_str);
    if component == Some("Icon") {
        if let Some(Value::Object(name)) = entry.get("name") {
            if name.len() == 1 && name.contains_key("path") {
                let mut out = entry.clone();
                out.insert("name".into(), json!({"svgPath": name["path"].clone()}));
                return (out, None);
            }
        }
    }
    if component == Some("Text") {
        if let Some(level) = heading_level(entry.get("variant")) {
            let hashes = format!("{} ", "#".repeat(level));
            let mut out = Obj::new();
            for (k, v) in entry {
                match k.as_str() {
                    "variant" => {}
                    // The dynamic form is written in 0.9 shape: the prop's own `dyn` converts it.
                    "text" => {
                        let text = match v {
                            Value::String(s) => Value::String(format!("{hashes}{s}")),
                            _ => json!({"call": "concat", "args": {"values": [hashes, v.clone()]}}),
                        };
                        out.insert(k.clone(), text);
                    }
                    _ => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            return (out, None);
        }
    }
    if component == Some("TextField") && entry.contains_key("validationRegexp") {
        let out: Obj = entry.iter().filter(|(k, _)| *k != "validationRegexp").map(|(k, v)| (k.clone(), v.clone())).collect();
        let bound = entry.get("value").and_then(Value::as_object).is_some_and(|o| o.len() == 1 && o.get("path").is_some_and(Value::is_string));
        let value = if bound { dyn_(&entry["value"], ctx) } else { json!({"@path": entry.get("id").cloned().unwrap_or(Value::Null)}) };
        let check = json!({"condition": {"@call": "regex", "args": {"value": value, "pattern": entry["validationRegexp"].clone()}}, "message": "Invalid value"});
        return (out, Some(check));
    }
    (entry.clone(), None)
}

/// One component's props (flat entry keys, or a card node's `props`).
/// A 0.9 binding or call.
fn is_directive09(value: &Value) -> bool {
    value.as_object().is_some_and(|o| (o.len() == 1 && o.get("path").is_some_and(Value::is_string)) || o.get("call").is_some_and(Value::is_string))
}

fn upgrade_prop(key: &str, value: &Value, component: Option<&Value>, basic: bool, rekey: &mut Rekey, ctx: &Ctx) -> Value {
    if key == "checks" {
        return upgrade_checks(value, ctx);
    }
    if key == "on" {
        return match value.as_object() {
            Some(o) => Value::Object(o.iter().map(|(event, action)| (event.clone(), upgrade_action(action, ctx))).collect()),
            None => value.clone(),
        };
    }
    if basic {
        if key == "action" {
            return upgrade_action(value, ctx);
        }
        if BASIC_ID_PROPS.contains(&key) {
            return rekey.reference(value);
        }
        if key == "tabs" && component.and_then(Value::as_str) == Some("Tabs") {
            if let Some(tabs) = value.as_array() {
                return Value::Array(
                    tabs.iter()
                        .map(|tab| match tab.as_object() {
                            Some(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), if k == "child" { rekey.reference(v) } else { dyn_(v, ctx) })).collect()),
                            None => dyn_(tab, ctx),
                        })
                        .collect(),
                );
            }
        }
        return dyn_(value, ctx);
    }
    if is_core_id_prop(component, key) {
        return rekey.reference(value);
    }
    // A binding or call AT a data prop converts; a literal's inside stays data.
    if crate::data::is_data_schema(core_prop_schema(component, key)) {
        return if is_directive09(value) { dyn_(value, ctx) } else { value.clone() };
    }
    dyn_(value, ctx)
}

fn upgrade_flat_entry(raw: &Value, surface_catalog: Option<&Value>, rekey: &mut Rekey, ctx: &Ctx) -> Value {
    let Some(raw) = raw.as_object() else { return raw.clone() };
    let basic = is_basic_id(if raw.contains_key("catalogId") { raw.get("catalogId") } else { surface_catalog });
    let (entry, extra_check) = if basic { basic_before(raw, ctx) } else { (raw.clone(), None) };
    let component = entry.get("component");
    let mut out = Obj::new();
    for (k, v) in &entry {
        let value = match k.as_str() {
            "id" => rekey.reference(v),
            "component" | "style" | "metadata" => v.clone(),
            "catalogId" => upgrade_catalog_id(v),
            "children" => rekey_children(v, rekey),
            "slots" => rekey_slots(v, rekey),
            "template" => rekey_template(v, rekey),
            "visible" => as_presence(&dyn_(v, ctx), Column::Visible),
            "accessibility" => upgrade_accessibility(v, ctx),
            _ => upgrade_prop(k, v, component, basic, rekey, ctx),
        };
        out.insert(k.clone(), value);
    }
    if let Some(check) = extra_check {
        match out.get_mut("checks") {
            Some(Value::Array(checks)) => checks.push(check),
            _ => {
                out.insert("checks".into(), Value::Array(vec![check]));
            }
        }
    }
    if basic && component.and_then(Value::as_str) == Some("TextField") && !entry.contains_key("value") {
        out.insert("value".into(), Value::String(String::new()));
    }
    Value::Object(out)
}

fn upgrade_components_with(components: &Value, catalog_id: Option<&Value>, state: &mut Upgrade09State, surface_id: &str, ctx: &Ctx) -> Value {
    let Some(items) = components.as_array() else { return components.clone() };
    let map = state.surfaces.entry(surface_id.to_string()).or_default();
    let mut rekey = Rekey::new(map, state.owners.entry(surface_id.to_string()).or_default());
    let ids: Vec<String> = items.iter().filter_map(|c| c.as_object()?.get("id")?.as_str().map(str::to_string)).collect();
    rekey.define(&ids);
    Value::Array(items.iter().map(|c| upgrade_flat_entry(c, catalog_id, &mut rekey, ctx)).collect())
}

/// A flat 0.9 component list (an `updateComponents` / `createSurface` list
/// or a package template) upgraded as ONE surface: the state is copied,
/// never mutated.
pub fn upgrade_components09(components: &Value, catalog_id: Option<&Value>, state: &Upgrade09State, surface_id: &str) -> (Value, Upgrade09State) {
    let mut next = state.clone();
    let out = upgrade_components_with(components, catalog_id, &mut next, surface_id, &Ctx::plain());
    (out, next)
}

// ---------------------------------------------------------------------------
// Messages

fn with_version(rest: Obj) -> Value {
    let mut out = Obj::new();
    out.insert("version".into(), Value::String("1.0".into()));
    out.extend(rest);
    Value::Object(out)
}

fn surface_id_of(body: &Obj) -> String {
    body.get("surfaceId").and_then(Value::as_str).unwrap_or_default().to_string()
}

fn upgrade_create_surface(body: &Obj, state: &mut Upgrade09State, ctx: &Ctx) -> Value {
    let surface_id = surface_id_of(body);
    let catalog_id = body.get("catalogId").map(upgrade_catalog_id);
    if let Some(Value::String(c)) = &catalog_id {
        state.catalogs.insert(surface_id.clone(), c.clone());
    }
    let mut out = Obj::new();
    for (k, v) in body {
        match k.as_str() {
            "theme" => {}
            "catalogId" => {
                out.insert(k.clone(), catalog_id.clone().unwrap_or(Value::Null));
            }
            "components" => {
                let c = upgrade_components_with(v, catalog_id.as_ref(), state, &surface_id, ctx);
                out.insert(k.clone(), c);
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    if let Some(theme) = body.get("theme") {
        let mut metadata = out.get("metadata").and_then(Value::as_object).cloned().unwrap_or_default();
        let mut extensions = metadata.get("extensions").and_then(Value::as_object).cloned().unwrap_or_default();
        extensions.insert("at_exponential_theme".into(), theme.clone());
        metadata.insert("extensions".into(), Value::Object(extensions));
        out.insert("metadata".into(), Value::Object(metadata));
    }
    Value::Object(out)
}

fn upgrade_update_components(body: &Obj, state: &mut Upgrade09State, ctx: &Ctx) -> Value {
    let surface_id = surface_id_of(body);
    let catalog = state.catalogs.get(&surface_id).map(|c| Value::String(c.clone()));
    let mut out = Obj::new();
    for (k, v) in body {
        let value = if k == "components" { upgrade_components_with(v, catalog.as_ref(), state, &surface_id, ctx) } else { v.clone() };
        out.insert(k.clone(), value);
    }
    Value::Object(out)
}

/// §14.2: one 0.9 agent → renderer (or host-internal) message → exactly one
/// 1.0 message, threading the id maps of a stream through `state` (copied,
/// never mutated).
pub fn upgrade_message09(message: &Value, state: &Upgrade09State) -> (Vec<Value>, Upgrade09State) {
    let mut next = state.clone();
    let messages = upgrade_message09_in(message, &mut next);
    (messages, next)
}

/// [`upgrade_message09`] advancing `state` in place: what a stream (the
/// router, a surface) calls per message, so a message never copies the id
/// maps of every message before it (#EXP-1742).
pub fn upgrade_message09_in(message: &Value, next: &mut Upgrade09State) -> Vec<Value> {
    let Some(m) = message.as_object() else { return vec![message.clone()] };
    let ctx = Ctx::plain();
    let mut rest = Obj::new();
    for (k, v) in m {
        let value = match (k.as_str(), v) {
            ("version", _) => continue,
            ("createSurface", Value::Object(b)) => upgrade_create_surface(b, next, &ctx),
            ("updateComponents", Value::Object(b)) => upgrade_update_components(b, next, &ctx),
            ("updateDataModel", Value::Object(b)) => {
                let mut b = b.clone();
                if !b.contains_key("value") {
                    b.insert("value".into(), Value::Null);
                }
                Value::Object(b)
            }
            _ => v.clone(),
        };
        rest.insert(k.clone(), value);
    }
    vec![with_version(rest)]
}

fn client_code(code: &str) -> Option<&'static str> {
    match code {
        "FUNCTION_NOT_FOUND" => Some("UNKNOWN_FUNCTION"),
        "FUNCTION_DENIED" => Some("INVALID_FUNCTION_CALL"),
        "ACTION_FAILED" => Some("EXECUTION_ERROR"),
        _ => None,
    }
}

fn upgrade_client_action(body: &Obj) -> Value {
    let mut out = Obj::new();
    let mut added = Obj::new();
    if let Some(v) = body.get("payload") {
        if present(Some(v)) && !v.as_object().is_some_and(Map::is_empty) {
            added.insert("at_exponential_payload".into(), v.clone());
        }
    }
    for (k, v) in body {
        if k == "payload" {
            continue;
        }
        if k == "sourceComponentId" {
            if let Value::String(s) = v {
                let authored = mangle(s.split('.').next().unwrap_or_default());
                if authored != *s {
                    added.insert("at_exponential_instance".into(), v.clone());
                }
                out.insert(k.clone(), Value::String(authored));
                continue;
            }
        }
        out.insert(k.clone(), v.clone());
    }
    if !added.is_empty() {
        let mut metadata = out.get("metadata").and_then(Value::as_object).cloned().unwrap_or_default();
        let mut extensions = metadata.get("extensions").and_then(Value::as_object).cloned().unwrap_or_default();
        extensions.extend(added);
        metadata.insert("extensions".into(), Value::Object(extensions));
        out.insert("metadata".into(), Value::Object(metadata));
    }
    Value::Object(out)
}

/// §14.2: a 0.9 renderer → agent message (`action`, `error`) → 1.0.
pub fn upgrade_client_message09(message: &Value) -> Value {
    let Some(m) = message.as_object() else { return message.clone() };
    let mut rest = Obj::new();
    for (k, v) in m {
        let value = match (k.as_str(), v) {
            ("version", _) => continue,
            ("action", Value::Object(b)) => upgrade_client_action(b),
            ("error", Value::Object(b)) => {
                let mut b = b.clone();
                if let Some(code) = b.get("code").and_then(Value::as_str).and_then(client_code) {
                    b.insert("code".into(), Value::String(code.into()));
                }
                Value::Object(b)
            }
            _ => v.clone(),
        };
        rest.insert(k.clone(), value);
    }
    with_version(rest)
}

// ---------------------------------------------------------------------------
// Cards (nested {root, data})

fn collect_card_ids(node: &Value, out: &mut Vec<String>) {
    crate::deep(|| {
        let Some(n) = node.as_object() else { return };
        if let Some(id) = n.get("id").and_then(Value::as_str) {
            out.push(id.to_string());
        }
        if let Some(slots) = n.get("slots").and_then(Value::as_object) {
            let mut names: Vec<&String> = slots.keys().collect();
            names.sort_by(|a, b| crate::host::utf16_cmp(a, b));
            for name in names {
                collect_card_ids(&slots[name.as_str()], out);
            }
        }
        if let Some(children) = n.get("children").and_then(Value::as_array) {
            for c in children {
                collect_card_ids(c, out);
            }
        }
    })
}

fn upgrade_card_node(node: &Value, rekey: &mut Rekey, ctx: &Ctx) -> Value {
    crate::deep(|| {
        let Some(n) = node.as_object() else { return node.clone() };
        let component = n.get("component");
        let mut out = Obj::new();
        for (k, v) in n {
            let value = match (k.as_str(), v) {
                ("id", _) => rekey.reference(v),
                ("props", Value::Object(props)) => {
                    let mut p = Obj::new();
                    for (pk, pv) in props {
                        let value = upgrade_prop(pk, pv, component, false, rekey, ctx);
                        p.insert(pk.clone(), value);
                    }
                    Value::Object(p)
                }
                ("children", Value::Array(children)) => Value::Array(children.iter().map(|c| upgrade_card_node(c, rekey, ctx)).collect()),
                ("slots", Value::Object(slots)) => {
                    let mut s = Obj::new();
                    for (sk, sv) in slots {
                        let value = upgrade_card_node(sv, rekey, ctx);
                        s.insert(sk.clone(), value);
                    }
                    Value::Object(s)
                }
                ("template", _) => rekey_template(v, rekey),
                ("visible", _) => as_presence(&dyn_(v, ctx), Column::Visible),
                ("accessibility", _) => upgrade_accessibility(v, ctx),
                ("on", _) => upgrade_prop(k, v, component, false, rekey, ctx),
                _ => v.clone(),
            };
            out.insert(k.clone(), value);
        }
        Value::Object(out)
    })
}

/// §14.2: a nested card `{root, data}` → 1.0, `"version": "1.0"` first.
pub fn upgrade_card09(surface: &Value) -> Value {
    let Some(s) = surface.as_object() else { return surface.clone() };
    let (mut map, mut owners) = (IndexMap::new(), HashMap::new());
    let mut rekey = Rekey::new(&mut map, &mut owners);
    let mut ids = Vec::new();
    if let Some(root) = s.get("root") {
        collect_card_ids(root, &mut ids);
    }
    rekey.define(&ids);
    let ctx = Ctx::plain();
    let mut rest = Obj::new();
    for (k, v) in s {
        match k.as_str() {
            "version" => {}
            "root" => {
                let root = upgrade_card_node(v, &mut rekey, &ctx);
                rest.insert(k.clone(), root);
            }
            _ => {
                rest.insert(k.clone(), v.clone());
            }
        }
    }
    with_version(rest)
}

// ---------------------------------------------------------------------------
// Packages (formatVersion 5 → 6)

/// `^[a-z][a-z0-9]*(_[a-z0-9]+)*$` (the v6 tool `idPattern`).
fn is_tool_id_v6(id: &str) -> bool {
    let mut chars = id.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }
    let mut prev_underscore = false;
    for c in chars {
        if c == '_' {
            if prev_underscore {
                return false;
            }
            prev_underscore = true;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_underscore = false;
        } else {
            return false;
        }
    }
    !prev_underscore
}

/// `^[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z0-9_-]+)+$`: a dotted 0.9 function name.
fn is_function_name09(s: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    let mut parts = s.split('.');
    let Some(first) = parts.next() else { return false };
    let mut first_chars = first.chars();
    if !first_chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') || !first_chars.all(word) {
        return false;
    }
    let mut more = 0;
    for p in parts {
        if p.is_empty() || !p.chars().all(word) {
            return false;
        }
        more += 1;
    }
    more > 0
}

fn tool_base(id: &str) -> String {
    let mut flat = String::new();
    for c in mangle(id).chars() {
        if c == '_' && flat.ends_with('_') {
            continue;
        }
        flat.push(c);
    }
    let flat = flat.trim_matches('_');
    if flat.is_empty() {
        "_".into()
    } else {
        flat.to_string()
    }
}

fn rekey_tools(ids: &[String]) -> ToolMap {
    let mut map = ToolMap::new();
    let kept: HashSet<&str> = ids.iter().filter(|id| is_tool_id_v6(id)).map(String::as_str).collect();
    let mut taken: HashSet<String> = kept.iter().map(|s| s.to_string()).collect();
    for id in ids {
        if kept.contains(id.as_str()) {
            map.insert(id.clone(), id.clone());
            continue;
        }
        let base = tool_base(id);
        let mut target = base.clone();
        let mut n = 2;
        while taken.contains(&target) {
            target = format!("{base}_{n}");
            n += 1;
        }
        taken.insert(target.clone());
        map.insert(id.clone(), target);
    }
    map
}

fn rename_expect(value: &Value, ctx: &Ctx) -> Value {
    crate::deep(|| match value {
        Value::String(s) if is_function_name09(s) => Value::String(rename_with(s, &ctx.tools)),
        Value::Array(items) => Value::Array(items.iter().map(|v| rename_expect(v, ctx)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), rename_expect(v, ctx))).collect()),
        other => other.clone(),
    })
}

fn step_key(k: &str) -> &str {
    match k {
        "records.upsert" => "records_upsert",
        "records.delete" => "records_delete",
        other => other,
    }
}

fn upgrade_actions(actions: &Value, ctx: &Ctx) -> Value {
    let Some(actions) = actions.as_object() else { return actions.clone() };
    let mut out = Obj::new();
    for (name, action) in actions {
        let next = match action.as_object() {
            Some(a) if a.get("steps").is_some_and(Value::is_array) => {
                let mut next = Obj::new();
                for (k, v) in a {
                    let value = if k == "steps" {
                        Value::Array(
                            v.as_array()
                                .unwrap()
                                .iter()
                                .map(|step| match step.as_object() {
                                    Some(s) => Value::Object(
                                        s.iter()
                                            .map(|(sk, sv)| match (sk.as_str(), sv) {
                                                ("call", Value::String(c)) => (sk.clone(), Value::String(rename_with(c, &ctx.tools))),
                                                ("call", _) => (sk.clone(), sv.clone()),
                                                _ => (step_key(sk).to_string(), sv.clone()),
                                            })
                                            .collect(),
                                    ),
                                    None => step.clone(),
                                })
                                .collect(),
                        )
                    } else {
                        v.clone()
                    };
                    next.insert(k.clone(), value);
                }
                Value::Object(next)
            }
            _ => action.clone(),
        };
        out.insert(name.clone(), next);
    }
    Value::Object(out)
}

fn upgrade_exposes(exposes: &Value, ctx: &Ctx) -> Value {
    let Some(items) = exposes.as_array() else { return exposes.clone() };
    Value::Array(
        items
            .iter()
            .map(|expose| {
                let action = expose.get("run").and_then(Value::as_object).and_then(|r| r.get("action")).and_then(Value::as_str);
                let (Some(e), Some(action)) = (expose.as_object(), action) else { return expose.clone() };
                let mut out = Obj::new();
                for (k, v) in e {
                    let value = if k == "run" {
                        let mut run = v.as_object().unwrap().clone();
                        run.insert("action".into(), Value::String(rename_with(action, &ctx.tools)));
                        Value::Object(run)
                    } else {
                        v.clone()
                    };
                    out.insert(k.clone(), value);
                }
                Value::Object(out)
            })
            .collect(),
    )
}

/// §14.3 Packages, steps 2–5, over a package OBJECT (`signature` dropped,
/// `author` kept). A package that does not name format 5 is returned
/// unchanged.
pub fn upgrade_package_body09(pkg: &Value) -> Value {
    let Some(p) = pkg.as_object() else { return pkg.clone() };
    if p.get("formatVersion").and_then(Value::as_f64) != Some(5.0) {
        return pkg.clone();
    }
    let tool_ids: Vec<String> = p.get("tools").and_then(Value::as_object).map(|t| t.keys().cloned().collect()).unwrap_or_default();
    let ctx = Ctx { tools: rekey_tools(&tool_ids) };
    let catalog_id = p.get("catalogId").map(upgrade_catalog_id);
    let mut out = Obj::new();
    for (k, v) in p {
        let value = match (k.as_str(), v) {
            ("signature", _) => continue,
            ("formatVersion", _) => json!(6),
            ("catalogId", _) => catalog_id.clone().unwrap_or(Value::Null),
            ("templates", Value::Object(templates)) => {
                let mut ts = Obj::new();
                for (id, template) in templates {
                    let t = match template.as_object() {
                        Some(t) => {
                            let mut state = Upgrade09State::default();
                            let mut nt = Obj::new();
                            for (tk, tv) in t {
                                let value = if tk == "components" { upgrade_components_with(tv, catalog_id.as_ref(), &mut state, "", &ctx) } else { tv.clone() };
                                nt.insert(tk.clone(), value);
                            }
                            Value::Object(nt)
                        }
                        None => template.clone(),
                    };
                    ts.insert(id.clone(), t);
                }
                Value::Object(ts)
            }
            ("functions", Value::Array(items)) => {
                Value::Array(items.iter().map(|f| match f.as_str() { Some(s) => Value::String(rename_pattern_with(s, &ctx.tools)), None => f.clone() }).collect())
            }
            ("actions", _) => upgrade_actions(v, &ctx),
            ("exposes", _) => upgrade_exposes(v, &ctx),
            ("tests", Value::Array(tests)) => Value::Array(
                tests
                    .iter()
                    .map(|t| match t.as_object() {
                        Some(o) if o.contains_key("expect") => {
                            let mut o = o.clone();
                            let expect = rename_expect(&o["expect"], &ctx);
                            o.insert("expect".into(), expect);
                            Value::Object(o)
                        }
                        _ => t.clone(),
                    })
                    .collect(),
            ),
            ("tools", Value::Object(tools)) => {
                Value::Object(tools.iter().map(|(id, tool)| (ctx.tools.get(id).cloned().unwrap_or_else(|| id.clone()), tool.clone())).collect())
            }
            _ => v.clone(),
        };
        out.insert(k.clone(), value);
    }
    Value::Object(out)
}

/// What a host's `verifyPackageFile` answered on the original. The core
/// holds no Ed25519 (`host::signature` builds the signed bytes only): the
/// host verifies.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageVerification {
    /// `unsigned` | `valid` | `invalid`
    pub status: String,
    /// The verified author (`valid` only).
    pub author: Option<Value>,
}

/// What [`upgrade_package09`] answers.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageUpgrade {
    /// The formatVersion 6 package; `None` when the original is `invalid`.
    pub package: Option<Value>,
    /// `unsigned` | `valid` | `invalid` (the ORIGINAL's).
    pub status: String,
    /// The verified author (`valid` only).
    pub author: Option<Value>,
}

/// §14.3 Packages: `verify` runs on the ORIGINAL; `invalid` → no package;
/// else the upgraded body with the original's status (`valid` keeps the
/// verified author).
pub fn upgrade_package09(file: &Value, verify: impl FnOnce(&Value) -> PackageVerification) -> PackageUpgrade {
    let verdict = verify(file);
    match verdict.status.as_str() {
        "invalid" => PackageUpgrade { package: None, status: verdict.status, author: None },
        "valid" => PackageUpgrade { package: Some(upgrade_package_body09(file)), status: verdict.status, author: verdict.author },
        _ => PackageUpgrade { package: Some(upgrade_package_body09(file)), status: "unsigned".into(), author: None },
    }
}

/// The verification a core without Ed25519 can do on its own: no
/// `signature` = `unsigned`, else `invalid` (a host verifies and passes its
/// answer to [`upgrade09_json`] as `verification`).
pub fn structural_verification(file: &Value) -> PackageVerification {
    let status = if file.get("signature").is_none() { "unsigned" } else { "invalid" };
    PackageVerification { status: status.into(), author: None }
}

/// The FFI's one entry (`upgrade09Json`):
/// - `{kind: "message", input, state?}` → `{messages, state}`;
/// - `{kind: "stream", inputs, state?}` → `{messages, state}` (every output in order);
/// - `{kind: "client", input}` → the 1.0 message;
/// - `{kind: "card", input}` → the 1.0 card;
/// - `{kind: "package", input, verification?: {status, author?}}` →
///   `{package, status, author?}` (`package` null when invalid; without
///   `verification`: [`structural_verification`]).
pub fn upgrade09_json(request: &Value) -> Value {
    let input = request.get("input").cloned().unwrap_or(Value::Null);
    let state: Upgrade09State = request.get("state").and_then(|s| serde_json::from_value(s.clone()).ok()).unwrap_or_default();
    match request.get("kind").and_then(Value::as_str) {
        Some("message") => {
            let (messages, state) = upgrade_message09(&input, &state);
            json!({"messages": messages, "state": state})
        }
        Some("stream") => {
            let mut state = state;
            let mut messages = Vec::new();
            for m in request.get("inputs").and_then(Value::as_array).into_iter().flatten() {
                messages.extend(upgrade_message09_in(m, &mut state));
            }
            json!({"messages": messages, "state": state})
        }
        Some("client") => upgrade_client_message09(&input),
        Some("card") => upgrade_card09(&input),
        Some("package") => {
            let given = request.get("verification").and_then(Value::as_object).map(|v| PackageVerification {
                status: v.get("status").and_then(Value::as_str).unwrap_or("invalid").to_string(),
                author: v.get("author").cloned(),
            });
            let u = upgrade_package09(&input, |f| given.unwrap_or_else(|| structural_verification(f)));
            let mut out = Obj::new();
            out.insert("package".into(), u.package.unwrap_or(Value::Null));
            out.insert("status".into(), Value::String(u.status));
            if let Some(a) = u.author {
                out.insert("author".into(), a);
            }
            Value::Object(out)
        }
        _ => json!({"error": "kind is message, stream, client, card or package"}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(m: Value) -> (Value, Upgrade09State) {
        let (out, state) = upgrade_message09(&m, &Upgrade09State::default());
        assert_eq!(out.len(), 1);
        (out.into_iter().next().unwrap(), state)
    }

    fn keys(v: &Value) -> Vec<String> {
        v.as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn version_detection() {
        assert!(is_v09(&json!({"createSurface": {}})));
        assert!(is_v09(&json!({"version": "v0.9"})));
        assert!(!is_v09(&json!({"version": "1.0"})));
    }

    #[test]
    fn rename_table() {
        for (from, to) in [
            ("app.navigate", "app_navigate"),
            ("app.toast", "app_toast"),
            ("records.upsert", "records_upsert"),
            ("actions.log", "actions_log"),
            ("harness.records.upsert", "harness_records_upsert"),
            ("exponential.issues.update_status", "exponential_issues_update_status"),
            ("camera.capture", "camera_capture"),
            ("clipboard.read", "clipboard_read"),
            ("files.open", "files_open"),
            ("http.fetch", "http_fetch"),
            ("location.current", "location_current"),
            ("notifications.post", "notifications_post"),
            ("photos.pick", "photos_pick"),
            ("print.pdf", "print_pdf"),
            ("share.send", "share_send"),
            ("tools.weather", "tools_weather"),
            ("tools.my-tool", "tools_my_tool"),
            ("tools.weather.get-forecast", "tools_weather__get_forecast"),
            ("tools.crm.a.b", "tools_crm__a_b"),
            ("upstream.crm.search_customers", "upstream_crm__search_customers"),
            ("upstream.my-crm.list.all", "upstream_my_crm__list_all"),
            ("openUrl", "openUrl"),
            ("set", "set"),
            ("formatDate", "formatDate"),
            ("acme.x", "acme.x"),
            ("harness.*", "harness_*"),
            ("exponential.issues.*", "exponential_issues_*"),
            ("tools.crm.*", "tools_crm__*"),
            ("tools.*", "tools_*"),
            ("upstream.crm.*", "upstream_crm__*"),
            ("acme.*", "acme.*"),
            ("*", "*"),
        ] {
            assert_eq!(rename09(from), to, "{from}");
        }
    }

    #[test]
    fn envelope_version_first() {
        let (out, _) = msg(json!({"version": "v0.9", "deleteSurface": {"surfaceId": "s"}}));
        assert_eq!(keys(&out), ["version", "deleteSurface"]);
        assert_eq!(out["version"], "1.0");
        let (out, _) = msg(json!({"deleteSurface": {"surfaceId": "s"}}));
        assert_eq!(keys(&out), ["version", "deleteSurface"]);
    }

    #[test]
    fn create_surface_theme_and_catalog() {
        let (out, state) = msg(json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://a2ui.org/specification/v0_9/basic_catalog.json", "theme": "neutral", "sendDataModel": true}}));
        assert_eq!(
            out,
            json!({"version": "1.0", "createSurface": {"surfaceId": "s", "catalogId": BASIC_CATALOG_ID_10, "sendDataModel": true, "metadata": {"extensions": {"at_exponential_theme": "neutral"}}}})
        );
        assert_eq!(keys(&out["createSurface"]), ["surfaceId", "catalogId", "sendDataModel", "metadata"]);
        assert_eq!(serde_json::to_value(&state).unwrap(), json!({"surfaces": {}, "catalogs": {"s": BASIC_CATALOG_ID_10}}));
        let (out, _) = msg(json!({"createSurface": {"surfaceId": "s", "catalogId": "https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json", "metadata": {"extensions": {"x": 1, "at_exponential_theme": "old"}}, "theme": "neutral"}}));
        assert_eq!(out["createSurface"]["catalogId"], BASIC_CATALOG_ID_10);
        assert_eq!(out["createSurface"]["metadata"], json!({"extensions": {"x": 1, "at_exponential_theme": "neutral"}}));
        assert_eq!(keys(&out["createSurface"]), ["surfaceId", "catalogId", "metadata"]);
    }

    #[test]
    fn update_data_model_value_null() {
        let (out, _) = msg(json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s", "path": "/a"}}));
        assert_eq!(out["updateDataModel"], json!({"surfaceId": "s", "path": "/a", "value": null}));
        let (out, _) = msg(json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s", "value": 1}}));
        assert_eq!(out["updateDataModel"], json!({"surfaceId": "s", "value": 1}));
    }

    #[test]
    fn host_internal_only_version() {
        let m = json!({"version": "v0.9", "applyTemplate": {"surfaceId": "s", "templateId": "t", "data": {"a": {"path": "/x"}}}});
        let (out, _) = msg(m.clone());
        assert_eq!(out["applyTemplate"], m["applyTemplate"]);
        let (out, _) = msg(json!({"version": "v0.9", "bindDataModel": {"surfaceId": "s", "path": "/a", "source": "records:x"}}));
        assert_eq!(out["version"], "1.0");
    }

    fn components(list: Value) -> (Value, Upgrade09State) {
        let (out, state) = msg(json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": list}}));
        (out["updateComponents"]["components"].clone(), state)
    }

    #[test]
    fn dyn_forms() {
        let (out, _) = components(json!([{"id": "root", "component": "Text",
            "text": {"path": "/name"},
            "tone": {"call": "cond", "args": {"if": {"path": "/x"}, "then": "a"}, "returnType": "string"},
            "lines": {"path": "/a", "x": 1},
            "align": {"@type": "x", "@@y": 2, "v": [{"path": "/p"}]},
            "style": {"color": {"path": "/c"}}}]));
        assert_eq!(out[0]["text"], json!({"@path": "/name"}));
        assert_eq!(out[0]["tone"], json!({"@call": "cond", "args": {"if": {"@path": "/x"}, "then": "a"}}));
        assert_eq!(out[0]["lines"], json!({"path": "/a", "x": 1}));
        assert_eq!(out[0]["align"], json!({"@@type": "x", "@@@y": 2, "v": [{"@path": "/p"}]}));
        assert_eq!(out[0]["style"], json!({"color": {"path": "/c"}}));
        let (out, _) = components(json!([{"id": "b", "component": "Button", "on": {"press": {"event": {"name": "go", "context": {"v": {"path": "/v"}}, "userMessage": {"path": "/m"}}, "functionCall": {"call": "actions.log", "args": {"x": {"path": "/x"}}}}}}]));
        assert_eq!(out[0]["on"], json!({"press": {"event": {"name": "go", "context": {"v": {"@path": "/v"}}, "userMessage": {"@path": "/m"}}, "functionCall": {"@call": "actions_log", "args": {"x": {"@path": "/x"}}}}}));
    }

    #[test]
    fn data_props_are_never_dyned() {
        let (out, _) = components(json!([{"id": "t", "component": "Table", "rows": [{"a": {"path": "/x"}}], "columns": [{"key": {"path": "/k"}}]}]));
        assert_eq!(out[0]["rows"], json!([{"a": {"path": "/x"}}]));
        assert_eq!(out[0]["columns"], json!([{"key": {"@path": "/k"}}]));
    }

    #[test]
    fn presence_wraps() {
        let c = |x: Value| json!({"@call": "cond", "args": {"if": x, "then": true, "else": false}});
        let (out, _) = components(json!([
            {"id": "a", "component": "Text", "visible": true},
            {"id": "b", "component": "Text", "visible": 0},
            {"id": "c", "component": "Text", "visible": ""},
            {"id": "d", "component": "Text", "visible": {"path": "/x"}},
            {"id": "e", "component": "Text", "visible": {"call": "eq", "args": {"a": 1, "b": 1}}},
            {"id": "f", "component": "Text", "visible": {"call": "len", "args": {"value": [1]}}},
            {"id": "g", "component": "Input", "checks": [
                {"condition": true, "message": "m"},
                {"condition": "x", "message": "m"},
                {"condition": {"path": "/v"}, "message": "m"},
                {"condition": {"call": "required", "args": {"value": {"path": "/v"}}}, "message": "m"},
                {"condition": {"call": "concat", "args": {"values": []}}, "message": "m"}]},
            {"id": "h", "component": "Text", "visible": {"call": "and", "args": {"values": [true, 0, "", {"path": "/a"}, {"call": "lt", "args": {"a": 1, "b": 2}}, {"call": "fill", "args": {}}]}}},
            {"id": "i", "component": "Text", "visible": {"call": "not", "args": {"value": {"path": "/a"}}}}]));
        assert_eq!(out[0]["visible"], json!(true));
        assert_eq!(out[1]["visible"], json!(true));
        assert_eq!(out[2]["visible"], json!(false));
        assert_eq!(out[3]["visible"], c(json!({"@path": "/x"})));
        assert_eq!(out[4]["visible"], json!({"@call": "eq", "args": {"a": 1, "b": 1}}));
        assert_eq!(out[5]["visible"], c(json!({"@call": "len", "args": {"value": [1]}})));
        let checks = &out[6]["checks"];
        assert_eq!(checks[0]["condition"], c(json!(true)));
        assert_eq!(checks[1]["condition"], c(json!("x")));
        assert_eq!(checks[2]["condition"], c(json!({"@path": "/v"})));
        assert_eq!(checks[3]["condition"], json!({"@call": "required", "args": {"value": {"@path": "/v"}}}));
        assert_eq!(checks[4]["condition"], c(json!({"@call": "concat", "args": {"values": []}})));
        assert_eq!(checks[4]["message"], "m");
        assert_eq!(
            out[7]["visible"],
            json!({"@call": "and", "args": {"values": [true, true, false, c(json!({"@path": "/a"})), {"@call": "lt", "args": {"a": 1, "b": 2}}, c(json!({"@call": "fill", "args": {}}))]}})
        );
        assert_eq!(out[8]["visible"], json!({"@call": "not", "args": {"value": c(json!({"@path": "/a"}))}}));
    }

    #[test]
    fn accessibility_label_and_description() {
        let (out, _) = components(json!([{"id": "a", "component": "Text", "accessibility": {"label": {"path": "/l"}, "description": "d", "live": "polite"}}]));
        assert_eq!(out[0]["accessibility"], json!({"label": {"@path": "/l"}, "description": "d", "live": "polite"}));
    }

    #[test]
    fn rekey_rules() {
        let (out, state) = components(json!([
            {"id": "root", "component": "Stack", "children": ["user-card", "user_card", "1st", "a.b", "ghost-x"], "slots": {"head": "ab c"}},
            {"id": "user-card", "component": "Text", "text": "x"},
            {"id": "user_card", "component": "Text", "text": "y"},
            {"id": "1st", "component": "Text", "text": "z"},
            {"id": "a.b", "component": "Text", "text": "z"},
            {"id": "ab c", "component": "Text", "text": "z"},
            {"id": "l", "component": "Label", "for": "user-card", "text": "L"},
            {"id": "list", "component": "List", "children": {"componentId": "row-t", "path": "/rows"}}]));
        assert_eq!(out[0]["children"], json!(["user_card_2", "user_card", "_1st", "a_b", "ghost_x"]));
        assert_eq!(out[0]["slots"], json!({"head": "ab_c"}));
        assert_eq!(out[1]["id"], "user_card_2");
        assert_eq!(out[2]["id"], "user_card");
        assert_eq!(out[6]["for"], "user_card_2");
        assert_eq!(out[7]["children"], json!({"componentId": "row_t", "path": "/rows"}));
        let map = &state.surfaces["s"];
        assert_eq!(map["user-card"], "user_card_2");
        assert_eq!(map["ghost-x"], "ghost_x");
        assert_eq!(map["root"], "root");
        // Cross-message: an identifier that is a different original's target
        // is rewritten; a known original keeps its target.
        let (out, _) = upgrade_message09(
            &json!({"updateComponents": {"surfaceId": "s", "components": [{"id": "ghost_x", "component": "Text", "text": "g"}, {"id": "user-card", "component": "Text", "text": "x"}]}}),
            &state,
        );
        assert_eq!(out[0]["updateComponents"]["components"][0]["id"], "ghost_x_2");
        assert_eq!(out[0]["updateComponents"]["components"][1]["id"], "user_card_2");
    }

    fn basic(list: Value) -> Vec<Value> {
        let state = Upgrade09State::default();
        let (_, state) = upgrade_message09(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://a2ui.org/specification/v0_9/basic_catalog.json"}}), &state);
        let (out, _) = upgrade_message09(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": list}}), &state);
        out[0]["updateComponents"]["components"].as_array().unwrap().clone()
    }

    #[test]
    fn basic_icon_and_text() {
        let out = basic(json!([
            {"id": "i", "component": "Icon", "name": {"path": "M0 0L1 1"}},
            {"id": "j", "component": "Icon", "name": "home"},
            {"id": "t1", "component": "Text", "text": "Hi", "variant": "h1"},
            {"id": "t3", "component": "Text", "variant": "h3", "text": {"path": "/t"}},
            {"id": "t5", "component": "Text", "text": "x", "variant": "h5"},
            {"id": "tc", "component": "Text", "text": "x", "variant": "caption"}]));
        assert_eq!(out[0]["name"], json!({"svgPath": "M0 0L1 1"}));
        assert_eq!(out[1]["name"], "home");
        assert_eq!(out[2], json!({"id": "t1", "component": "Text", "text": "# Hi"}));
        assert_eq!(out[3], json!({"id": "t3", "component": "Text", "text": {"@call": "concat", "args": {"values": ["### ", {"@path": "/t"}]}}}));
        assert_eq!(out[4]["text"], "##### x");
        assert_eq!(out[5]["variant"], "caption");
    }

    #[test]
    fn basic_text_field() {
        let out = basic(json!([
            {"id": "f-1", "component": "TextField", "label": "A", "value": {"path": "/a"}, "validationRegexp": "^a$"},
            {"id": "f-2", "component": "TextField", "label": "B", "validationRegexp": "^b$", "checks": [{"condition": {"call": "required", "args": {"value": {"path": "/b"}}}, "message": "req"}]},
            {"id": "f3", "component": "TextField", "label": "C"}]));
        assert_eq!(
            out[0],
            json!({"id": "f_1", "component": "TextField", "label": "A", "value": {"@path": "/a"},
                "checks": [{"condition": {"@call": "regex", "args": {"value": {"@path": "/a"}, "pattern": "^a$"}}, "message": "Invalid value"}]})
        );
        assert_eq!(keys(&out[0]), ["id", "component", "label", "value", "checks"]);
        assert_eq!(out[1]["checks"][1], json!({"condition": {"@call": "regex", "args": {"value": {"@path": "f-2"}, "pattern": "^b$"}}, "message": "Invalid value"}));
        assert_eq!(keys(&out[1]), ["id", "component", "label", "checks", "value"]);
        assert_eq!(out[1]["value"], "");
        assert_eq!(keys(&out[2]), ["id", "component", "label", "value"]);
    }

    #[test]
    fn basic_references() {
        let out = basic(json!([
            {"id": "root", "component": "Card", "child": "t-1"},
            {"id": "tabs", "component": "Tabs", "tabs": [{"title": {"path": "/t"}, "child": "t-1"}]},
            {"id": "m", "component": "Modal", "trigger": "b-1", "content": "t-1"},
            {"id": "b-1", "component": "Button", "child": "t-1", "action": {"event": {"name": "x", "context": {"a": {"path": "/a"}}}}}]));
        assert_eq!(out[0]["child"], "t_1");
        assert_eq!(out[1]["tabs"], json!([{"title": {"@path": "/t"}, "child": "t_1"}]));
        assert_eq!(out[2]["trigger"], "b_1");
        assert_eq!(out[2]["content"], "t_1");
        assert_eq!(out[3]["action"], json!({"event": {"name": "x", "context": {"a": {"@path": "/a"}}}}));
    }

    #[test]
    fn client_action_and_error() {
        let out = upgrade_client_message09(&json!({"version": "v0.9", "action": {"name": "pick", "surfaceId": "s1", "sourceComponentId": "row-btn.a1", "timestamp": "t", "context": {"id": "a1"}, "payload": {"value": "b"}}}));
        assert_eq!(
            out,
            json!({"version": "1.0", "action": {"name": "pick", "surfaceId": "s1", "sourceComponentId": "row_btn", "timestamp": "t", "context": {"id": "a1"},
                "metadata": {"extensions": {"at_exponential_instance": "row-btn.a1", "at_exponential_payload": {"value": "b"}}}}})
        );
        assert_eq!(keys(&out["action"]), ["name", "surfaceId", "sourceComponentId", "timestamp", "context", "metadata"]);
        let out = upgrade_client_message09(&json!({"version": "v0.9", "action": {"name": "go", "surfaceId": "s", "sourceComponentId": "b", "timestamp": "t", "context": {}, "payload": {}}}));
        assert_eq!(out["action"], json!({"name": "go", "surfaceId": "s", "sourceComponentId": "b", "timestamp": "t", "context": {}}));
        for (from, to) in [("FUNCTION_NOT_FOUND", "UNKNOWN_FUNCTION"), ("FUNCTION_DENIED", "INVALID_FUNCTION_CALL"), ("ACTION_FAILED", "EXECUTION_ERROR"), ("VALIDATION_FAILED", "VALIDATION_FAILED")] {
            let out = upgrade_client_message09(&json!({"version": "v0.9", "error": {"code": from, "surfaceId": "s", "message": "m"}}));
            assert_eq!(out, json!({"version": "1.0", "error": {"code": to, "surfaceId": "s", "message": "m"}}));
        }
    }

    #[test]
    fn card() {
        let out = upgrade_card09(&json!({"root": {"id": "s", "component": "Stack", "children": [
            {"id": "r-1", "component": "Row", "props": {"title": {"path": "/title"}}},
            {"id": "b", "component": "Button", "props": {"label": "Ship"}, "visible": {"path": "/v"},
             "on": {"press": {"event": {"name": "reply", "context": {"text": "Ship"}}}}},
            {"id": "l", "component": "List", "template": {"component": "r-1", "path": "/rows"}},
            {"id": "p", "component": "Popover", "props": {"anchor": "b-2"}, "slots": {"trigger": {"id": "b-2", "component": "Button", "props": {"label": "x"}}}}]},
            "data": {"title": "T"}}));
        assert_eq!(keys(&out), ["version", "root", "data"]);
        let kids = &out["root"]["children"];
        assert_eq!(kids[0], json!({"id": "r_1", "component": "Row", "props": {"title": {"@path": "/title"}}}));
        assert_eq!(kids[1]["visible"], json!({"@call": "cond", "args": {"if": {"@path": "/v"}, "then": true, "else": false}}));
        assert_eq!(kids[1]["on"], json!({"press": {"event": {"name": "reply", "context": {"text": "Ship"}}}}));
        assert_eq!(kids[2]["template"], json!({"component": "r_1", "path": "/rows"}));
        assert_eq!(kids[3]["props"]["anchor"], "b_2");
        assert_eq!(kids[3]["slots"]["trigger"]["id"], "b_2");
        assert_eq!(out["data"], json!({"title": "T"}));
    }

    #[test]
    fn card_preorder_sorted_slots_then_children() {
        let out = upgrade_card09(&json!({"root": {"id": "s", "component": "Stack", "children": [{"id": "a.b", "component": "Text", "props": {"text": "1"}}],
            "slots": {"z": {"id": "a b", "component": "Text", "props": {"text": "2"}}, "a": {"id": "a-b", "component": "Text", "props": {"text": "3"}}}}}));
        assert_eq!(out["root"]["slots"]["a"]["id"], "a_b");
        assert_eq!(out["root"]["slots"]["z"]["id"], "a_b_2");
        assert_eq!(out["root"]["children"][0]["id"], "a_b_3");
        assert_eq!(keys(&out["root"]["slots"]), ["z", "a"]);
    }

    fn unsigned(_: &Value) -> PackageVerification {
        PackageVerification { status: "unsigned".into(), author: None }
    }

    #[test]
    fn package() {
        let pkg = json!({"formatVersion": 5, "id": "acme.hours", "name": "H", "version": "1.0.0",
            "catalogId": "https://ui.exponential.at/catalogs/core/v1",
            "templates": {
                "t": {"components": [{"id": "root", "component": "Stack", "children": ["b-1"]},
                    {"id": "b-1", "component": "Button", "label": "Go", "on": {"press": {"functionCall": {"call": "tools.my-tool.get-x", "args": {"a": {"path": "/a"}}}}}}],
                    "data": {"a": 1}, "bindings": [{"path": "/e", "source": "records:entry"}]},
                "u": {"components": [{"id": "b-1", "component": "Text", "text": "x"}]}},
            "functions": ["records.*", "tools.my-tool.*", "actions.log", "openUrl"],
            "tools": {"my-tool": {"kind": "mcp", "title": "T", "url": "https://x"}, "my_tool": {"kind": "script", "title": "S", "command": "x"}, "_odd__x_": {"kind": "script", "title": "O", "command": "y"}},
            "actions": {"log": {"steps": [{"require": "true", "message": "m"}, {"records.upsert": {"kind": "entry", "data": {}}}, {"records.delete": {"id": "params.id"}}, {"call": "harness.toast", "args": {}}]}},
            "exposes": [{"name": "c", "run": {"action": "exponential.comments.create", "args": {}}}],
            "tests": [{"name": "t", "action": {"name": "log"}, "expect": {"calls": [{"name": "harness.toast"}], "x": "plain", "y": "acme.x"}}]});
        let u = upgrade_package09(&pkg, unsigned);
        assert_eq!(u.status, "unsigned");
        let p = u.package.unwrap();
        assert_eq!(p["formatVersion"], 6);
        let tpl = &p["templates"]["t"]["components"];
        assert_eq!(tpl[0]["children"], json!(["b_1"]));
        assert_eq!(tpl[1]["on"]["press"]["functionCall"], json!({"@call": "tools_my_tool_2__get_x", "args": {"a": {"@path": "/a"}}}));
        // A fresh state per template.
        assert_eq!(p["templates"]["u"]["components"][0]["id"], "b_1");
        assert_eq!(p["templates"]["t"]["bindings"], pkg["templates"]["t"]["bindings"]);
        assert_eq!(keys(&p["tools"]), ["my_tool_2", "my_tool", "odd_x"]);
        assert_eq!(p["functions"], json!(["records_*", "tools_my_tool_2__*", "actions_log", "openUrl"]));
        assert_eq!(keys(&p["actions"]["log"]["steps"][1]), ["records_upsert"]);
        assert_eq!(keys(&p["actions"]["log"]["steps"][2]), ["records_delete"]);
        assert_eq!(p["actions"]["log"]["steps"][3]["call"], "harness_toast");
        assert_eq!(p["exposes"][0]["run"]["action"], "exponential_comments_create");
        assert_eq!(p["tests"][0]["expect"], json!({"calls": [{"name": "harness_toast"}], "x": "plain", "y": "acme.x"}));
        // Not format 5: unchanged.
        let v6 = json!({"formatVersion": 6, "id": "a"});
        assert_eq!(upgrade_package_body09(&v6), v6);
    }

    #[test]
    fn package_status() {
        let file = json!({"formatVersion": 5, "id": "a", "templates": {}, "author": {"name": "A", "key": "ed25519:k"}, "signature": {"alg": "ed25519", "value": "v"}});
        let u = upgrade_package09(&file, |_| PackageVerification { status: "invalid".into(), author: None });
        assert_eq!(u, PackageUpgrade { package: None, status: "invalid".into(), author: None });
        let u = upgrade_package09(&file, |f| PackageVerification { status: "valid".into(), author: f.get("author").cloned() });
        assert_eq!(u.status, "valid");
        assert_eq!(u.author, Some(json!({"name": "A", "key": "ed25519:k"})));
        assert_eq!(u.package.unwrap(), json!({"formatVersion": 6, "id": "a", "templates": {}, "author": {"name": "A", "key": "ed25519:k"}}));
        let mut claim = file.clone();
        claim.as_object_mut().unwrap().remove("signature");
        let out = upgrade09_json(&json!({"kind": "package", "input": claim}));
        assert_eq!(out["status"], "unsigned");
        assert!(out.get("author").is_none());
        assert_eq!(out["package"]["author"]["name"], "A");
        let out = upgrade09_json(&json!({"kind": "package", "input": file}));
        assert_eq!(out, json!({"package": null, "status": "invalid"}));
    }

    #[test]
    fn json_entry_stream() {
        let out = upgrade09_json(&json!({"kind": "stream", "inputs": [
            {"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://a2ui.org/specification/v0_9/basic_catalog.json"}},
            {"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [{"id": "f", "component": "TextField", "label": "x"}]}}]}));
        assert_eq!(out["messages"][1]["updateComponents"]["components"][0]["value"], "");
        assert_eq!(out["state"], json!({"surfaces": {"s": {"f": "f"}}, "catalogs": {"s": BASIC_CATALOG_ID_10}}));
    }
}
