//! The host API contract (`catalog/host.json`) as constants, plus the client
//! messages every platform host sends (`tests/host_fixtures.rs` gates the
//! constants against the JSON).

use serde_json::{json, Map, Value};

use crate::catalog::A2UI_VERSION;

pub const HOST_CONTRACT_VERSION: u32 = 2;
/// The four A2UI v0.9 server messages.
pub const A2UI_MESSAGE_KINDS: &[&str] = &["createSurface", "updateComponents", "updateDataModel", "deleteSurface"];
/// The two Exponential UI extensions.
pub const EXTENSION_MESSAGE_KINDS: &[&str] = &["applyTemplate", "bindDataModel"];
/// Every message kind the router takes, A2UI first.
pub const MESSAGE_KINDS: &[&str] = &["createSurface", "updateComponents", "updateDataModel", "deleteSurface", "applyTemplate", "bindDataModel"];
/// What the router turns a message into.
pub const OP_KINDS: &[&str] = &["create", "components", "data", "bind", "delete", "send"];
pub const HOST_ERROR_CODES: &[&str] =
    &["VALIDATION_FAILED", "INVALID_MESSAGE", "UNSUPPORTED_CATALOG", "SURFACE_NOT_FOUND", "TEMPLATE_NOT_FOUND", "FUNCTION_NOT_FOUND", "FUNCTION_DENIED", "RENDER_FAILED"];
pub const VALIDATION_FAILED: &str = "VALIDATION_FAILED";
pub const INVALID_MESSAGE: &str = "INVALID_MESSAGE";
pub const UNSUPPORTED_CATALOG: &str = "UNSUPPORTED_CATALOG";
pub const SURFACE_NOT_FOUND: &str = "SURFACE_NOT_FOUND";
pub const TEMPLATE_NOT_FOUND: &str = "TEMPLATE_NOT_FOUND";
pub const FUNCTION_NOT_FOUND: &str = "FUNCTION_NOT_FOUND";
pub const FUNCTION_DENIED: &str = "FUNCTION_DENIED";
/// A component's painter failed (`paint.errorCode`).
pub const RENDER_FAILED: &str = "RENDER_FAILED";
pub const FUNCTION_DECISIONS: &[&str] = &["allow", "ask", "deny", "not_found"];
/// The default function policy's `default`.
pub const DEFAULT_FUNCTION_DECISION: &str = "allow";
pub const DEFAULT_URL_SCHEMES: &[&str] = &["https", "http", "mailto", "tel"];
pub const MEDIA_RULE_KEYS: &[&str] = &["prefix", "headers"];
/// The media loader's schemes when the host lists none (no `file`).
pub const DEFAULT_MEDIA_SCHEMES: &[&str] = &["https", "http", "data"];
/// `media.limits`: what every image loader enforces.
pub const MEDIA_MAX_BYTES: u64 = 20_971_520;
pub const MEDIA_TIMEOUT_MS: u64 = 30_000;
pub const MEDIA_MAX_PIXELS: u64 = 33_554_432;
/// The renderer → host hook a failed painter calls (`paint.hook`).
pub const PAINT_ERROR_HOOK: &str = "onPaintError";
/// A binding source URI (`<scheme>:<name>[?k=v&…]`).
pub const SOURCE_PATTERN: &str = "^([a-zA-Z][a-zA-Z0-9+.-]*):([^?]+)(?:\\?(.*))?$";
pub const MCP_MIME_TYPES: &[&str] = &["application/json+a2ui", "application/a2ui+json"];
pub const MCP_ACTION_TOOL: &str = "a2ui_event";
pub const SSE_EVENTS: &[&str] = &["message", "a2ui"];
pub const PACKAGE_REQUIRED: &[&str] = &["id", "name", "version", "catalogId", "templates"];
/// Round 2: what a host hands each surface besides messages — the
/// `SurfaceSettings` keys, the host formatter's methods, the commands.
pub const SURFACE_SETTINGS: &[&str] = &["locale", "timeZone", "strings", "mode", "density", "contrast", "theme"];
pub const SURFACE_FORMATTER: &[&str] = &["number", "currency", "percent", "date", "relativeTime", "plural"];
pub const SURFACE_COMMANDS: &[&str] = &["focus", "announce", "scrollIntoView", "scrollToIndex"];

/// `catalog/host.json` without its `$comment`s, rebuilt from the constants.
pub fn host_contract() -> Value {
    json!({
        "version": HOST_CONTRACT_VERSION,
        "a2uiVersion": A2UI_VERSION,
        "messages": {"a2ui": A2UI_MESSAGE_KINDS, "extensions": EXTENSION_MESSAGE_KINDS},
        "ops": {"kinds": OP_KINDS},
        "clientMessages": {"errorCodes": HOST_ERROR_CODES},
        "functions": {"decisions": FUNCTION_DECISIONS, "defaultPolicy": {"allow": [], "ask": [], "deny": [], "default": DEFAULT_FUNCTION_DECISION}},
        "urls": {"defaultSchemes": DEFAULT_URL_SCHEMES},
        "media": {
            "ruleKeys": MEDIA_RULE_KEYS,
            "defaultSchemes": DEFAULT_MEDIA_SCHEMES,
            "limits": {"maxBytes": MEDIA_MAX_BYTES, "timeoutMs": MEDIA_TIMEOUT_MS, "maxPixels": MEDIA_MAX_PIXELS},
        },
        "sources": {"pattern": SOURCE_PATTERN},
        "transport": {"mcpMimeTypes": MCP_MIME_TYPES, "mcpActionTool": MCP_ACTION_TOOL, "sseEvents": SSE_EVENTS},
        "negotiation": {},
        "package": {"required": PACKAGE_REQUIRED},
        "paint": {"hook": PAINT_ERROR_HOOK, "errorCode": RENDER_FAILED},
        "surface": {"settings": SURFACE_SETTINGS, "formatter": SURFACE_FORMATTER, "commands": SURFACE_COMMANDS},
    })
}

/// A client error message (A2UI v0.9 `error`); `path` only when given.
pub fn error_message(code: &str, surface_id: &str, message: &str, path: Option<&str>) -> Value {
    let mut error = Map::new();
    error.insert("code".into(), Value::String(code.into()));
    error.insert("surfaceId".into(), Value::String(surface_id.into()));
    error.insert("message".into(), Value::String(message.into()));
    if let Some(path) = path {
        error.insert("path".into(), Value::String(path.into()));
    }
    json!({"version": A2UI_VERSION, "error": error})
}

/// A client action message (A2UI v0.9 `action`). `context` null = `{}`;
/// `payload` only when it has keys (JS `Object.keys(payload).length`).
pub fn action_message(surface_id: &str, component_id: &str, name: &str, context: Value, payload: Option<Value>, timestamp: &str) -> Value {
    let mut action = Map::new();
    action.insert("name".into(), Value::String(name.into()));
    action.insert("surfaceId".into(), Value::String(surface_id.into()));
    action.insert("sourceComponentId".into(), Value::String(component_id.into()));
    action.insert("timestamp".into(), Value::String(timestamp.into()));
    action.insert("context".into(), if context.is_null() { Value::Object(Map::new()) } else { context });
    if let Some(payload) = payload {
        let has_keys = match &payload {
            Value::Object(o) => !o.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::String(s) => !s.is_empty(),
            _ => false,
        };
        if has_keys {
            action.insert("payload".into(), payload);
        }
    }
    json!({"version": A2UI_VERSION, "action": action})
}
