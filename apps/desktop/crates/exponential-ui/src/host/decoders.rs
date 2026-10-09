//! The wire decoders every transport adapter shares (pure, incremental,
//! fixture-locked in `fixtures/host-transport.json`): A2UI JSONL over a byte
//! stream, Server-Sent Events, and the A2UI-over-MCP carrier.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::contract::{MCP_ACTION_TOOL, MCP_MIME_TYPES, SSE_EVENTS};
use super::js_trim;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodeIssue {
    /// 1-based line (JSONL) or event (SSE) number.
    pub at: u32,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Decoded {
    pub messages: Vec<Value>,
    pub issues: Vec<DecodeIssue>,
}

impl Decoded {
    pub fn extend(&mut self, other: Decoded) {
        self.messages.extend(other.messages);
        self.issues.extend(other.issues);
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

fn parse_line(line: &str, at: u32, out: &mut Decoded) {
    let text = js_trim(line);
    if text.is_empty() {
        return;
    }
    match serde_json::from_str::<Value>(text) {
        Ok(v) => out.messages.push(v),
        Err(_) => out.issues.push(DecodeIssue { at, message: format!("line {at} is not JSON") }),
    }
}

/// One message per line. `push` takes any chunking (a line may span chunks),
/// `end` flushes a last line without a newline.
#[derive(Debug, Clone, Default)]
pub struct JsonlDecoder {
    buffer: String,
    line: u32,
}

impl JsonlDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> Decoded {
        let mut out = Decoded::default();
        self.buffer.push_str(chunk);
        while let Some(nl) = self.buffer.find('\n') {
            let raw: String = self.buffer[..nl].to_string();
            self.buffer.drain(..=nl);
            self.line += 1;
            parse_line(&raw, self.line, &mut out);
        }
        out
    }

    pub fn end(&mut self) -> Decoded {
        let mut out = Decoded::default();
        if !js_trim(&self.buffer).is_empty() {
            self.line += 1;
            let raw = std::mem::take(&mut self.buffer);
            parse_line(&raw, self.line, &mut out);
        }
        self.buffer.clear();
        out
    }
}

/// A whole JSONL document (or one JSON value / a JSON array of messages).
pub fn decode_jsonl(text: &str) -> Decoded {
    let trimmed = js_trim(text);
    if trimmed.starts_with('[') {
        if let Ok(Value::Array(messages)) = serde_json::from_str::<Value>(trimmed) {
            return Decoded { messages, issues: vec![] };
        }
        // fall through: JSONL whose first line is an array is still JSONL
    }
    let mut d = JsonlDecoder::new();
    let mut out = d.push(text);
    out.extend(d.end());
    out
}

/// Server-Sent Events: `data:` lines join with `\n` per event, a blank line
/// dispatches, `:` comments and other fields are ignored (`retry:` is kept
/// as [`SseDecoder::retry_ms`]), an `event:` name outside SSE_EVENTS drops
/// the event. An event's data is one message or JSONL.
#[derive(Debug, Clone, Default)]
pub struct SseDecoder {
    buffer: String,
    data: Vec<String>,
    event: String,
    count: u32,
    retry_ms: Option<u64>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The last `retry:` field (ms): the server resumes the stream after it
    /// ends (a transport reconnects on a clean end only then).
    pub fn retry_ms(&self) -> Option<u64> {
        self.retry_ms
    }

    pub fn push(&mut self, chunk: &str) -> Decoded {
        let mut out = Decoded::default();
        self.buffer.push_str(chunk);
        while let Some(nl) = self.buffer.find('\n') {
            let mut line: String = self.buffer[..nl].to_string();
            self.buffer.drain(..=nl);
            if line.ends_with('\r') {
                line.pop();
            }
            self.line(&line, &mut out);
        }
        out
    }

    pub fn end(&mut self) -> Decoded {
        let mut out = Decoded::default();
        if !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.line(&line, &mut out);
        }
        self.buffer.clear();
        self.dispatch(&mut out);
        out
    }

    fn line(&mut self, line: &str, out: &mut Decoded) {
        if line.is_empty() {
            return self.dispatch(out);
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.find(':') {
            Some(colon) => (&line[..colon], &line[colon + 1..]),
            None => (line, ""),
        };
        let value = value.strip_prefix(' ').unwrap_or(value);
        if field == "data" {
            self.data.push(value.to_string());
        } else if field == "event" {
            self.event = value.to_string();
        } else if field == "retry" && !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(ms) = value.parse() {
                self.retry_ms = Some(ms);
            }
        }
    }

    fn dispatch(&mut self, out: &mut Decoded) {
        if self.data.is_empty() {
            self.event.clear();
            return;
        }
        self.count += 1;
        let name = if self.event.is_empty() { "message".to_string() } else { std::mem::take(&mut self.event) };
        let data = std::mem::take(&mut self.data).join("\n");
        self.event.clear();
        if !SSE_EVENTS.contains(&name.as_str()) {
            return;
        }
        let d = decode_jsonl(&data);
        out.messages.extend(d.messages);
        if !d.issues.is_empty() {
            out.issues.push(DecodeIssue { at: self.count, message: format!("event {} is not JSON", self.count) });
        }
    }
}

/// The A2UI-over-MCP carrier: the messages inside an MCP tool result
/// (`content[]` resources with an A2UI mime type, or
/// `structuredContent.a2ui`).
pub fn messages_from_mcp_result(result: &Value) -> Decoded {
    let mut out = Decoded::default();
    if !matches!(result, Value::Object(_) | Value::Array(_)) {
        return out;
    }
    if let Some(Value::Array(structured)) = result.get("structuredContent").and_then(|s| s.get("a2ui")) {
        out.messages.extend(structured.iter().cloned());
    }
    if let Some(Value::Array(content)) = result.get("content") {
        for (i, item) in content.iter().enumerate() {
            let Some(resource) = item.get("resource").filter(|r| super::js_truthy(r)) else { continue };
            if item.get("type").and_then(Value::as_str) != Some("resource") {
                continue;
            }
            let mime = resource.get("mimeType").and_then(Value::as_str).unwrap_or("");
            if !MCP_MIME_TYPES.contains(&mime) {
                continue;
            }
            let Some(Value::String(text)) = resource.get("text") else { continue };
            let d = decode_jsonl(text);
            out.messages.extend(d.messages);
            for issue in d.issues {
                out.issues.push(DecodeIssue { at: i as u32 + 1, message: format!("content[{i}]: {}", issue.message) });
            }
        }
    }
    out
}

/// A client message as the MCP tools/call that carries it back (`tool`
/// defaults to `a2ui_event`).
pub fn mcp_action_call(message: &Value, tool: Option<&str>) -> Value {
    json!({"method": "tools/call", "params": {"name": tool.unwrap_or(MCP_ACTION_TOOL), "arguments": {"message": message}}})
}

#[cfg(test)]
mod tests {
    use super::SseDecoder;

    #[test]
    fn sse_keeps_the_last_numeric_retry_field_and_its_messages_are_unchanged() {
        let mut d = SseDecoder::new();
        assert_eq!(d.retry_ms(), None);
        let mut out = d.push("retry: 1500\ndata: {\"a\":1}\n\nretry: soon\n");
        out.extend(d.end());
        assert_eq!(out.messages, vec![serde_json::json!({"a": 1})]);
        assert_eq!(d.retry_ms(), Some(1500), "a non-numeric retry is ignored");
        d.push("retry:250\n\n");
        assert_eq!(d.retry_ms(), Some(250));
    }
}
