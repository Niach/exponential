//! The host API contract (`catalog/host.json`) as constants, plus the client
//! messages every platform host sends (`tests/host_fixtures.rs` gates the
//! constants against the JSON).

use serde_json::{json, Map, Value};

use crate::catalog::A2UI_VERSION;

pub const HOST_CONTRACT_VERSION: u32 = 4;
/// The six A2UI 1.0 agent → renderer messages.
pub const A2UI_MESSAGE_KINDS: &[&str] = &["createSurface", "updateComponents", "updateDataModel", "deleteSurface", "callRendererFunction", "agentFunctionResponse"];
/// The two host-internal kinds (only through a TRUSTED route, round 9 §2).
pub const HOST_MESSAGE_KINDS: &[&str] = &["applyTemplate", "bindDataModel"];
/// The pre-1.0 name of [`HOST_MESSAGE_KINDS`].
pub const EXTENSION_MESSAGE_KINDS: &[&str] = HOST_MESSAGE_KINDS;
/// Every message kind the router takes, A2UI first.
pub const MESSAGE_KINDS: &[&str] = &["createSurface", "updateComponents", "updateDataModel", "deleteSurface", "callRendererFunction", "agentFunctionResponse", "applyTemplate", "bindDataModel"];
/// What the router turns a message into.
pub const OP_KINDS: &[&str] = &["create", "components", "data", "bind", "delete", "send", "call", "respond"];
/// The four renderer → agent kinds.
pub const CLIENT_MESSAGE_KINDS: &[&str] = &["action", "callAgentFunction", "rendererFunctionResponse", "error"];
/// Round 9 §7: the 14 codes, in order.
pub const HOST_ERROR_CODES: &[&str] = &[
    "VALIDATION_FAILED",
    "UNALLOWED_PARENT",
    "UNALLOWED_CHILD",
    "INTEGRITY_ERROR",
    "INVALID_FUNCTION_CALL",
    "EXPRESSION_ERROR",
    "EXECUTION_ERROR",
    "UNKNOWN_FUNCTION",
    "INVALID_MESSAGE",
    "UNSUPPORTED_CATALOG",
    "SURFACE_NOT_FOUND",
    "TEMPLATE_NOT_FOUND",
    "SOURCE_DENIED",
    "RENDER_FAILED",
];
pub const VALIDATION_FAILED: &str = "VALIDATION_FAILED";
pub const UNALLOWED_PARENT: &str = "UNALLOWED_PARENT";
pub const UNALLOWED_CHILD: &str = "UNALLOWED_CHILD";
/// §1.4: a surface id created twice in one renderer lifetime.
pub const INTEGRITY_ERROR: &str = "INTEGRITY_ERROR";
/// A policy deny, a refused consent, a §6.9 refusal (was FUNCTION_DENIED).
pub const INVALID_FUNCTION_CALL: &str = "INVALID_FUNCTION_CALL";
/// §6.8: a failed binding / check / routed action call; a refused regex.
pub const EXPRESSION_ERROR: &str = "EXPRESSION_ERROR";
/// A local action function failed (was ACTION_FAILED).
pub const EXECUTION_ERROR: &str = "EXECUTION_ERROR";
/// Allowed, unregistered, no agent (was FUNCTION_NOT_FOUND).
pub const UNKNOWN_FUNCTION: &str = "UNKNOWN_FUNCTION";
pub const INVALID_MESSAGE: &str = "INVALID_MESSAGE";
pub const UNSUPPORTED_CATALOG: &str = "UNSUPPORTED_CATALOG";
pub const SURFACE_NOT_FOUND: &str = "SURFACE_NOT_FOUND";
pub const TEMPLATE_NOT_FOUND: &str = "TEMPLATE_NOT_FOUND";
/// VAPP-107: a `bindDataModel` outside a package surface's `sources`.
pub const SOURCE_DENIED: &str = "SOURCE_DENIED";
/// A component's painter failed (`paint.errorCode`).
pub const RENDER_FAILED: &str = "RENDER_FAILED";
/// The 0.9 names, kept as aliases of their 1.0 codes.
pub const FUNCTION_NOT_FOUND: &str = UNKNOWN_FUNCTION;
pub const FUNCTION_DENIED: &str = INVALID_FUNCTION_CALL;
/// VAPP-194: a package action that landed nothing (now EXECUTION_ERROR).
pub const ACTION_FAILED: &str = EXECUTION_ERROR;
/// The codes of the VALIDATION branch: `{code, surfaceId, message, path}`
/// with `path` always set.
pub const VALIDATION_CODES: &[&str] = &[VALIDATION_FAILED, UNALLOWED_PARENT, UNALLOWED_CHILD];
pub const FUNCTION_DECISIONS: &[&str] = &["allow", "ask", "deny", "not_found"];
/// The default function policy's `default`.
pub const DEFAULT_FUNCTION_DECISION: &str = "allow";
pub const DEFAULT_URL_SCHEMES: &[&str] = &["https", "http", "mailto", "tel"];
/// VAPP-107: the hosts `decide_url` denies by default (`urls.privateHosts`,
/// the rule itself is [`super::is_private_host`]).
pub const PRIVATE_HOSTS: &[&str] = &["localhost", "*.localhost", "*.local", "0.0.0.0/8", "10.0.0.0/8", "127.0.0.0/8", "169.254.0.0/16", "172.16.0.0/12", "192.168.0.0/16", "100.64.0.0/10", "::", "::1", "fe80::/10", "fc00::/7", "::ffff:<private IPv4>", "::<private IPv4>", "64:ff9b::<private IPv4>", "2002:<private IPv4>::/48"];
pub const URL_REASONS: &[&str] = &["invalid", "scheme", "host", "private"];
pub const MEDIA_RULE_KEYS: &[&str] = &["prefix", "headers"];
pub const MEDIA_OPTION_KEYS: &[&str] = &["baseUrl", "rules", "schemes", "hosts", "allowPrivate"];
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
pub const MCP_MIME_TYPES: &[&str] = &["application/a2ui+json"];
/// Round 9 §10: the A2A negotiation constants (`negotiation`).
pub const A2A_CAPABILITIES_KEY: &str = "a2uiRendererCapabilities";
pub const A2A_DATA_MODEL_KEY: &str = "a2uiRendererDataModel";
pub const A2A_EXTENSION_URI: &str = "https://a2ui.org/a2a-extension/a2ui/v1.0";
pub const A2UI_MIME_TYPE: &str = "application/a2ui+json";
/// `limits.json` `agentFunctionTimeoutMs`.
pub const AGENT_FUNCTION_TIMEOUT_MS: u64 = 30_000;
pub const MCP_ACTION_TOOL: &str = "a2ui_event";
pub const SSE_EVENTS: &[&str] = &["message", "a2ui"];
pub const PACKAGE_REQUIRED: &[&str] = &["id", "name", "version", "catalogId", "templates"];
/// Round 2: what a host hands each surface besides messages — the
/// `SurfaceSettings` keys, the host formatter's methods, the commands.
pub const SURFACE_SETTINGS: &[&str] = &["locale", "timeZone", "strings", "mode", "density", "contrast", "theme", "readOnly"];
pub const SURFACE_FORMATTER: &[&str] = &["number", "currency", "percent", "date", "relativeTime", "plural"];
pub const SURFACE_COMMANDS: &[&str] = &["focus", "announce", "scrollIntoView", "scrollToIndex"];

