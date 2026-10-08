//! The catalog as data: `core.catalog.json` (embedded by the generator), the
//! catalog ids, token groups, and the merged VIEW of the core plus registered
//! extensions every lookup goes through. Mirrors `src/catalog.ts`.

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
/// The A2UI wire `version` the vendored schemas carry.
pub const A2UI_VERSION: &str = g::BASIC_VERSION;
/// The placeholder component painted for anything unmapped.
pub const UNKNOWN_COMPONENT: &str = g::UNKNOWN_COMPONENT;
/// What a client advertises as `supportedCatalogIds`.
pub const SUPPORTED_CATALOG_IDS: &[&str] = &[CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, A2UI_BASIC_CATALOG_ID];

/// The parsed core catalog, once per process.
pub static CORE: LazyLock<CatalogSource> =
    LazyLock::new(|| serde_json::from_str(g::CORE_CATALOG_JSON).expect("core.catalog.json"));

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
#[derive(Debug, Clone)]
pub struct CatalogView {
    pub components: IndexMap<String, ComponentDef>,
    pub enums: IndexMap<String, Vec<String>>,
    pub defs: IndexMap<String, DefSchema>,
    pub macros: IndexMap<String, MacroDef>,
    /// The extension ids this view was built with, in registration order.
    pub extension_ids: Vec<String>,
    /// Extension component name → the extension id that defines it.
    pub owner: IndexMap<String, String>,
}

static CORE_VIEW: LazyLock<Arc<CatalogView>> = LazyLock::new(|| Arc::new(CatalogView::build(&[])));

impl CatalogView {
    fn build(extensions: &[ExtensionDef]) -> CatalogView {
        let core = &*CORE;
        let mut view = CatalogView {
            components: core.components.clone(),
            enums: core.enums.clone(),
            defs: core.defs.clone(),
            macros: CORE_MACROS.clone(),
            extension_ids: Vec::new(),
            owner: IndexMap::new(),
        };
        for ext in extensions {
            for (k, v) in &ext.components {
                view.components.insert(k.clone(), v.clone());
                view.owner.insert(k.clone(), ext.id.clone());
            }
            for (k, v) in ext.enums.iter().flatten() {
                view.enums.insert(k.clone(), v.clone());
            }
            for (k, v) in ext.defs.iter().flatten() {
                view.defs.insert(k.clone(), v.clone());
            }
            for (k, v) in ext.macros.iter().flatten() {
                view.macros.insert(k.clone(), v.clone());
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
        .filter(|(_, def)| !def.is_hidden() && (!lite || def.lite))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The component definition in the core (no extensions).
pub fn component_def(name: &str) -> Option<&'static ComponentDef> {
    CORE.components.get(name)
}
