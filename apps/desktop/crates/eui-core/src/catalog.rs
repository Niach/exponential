//! The catalog as data: `core.catalog.json` (embedded by the generator), the
//! catalog ids, token groups, and the merged VIEW of the core plus registered
//! extensions every lookup goes through. Mirrors `src/catalog.ts`.

use std::borrow::Cow;
use std::sync::{Arc, LazyLock};

use indexmap::IndexMap;
use serde_json::Value;

use crate::generated::catalog as g;
use crate::types::{CatalogSource, ComponentDef, DefSchema, ExtensionDef, MacroDef};

/// `https://ui.exponential.at/catalogs/core/v1`
pub const CORE_CATALOG_ID: &str = g::CATALOG_ID;
/// The prompt-sized subset: a lite surface is a valid core surface.
pub const CORE_LITE_CATALOG_ID: &str = g::LITE_CATALOG_ID;
/// The vendored A2UI basic catalog.
pub const A2UI_BASIC_CATALOG_ID: &str = g::BASIC_CATALOG_ID;
/// The A2UI wire `version` (round 9: `"1.0"`, `host.json` `a2uiVersion`).
pub const A2UI_VERSION: &str = g::A2UI_VERSION;
/// Round 9 §14.1: while true, every core entry upgrades 0.9 input
/// (`upgrade09`) instead of refusing it (`host.json` `transition.acceptV09`).
pub const ACCEPT_V09: bool = g::ACCEPT_V09;
/// The A2UI 0.9 wire version the bridge reads.
pub const A2UI_VERSION_09: &str = "v0.9";
/// The placeholder component painted for anything unmapped.
pub const UNKNOWN_COMPONENT: &str = g::UNKNOWN_COMPONENT;
/// The reserved container `createSurface` implies (round 9 §5.4).
pub const SURFACE_COMPONENT: &str = "Surface";
/// What a client advertises as `supportedCatalogIds`.
pub const SUPPORTED_CATALOG_IDS: &[&str] = &[CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, A2UI_BASIC_CATALOG_ID];

/// The parsed core catalog, once per process. VAPP-106: the RUNTIME catalog,
/// without the model-facing descriptions and examples (`descriptions()`).
pub static CORE: LazyLock<CatalogSource> =
    LazyLock::new(|| serde_json::from_str(g::CORE_CATALOG_JSON).expect("core.catalog.json"));

/// VAPP-106: the catalog's model-facing text (`generated/core.descriptions.json`
/// of the package: per component its description, example and prop
/// descriptions; per def; per function), keyed like the catalog, for a host
/// that builds its own prompt. Parsed once; only with the `descriptions`
/// feature.
#[cfg(feature = "descriptions")]
pub fn descriptions() -> &'static Value {
    static DESCRIPTIONS: LazyLock<Value> =
        LazyLock::new(|| serde_json::from_str(crate::generated::descriptions::CORE_CATALOG_DESCRIPTIONS_JSON).expect("core.descriptions.json"));
    &DESCRIPTIONS
}

#[derive(serde::Deserialize)]
struct MacrosFile {
    macros: IndexMap<String, MacroDef>,
}

/// The core macro table, once per process.
pub static CORE_MACROS: LazyLock<IndexMap<String, MacroDef>> =
    LazyLock::new(|| serde_json::from_str::<MacrosFile>(g::MACROS_JSON).expect("macros.json").macros);

/// Token groups → names, flattened (`type.size`, not nested), in source order.
pub static TOKEN_GROUPS: LazyLock<IndexMap<String, Vec<String>>> = LazyLock::new(|| {
    let file: IndexMap<String, Value> = serde_json::from_str(g::TOKENS_JSON).expect("tokens.json");
    let mut out = IndexMap::new();
    for (group, value) in file {
        if group.starts_with('$') {
            continue;
        }
        match value {
            Value::Array(names) => {
                out.insert(group, names.iter().filter_map(|v| v.as_str().map(str::to_string)).collect());
            }
            Value::Object(subs) => {
                // serde_json's maps are sorted; the source order comes from the
                // generated flat group list instead.
                let order: Vec<&str> = g::TOKEN_GROUPS.iter().filter_map(|k| k.strip_prefix(&format!("{group}."))).collect();
                let mut subs: Vec<(String, Value)> = subs.into_iter().collect();
                subs.sort_by_key(|(sub, _)| order.iter().position(|o| *o == sub).unwrap_or(usize::MAX));
                for (sub, names) in subs {
                    let names = names.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
                    out.insert(format!("{group}.{sub}"), names);
                }
            }
            _ => {}
        }
    }
    out
});