/// #VAPP-258: the Markdown host hooks (`markdown`): what a `textLinks`
/// segment's `chip` and an `attachment` answer may be, and the block image
/// box's max height.
pub const MARKDOWN_HOOKS: &[&str] = &["textLinks", "imageSize", "onImagePress", "attachment"];
pub const MARKDOWN_CHIP_KINDS: &[&str] = &["issue", "mention", "entity"];
pub const MARKDOWN_ATTACHMENT_KINDS: &[&str] = &["video", "audio", "file"];
pub const MARKDOWN_IMAGE_MAX_HEIGHT: f64 = 160.0;
/// #VAPP-258: an unsized block image's aspect (height / width) under
/// `markdown.options.imageMaxHeight: null` (no cap): 4:3.
pub const MARKDOWN_UNSIZED_ASPECT: f64 = 0.75;
/// #VAPP-258 (step 5): the host-level render options (`markdown.options`):
/// each one's values and default (`selectable` unset = the renderer's own).
pub const MARKDOWN_BREAKS: &[&str] = &["soft", "hard"];
pub const MARKDOWN_DENSITIES: &[&str] = &["document", "chat"];
/// From this many blocks on, gpui and Compose lay out only visible blocks.
pub const MARKDOWN_LAZY_BLOCKS: usize = 24;

