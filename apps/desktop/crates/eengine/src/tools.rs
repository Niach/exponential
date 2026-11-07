//! VAPP-265 (E5): capabilities v1. The catalog's TOOLS (their app-facing
//! `inputSchema`/`outputSchema` and the `nativeOutputSchema` a host's native
//! provider answers), the JSON-schema subset they are checked with, and THE
//! BRIDGE between the broker's `call` verdict and the native: validate → map
//! to the native arguments (app files through the [`BlobStore`]) → call →
//! check the native's result → store every file it carries → the app-facing
//! MCP result. `print.pdf` runs in the Engine ([`crate::pdf`]). Locked by
//! `packages/eengine/fixtures/capabilities.json` (`tools`, `validateCases`,
//! `bridgeCases`; the TS twin: `@exp/eengine/capabilities`).

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::broker::CallOutcome;
use crate::capability::Capability;
use crate::contract::CAPABILITIES;
use crate::providers::BlobStore;

/// Who answers a tool: the host's native provider, or the Engine itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runs {
    Native,
    Engine,
}

/// One v1 tool (capabilities.json `tools`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    /// `<capability>.<tool>`.
    pub name: String,
    pub runs: Runs,
    pub description: String,
    pub input_schema: Value,
    /// What the native answers (`null` for `runs: engine`).
    pub native_output_schema: Option<Value>,
    pub output_schema: Value,
}

impl ToolDef {
    /// The tool's part after the capability (`pick`).
    pub fn tool(&self) -> &str {
        self.name.split_once('_').map_or(self.name.as_str(), |(_, t)| t)
    }

    /// The app-facing MCP tool object under `name` (an agent sees it as
    /// `<capability>__<tool>`; the hub qualifies the bare tool name).
    pub fn tool_object(&self, name: &str) -> Value {
        json!({"name": name, "description": self.description, "inputSchema": self.input_schema, "outputSchema": self.output_schema})
    }
}

pub static TOOLS: LazyLock<Vec<ToolDef>> = LazyLock::new(|| serde_json::from_value(CAPABILITIES["tools"].clone()).expect("capabilities.json tools parse"));

/// The v1 tool `<capability>_<tool>`, if the catalog defines one.
pub fn tool_def(capability: &str, tool: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name.split_once('_') == Some((capability, tool)))
}

// ---------------------------------------------------------------------------
// The schema subset.

/// The first violation: the JSON pointer of the value (`/` for the root) and the keyword.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaError {
    pub pointer: String,
    pub keyword: &'static str,
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pointer = if self.pointer.is_empty() { "/" } else { self.pointer.as_str() };
        write!(f, "{pointer} {}", self.keyword)
    }
}

fn child(pointer: &str, token: &str) -> String {
    format!("{pointer}/{}", token.replace('~', "~0").replace('/', "~1"))
}

fn fail(pointer: String, keyword: &'static str) -> Result<(), SchemaError> {
    Err(SchemaError { pointer, keyword })
}