/// `$spacing.md` → `("spacing", "md")`, `$type.size.sm` → `("type.size", "sm")`;
/// `None` when the string is not a well-formed reference.
pub fn parse_token_ref_str(value: &str) -> Option<(String, String)> {
    let body = value.strip_prefix('$')?;
    let (group, name) = body.rsplit_once('.')?;
    let ident = |s: &str, first_lower: bool| {
        let mut chars = s.chars();
        let first = chars.next()?;
        let ok_first = if first_lower { first.is_ascii_lowercase() } else { first.is_ascii_alphanumeric() };
        if !ok_first || !chars.all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        Some(())
    };
    for segment in group.split('.') {
        ident(segment, true)?;
    }
    ident(name, false)?;
    Some((group.to_string(), name.to_string()))
}

/// [`parse_token_ref_str`] over a JSON value.
pub fn parse_token_ref(value: &Value) -> Option<(String, String)> {
    value.as_str().and_then(parse_token_ref_str)
}

/// True when the reference names a token group AND a name in it.
pub fn is_known_token(value: &Value) -> bool {
    match parse_token_ref(value) {
        Some((group, name)) => TOKEN_GROUPS.get(&group).is_some_and(|names| names.contains(&name)),
        None => false,
    }
}

/// The core merged with registered extensions. Extension names never shadow
/// a core name (`define_extension` refuses that), so a plain overlay is exact.
/// The tables BORROW the parsed core (`CORE`, `CORE_MACROS`) and copy only
/// when an extension writes into them: the core view clones nothing (#VAPP-281,
/// ~11.7k of the cold start's allocations).
#[derive(Debug, Clone)]
pub struct CatalogView {
    pub components: Cow<'static, IndexMap<String, ComponentDef>>,
    pub enums: Cow<'static, IndexMap<String, Vec<String>>>,
    pub defs: Cow<'static, IndexMap<String, DefSchema>>,
    pub macros: Cow<'static, IndexMap<String, MacroDef>>,
    /// The extension ids this view was built with, in registration order.
    pub extension_ids: Vec<String>,
    /// Extension component name → the extension id that defines it.
    pub owner: IndexMap<String, String>,
}

static CORE_VIEW: LazyLock<Arc<CatalogView>> = LazyLock::new(|| Arc::new(CatalogView::build(&[])));

impl CatalogView {
    fn build(extensions: &[ExtensionDef]) -> CatalogView {
        let core: &'static CatalogSource = &CORE;
        let mut view = CatalogView {
            components: Cow::Borrowed(&core.components),
            enums: Cow::Borrowed(&core.enums),
            defs: Cow::Borrowed(&core.defs),
            macros: Cow::Borrowed(&*CORE_MACROS),
            extension_ids: Vec::new(),
            owner: IndexMap::new(),
        };
        for ext in extensions {
            for (k, v) in &ext.components {
                view.components.to_mut().insert(k.clone(), v.clone());
                view.owner.insert(k.clone(), ext.id.clone());
            }
            for (k, v) in ext.enums.iter().flatten() {
                view.enums.to_mut().insert(k.clone(), v.clone());
            }
            for (k, v) in ext.defs.iter().flatten() {
                view.defs.to_mut().insert(k.clone(), v.clone());
            }
            for (k, v) in ext.macros.iter().flatten() {
                view.macros.to_mut().insert(k.clone(), v.clone());
            }
            view.extension_ids.push(ext.id.clone());
        }
        view
    }

    /// The core alone (shared, built once).
    pub fn core() -> Arc<CatalogView> {
        CORE_VIEW.clone()
    }

    /// The core plus the given extensions (the core alone when none).
    pub fn with(extensions: &[ExtensionDef]) -> Arc<CatalogView> {
        if extensions.is_empty() {
            Self::core()
        } else {
            Arc::new(Self::build(extensions))
        }
    }

