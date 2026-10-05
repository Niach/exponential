//! The computer-use MCP endpoint: streamable HTTP on loopback, answered with
//! plain JSON (no SSE stream; a `GET` is refused, which the transport
//! allows). One listener per host process; each run reaches it with its own
//! bearer token ([`crate::grant`]), which is what names the run.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use base64::Engine as _;
use serde_json::{json, Value};

use crate::backend::{Button, Delivery, Target};
use crate::guard::{Guard, Mapping, ToolOutput};

/// The config key and server name the agents see (`mcp__computer__click`).
pub const SERVER_NAME: &str = "computer";
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const BODY_MAX_BYTES: u64 = 1024 * 1024;

pub(crate) struct Run {
    pub session_id: String,
    mapping: Option<Mapping>,
    acted: bool,
}

pub(crate) struct Hub {
    pub guard: Guard,
    /// Token → run.
    runs: Mutex<HashMap<String, Run>>,
    /// The last input action, for the "agent is driving" indicator.
    driving: Mutex<Option<(Instant, String)>>,
}

impl Hub {
    pub fn new(guard: Guard) -> Self {
        Self {
            guard,
            runs: Mutex::default(),
            driving: Mutex::default(),
        }
    }

    /// A fresh token for `session_id`; an earlier one of the same run dies.
    pub fn grant(&self, session_id: &str) -> String {
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let mut runs = self.runs.lock().unwrap();
        runs.retain(|_, run| run.session_id != session_id);
        runs.insert(
            token.clone(),
            Run { session_id: session_id.to_string(), mapping: None, acted: false },
        );
        token
    }

    pub fn revoke(&self, session_id: &str) {
        self.runs.lock().unwrap().retain(|_, run| run.session_id != session_id);
    }

    pub fn driving(&self) -> Option<(Instant, String)> {
        self.driving.lock().unwrap().clone()
    }

    fn mapping(&self, token: &str) -> Option<Mapping> {
        self.runs.lock().unwrap().get(token).and_then(|run| run.mapping)
    }

    /// Stamp an input action of the run behind `token`.
    fn acted(&self, token: &str) {
        let (session_id, first) = {
            let mut runs = self.runs.lock().unwrap();
            let Some(run) = runs.get_mut(token) else { return };
            let first = !run.acted;
            run.acted = true;
            (run.session_id.clone(), first)
        };
        *self.driving.lock().unwrap() = Some((Instant::now(), session_id.clone()));
        if first {
            crate::fire_first_action(&session_id);
        }
    }

    /// One JSON-RPC message → its response (`None` for a notification).
    pub fn handle(&self, token: &str, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned()?;
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
                let version = PROTOCOL_VERSIONS
                    .iter()
                    .find(|version| **version == asked)
                    .unwrap_or(&PROTOCOL_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                Ok(tool_result(self.call(token, name, &args)))
            }
            _ => Err(json!({ "code": -32601, "message": format!("Method not found: {method}") })),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        })
    }

    fn call(&self, token: &str, name: &str, args: &Value) -> Result<ToolOutput, String> {
        let number = |key: &str| args.get(key).and_then(Value::as_f64);
        let point = || match (number("x"), number("y")) {
            (Some(x), Some(y)) => Ok((x, y)),
            _ => Err("x and y are required.".to_string()),
        };
        let window = args.get("window").and_then(Value::as_u64).map(|id| id as u32);
        let delivery = || Delivery::parse(args.get("delivery").and_then(Value::as_str));
        let output = match name {
            "screenshot" => {
                let target = match window {
                    Some(id) => Target::Window(id),
                    None => Target::Display(number("display").unwrap_or(0.0).max(0.0) as usize),
                };
                let path = args.get("path").and_then(Value::as_str).map(std::path::PathBuf::from);
                let (output, mapping) = self.guard.screenshot(target, path.as_deref())?;
                if let Some(run) = self.runs.lock().unwrap().get_mut(token) {
                    run.mapping = Some(mapping);
                }
                return Ok(output);
            }
            "list_windows" => return self.guard.list_windows(),
            "read_ui" => return self.guard.read_ui(self.mapping(token), window),
            "click" => {
                let (x, y) = point()?;
                let button = match args.get("button").and_then(Value::as_str).unwrap_or("left") {
                    "left" => Button::Left,
                    "right" => Button::Right,
                    "middle" => Button::Middle,
                    other => return Err(format!("`{other}` is not a button (left, right, middle).")),
                };
                let count = number("count").unwrap_or(1.0) as u8;
                self.guard.click(self.mapping(token), x, y, button, count, delivery()?, window)?
            }
            "scroll" => {
                let (x, y) = point()?;
                let dx = number("dx").unwrap_or(0.0) as i32;
                let dy = number("dy").unwrap_or(0.0) as i32;
                if dx == 0 && dy == 0 {
                    return Err("Give dx or dy, in wheel notches.".to_string());
                }
                self.guard.scroll(self.mapping(token), x, y, dx, dy, delivery()?, window)?
            }
            "type" => {
                let text = args.get("text").and_then(Value::as_str).filter(|text| !text.is_empty());
                let text = text.ok_or("text is required.")?;
                self.guard.type_text(text, delivery()?, window, self.mapping(token))?
            }
            "key" => {
                let keys = args.get("keys").and_then(Value::as_str).filter(|keys| !keys.is_empty());
                let keys = keys.ok_or("keys is required.")?;
                self.guard.key(keys, delivery()?, window, self.mapping(token))?
            }
            "focus_window" => self.guard.focus_window(window.ok_or("window is required.")?)?,
            other => return Err(format!("Unknown tool: {other}")),
        };
        // Only the real pointer and keyboard count as driving: a background
        // action leaves the person's input alone, so no pill and no notice.
        if !output.background {
            self.acted(token);
        }
        Ok(output)
    }
}