fn type_matches(t: &str, v: &Value) -> bool {
    match t {
        "null" => v.is_null(),
        "boolean" => v.is_boolean(),
        "string" => v.is_string(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        "number" => v.is_number(),
        "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|f| f.is_finite() && f.fract() == 0.0),
        _ => false,
    }
}

/// Check `value` against `schema` (the subset of capabilities.json's
/// comment, in its order); the first violation wins.
pub fn validate_schema(schema: &Value, value: &Value) -> Result<(), SchemaError> {
    check(schema, value, "")
}

fn check(schema: &Value, v: &Value, ptr: &str) -> Result<(), SchemaError> {
    let Some(s) = schema.as_object() else { return Ok(()) };
    if let Some(t) = s.get("type") {
        let ok = match t {
            Value::String(t) => type_matches(t, v),
            Value::Array(ts) => ts.iter().filter_map(Value::as_str).any(|t| type_matches(t, v)),
            _ => true,
        };
        if !ok {
            return fail(ptr.to_string(), "type");
        }
    }
    match v {
        Value::Object(o) => {
            for k in s.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                if !o.contains_key(k) {
                    return fail(child(ptr, k), "required");
                }
            }
            let props = s.get("properties").and_then(Value::as_object);
            if s.get("additionalProperties") == Some(&Value::Bool(false)) {
                let mut unknown: Vec<&String> = o.keys().filter(|k| !props.is_some_and(|p| p.contains_key(*k))).collect();
                unknown.sort();
                if let Some(k) = unknown.first() {
                    return fail(child(ptr, k), "additionalProperties");
                }
            }
            for (k, sub) in props.into_iter().flatten() {
                if let Some(x) = o.get(k) {
                    check(sub, x, &child(ptr, k))?;
                }
            }
            for (k, deps) in s.get("dependentRequired").and_then(Value::as_object).into_iter().flatten() {
                if !o.contains_key(k) {
                    continue;
                }
                for d in deps.as_array().into_iter().flatten().filter_map(Value::as_str) {
                    if !o.contains_key(d) {
                        return fail(child(ptr, d), "dependentRequired");
                    }
                }
            }
            if let Some(any) = s.get("anyOf").and_then(Value::as_array) {
                if !any.iter().any(|sub| check(sub, v, ptr).is_ok()) {
                    return fail(ptr.to_string(), "anyOf");
                }
            }
            if let Some(one) = s.get("oneOf").and_then(Value::as_array) {
                if one.iter().filter(|sub| check(sub, v, ptr).is_ok()).count() != 1 {
                    return fail(ptr.to_string(), "oneOf");
                }
            }
        }
        Value::Array(a) => {
            if s.get("minItems").and_then(Value::as_u64).is_some_and(|n| (a.len() as u64) < n) {
                return fail(ptr.to_string(), "minItems");
            }
            if s.get("maxItems").and_then(Value::as_u64).is_some_and(|n| (a.len() as u64) > n) {
                return fail(ptr.to_string(), "maxItems");
            }
            if let Some(items) = s.get("items") {
                for (i, x) in a.iter().enumerate() {
                    check(items, x, &child(ptr, &i.to_string()))?;
                }
            }
        }
        Value::Number(n) => {
            let n = n.as_f64().unwrap_or(f64::NAN);
            if s.get("minimum").and_then(Value::as_f64).is_some_and(|m| n < m) {
                return fail(ptr.to_string(), "minimum");
            }
            if s.get("maximum").and_then(Value::as_f64).is_some_and(|m| n > m) {
                return fail(ptr.to_string(), "maximum");
            }
        }
        Value::String(text) => {
            let len = text.chars().count() as u64;
            if s.get("minLength").and_then(Value::as_u64).is_some_and(|m| len < m) {
                return fail(ptr.to_string(), "minLength");
            }
            if s.get("maxLength").and_then(Value::as_u64).is_some_and(|m| len > m) {
                return fail(ptr.to_string(), "maxLength");
            }
        }
        _ => {}
    }
    if let Some(e) = s.get("enum").and_then(Value::as_array) {
        if !e.contains(v) {
            return fail(ptr.to_string(), "enum");
        }
    }
    if let (Some(format), Value::String(text)) = (s.get("format").and_then(Value::as_str), v) {
        if !format_valid(format, text) {
            return fail(ptr.to_string(), "format");
        }
    }
    Ok(())
}