    pub fn component(&self, name: &str) -> Option<&ComponentDef> {
        self.components.get(name)
    }

    /// Is `catalog_id` one this view can reduce (core, lite, basic, an extension)?
    pub fn knows_catalog(&self, catalog_id: &str) -> bool {
        catalog_id == CORE_CATALOG_ID
            || catalog_id == CORE_LITE_CATALOG_ID
            || catalog_id == A2UI_BASIC_CATALOG_ID
            || self.extension_ids.iter().any(|id| id == catalog_id)
    }

    /// Round 9 §5.5: the ids a surface may name by default: the core,
    /// core-lite, basic, then the extensions, de-duplicated.
    pub fn available_catalogs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for id in SUPPORTED_CATALOG_IDS.iter().copied().chain(self.extension_ids.iter().map(String::as_str)) {
            if !out.iter().any(|x| x == id) {
                out.push(id.to_string());
            }
        }
        out
    }

    /// Round 9 §5.5: a component name looked up ONLY in its resolved
    /// catalog: the core's components; core-lite = the `lite` subset; an
    /// extension = its components ∪ the core's. (Basic components are
    /// mapped first.)
    pub fn catalog_component(&self, catalog_id: &str, name: &str) -> Option<&ComponentDef> {
        if name == SURFACE_COMPONENT {
            return None;
        }
        if catalog_id == CORE_CATALOG_ID {
            return CORE.components.get(name);
        }
        if catalog_id == CORE_LITE_CATALOG_ID {
            return CORE.components.get(name).filter(|d| d.lite || d.hidden == Some(true));
        }
        if !self.extension_ids.iter().any(|id| id == catalog_id) {
            return None;
        }
        if self.owner.get(name).is_some_and(|o| o == catalog_id) {
            return self.components.get(name);
        }
        CORE.components.get(name)
    }

    /// The extension that owns `component`, if it is not a core one.
    pub fn extension_of(&self, component: &str) -> Option<&str> {
        if CORE.components.contains_key(component) {
            return None;
        }
        self.owner.get(component).map(String::as_str)
    }
}

/// Component names in source order, optionally the lite subset, never the
/// hidden placeholder.
pub fn component_names(lite: bool) -> Vec<String> {
    CORE.components
        .iter()
        .filter(|(_, def)| def.is_offered() && (!lite || def.lite))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The component definition in the core (no extensions).
pub fn component_def(name: &str) -> Option<&'static ComponentDef> {
    CORE.components.get(name)
}

/// Round 9 §3.2: is `<component>.<prop>` a DATA position of the core
/// catalog (`DATA_PROPS`: shape-less objects, Table `rows`)? Never
/// interpreted: no directive check, no resolution inside.
pub fn is_data_prop(component: &str, prop: &str) -> bool {
    g::DATA_PROPS.iter().any(|p| p.split_once('.').is_some_and(|(c, k)| c == component && k == prop))
}

// ---------------------------------------------------------------------------
// Round 9 §11: the structural checker (no JSON Schema engine), for inline
// catalogs. Mirrors `src/schema.ts checkCatalogDefinition` issue for issue.
// ---------------------------------------------------------------------------

const CATALOG_ROOT_KEYS: &[&str] = &["$schema", "$id", "protocolVersion", "title", "description", "catalogId", "metadata", "instructions", "components", "functions", "$defs"];
const COMMON_TYPE_REFS: &[&str] = &[
    "ComponentId", "Child", "ChildList", "DynamicString", "DynamicNumber", "DynamicBoolean", "DynamicStringList", "DynamicValue", "AccessibilityAttributes", "CheckRule", "Checkable", "Action", "DataBinding", "FunctionCall",
];
const RETURN_TYPES: &[&str] = &["string", "number", "boolean", "array", "object", "validationResult", "any", "void"];
const FUNCTION_KEYS: &[&str] = &["type", "description", "properties", "required", "returnType", "allowedCallers", "requiresUserActivation"];
const CALLERS: &[&str] = &["rendererOnly", "rendererOrAgent", "agentOnly"];