const INSTRUCTIONS: &str = "Sees and drives this computer's desktop. Take a screenshot first; \
click and scroll take pixel positions in the last screenshot. On macOS input goes to the target \
window in the background by default, without touching the person's pointer or focus; pass \
delivery \"foreground\" when an app ignores background input (games, canvas apps). A background \
action only reports that it was posted: verify with a screenshot. Foreground input waits while \
the person is using the keyboard or mouse.";

fn tool_result(outcome: Result<ToolOutput, String>) -> Value {
    match outcome {
        Ok(output) => {
            let mut content = Vec::new();
            if let Some(png) = output.png {
                content.push(json!({
                    "type": "image",
                    "data": base64::engine::general_purpose::STANDARD.encode(png),
                    "mimeType": "image/png",
                }));
            }
            content.push(json!({ "type": "text", "text": output.text }));
            json!({ "content": content })
        }
        Err(text) => json!({ "content": [{ "type": "text", "text": text }], "isError": true }),
    }
}

fn tool_definitions() -> Value {
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "name": name,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            },
        })
    };
    let x = json!({ "type": "number", "description": "Pixel x in the last screenshot" });
    let y = json!({ "type": "number", "description": "Pixel y in the last screenshot" });
    let window = |description: &str| json!({ "type": "integer", "description": description });
    let delivery = json!({
        "type": "string",
        "enum": ["foreground", "background"],
        "description": "Default: background where the OS allows (macOS: posted to the target \
            window, the person's pointer and focus untouched), else foreground. foreground = the \
            real pointer and keyboard, for apps that ignore background input",
    });
    let target = window("The window a background action goes to (default: the last screenshot's \
        window, else the one under the point or the focused one)");
    json!([
        tool(
            "screenshot",
            "Capture a display (default: the primary one) or one window. Later click/scroll \
             positions are pixels in THIS image. `path` also saves the PNG, e.g. to show it \
             with exponential_sessions_show.",
            json!({
                "display": { "type": "integer", "description": "Display index, 0 = primary" },
                "window": window("A window id from list_windows, instead of a display"),
                "path": { "type": "string", "description": "Absolute file path to also save the PNG to" },
            }),
            &[],
        ),
        tool(
            "click",
            "Click at a position of the last screenshot.",
            json!({
                "x": x, "y": y,
                "button": { "type": "string", "enum": ["left", "right", "middle"] },
                "count": { "type": "integer", "minimum": 1, "maximum": 3, "description": "2 = double click" },
                "delivery": delivery, "window": target,
            }),
            &["x", "y"],
        ),
        tool(
            "scroll",
            "Scroll at a position of the last screenshot, in wheel notches (dy > 0 = down, dx > 0 = right).",
            json!({
                "x": x, "y": y, "dx": { "type": "integer" }, "dy": { "type": "integer" },
                "delivery": delivery, "window": target,
            }),
            &["x", "y"],
        ),
        tool(
            "type",
            "Type text into the focused window.",
            json!({ "text": { "type": "string" }, "delivery": delivery, "window": target }),
            &["text"],
        ),
        tool(
            "key",
            "Press one key or chord in the focused window: `enter`, `cmd+shift+t`, `ctrl+c`, `F5`.",
            json!({ "keys": { "type": "string" }, "delivery": delivery, "window": target }),
            &["keys"],
        ),
        tool("list_windows", "List the open windows, front to back, with their ids.", json!({}), &[]),
        tool(
            "focus_window",
            "Bring a window to the front.",
            json!({ "window": window("A window id from list_windows") }),
            &["window"],
        ),
        tool(
            "read_ui",
            "Read a window's accessibility tree (the focused window by default) as text: roles, \
             labels and, for elements the last screenshot shows, their position in it. Cheaper \
             than a screenshot when you need text or a control's place.",
            json!({ "window": window("A window id from list_windows") }),
            &[],
        ),
    ])
}

