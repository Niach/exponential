//! Round 9 §6.1: the catalog function map as the router and the evaluator
//! read it: which functions a catalog declares, their `allowedCallers`,
//! `requiresUserActivation` and arg schemas.

use serde_json::{Map, Value};

use crate::catalog::{A2UI_BASIC_CATALOG_ID, CORE, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID};

/// `allowedCallers` (§6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedCallers {
    RendererOrAgent,
    RendererOnly,
    AgentOnly,
}

impl AllowedCallers {
    pub fn as_str(self) -> &'static str {
        match self {
            AllowedCallers::RendererOrAgent => "rendererOrAgent",
            AllowedCallers::RendererOnly => "rendererOnly",
            AllowedCallers::AgentOnly => "agentOnly",
        }
    }
}

/// The local `@index` function (§6.2): never routed.
pub const INDEX_FUNCTION: &str = "@index";

/// The 1.0 basic catalog id (§9.1).
pub const BASIC_CATALOG_ID_10: &str = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";

/// Is `id` the basic catalog (the generated id, or the 1.0 id)?
pub fn is_basic_catalog(id: &str) -> bool {
    id == A2UI_BASIC_CATALOG_ID || id == BASIC_CATALOG_ID_10
}

/// The signature `{args, returns}` a catalog declares for `name`; `None` =
/// not declared there. Core and core-lite: the core map (+ the basic
/// validators, which the core declares too); basic: the 14; an extension:
/// the core's (its own functions are the host's to add).
pub fn declared_function(catalog_id: &str, name: &str) -> Option<&'static Value> {
    let core = &CORE.functions;
    if is_basic_catalog(catalog_id) {
        return core.basic.get(name);
    }
    let _ = (CORE_CATALOG_ID, CORE_LITE_CATALOG_ID);
    core.core.get(name).or_else(|| core.basic.get(name))
}

/// §6.1: `set`, `bind`, `openUrl` and every host function are
/// `rendererOnly`; every other (pure) function `rendererOrAgent`.
pub fn allowed_callers(name: &str) -> AllowedCallers {
    match name {
        "set" | "bind" | "openUrl" => AllowedCallers::RendererOnly,
        _ if declared_function(CORE_CATALOG_ID, name).is_some() => AllowedCallers::RendererOrAgent,
        _ => AllowedCallers::RendererOnly,
    }
}

/// §6.1: only `openUrl` needs a user gesture.
pub fn requires_user_activation(name: &str) -> bool {
    name == "openUrl"
}

/// §6.1/§6.4: the effectful catalog functions (never in a reactive
/// position): `set` writes, `bind` is a host effect, `openUrl` needs a gesture.
pub fn is_effect_function(name: &str) -> bool {
    matches!(name, "set" | "bind" | "openUrl")
}

const HOST_PREFIXES: &[&str] =
    &["actions_", "harness_", "exponential_", "tools_", "upstream_", "camera_", "clipboard_", "files_", "http_", "location_", "notifications_", "photos_", "print_", "share_", "app_"];

/// §6.3: does `name` start with a reserved host prefix?
pub fn has_host_prefix(name: &str) -> bool {
    HOST_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// §6.3: a host function name (an exact host name or a reserved prefix).
pub fn is_host_function_name(name: &str) -> bool {
    matches!(name, "app_navigate" | "app_toast") || HOST_PREFIXES.iter().any(|p| name.starts_with(p) && name.len() > p.len())
}

/// §6.1 `returnType` of a declared function (`validationResult` for the
/// validators).
pub fn return_type(name: &str) -> Option<&'static str> {
    if matches!(name, "required" | "regex" | "length" | "numeric" | "email") {
        return Some("validationResult");
    }
    declared_function(CORE_CATALOG_ID, name).and_then(|f| f.get("returns")).and_then(Value::as_str)
}

/// The first issue of `args` against the declared signature (unknown arg,
/// type mismatch); `None` = valid.
pub fn check_args(signature: &Value, args: &Map<String, Value>) -> Option<String> {
    let declared = signature.get("args").and_then(Value::as_object);
    for (k, v) in args {
        let Some(ty) = declared.and_then(|d| d.get(k)) else {
            return Some(format!("{k} is not an argument"));
        };
        let ty = ty.as_str().or_else(|| ty.get("type").and_then(Value::as_str)).unwrap_or("any");
        let ok = match ty {
            "string" => v.is_string(),
            "number" | "integer" => v.is_number(),
            "boolean" => v.is_boolean(),
            "array" => v.is_array(),
            "object" => v.is_object(),
            _ => true,
        };
        if !ok && !v.is_null() {
            return Some(format!("{k} is not a {ty}"));
        }
    }
    None
}
