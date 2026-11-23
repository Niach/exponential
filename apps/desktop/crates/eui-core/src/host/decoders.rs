//! The wire decoders every transport adapter shares (pure, incremental,
//! fixture-locked in `fixtures/host-transport.json`): A2UI JSONL over a byte
//! stream, Server-Sent Events, and the A2UI-over-MCP carrier.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::contract::{EXPRESS_MIME_TYPE, EXPRESS_SSE_EVENT, EXPRESS_STRUCTURED_KEY, EXPRESS_WS_KEY, MCP_ACTION_TOOL, MCP_MIME_TYPES, SSE_EVENTS};
use super::js_trim;
use crate::limits::MAX_MESSAGE_BYTES;

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
    /// #AXC-12: the prose around an Express response's `<a2ui>` blocks, in
    /// order, as it streamed in (the host shows it: `onText`; the SDK draws
    /// none). Always empty for JSONL and SSE.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text: Vec<String>,
}

impl Decoded {
    pub fn extend(&mut self, other: Decoded) {
        self.messages.extend(other.messages);
        self.issues.extend(other.issues);
        self.text.extend(other.text);
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

/// VAPP-103 rfix: one line of a byte stream, or a line past
/// `maxMessageBytes` (its bytes are dropped up to the next newline).
#[derive(Debug, Clone, PartialEq)]
enum Line {
    Text(String),
    Oversized,
}

/// The line splitter both stream decoders share: the buffer holds only the
/// unterminated tail (each push scans the new bytes once and drains once),
/// never more than `maxMessageBytes`.
#[derive(Debug, Clone, Default)]
struct Lines {
    buffer: String,
    /// Bytes of `buffer` already scanned (no newline in them).
    scanned: usize,
    /// Inside an oversized line: drop until its newline.
    skipping: bool,
}

impl Lines {
    fn push(&mut self, chunk: &str) -> Vec<Line> {
        let mut out = Vec::new();
        let mut rest = chunk;
        if self.skipping {
            match rest.find('\n') {
                None => return out,
                Some(i) => {
                    self.skipping = false;
                    rest = &rest[i + 1..];
                }
            }
        }
        self.buffer.push_str(rest);
        let mut start = 0;
        let mut from = self.scanned;
        while let Some(i) = self.buffer[from..].find('\n') {
            let nl = from + i;
            let line = &self.buffer[start..nl];
            out.push(if line.len() > MAX_MESSAGE_BYTES { Line::Oversized } else { Line::Text(line.to_string()) });
            start = nl + 1;
            from = start;
        }
        if start > 0 {
            self.buffer.drain(..start);
        }
        self.scanned = self.buffer.len();
        if self.buffer.len() > MAX_MESSAGE_BYTES {
            out.push(Line::Oversized);
            self.buffer = String::new();
            self.scanned = 0;
            self.skipping = true;
        }
        out
    }

    /// The unterminated tail (`None` inside an oversized line).
    fn end(&mut self) -> Option<String> {
        let skipping = std::mem::take(&mut self.skipping);
        self.scanned = 0;
        let tail = std::mem::take(&mut self.buffer);
        (!skipping).then_some(tail)
    }
}

/// One message per line. `push` takes any chunking (a line may span chunks),
/// `end` flushes a last line without a newline. A line past
/// `maxMessageBytes` is an issue (`line N: message larger than … bytes`),
/// never buffered.
#[derive(Debug, Clone, Default)]
pub struct JsonlDecoder {
    lines: Lines,
    line: u32,
}

impl JsonlDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> Decoded {
        let mut out = Decoded::default();
        for line in self.lines.push(chunk) {
            self.line += 1;
            match line {
                Line::Text(raw) => parse_line(&raw, self.line, &mut out),
                Line::Oversized => out.issues.push(oversized(self.line, "line")),
            }
        }
        out
    }

    pub fn end(&mut self) -> Decoded {
        let mut out = Decoded::default();
        if let Some(raw) = self.lines.end().filter(|raw| !js_trim(raw).is_empty()) {
            self.line += 1;
            parse_line(&raw, self.line, &mut out);
        }
        out
    }
}

fn oversized(at: u32, what: &str) -> DecodeIssue {
    DecodeIssue { at, message: format!("{what} {at}: {}", crate::limits::message_bytes_issue()) }
}

/// A whole JSONL document (or one JSON value / a JSON array of messages).
pub fn decode_jsonl(text: &str) -> Decoded {
    let trimmed = js_trim(text);
    if trimmed.starts_with('[') {
        if let Ok(Value::Array(messages)) = serde_json::from_str::<Value>(trimmed) {
            return Decoded { messages, ..Decoded::default() };
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
/// the event. An event's data is one message or JSONL. #AXC-12: events named
/// `a2ui-express` are the chunks of the stream's ONE Express response (with
/// the `express` feature; without it each is an issue).
#[derive(Debug, Default)]
pub struct SseDecoder {
    lines: Lines,
    data: Vec<String>,
    /// The event's data bytes so far (past `maxMessageBytes` it is dropped
    /// and dispatches as an issue).
    data_bytes: usize,
    oversized: bool,
    event: String,
    count: u32,
    retry_ms: Option<u64>,
    /// The surface Express statements before any `surface("…")` target.
    surface_id: Option<String>,
    express: Option<Express>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// #AXC-12: Express events target `surface_id` (default
    /// `EXPRESS_DEFAULT_SURFACE_ID`).
    pub fn with_surface(surface_id: Option<&str>) -> Self {
        SseDecoder { surface_id: surface_id.map(str::to_string), ..Self::default() }
    }

    /// The last `retry:` field (ms): the server resumes the stream after it
    /// ends (a transport reconnects on a clean end only then).
    pub fn retry_ms(&self) -> Option<u64> {
        self.retry_ms
    }

    pub fn push(&mut self, chunk: &str) -> Decoded {
        let mut out = Decoded::default();
        for line in self.lines.push(chunk) {
            match line {
                Line::Text(mut line) => {
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    self.line(&line, &mut out);
                }
                Line::Oversized => self.drop_data(),
            }
        }
        out
    }

    pub fn end(&mut self) -> Decoded {
        let mut out = Decoded::default();
        match self.lines.end() {
            Some(line) if !line.is_empty() => self.line(&line, &mut out),
            Some(_) => {}
            None => self.drop_data(),
        }
        self.dispatch(&mut out);
        if let Some(express) = self.express.as_mut() {
            let d = express.end();
            express_out(d, self.count, "event", &mut out);
        }
        out
    }

    /// The event is past `maxMessageBytes`: forget its data (it dispatches
    /// as an issue).
    fn drop_data(&mut self) {
        self.data.clear();
        self.data_bytes = 0;
        self.oversized = true;
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
            if self.oversized {
                return;
            }
            self.data_bytes += value.len() + 1;
            if self.data_bytes > MAX_MESSAGE_BYTES {
                return self.drop_data();
            }
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
        self.data_bytes = 0;
        if std::mem::take(&mut self.oversized) {
            self.count += 1;
            self.data.clear();
            self.event.clear();
            out.issues.push(oversized(self.count, "event"));
            return;
        }
        if self.data.is_empty() {
            self.event.clear();
            return;
        }
        self.count += 1;
        let name = if self.event.is_empty() { "message".to_string() } else { std::mem::take(&mut self.event) };
        let data = std::mem::take(&mut self.data).join("\n");
        self.event.clear();
        if name == EXPRESS_SSE_EVENT {
            let count = self.count;
            let express = self.express.get_or_insert_with(|| Express::new(self.surface_id.as_deref()));
            let d = express.push(&data);
            return express_out(d, count, "event", out);
        }
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
/// `structuredContent.a2ui`; #AXC-12: Express resources and
/// `structuredContent.a2uiExpress`, each one bare body).
pub fn messages_from_mcp_result(result: &Value) -> Decoded {
    messages_from_mcp_result_with(result, None)
}

/// [`messages_from_mcp_result`] with the surface Express statements target.
pub fn messages_from_mcp_result_with(result: &Value, surface_id: Option<&str>) -> Decoded {
    let mut out = Decoded::default();
    if !matches!(result, Value::Object(_) | Value::Array(_)) {
        return out;
    }
    if let Some(Value::Array(structured)) = result.get("structuredContent").and_then(|s| s.get("a2ui")) {
        out.messages.extend(structured.iter().cloned());
    }
    if let Some(Value::String(body)) = result.get("structuredContent").and_then(|s| s.get(EXPRESS_STRUCTURED_KEY)) {
        let d = express_body(body, surface_id);
        let issues = d.issues.iter().map(|i| DecodeIssue { at: 0, message: format!("structuredContent.{EXPRESS_STRUCTURED_KEY}: {}", i.message) }).collect();
        out.extend(Decoded { issues, ..d });
    }
    if let Some(Value::Array(content)) = result.get("content") {
        for (i, item) in content.iter().enumerate() {
            let Some(resource) = item.get("resource").filter(|r| super::js_truthy(r)) else { continue };
            if item.get("type").and_then(Value::as_str) != Some("resource") {
                continue;
            }
            let mime = resource.get("mimeType").and_then(Value::as_str).unwrap_or("");
            let express = mime == EXPRESS_MIME_TYPE;
            if !express && !MCP_MIME_TYPES.contains(&mime) {
                continue;
            }
            let Some(Value::String(text)) = resource.get("text") else { continue };
            let d = if express { express_body(text, surface_id) } else { decode_jsonl(text) };
            let issues = d.issues.iter().map(|issue| DecodeIssue { at: i as u32 + 1, message: format!("content[{i}]: {}", issue.message) }).collect();
            out.extend(Decoded { issues, ..d });
        }
    }
    out
}

/// #AXC-12: the inference format of an HTTP body by its Content-Type
/// (parameters and case ignored): `express` for `application/a2ui+express`,
/// else `json`.
pub fn inference_format_of(content_type: &str) -> &'static str {
    let mime = content_type.split(';').next().unwrap_or("");
    if js_trim(mime).eq_ignore_ascii_case(EXPRESS_MIME_TYPE) {
        "express"
    } else {
        "json"
    }
}

/// #AXC-12: the Express chunk a WebSocket frame carries (`{"express": "…"}`:
/// a JSON object whose ONE key is `express`, a string), else `None`.
pub fn express_frame_chunk(frame: &str) -> Option<String> {
    let text = js_trim(frame);
    if !text.starts_with('{') || !text.contains(EXPRESS_WS_KEY) {
        return None;
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) if map.len() == 1 => map.get(EXPRESS_WS_KEY).and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// #AXC-12: WebSocket frames in order: an `{express}` frame is the next
/// chunk of the connection's ONE Express response (issues `frame N: …`);
/// any other frame is one message or JSONL (`frame N is not JSON`).
/// `end()` at close flushes the Express response.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    frames: u32,
    surface_id: Option<String>,
    express: Option<Express>,
}

impl FrameDecoder {
    pub fn new(surface_id: Option<&str>) -> Self {
        FrameDecoder { surface_id: surface_id.map(str::to_string), ..Self::default() }
    }

    pub fn push(&mut self, frame: &str) -> Decoded {
        self.frames += 1;
        let n = self.frames;
        let Some(chunk) = express_frame_chunk(frame) else {
            let d = decode_jsonl(frame);
            let issues = if d.issues.is_empty() { vec![] } else { vec![DecodeIssue { at: n, message: format!("frame {n} is not JSON") }] };
            return Decoded { messages: d.messages, issues, ..Decoded::default() };
        };
        let surface_id = self.surface_id.clone();
        let d = self.express.get_or_insert_with(|| Express::new(surface_id.as_deref())).push(&chunk);
        let mut out = Decoded::default();
        express_out(d, n, "frame", &mut out);
        out
    }

    pub fn end(&mut self) -> Decoded {
        let mut out = Decoded::default();
        if let Some(express) = self.express.as_mut() {
            express_out(express.end(), self.frames, "frame", &mut out);
        }
        out
    }
}

/// An Express read's output inside an outer stream: its issues carry the
/// outer event / frame number.
fn express_out(d: Decoded, n: u32, what: &str, out: &mut Decoded) {
    let issues = d.issues.into_iter().map(|issue| DecodeIssue { at: n, message: format!("{what} {n}: {}", issue.message) }).collect();
    out.extend(Decoded { messages: d.messages, issues, text: d.text });
}

/// The Express reader an outer decoder holds: the real one with the
/// `express` feature, else one that answers every chunk with an issue.
#[cfg(feature = "express")]
struct Express(crate::express::ExpressDecoder);

#[cfg(feature = "express")]
impl Express {
    fn new(surface_id: Option<&str>) -> Self {
        Express(crate::express::ExpressDecoder::new(true, surface_id.unwrap_or(super::contract::EXPRESS_DEFAULT_SURFACE_ID)))
    }
    fn push(&mut self, chunk: &str) -> Decoded {
        self.0.push(chunk)
    }
    fn end(&mut self) -> Decoded {
        self.0.end()
    }
}

#[cfg(not(feature = "express"))]
struct Express;

#[cfg(not(feature = "express"))]
const EXPRESS_UNAVAILABLE: &str = "Express is not available (eui-core feature `express`)";

#[cfg(not(feature = "express"))]
impl Express {
    fn new(_surface_id: Option<&str>) -> Self {
        Express
    }
    fn push(&mut self, _chunk: &str) -> Decoded {
        Decoded { issues: vec![DecodeIssue { at: 1, message: EXPRESS_UNAVAILABLE.into() }], ..Decoded::default() }
    }
    fn end(&mut self) -> Decoded {
        Decoded::default()
    }
}

impl std::fmt::Debug for Express {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Express")
    }
}

/// One bare Express body (an MCP resource, `structuredContent`), whole.
fn express_body(text: &str, surface_id: Option<&str>) -> Decoded {
    #[cfg(feature = "express")]
    {
        let mut d = crate::express::ExpressDecoder::new(false, surface_id.unwrap_or(super::contract::EXPRESS_DEFAULT_SURFACE_ID));
        let mut out = d.push(text);
        out.extend(d.end());
        out
    }
    #[cfg(not(feature = "express"))]
    {
        let _ = (text, surface_id);
        Decoded { issues: vec![DecodeIssue { at: 1, message: EXPRESS_UNAVAILABLE.into() }], ..Decoded::default() }
    }
}

/// A client message as the MCP tools/call that carries it back (`tool`
/// defaults to `a2ui_event`).
pub fn mcp_action_call(message: &Value, tool: Option<&str>) -> Value {
    mcp_action_call_with(message, tool, None)
}

/// [`mcp_action_call`] with the §1.3 metadata beside the message.
pub fn mcp_action_call_with(message: &Value, tool: Option<&str>, metadata: Option<&Value>) -> Value {
    let arguments = match metadata {
        Some(metadata) => json!({"message": message, "metadata": metadata}),
        None => json!({"message": message}),
    };
    json!({"method": "tools/call", "params": {"name": tool.unwrap_or(MCP_ACTION_TOOL), "arguments": arguments}})
}

#[cfg(test)]
mod tests {
    use super::{JsonlDecoder, SseDecoder, MAX_MESSAGE_BYTES};

    /// VAPP-103 rfix: a line past `maxMessageBytes` is ONE issue and never
    /// buffered; the lines around it decode; a stream fed byte by byte
    /// scans linearly (the old find+drain per line was O(n²)).
    #[test]
    fn jsonl_caps_a_line_at_max_message_bytes() {
        let mut d = JsonlDecoder::new();
        let mut out = d.push("{\"a\":1}\n");
        let big = "x".repeat(1 << 20);
        for _ in 0..5 {
            out.extend(d.push(&big));
        }
        assert!(d.lines.buffer.is_empty(), "the oversized line is not buffered");
        out.extend(d.push("still the big line\n{\"b\":2}\n"));
        out.extend(d.end());
        assert_eq!(out.messages, vec![serde_json::json!({"a": 1}), serde_json::json!({"b": 2})]);
        assert_eq!(out.issues.len(), 1);
        assert_eq!(out.issues[0].at, 2);
        assert_eq!(out.issues[0].message, format!("line 2: message larger than {MAX_MESSAGE_BYTES} bytes"));
        // A one-chunk oversized line too.
        let mut d = JsonlDecoder::new();
        let mut out = d.push(&format!("{}\n{{\"c\":3}}\n", "y".repeat(MAX_MESSAGE_BYTES + 1)));
        out.extend(d.end());
        assert_eq!((out.messages.len(), out.issues.len()), (1, 1));
    }

    #[test]
    fn jsonl_fed_byte_by_byte_is_linear() {
        let line = format!("{{\"text\":\"{}\"}}\n", "z".repeat(200_000));
        let mut d = JsonlDecoder::new();
        let t = std::time::Instant::now();
        let mut messages = 0;
        for chunk in line.as_bytes().chunks(7) {
            messages += d.push(std::str::from_utf8(chunk).unwrap()).messages.len();
        }
        assert_eq!(messages, 1);
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
    }

    #[test]
    fn sse_caps_an_event_at_max_message_bytes() {
        let mut d = SseDecoder::new();
        let mut out = d.push("data: {\"a\":1}\n\n");
        let chunk = format!("data: {}\n", "x".repeat(1 << 20));
        for _ in 0..5 {
            out.extend(d.push(&chunk));
        }
        assert!(d.data.is_empty(), "the oversized event's data is dropped");
        out.extend(d.push("\ndata: {\"b\":2}\n\n"));
        out.extend(d.end());
        assert_eq!(out.messages, vec![serde_json::json!({"a": 1}), serde_json::json!({"b": 2})]);
        assert_eq!(out.issues.len(), 1);
        assert_eq!(out.issues[0].message, format!("event 2: message larger than {MAX_MESSAGE_BYTES} bytes"));
    }

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