/// Bind loopback and serve `hub` on a background thread; returns the URL.
pub(crate) fn serve(hub: Arc<Hub>) -> Result<String, String> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|err| format!("Could not start the computer-use server: {err}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or("The computer-use server has no TCP address.")?;
    std::thread::Builder::new()
        .name("computer-use-mcp".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                let hub = hub.clone();
                // A tool call may wait on the person or the turn lock; never
                // hold the accept loop for it.
                let _ = std::thread::Builder::new()
                    .name("computer-use-call".into())
                    .spawn(move || respond(&hub, request));
            }
        })
        .map_err(|err| format!("Could not start the computer-use server: {err}"))?;
    Ok(format!("http://127.0.0.1:{port}/mcp"))
}

fn respond(hub: &Hub, mut request: tiny_http::Request) {
    let json_header = || {
        tiny_http::Header::from_bytes("Content-Type", "application/json").expect("static header")
    };
    let plain = |status: u16| tiny_http::Response::empty(status).boxed();
    let token = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .and_then(|header| header.value.as_str().strip_prefix("Bearer "))
        .map(str::to_string)
        .filter(|token| hub.runs.lock().unwrap().contains_key(token));
    // Read the body before answering anything: a refusal sent over an unread
    // body makes the socket reset under the client instead of delivering it.
    let mut body = String::new();
    let read = request.as_reader().take(BODY_MAX_BYTES).read_to_string(&mut body).is_ok();
    let response = match (token, request.method()) {
        (None, _) => plain(401),
        (Some(token), tiny_http::Method::Post) => {
            match read.then(|| serde_json::from_str::<Value>(&body).ok()).flatten() {
                None => plain(400),
                Some(message) => match hub.handle(&token, &message) {
                    // A notification or a response of the client's: accepted.
                    None => plain(202),
                    Some(reply) => tiny_http::Response::from_string(reply.to_string())
                        .with_header(json_header())
                        .boxed(),
                },
            }
        }
        // No server-initiated stream, no session to delete.
        (Some(_), _) => plain(405),
    };
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Rect, WindowInfo};
    use crate::fake::FakeBackend;

    /// `background` = whether the fake implements background delivery.
    fn hub_with(background: bool) -> (Hub, Arc<Mutex<Vec<String>>>) {
        let backend = FakeBackend::with_windows(vec![WindowInfo {
            id: 9,
            pid: 7,
            app: "TextEdit".into(),
            exe: "TextEdit".into(),
            title: "Untitled".into(),
            rect: Rect { x: 0.0, y: 0.0, width: 1568.0, height: 1000.0 },
            focused: true,
            layer: 0,
        }]);
        backend.set_background(background);
        let log = backend.log();
        (Hub::new(Guard::new(Box::new(backend))), log)
    }

    /// A backend without background delivery (every OS but macOS).
    fn hub() -> (Hub, Arc<Mutex<Vec<String>>>) {
        hub_with(false)
    }

    fn call(hub: &Hub, token: &str, name: &str, arguments: Value) -> Value {
        hub.handle(
            token,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
        )
        .unwrap()["result"]
            .clone()
    }

    #[test]
    fn initialize_echoes_a_known_protocol_version_and_lists_the_tools() {
        let (hub, _) = hub();
        let token = hub.grant("s1");
        let init = hub
            .handle(&token, &json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }))
            .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "computer");
        assert!(hub.handle(&token, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let tools = hub.handle(&token, &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["screenshot", "click", "scroll", "type", "key", "list_windows", "focus_window", "read_ui"]
        );
        let unknown = hub.handle(&token, &json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn a_screenshot_returns_an_image_and_scopes_the_clicks_of_its_run() {
        let (hub, log) = hub();
        let (one, two) = (hub.grant("s1"), hub.grant("s2"));
        let shot = call(&hub, &one, "screenshot", json!({}));
        assert_eq!(shot["content"][0]["type"], "image");
        assert_eq!(shot["content"][0]["mimeType"], "image/png");
        // The other run took no screenshot: its click has nothing to map.
        let refused = call(&hub, &two, "click", json!({ "x": 10, "y": 10 }));
        assert_eq!(refused["isError"], true);
        assert!(hub.driving().is_none());
        let clicked = call(&hub, &one, "click", json!({ "x": 10, "y": 10, "button": "right" }));
        assert_eq!(clicked["content"][0]["text"], "Clicked (10, 10) in TextEdit.");
        assert_eq!(log.lock().unwrap().as_slice(), ["click 10,10 Right x1"]);
        assert_eq!(hub.driving().unwrap().1, "s1");
    }

    #[test]
    fn the_first_action_of_a_run_fires_the_hook_once() {
        let (hub, _) = hub();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        // The hooks are process-wide: keep only this test's run.
        crate::on_first_action(move |session| {
            if session == "first-action-run" {
                sink.lock().unwrap().push(session.to_string());
            }
        });
        let token = hub.grant("first-action-run");
        call(&hub, &token, "list_windows", json!({}));
        assert!(seen.lock().unwrap().is_empty());
        call(&hub, &token, "type", json!({ "text": "hi" }));
        call(&hub, &token, "key", json!({ "keys": "enter" }));
        assert_eq!(seen.lock().unwrap().as_slice(), ["first-action-run"]);
    }

    #[test]
    fn a_background_action_is_not_driving_and_fires_no_hook() {
        let (hub, log) = hub_with(true);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        crate::on_first_action(move |session| {
            if session == "background-run" {
                sink.lock().unwrap().push(session.to_string());
            }
        });
        let token = hub.grant("background-run");
        call(&hub, &token, "screenshot", json!({}));
        // No delivery given: the window under the point, in the background.
        let clicked = call(&hub, &token, "click", json!({ "x": 10, "y": 10 }));
        assert_eq!(clicked["content"][0]["text"], "Posted a click at (10, 10) to TextEdit (Untitled) in the background.");
        call(&hub, &token, "type", json!({ "text": "hi" }));
        assert_eq!(log.lock().unwrap().as_slice(), ["bg click 9 10,10 Left x1", "bg type 9 hi"]);
        assert!(hub.driving().is_none());
        assert!(seen.lock().unwrap().is_empty());
        // Foreground = the real pointer: driving, and the hook fires.
        call(&hub, &token, "key", json!({ "keys": "enter", "delivery": "foreground" }));
        assert_eq!(hub.driving().unwrap().1, "background-run");
        assert_eq!(seen.lock().unwrap().as_slice(), ["background-run"]);
    }

    #[test]
    fn a_new_grant_replaces_the_runs_old_token_and_a_revoke_ends_it() {
        let (hub, _) = hub();
        let old = hub.grant("s1");
        let new = hub.grant("s1");
        assert!(!hub.runs.lock().unwrap().contains_key(&old));
        assert!(hub.runs.lock().unwrap().contains_key(&new));
        hub.revoke("s1");
        assert!(hub.runs.lock().unwrap().is_empty());
    }

    #[test]
    fn the_endpoint_wants_the_runs_bearer_token() {
        let (hub, _) = hub();
        let hub = Arc::new(hub);
        let token = hub.grant("s1");
        let url = serve(hub).unwrap();
        let addr = url.trim_start_matches("http://").trim_end_matches("/mcp").to_string();
        let post = |auth: &str| {
            use std::io::Write;
            let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
            let mut stream = std::net::TcpStream::connect(&addr).unwrap();
            write!(
                stream,
                "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {auth}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        assert!(post("nope").starts_with("HTTP/1.1 401"));
        let ok = post(&token);
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        // Compared as JSON: the key order follows serde_json's features.
        let body: Value = serde_json::from_str(ok.rsplit("\r\n\r\n").next().unwrap()).unwrap();
        assert_eq!(body, json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
    }
}