/// The formats: `date-time` (RFC 3339 with a zone), `token`, `uri` (absolute http(s)), `base64` (standard, padded).
pub fn format_valid(format: &str, text: &str) -> bool {
    match format {
        "date-time" => date_time_valid(text),
        "token" => (1..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')),
        "uri" => url::Url::parse(text).is_ok_and(|u| matches!(u.scheme(), "http" | "https")),
        "base64" => base64_decode(text).is_some(),
        _ => true,
    }
}

/// `YYYY-MM-DDTHH:MM:SS[.fraction](Z|±HH:MM)`, uppercase `T`/`Z`, every field in range (leap years; second ≤ 60).
fn date_time_valid(text: &str) -> bool {
    let b = text.as_bytes();
    let digits = |from: usize, n: usize| -> Option<u32> {
        let part = b.get(from..from + n)?;
        part.iter().all(u8::is_ascii_digit).then(|| part.iter().fold(0, |acc, d| acc * 10 + u32::from(d - b'0')))
    };
    let at = |i: usize, c: u8| b.get(i) == Some(&c);
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (digits(0, 4), digits(5, 2), digits(8, 2), digits(11, 2), digits(14, 2), digits(17, 2)) else {
        return false;
    };
    if !(at(4, b'-') && at(7, b'-') && at(10, b'T') && at(13, b':') && at(16, b':')) {
        return false;
    }
    let mut i = 19;
    if at(i, b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    let zone_ok = if at(i, b'Z') {
        i + 1 == b.len()
    } else if at(i, b'+') || at(i, b'-') {
        i + 6 == b.len() && at(i + 3, b':') && digits(i + 1, 2).is_some_and(|h| h <= 23) && digits(i + 4, 2).is_some_and(|m| m <= 59)
    } else {
        false
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    zone_ok && (1..=days).contains(&day) && hour <= 23 && minute <= 59 && second <= 60
}

/// The arguments of a call to `tool` after validation, with the top-level
/// defaults filled in (`validateCases`); `Err` = `invalidParams: <pointer> <keyword>`.
/// Absent arguments (`null`) are `{}`.
pub fn validate_arguments(def: &ToolDef, arguments: &Value) -> Result<Value, String> {
    let arguments = if arguments.is_null() { Value::Object(Map::new()) } else { arguments.clone() };
    validate_schema(&def.input_schema, &arguments).map_err(|e| format!("invalidParams: {e}"))?;
    let mut arguments = arguments;
    if let (Some(o), Some(props)) = (arguments.as_object_mut(), def.input_schema.get("properties").and_then(Value::as_object)) {
        for (k, sub) in props {
            if let (false, Some(default)) = (o.contains_key(k), sub.get("default")) {
                o.insert(k.clone(), default.clone());
            }
        }
    }
    Ok(arguments)
}

// ---------------------------------------------------------------------------
// Bytes: base64, MIME types.

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64, padded.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Standard base64, padded (a length of 4n, `=` only as the last one or
/// two characters) → bytes; `None` for anything else.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let b = text.as_bytes();
    if b.len() % 4 != 0 {
        return None;
    }
    let pad = b.iter().rev().take_while(|&&c| c == b'=').count();
    if pad > 2 {
        return None;
    }
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in &b[..b.len() - pad] {
        let v = B64.iter().position(|&x| x == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// `v1.mimeByExtension` of a name's last `.ext` (case-insensitive).
pub fn mime_by_extension(name: &str) -> Option<&'static str> {
    let (_, ext) = name.rsplit_once('.')?;
    CAPABILITIES["v1"]["mimeByExtension"].get(ext.to_ascii_lowercase().as_str()).and_then(Value::as_str)
}

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The type the bytes tell: PDF, JPEG, PNG or GIF.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(PNG_SIGNATURE) {
        Some("image/png")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else {
        None
    }
}

/// A file's type: the given one, else its name's extension, else the bytes, else `fallback`.
fn mime_of(given: Option<&str>, name: &str, bytes: &[u8], fallback: &'static str) -> String {
    given.map(str::to_string).or_else(|| mime_by_extension(name).map(str::to_string)).or_else(|| sniff_mime(bytes).map(str::to_string)).unwrap_or_else(|| fallback.to_string())
}

/// A fresh notification id: `n-` + 32 lowercase hex.
pub fn notification_id() -> String {
    format!("n-{}", uuid::Uuid::new_v4().simple())
}

// ---------------------------------------------------------------------------
// The bridge.

/// What the bridge runs against: the app, its screens and its files.
pub struct BridgeContext<'a> {
    pub app: &'a str,
    /// The package's screen names (a notification's tap must open one).
    pub screens: &'a [String],
    pub blobs: &'a dyn BlobStore,
}

/// An app-facing MCP call result.
pub fn mcp_result(structured: Value) -> Value {
    json!({"content": [{"type": "text", "text": structured.to_string()}], "structuredContent": structured})
}

/// An MCP error result whose text is `reason`.
pub fn mcp_error(reason: &str) -> Value {
    json!({"content": [{"type": "text", "text": reason}], "isError": true})
}

fn str_of<'v>(args: &'v Value, key: &str) -> Option<&'v str> {
    args.get(key).and_then(Value::as_str)
}

/// An app file's bytes, or the error reason.
fn file_bytes(cx: &BridgeContext, id: &str) -> Result<Vec<u8>, String> {
    match cx.blobs.get(cx.app, id) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => Err(format!("notFound: {id}")),
        Err(failure) => Err(failure.message),
    }
}

/// `{name, mimeType, data}` of a file for the native.
fn native_file(name: &str, mime: String, bytes: &[u8]) -> Value {
    json!({"name": name, "mimeType": mime, "data": base64_encode(bytes)})
}

/// One call of `<capability>.<tool>` through the bridge (`bridgeCases`): the
/// MCP result the app or agent gets and the outcome the audit row records.
/// A tool the v1 catalog does not define yet passes through to the native as is.
pub fn bridge_call(cx: &BridgeContext, native: &dyn Capability, tool: &str, arguments: &Value) -> (Value, CallOutcome) {
    let Some(def) = tool_def(native.name(), tool) else { return call_native(native, tool, arguments) };
    let args = match validate_arguments(def, arguments) {
        Ok(args) => args,
        Err(reason) => return (mcp_error(&reason), CallOutcome::InvalidParams),
    };
    let refused = |reason: String| (mcp_error(&reason), CallOutcome::IsError);
    if def.runs == Runs::Engine {
        return match crate::pdf::print_pdf(cx, &args) {
            Ok(structured) => (mcp_result(structured), CallOutcome::Ok),
            Err(reason) => refused(reason),
        };
    }
    // The answer the app sees when it differs from the stored native result (notifications: the app's own id).
    let mut answer: Option<Value> = None;
    let native_args = match def.name.as_str() {
        "files_save" => {
            let name = str_of(&args, "name").unwrap_or_default();
            let (bytes, fallback) = match str_of(&args, "fileId") {
                Some(id) => match file_bytes(cx, id) {
                    Ok(bytes) => (bytes, "application/octet-stream"),
                    Err(reason) => return refused(reason),
                },
                None => (str_of(&args, "text").unwrap_or_default().as_bytes().to_vec(), "text/plain"),
            };
            native_file(name, mime_of(str_of(&args, "mimeType"), name, &bytes, fallback), &bytes)
        }
        "share_send" => {
            let mut out = Map::new();
            for key in ["title", "text", "url"] {
                if let Some(v) = args.get(key) {
                    out.insert(key.to_string(), v.clone());
                }
            }
            if let Some(id) = str_of(&args, "fileId") {
                let bytes = match file_bytes(cx, id) {
                    Ok(bytes) => bytes,
                    Err(reason) => return refused(reason),
                };
                let name = str_of(&args, "name").unwrap_or_default();
                out.insert("file".into(), native_file(name, mime_of(str_of(&args, "mimeType"), name, &bytes, "application/octet-stream"), &bytes));
            }
            Value::Object(out)
        }
        "print_print" => {
            let bytes = match file_bytes(cx, str_of(&args, "fileId").unwrap_or_default()) {
                Ok(bytes) => bytes,
                Err(reason) => return refused(reason),
            };
            let (mime, ext) = match sniff_mime(&bytes) {
                Some("application/pdf") => ("application/pdf", "pdf"),
                Some("image/png") => ("image/png", "png"),
                Some("image/jpeg") => ("image/jpeg", "jpg"),
                _ => return refused("unsupportedFile".into()),
            };
            let name = str_of(&args, "name").map_or_else(|| format!("document.{ext}"), str::to_string);
            native_file(&name, mime.to_string(), &bytes)
        }
        "notifications_post" => {
            let tap = match str_of(&args, "screen") {
                Some(screen) if !cx.screens.iter().any(|s| s == screen) => return (mcp_error("invalidParams: /screen enum"), CallOutcome::InvalidParams),
                Some(screen) => json!({"type": "notificationTap", "app": cx.app, "screen": screen, "params": args.get("params").cloned().unwrap_or_else(|| json!({}))}),
                None => Value::Null,
            };
            let id = str_of(&args, "id").map_or_else(notification_id, str::to_string);
            answer = Some(json!({"id": id}));
            json!({"id": format!("{}/{id}", cx.app), "title": args["title"], "body": args.get("body").cloned().unwrap_or(Value::Null), "at": args.get("at").cloned().unwrap_or(Value::Null), "tap": tap})
        }
        "notifications_cancel" => json!({"id": format!("{}/{}", cx.app, str_of(&args, "id").unwrap_or_default())}),
        _ => args,
    };
    let result = match native.call(tool, &native_args) {
        Ok(result) => result,
        Err(failure) => return (mcp_error(&failure.message), CallOutcome::RpcError),
    };
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return (result, CallOutcome::IsError);
    }
    let mut structured = result.get("structuredContent").cloned().unwrap_or(Value::Null);
    if let Some(schema) = &def.native_output_schema {
        if let Err(e) = validate_schema(schema, &structured) {
            return refused(format!("badResult: {e}"));
        }
    }
    if let Err(reason) = store_files(cx, &mut structured) {
        return refused(reason);
    }
    (mcp_result(answer.unwrap_or(structured)), CallOutcome::Ok)
}