type Issues = Vec<String>;
type Obj = serde_json::Map<String, Value>;

fn add(issues: &mut Issues, at: &str, message: impl AsRef<str>) {
    issues.push(format!("{}: {}", if at.is_empty() { "/" } else { at }, message.as_ref()));
}

fn quoted(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

fn check_definition_metadata(value: &Value, at: &str, issues: &mut Issues) {
    let Some(o) = value.as_object() else { return add(issues, at, "an object") };
    for key in o.keys().filter(|k| *k != "extensions") {
        add(issues, at, format!("key {key} is not allowed (only extensions)"));
    }
    let Some(ext) = o.get("extensions") else { return };
    let here = format!("{at}/extensions");
    let Some(ext) = ext.as_object() else { return add(issues, &here, "an object") };
    for key in ext.keys().filter(|k| !crate::ident::is_identifier(k)) {
        add(issues, &here, format!("key {} is not an identifier (UAX #31)", quoted(key)));
    }
}

fn defines_component(schema: &Value) -> bool {
    let has = |s: &Value| s.get("properties").and_then(|p| p.get("component")).and_then(Value::as_object).is_some_and(|c| c.contains_key("const"));
    schema.is_object() && (has(schema) || schema.get("allOf").and_then(Value::as_array).is_some_and(|l| l.iter().any(has)))
}

fn check_component(name: &str, def: &Value, components: &Obj, flat: bool, issues: &mut Issues) {
    let at = format!("/components/{name}");
    if !crate::ident::is_identifier(name) {
        add(issues, &at, "component name is not an identifier (UAX #31)");
    }
    if name == SURFACE_COMPONENT {
        add(issues, &at, "Surface is reserved (§5.4)");
    }
    let Some(d) = def.as_object() else { return add(issues, &at, "a component definition is an object") };
    if d.get("type").and_then(Value::as_str) != Some("object") {
        add(issues, &format!("{at}/type"), "must be \"object\"");
    }
    if d.contains_key("oneOf") || d.contains_key("anyOf") {
        add(issues, &at, "no root oneOf/anyOf (flat)");
    }
    let mut props = d.get("properties").and_then(Value::as_object);
    let mut required = d.get("required");
    if let Some(all) = d.get("allOf") {
        for (i, part) in all.as_array().into_iter().flatten().enumerate() {
            if part.as_object().is_some_and(|p| p.len() == 1 && p.get("$ref").and_then(Value::as_str) == Some("common_types.json#/$defs/Checkable")) {
                continue;
            }
            if let (false, Some(inner), None) = (flat, part.get("properties").and_then(Value::as_object), props) {
                props = Some(inner);
                required = part.get("required");
                continue;
            }
            add(issues, &format!("{at}/allOf/{i}"), "the only allOf member allowed is common_types.json#/$defs/Checkable (flat)");
        }
    }
    let Some(props) = props else { return add(issues, &format!("{at}/properties"), "required (flat: the props sit on the definition)") };
    if props.get("component").and_then(|c| c.get("const")).and_then(Value::as_str) != Some(name) {
        add(issues, &format!("{at}/properties/component"), format!("must be {{\"const\": \"{name}\"}}"));
    }
    if !required.and_then(Value::as_array).is_some_and(|r| r.iter().any(|v| v == "component")) {
        add(issues, &format!("{at}/required"), "must list component");
    }
    for prop in props.keys().filter(|p| !crate::ident::is_identifier(p)) {
        add(issues, &format!("{at}/properties"), format!("prop {} is not an identifier (UAX #31)", quoted(prop)));
    }
    for key in ["allowedParents", "allowedChildren"] {
        let Some(list) = d.get(key) else { continue };
        let here = format!("{at}/{key}");
        let names: Option<Vec<&str>> = list.as_array().and_then(|l| l.iter().map(Value::as_str).collect());
        let unique = names.as_ref().is_some_and(|n| n.iter().collect::<std::collections::HashSet<_>>().len() == n.len());
        let (Some(names), true) = (names, unique) else {
            add(issues, &here, "an array of unique names");
            continue;
        };
        for other in names {
            if !components.contains_key(other) && !(key == "allowedParents" && other == SURFACE_COMPONENT) {
                add(issues, &here, format!("{other} is not a component of this catalog"));
            }
        }
    }
    if let Some(metadata) = d.get("metadata") {
        check_definition_metadata(metadata, &format!("{at}/metadata"), issues);
    }
}

fn check_function(name: &str, def: &Value, issues: &mut Issues) {
    let at = format!("/functions/{name}");
    if !crate::ident::is_identifier(name) {
        add(issues, &at, "function name is not an identifier (UAX #31)");
    }
    if crate::functions::has_host_prefix(name) {
        add(issues, &at, "function name has a reserved host prefix (§6.3)");
    }
    if name.contains("__") {
        add(issues, &at, "function name contains __ (§6.3)");
    }
    let Some(d) = def.as_object() else { return add(issues, &at, "a function definition is an object") };
    for key in d.keys().filter(|k| !FUNCTION_KEYS.contains(&k.as_str())) {
        add(issues, &at, format!("key {key} is not allowed in a function definition"));
    }
    if d.get("type").and_then(Value::as_str) != Some("object") {
        add(issues, &format!("{at}/type"), "must be \"object\"");
    }
    match d.get("properties").and_then(Value::as_object) {
        None => add(issues, &format!("{at}/properties"), "required"),
        Some(p) => {
            for key in p.keys().filter(|k| *k != "@call" && *k != "args") {
                add(issues, &format!("{at}/properties"), format!("key {key} is not allowed (only @call, args)"));
            }
            if p.get("@call").and_then(|c| c.get("const")).and_then(Value::as_str) != Some(name) {
                add(issues, &format!("{at}/properties/@call"), format!("must be {{\"const\": \"{name}\"}}"));
            }
            if let Some(args) = p.get("args") {
                if args.get("type").and_then(Value::as_str) != Some("object") {
                    add(issues, &format!("{at}/properties/args"), "an object schema");
                } else if let Some(list) = args.get("properties").and_then(Value::as_object) {
                    for arg in list.keys().filter(|a| !crate::ident::is_identifier(a)) {
                        add(issues, &format!("{at}/properties/args"), format!("arg {} is not an identifier (UAX #31)", quoted(arg)));
                    }
                }
            }
        }
    }
    if !d.get("required").and_then(Value::as_array).is_some_and(|r| r.iter().any(|v| v == "@call")) {
        add(issues, &format!("{at}/required"), "must list @call");
    }
    if !d.get("returnType").and_then(Value::as_str).is_some_and(|r| RETURN_TYPES.contains(&r)) {
        add(issues, &format!("{at}/returnType"), format!("one of {}", RETURN_TYPES.join("|")));
    }
    if let Some(c) = d.get("allowedCallers") {
        if !c.as_str().is_some_and(|c| CALLERS.contains(&c)) {
            add(issues, &format!("{at}/allowedCallers"), format!("one of {}", CALLERS.join("|")));
        }
    }
    if d.get("requiresUserActivation").is_some_and(|v| !v.is_boolean()) {
        add(issues, &format!("{at}/requiresUserActivation"), "a boolean");
    }
    if d.get("requiresUserActivation") == Some(&Value::Bool(true)) && d.get("allowedCallers").is_some_and(|c| c != "rendererOnly") {
        add(issues, &format!("{at}/allowedCallers"), "a function that requires user activation is rendererOnly");
    }
}

fn walk_refs(value: &Value, at: &str, visit: &mut dyn FnMut(&str, &str)) {
    match value {
        Value::Array(items) => items.iter().enumerate().for_each(|(i, v)| walk_refs(v, &format!("{at}/{i}"), visit)),
        Value::Object(o) => {
            for (key, v) in o {
                let here = format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (key.as_str(), v.as_str()) {
                    ("$ref", Some(target)) => visit(target, &here),
                    _ => walk_refs(v, &here, visit),
                }
            }
        }
        _ => {}
    }
}

/// Round 9 §11: every structural rule a 1.0 catalog definition must hold, as
/// issues (`<pointer>: <message>`); empty = conforming. `flat` = component
/// props sit at the definition's top level (ours); `false` = upstream's
/// style, props inside an `allOf` member (the vendored 1.0 basic catalog).
pub fn check_catalog_definition(doc: &Value, flat: bool) -> Vec<String> {
    let mut issues = Vec::new();
    let Some(root) = doc.as_object() else { return vec!["/: a catalog definition is an object".to_string()] };
    for key in root.keys().filter(|k| !CATALOG_ROOT_KEYS.contains(&k.as_str())) {
        add(&mut issues, "", format!("root key {key} is not allowed"));
    }
    if !root.get("catalogId").and_then(Value::as_str).is_some_and(|c| !c.is_empty()) {
        add(&mut issues, "/catalogId", "required, a non-empty string");
    }
    if root.get("$id").is_some_and(|id| Some(id) != root.get("catalogId")) {
        add(&mut issues, "/$id", "must equal catalogId");
    }
    if root.get("protocolVersion").and_then(Value::as_str) != Some(A2UI_VERSION) {
        add(&mut issues, "/protocolVersion", format!("must be \"{A2UI_VERSION}\""));
    }
    for key in ["$schema", "title", "description", "instructions"] {
        if root.get(key).is_some_and(|v| !v.is_string()) {
            add(&mut issues, &format!("/{key}"), "a string");
        }
    }
    if let Some(metadata) = root.get("metadata") {
        check_definition_metadata(metadata, "/metadata", &mut issues);
    }
    let empty = Obj::new();
    let components = root.get("components").and_then(Value::as_object).unwrap_or(&empty);
    if root.get("components").is_some_and(|c| !c.is_object()) {
        add(&mut issues, "/components", "an object");
    }
    let functions = root.get("functions").and_then(Value::as_object).unwrap_or(&empty);
    if root.get("functions").is_some_and(|c| !c.is_object()) {
        add(&mut issues, "/functions", "an object");
    }
    let defs = root.get("$defs").and_then(Value::as_object).unwrap_or(&empty);
    if !root.get("$defs").is_some_and(Value::is_object) {
        add(&mut issues, "/$defs", "an object with anyComponent and anyFunction");
    }
    for (name, def) in components {
        check_component(name, def, components, flat, &mut issues);
    }
    for (name, def) in functions {
        check_function(name, def, &mut issues);
    }
    for (key, list, kind) in [("anyComponent", components, "components"), ("anyFunction", functions, "functions")] {
        let Some(any) = defs.get(key).filter(|a| a.is_object()) else {
            add(&mut issues, &format!("/$defs/{key}"), "required");
            continue;
        };
        let refs: Vec<Option<&str>> = any.get("oneOf").and_then(Value::as_array).map(|l| l.iter().map(|r| r.get("$ref").and_then(Value::as_str)).collect()).unwrap_or_default();
        let expected: Vec<String> = list.keys().map(|n| format!("#/{kind}/{n}")).collect();
        if refs.len() != expected.len() || refs.iter().zip(&expected).any(|(r, e)| *r != Some(e.as_str())) {
            add(&mut issues, &format!("/$defs/{key}"), format!("oneOf must reference every entry of {kind}, in order"));
        }
    }
    for (name, def) in defs {
        if name != "anyComponent" && name != "anyFunction" && defines_component(def) {
            add(&mut issues, &format!("/$defs/{name}"), "a leaf must not define a component (properties.component.const)");
        }
    }
    let mut found = Vec::new();
    walk_refs(doc, "", &mut |target, at| found.push((target.to_string(), at.to_string())));
    for (target, at) in found {
        if target.strip_prefix("common_types.json#/$defs/").is_some_and(|t| COMMON_TYPE_REFS.contains(&t)) {
            continue;
        }
        let local = target.strip_prefix("#/").and_then(|t| t.split_once('/')).filter(|(_, name)| !name.is_empty() && !name.contains('/'));
        let table = match local {
            Some(("components", name)) => Some((components, name)),
            Some(("functions", name)) => Some((functions, name)),
            Some(("$defs", name)) => Some((defs, name)),
            _ => None,
        };
        match table {
            None => add(&mut issues, &at, format!("$ref {target} is not an allowed target")),
            Some((t, name)) if !t.contains_key(name) => add(&mut issues, &at, format!("$ref {target} names nothing")),
            Some(_) => {}
        }
    }
    issues
}
