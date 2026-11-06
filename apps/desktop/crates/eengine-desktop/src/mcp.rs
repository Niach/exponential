//! MCP call results and tool objects (capabilities.json `tools`).

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use eengine::contract::CAPABILITIES;
use eengine::providers::Failure;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::{NativeError, NativeResult};

/// `{content: [{type: text, text: <json>}], structuredContent}`.
pub(crate) fn ok(structured: Value) -> Value {
    json!({"content": [{"type": "text", "text": structured.to_string()}], "structuredContent": structured})
}

/// `{content: [{type: text, text: <reason>}], isError: true}`.
pub(crate) fn error(reason: &str) -> Value {
    json!({"content": [{"type": "text", "text": reason}], "isError": true})
}

pub(crate) fn failed(message: &str) -> Value {
    error(&format!("failed: {message}"))
}

/// A seam's answer as an MCP result.
pub(crate) fn answer(result: NativeResult<Value>) -> Value {
    match result {
        Ok(structured) => ok(structured),
        Err(NativeError::Unavailable) => error("unavailable"),
        Err(NativeError::NotFound) => error("notFound"),
        Err(NativeError::Failed(message)) => failed(&message),
    }
}

/// The native tools of `capability` as MCP tool objects: the short `name`
/// (`pick`), the catalog's description and schemas.
pub(crate) fn tools(capability: &str) -> Vec<Value> {
    let prefix = format!("{capability}.");
    CAPABILITIES["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|tool| tool["runs"] == "native")
        .filter_map(|tool| {
            let name = tool["name"].as_str()?.strip_prefix(&prefix)?;
            Some(json!({
                "name": name,
                "description": tool["description"],
                "inputSchema": tool["inputSchema"],
                "outputSchema": tool["outputSchema"],
            }))
        })
        .collect()
}

pub(crate) fn unknown(capability: &str, tool: &str) -> Failure {
    Failure::new("UNKNOWN_TOOL", format!("unknown tool {capability}.{tool}"))
}

/// The bridge validated the arguments already; a mismatch is the host's bug.
pub(crate) fn arguments<T: DeserializeOwned>(arguments: &Value) -> NativeResult<T> {
    serde_json::from_value(arguments.clone()).map_err(|e| NativeError::Failed(format!("bad arguments: {e}")))
}

pub(crate) fn encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub(crate) fn decode(data: &str) -> NativeResult<Vec<u8>> {
    STANDARD.decode(data).map_err(|_| NativeError::Failed("bad base64 data".into()))
}

/// The most bytes one file may carry through a call (base64 in JSON).
pub(crate) const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Read a file the person picked, refusing one too big to carry.
pub(crate) fn read_file(path: &std::path::Path) -> NativeResult<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(|e| NativeError::Failed(format!("cannot read {}: {e}", path.display())))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(NativeError::Failed(format!("{} is larger than 64 MB", file_name(path))));
    }
    std::fs::read(path).map_err(|e| NativeError::Failed(format!("cannot read {}: {e}", path.display())))
}

/// A path's file name, at most 255 characters (the schemas' `maxLength`).
pub(crate) fn file_name(path: &std::path::Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = if name.is_empty() { "file".to_string() } else { name };
    truncate(&name, 255)
}

pub(crate) fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// A file's mtime as RFC 3339 UTC (`takenAt`), None when the platform has none.
pub(crate) fn mtime_rfc3339(meta: &std::fs::Metadata) -> Option<String> {
    let modified = meta.modified().ok()?;
    let at: chrono::DateTime<chrono::Utc> = modified.into();
    Some(at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}