/// A tool outside the v1 catalog: the native's own result.
fn call_native(native: &dyn Capability, tool: &str, arguments: &Value) -> (Value, CallOutcome) {
    match native.call(tool, arguments) {
        Ok(result) => {
            let outcome = if result.get("isError").and_then(Value::as_bool) == Some(true) { CallOutcome::IsError } else { CallOutcome::Ok };
            (result, outcome)
        }
        Err(failure) => (mcp_error(&failure.message), CallOutcome::RpcError),
    }
}

/// Store every object carrying `data` (depth first, in order: an object,
/// then its members): it loses `data` and gains `size` and `fileId`.
fn store_files(cx: &BridgeContext, value: &mut Value) -> Result<(), String> {
    match value {
        Value::Object(o) => {
            if let Some(Value::String(data)) = o.get("data") {
                let bytes = base64_decode(data).ok_or_else(|| "badResult: data".to_string())?;
                let name = o.get("name").and_then(Value::as_str).unwrap_or_default();
                let mime = o.get("mimeType").and_then(Value::as_str).unwrap_or_default();
                let id = cx.blobs.put(cx.app, name, mime, &bytes).map_err(|f| f.message)?;
                o.shift_remove("data");
                o.insert("size".into(), json!(bytes.len()));
                o.insert("fileId".into(), json!(id));
            }
            for (_, v) in o.iter_mut() {
                store_files(cx, v)?;
            }
        }
        Value::Array(a) => {
            for v in a {
                store_files(cx, v)?;
            }
        }
        _ => {}
    }
    Ok(())
}