/// VAPP-122: how the `bind` action appends its params (`sources.paramsEncoding`).
pub const SOURCE_PARAMS_ENCODING: &str = "sourceWithParams: keys sorted; key and value percent-encoded (everything but A-Z a-z 0-9 - _ . ! ~ * ' ( )); null skipped; an object or array as JSON; after ? or &";
/// VAPP-122: a record kind's name (`package.records.kindPattern`).
pub const RECORD_KIND_PATTERN: &str = "^[a-z][a-z0-9_]{0,31}$";

/// `catalog/host.json` without its `$comment`s, rebuilt from the constants.
pub fn host_contract() -> Value {
    use super::{actions as act, exposes as ex, record_store as rs, signature as sig, tests as pt, tools};
    json!({
        "version": HOST_CONTRACT_VERSION,
        "a2uiVersion": A2UI_VERSION,
        "messages": {"a2ui": A2UI_MESSAGE_KINDS, "host": HOST_MESSAGE_KINDS},
        "ops": {"kinds": OP_KINDS},
        "clientMessages": {"kinds": CLIENT_MESSAGE_KINDS, "errorCodes": HOST_ERROR_CODES},
        "functions": {"decisions": FUNCTION_DECISIONS, "defaultPolicy": {"allow": [], "ask": [], "deny": [], "default": DEFAULT_FUNCTION_DECISION}},
        "urls": {"defaultSchemes": DEFAULT_URL_SCHEMES, "privateHosts": PRIVATE_HOSTS, "reasons": URL_REASONS},
        "media": {
            "ruleKeys": MEDIA_RULE_KEYS,
            "optionKeys": MEDIA_OPTION_KEYS,
            "defaultSchemes": DEFAULT_MEDIA_SCHEMES,
            "limits": {"maxBytes": MEDIA_MAX_BYTES, "timeoutMs": MEDIA_TIMEOUT_MS, "maxPixels": MEDIA_MAX_PIXELS},
            "appFile": {"scheme": super::policy::APP_FILE_SCHEME, "idPattern": super::policy::APP_FILE_ID_PATTERN},
        },
        "markdown": {"hooks": MARKDOWN_HOOKS, "chipKinds": MARKDOWN_CHIP_KINDS, "attachmentKinds": MARKDOWN_ATTACHMENT_KINDS, "imageMaxHeight": MARKDOWN_IMAGE_MAX_HEIGHT as u64, "options": {"breaks": {"values": MARKDOWN_BREAKS, "default": MARKDOWN_BREAKS[0]}, "density": {"values": MARKDOWN_DENSITIES, "default": MARKDOWN_DENSITIES[0]}, "selectable": {"default": null}, "streaming": {"default": false}, "imageMaxHeight": {"default": MARKDOWN_IMAGE_MAX_HEIGHT as u64}}, "lazyBlocks": MARKDOWN_LAZY_BLOCKS},
        "sources": {"pattern": SOURCE_PATTERN, "paramsEncoding": SOURCE_PARAMS_ENCODING},
        "transport": {"mcpMimeTypes": MCP_MIME_TYPES, "mcpActionTool": MCP_ACTION_TOOL, "sseEvents": SSE_EVENTS},
        "negotiation": {"a2aCapabilitiesKey": A2A_CAPABILITIES_KEY, "a2aDataModelKey": A2A_DATA_MODEL_KEY, "a2aExtensionUri": A2A_EXTENSION_URI, "mimeType": A2UI_MIME_TYPE},
        "package": {
            "required": PACKAGE_REQUIRED,
            "formatVersion": super::package::PACKAGE_FORMAT_VERSION,
            "records": {
                "kindPattern": RECORD_KIND_PATTERN,
                "reserved": super::records::RECORD_RESERVED_KEYS,
                "types": super::records::RECORD_FIELD_TYPES,
                "formats": super::records::RECORD_FORMATS,
                "scheme": rs::RECORDS_SCHEME,
                "functions": rs::RECORDS_FUNCTIONS,
                "query": {"sortDefault": rs::RECORDS_SORT_DEFAULT, "orderDefault": rs::RECORDS_ORDER_DEFAULT, "limitDefault": rs::RECORDS_LIMIT_DEFAULT, "limitMax": rs::RECORDS_LIMIT_MAX, "queryNamePattern": super::package::QUERY_NAME_PATTERN, "reserved": rs::RECORDS_RESERVED_PARAMS},
            },
            "actions": {
                "limits": {"actions": act::ACTION_LIMIT_ACTIONS, "steps": act::ACTION_LIMIT_STEPS, "maxSteps": act::ACTION_LIMIT_MAX_STEPS},
                "namePattern": act::ACTION_NAME_PATTERN,
                "stepKinds": act::ACTION_STEP_KINDS,
                "callPrefix": act::ACTION_CALL_PREFIX,
                "batchFunction": act::RECORDS_BATCH_FUNCTION,
                "resultPattern": act::ACTION_RESULT_PATTERN,
            },
            "screens": {
                "max": super::screens::MAX_SCREENS,
                "titleMax": super::screens::MAX_SCREEN_TITLE,
                "exposedDescriptionMax": super::screens::MAX_EXPOSED_DESCRIPTION,
                "navigateFunction": super::screens::NAVIGATE_FUNCTION,
                "paramsKey": super::screens::SCREEN_PARAMS_KEY,
            },
            "capabilities": {
                "max": super::capabilities::MAX_CAPABILITIES,
                "whyMax": super::capabilities::MAX_CAPABILITY_WHY,
                "scopes": super::capabilities::CAPABILITY_SCOPES,
                "origins": super::capabilities::CAPABILITY_ORIGIN_SCOPES,
                "originPattern": super::capabilities::CAPABILITY_ORIGIN_PATTERN,
            },
            "share": {
                "max": super::share::MAX_SHARES,
                "namePattern": "^[a-z][a-z0-9_]{0,23}$",
                "modes": ["submit", "view"],
                "expiresMax": super::share::MAX_SHARE_EXPIRES_DAYS,
            },
            "tests": {
                "limits": {"tests": pt::PACKAGE_TEST_LIMIT_TESTS, "gotMax": pt::PACKAGE_TEST_LIMIT_GOT_MAX},
                "defaults": {"now": pt::PACKAGE_TEST_DEFAULT_NOW, "as": pt::PACKAGE_TEST_DEFAULT_AS, "role": pt::PACKAGE_TEST_DEFAULT_ROLE},
                "roles": pt::PACKAGE_TEST_ROLES,
                "kinds": pt::PACKAGE_TEST_KINDS,
            },
            "tools": {"idPattern": tools::TOOL_ID_PATTERN, "kinds": tools::TOOL_KINDS, "transports": tools::TOOL_TRANSPORTS, "functionPrefix": tools::TOOL_FUNCTION_PREFIX},
            "exposes": {
                "limits": {"namePattern": ex::EXPOSE_NAME_PATTERN, "nameMax": ex::EXPOSE_NAME_MAX, "titleMax": ex::EXPOSE_TITLE_MAX, "descriptionMax": ex::EXPOSE_DESCRIPTION_MAX, "maxTools": ex::EXPOSE_MAX_TOOLS, "separator": ex::EXPOSE_SEPARATOR},
                "runKinds": ex::EXPOSE_RUN_KINDS,
                "recordOps": ex::EXPOSE_RECORD_OPS,
                "readRecordOps": ex::EXPOSE_READ_RECORD_OPS,
                "readOpWords": ex::EXPOSE_READ_OP_WORDS,
                "destructiveWords": ex::EXPOSE_DESTRUCTIVE_WORDS,
                "actionPattern": ex::EXPOSE_ACTION_PATTERN,
                "agent": {"slugPattern": "^[a-z0-9]+(-[a-z0-9]+)*$", "slugMax": ex::EXPOSE_AGENT_SLUG_MAX, "promptMax": ex::EXPOSE_AGENT_PROMPT_MAX, "devices": ex::EXPOSE_AGENT_DEVICES},
            },
            "extensions": {"versionPattern": super::package::EXTENSION_VERSION_PATTERN},
            "signature": {"alg": sig::PACKAGE_FILE_SIGNATURE_ALG, "context": sig::PACKAGE_FILE_SIGNATURE_CONTEXT, "keyPrefix": sig::AUTHOR_KEY_PREFIX, "nameMax": sig::AUTHOR_NAME_MAX, "statuses": sig::PACKAGE_FILE_STATUSES},
        },
        "paint": {"hook": PAINT_ERROR_HOOK, "errorCode": RENDER_FAILED},
        "surface": {"settings": SURFACE_SETTINGS, "formatter": SURFACE_FORMATTER, "commands": SURFACE_COMMANDS},
        "transition": {"acceptV09": crate::catalog::ACCEPT_V09},
    })
}

/// Who an `error` is about (round 9 §1.3: exactly one of the two).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorTarget<'a> {
    Surface(&'a str),
    FunctionCall(&'a str),
}

/// A client `error` message about a surface (A2UI 1.0). The validation
/// codes always carry a `path` (`/` when none is given).
pub fn error_message(code: &str, surface_id: &str, message: &str, path: Option<&str>) -> Value {
    error_message_to(code, ErrorTarget::Surface(surface_id), message, path)
}

/// A client `error` message (round 9 §1.3 `errorMessage(code, target,
/// message, path?)`): keys `code`, `surfaceId` | `functionCallId`,
/// `message`, `path`.
pub fn error_message_to(code: &str, target: ErrorTarget, message: &str, path: Option<&str>) -> Value {
    let mut error = Map::new();
    error.insert("code".into(), Value::String(code.into()));
    match target {
        ErrorTarget::Surface(id) => error.insert("surfaceId".into(), Value::String(id.into())),
        ErrorTarget::FunctionCall(id) => error.insert("functionCallId".into(), Value::String(id.into())),
    };
    error.insert("message".into(), Value::String(message.into()));
    let path = path.or_else(|| (VALIDATION_CODES.contains(&code) && matches!(target, ErrorTarget::Surface(_))).then_some("/"));
    if let Some(path) = path {
        error.insert("path".into(), Value::String(path.into()));
    }
    json!({"version": A2UI_VERSION, "error": error})
}

/// The authored id of a painted id: up to its first `.` (round 9 §4.3).
pub fn authored_id(painted: &str) -> &str {
    painted.split_once('.').map_or(painted, |(head, _)| head)
}

/// A client action message (A2UI 1.0 `action`, round 9 §1.3).
pub fn action_message(surface_id: &str, component_id: &str, name: &str, context: Value, payload: Option<Value>, timestamp: &str) -> Value {
    action_message_with(surface_id, component_id, name, context, payload, timestamp, None)
}

/// [`action_message`] with the Action's resolved `event.userMessage`.
/// Keys: `name`, `surfaceId`, `sourceComponentId` (the AUTHORED id),
/// `timestamp`, `context` (null = `{}`), `userMessage`, then `metadata`
/// with `extensions.at_exponential_payload` (the component's own event data,
/// when it has keys) and `at_exponential_instance` (the painted id, when it
/// differs from the authored one).
pub fn action_message_with(surface_id: &str, component_id: &str, name: &str, context: Value, payload: Option<Value>, timestamp: &str, user_message: Option<&str>) -> Value {
    let authored = authored_id(component_id);
    let mut action = Map::new();
    action.insert("name".into(), Value::String(name.into()));
    action.insert("surfaceId".into(), Value::String(surface_id.into()));
    action.insert("sourceComponentId".into(), Value::String(authored.into()));
    action.insert("timestamp".into(), Value::String(timestamp.into()));
    action.insert("context".into(), if context.is_null() { Value::Object(Map::new()) } else { context });
    if let Some(user_message) = user_message {
        action.insert("userMessage".into(), Value::String(user_message.into()));
    }
    let mut extensions = Map::new();
    if let Some(payload) = payload {
        let has_keys = match &payload {
            Value::Object(o) => !o.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::String(s) => !s.is_empty(),
            _ => false,
        };
        if has_keys {
            extensions.insert("at_exponential_payload".into(), payload);
        }
    }
    if authored != component_id {
        extensions.insert("at_exponential_instance".into(), Value::String(component_id.into()));
    }
    if !extensions.is_empty() {
        action.insert("metadata".into(), json!({"extensions": extensions}));
    }
    json!({"version": A2UI_VERSION, "action": action})
}

/// Round 9 §1.3 `callAgentFunction`: `{surfaceId, functionCallId,
/// callFunction: {"@call", catalogId, args}}`.
pub fn call_agent_function_message(surface_id: &str, function_call_id: &str, name: &str, catalog_id: &str, args: Map<String, Value>) -> Value {
    json!({"version": A2UI_VERSION, "callAgentFunction": {
        "surfaceId": surface_id,
        "functionCallId": function_call_id,
        "callFunction": {"@call": name, "catalogId": catalog_id, "args": args},
    }})
}

/// Round 9 §1.3 `rendererFunctionResponse`: `{functionCallId, value}` (a
/// function returning nothing answers `null`) or `{functionCallId, error:
/// {code, message}}`.
pub fn renderer_function_response(function_call_id: &str, result: Result<Option<Value>, (&str, &str)>) -> Value {
    let body = match result {
        Ok(value) => json!({"functionCallId": function_call_id, "value": value.unwrap_or(Value::Null)}),
        Err((code, message)) => json!({"functionCallId": function_call_id, "error": {"code": code, "message": message}}),
    };
    json!({"version": A2UI_VERSION, "rendererFunctionResponse": body})
}
