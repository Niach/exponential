//! VAPP-91: the transports an [`crate::runtime::ExponentialHost`] reads A2UI
//! messages from and sends client messages to (the Rust mirror of
//! `packages/exponential-ui/src/host/transports.ts`). Every adapter decodes
//! through the core's decoders (`exponential_ui::host::{JsonlDecoder,
//! SseDecoder, messages_from_mcp_result, mcp_action_call}`), never its own
//! parsing.
//!
//! Threading: a transport may deliver from ANY thread. It hands messages and
//! status changes to its [`TransportSink`]; the host's pump lands them on
//! the gpui main thread. The `net` adapters run their I/O on their own std
//! threads (the blocking reqwest client, a private current-thread tokio
//! runtime per WebSocket) and never touch gpui's executors.

use std::sync::{Arc, Mutex};

use exponential_ui::host::decode_jsonl;
use serde_json::Value;

/// The connection state a host shows (`host_offline` when not `Open`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportStatus {
    Connecting,
    Open,
    Closed,
    Error,
}

impl TransportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TransportStatus::Connecting => "connecting",
            TransportStatus::Open => "open",
            TransportStatus::Closed => "closed",
            TransportStatus::Error => "error",
        }
    }
}

/// What a transport delivers.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportEvent {
    Message(Value),
    Status(TransportStatus, Option<String>),
}

/// Where a started transport delivers. `Send + Clone`: hand it to any thread.
#[derive(Clone)]
pub struct TransportSink {
    deliver: Arc<dyn Fn(TransportEvent) + Send + Sync>,
}

impl TransportSink {
    /// A sink over any callback (the host builds its own; tests may too).
    pub fn new(deliver: impl Fn(TransportEvent) + Send + Sync + 'static) -> Self {
        TransportSink { deliver: Arc::new(deliver) }
    }

    pub fn message(&self, message: Value) {
        (self.deliver)(TransportEvent::Message(message))
    }

    pub fn status(&self, status: TransportStatus, detail: Option<String>) {
        (self.deliver)(TransportEvent::Status(status, detail))
    }
}

/// Messages in, client messages out.
pub trait Transport: 'static {
    /// Start delivering messages and status changes to `sink`.
    fn start(&mut self, sink: TransportSink);
    /// One client message (A2UI `action` / `error`) to the server.
    fn send(&mut self, message: &Value);
    fn close(&mut self);
}

#[derive(Default)]
struct MemoryInner {
    sink: Option<TransportSink>,
    pending: Vec<Value>,
    sent: Vec<Value>,
}

/// In-memory: [`MemoryTransport::feed`] messages in, read what the host sent
/// from [`MemoryTransport::sent`]. Cheap to clone; every clone is the same
/// transport (keep one, hand one to the host).
#[derive(Clone, Default)]
pub struct MemoryTransport {
    inner: Arc<Mutex<MemoryInner>>,
}

impl MemoryTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Deliver `messages` (queued until the host connects).
    pub fn feed(&self, messages: impl IntoIterator<Item = Value>) {
        let mut inner = self.inner.lock().unwrap();
        match inner.sink.clone() {
            Some(sink) => {
                drop(inner);
                for m in messages {
                    sink.message(m);
                }
            }
            None => inner.pending.extend(messages),
        }
    }

    /// JSONL text, as a stream server would send it.
    pub fn feed_jsonl(&self, text: &str) {
        self.feed(decode_jsonl(text).messages);
    }

    /// Every client message the host sent, oldest first.
    pub fn sent(&self) -> Vec<Value> {
        self.inner.lock().unwrap().sent.clone()
    }
}

impl Transport for MemoryTransport {
    fn start(&mut self, sink: TransportSink) {
        sink.status(TransportStatus::Open, None);
        let pending = {
            let mut inner = self.inner.lock().unwrap();
            inner.sink = Some(sink.clone());
            std::mem::take(&mut inner.pending)
        };
        for m in pending {
            sink.message(m);
        }
    }

    fn send(&mut self, message: &Value) {
        self.inner.lock().unwrap().sent.push(message.clone());
    }

    fn close(&mut self) {
        self.inner.lock().unwrap().sink = None;
    }
}

#[cfg(feature = "net")]
pub use net::*;

#[cfg(feature = "net")]
mod net {
    use std::io::Read;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use exponential_ui::host::{decode_jsonl, mcp_action_call, messages_from_mcp_result, Decoded, JsonlDecoder, SseDecoder};
    use serde_json::{json, Value};

    use super::{Transport, TransportSink, TransportStatus};

    /// Options of the HTTP stream transports (JSONL, SSE).
    #[derive(Debug, Clone)]
    pub struct HttpTransportOptions {
        /// The stream to read (GET).
        pub url: String,
        /// Where client messages go (POST, JSON body); `None` = `url`.
        pub post_url: Option<String>,
        pub headers: Vec<(String, String)>,
        /// Reconnect after a drop, ms (0 = never). Default 2000.
        pub reconnect_ms: u64,
    }

    impl HttpTransportOptions {
        pub fn new(url: impl Into<String>) -> Self {
            HttpTransportOptions { url: url.into(), post_url: None, headers: Vec::new(), reconnect_ms: 2000 }
        }

        pub fn post_url(mut self, url: impl Into<String>) -> Self {
            self.post_url = Some(url.into());
            self
        }

        pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
            self.headers.push((name.into(), value.into()));
            self
        }

        pub fn reconnect_ms(mut self, ms: u64) -> Self {
            self.reconnect_ms = ms;
            self
        }
    }

    fn client() -> reqwest::blocking::Client {
        // No total timeout: a live stream stays open for as long as the
        // server keeps it.
        reqwest::blocking::Client::builder().timeout(None::<Duration>).build().unwrap_or_else(|_| reqwest::blocking::Client::new())
    }

    /// Sleep up to `ms`, waking early when `closed` flips.
    fn wait(ms: u64, closed: &AtomicBool) {
        let mut left = ms;
        while left > 0 && !closed.load(Ordering::SeqCst) {
            let step = left.min(50);
            std::thread::sleep(Duration::from_millis(step));
            left -= step;
        }
    }

    /// A client message POST may take this long to connect...
    const POST_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
    /// ...and this long in total. The POST thread sends in order, so an
    /// unbounded request (a half-open connection) would stall every later
    /// client message forever; past this the message is dropped.
    const POST_TIMEOUT: Duration = Duration::from_secs(30);

    /// The POST side's client: bounded, unlike the stream's [`client`].
    fn post_client(connect: Duration, total: Duration) -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder().connect_timeout(connect).timeout(total).build().unwrap_or_else(|_| reqwest::blocking::Client::new())
    }

    /// The POST side: one thread sends client messages in order. Like the
    /// TS `StreamTransport.send` (a rejected fetch, no retry), a failed or
    /// timed-out POST drops that message; it reports `Error` (detail = why)
    /// to `sink` unless `closed`, and the next successful POST reports
    /// `Open` again. The thread exits once every [`flume::Sender`] is
    /// dropped or `closed` flips (`close`/`start`; still-queued messages
    /// are dropped) and its in-flight request (bounded by `total`) ends;
    /// it is detached, never joined (a join could block the main thread
    /// for `total`).
    fn poster(url: String, headers: Vec<(String, String)>, sink: Option<TransportSink>, closed: Arc<AtomicBool>, connect: Duration, total: Duration) -> (flume::Sender<Value>, std::thread::JoinHandle<()>) {
        let (tx, rx) = flume::unbounded::<Value>();
        let handle = std::thread::Builder::new()
            .name("exponential-ui post".into())
            .spawn(move || {
                let client = post_client(connect, total);
                let report = |status: TransportStatus, detail: Option<String>| {
                    if let Some(sink) = &sink {
                        if !closed.load(Ordering::SeqCst) {
                            sink.status(status, detail);
                        }
                    }
                };
                let mut failed = false;
                while let Ok(message) = rx.recv() {
                    if closed.load(Ordering::SeqCst) {
                        break;
                    }
                    let mut req = client.post(&url).header("content-type", "application/json");
                    for (k, v) in &headers {
                        req = req.header(k.as_str(), v.as_str());
                    }
                    let outcome = match req.body(message.to_string()).send() {
                        Ok(res) if res.status().is_success() => Ok(()),
                        Ok(res) => Err(format!("HTTP {}", res.status().as_u16())),
                        Err(e) => Err(e.to_string()),
                    };
                    match outcome {
                        Ok(()) if failed => {
                            failed = false;
                            report(TransportStatus::Open, None);
                        }
                        Ok(()) => {}
                        Err(e) => {
                            failed = true;
                            report(TransportStatus::Error, Some(format!("send failed: {e}")));
                        }
                    }
                }
            })
            .expect("spawn the post thread");
        (tx, handle)
    }

    /// Bytes → text across chunk boundaries (a code point may span two).
    #[derive(Default)]
    struct Utf8Chunks {
        rest: Vec<u8>,
    }

    impl Utf8Chunks {
        fn push(&mut self, bytes: &[u8]) -> String {
            self.rest.extend_from_slice(bytes);
            match std::str::from_utf8(&self.rest) {
                Ok(s) => {
                    let out = s.to_string();
                    self.rest.clear();
                    out
                }
                Err(e) if e.error_len().is_none() => {
                    let valid = e.valid_up_to();
                    let out = String::from_utf8_lossy(&self.rest[..valid]).into_owned();
                    self.rest.drain(..valid);
                    out
                }
                Err(_) => String::from_utf8_lossy(&std::mem::take(&mut self.rest)).into_owned(),
            }
        }

        fn end(&mut self) -> String {
            String::from_utf8_lossy(&std::mem::take(&mut self.rest)).into_owned()
        }
    }

    enum Wire {
        Jsonl(JsonlDecoder),
        Sse(SseDecoder),
    }

    impl Wire {
        fn push(&mut self, chunk: &str) -> Decoded {
            match self {
                Wire::Jsonl(d) => d.push(chunk),
                Wire::Sse(d) => d.push(chunk),
            }
        }

        fn end(&mut self) -> Decoded {
            match self {
                Wire::Jsonl(d) => d.end(),
                Wire::Sse(d) => d.end(),
            }
        }
    }

    #[derive(Clone, Copy)]
    enum Kind {
        Jsonl,
        Sse,
    }

    struct StreamTransport {
        options: HttpTransportOptions,
        kind: Kind,
        closed: Arc<AtomicBool>,
        sink: Option<TransportSink>,
        post: Option<flume::Sender<Value>>,
    }

    impl StreamTransport {
        fn new(options: HttpTransportOptions, kind: Kind) -> Self {
            StreamTransport { options, kind, closed: Arc::new(AtomicBool::new(false)), sink: None, post: None }
        }

        fn accept(&self) -> &'static str {
            match self.kind {
                Kind::Jsonl => "application/jsonl, application/x-ndjson",
                Kind::Sse => "text/event-stream",
            }
        }

        fn start(&mut self, sink: TransportSink) {
            self.close();
            let closed = Arc::new(AtomicBool::new(false));
            self.closed = closed.clone();
            self.sink = Some(sink.clone());
            let options = self.options.clone();
            let accept = self.accept();
            let kind = self.kind;
            std::thread::Builder::new()
                .name("exponential-ui stream".into())
                .spawn(move || {
                    let client = client();
                    let deliver = |d: Decoded| {
                        if !closed.load(Ordering::SeqCst) {
                            d.messages.into_iter().for_each(|m| sink.message(m));
                        }
                    };
                    while !closed.load(Ordering::SeqCst) {
                        sink.status(TransportStatus::Connecting, None);
                        let mut req = client.get(&options.url).header("accept", accept);
                        for (k, v) in &options.headers {
                            req = req.header(k.as_str(), v.as_str());
                        }
                        let outcome: Result<(), String> = (|| {
                            let mut res = req.send().map_err(|e| e.to_string())?;
                            if !res.status().is_success() {
                                return Err(format!("HTTP {}", res.status().as_u16()));
                            }
                            sink.status(TransportStatus::Open, None);
                            let mut wire = match kind {
                                Kind::Jsonl => Wire::Jsonl(JsonlDecoder::new()),
                                Kind::Sse => Wire::Sse(SseDecoder::new()),
                            };
                            let mut text = Utf8Chunks::default();
                            let mut buf = [0u8; 16 * 1024];
                            loop {
                                let n = res.read(&mut buf).map_err(|e| e.to_string())?;
                                if closed.load(Ordering::SeqCst) {
                                    return Ok(());
                                }
                                if n == 0 {
                                    break;
                                }
                                deliver(wire.push(&text.push(&buf[..n])));
                            }
                            let tail = text.end();
                            if !tail.is_empty() {
                                deliver(wire.push(&tail));
                            }
                            deliver(wire.end());
                            Ok(())
                        })();
                        if closed.load(Ordering::SeqCst) {
                            return;
                        }
                        match outcome {
                            Ok(()) => sink.status(TransportStatus::Closed, None),
                            Err(e) => sink.status(TransportStatus::Error, Some(e)),
                        }
                        if options.reconnect_ms == 0 {
                            return;
                        }
                        wait(options.reconnect_ms, &closed);
                    }
                })
                .expect("spawn the stream thread");
        }

        fn send(&mut self, message: &Value) {
            let post = self.post.get_or_insert_with(|| {
                let url = self.options.post_url.clone().unwrap_or_else(|| self.options.url.clone());
                poster(url, self.options.headers.clone(), self.sink.clone(), self.closed.clone(), POST_CONNECT_TIMEOUT, POST_TIMEOUT).0
            });
            let _ = post.send(message.clone());
        }

        /// Stops delivery at once; the reader thread exits at its next
        /// chunk (a blocking read cannot be interrupted). Dropping the POST
        /// sender ends the POST thread after its in-flight request (bounded
        /// by [`POST_TIMEOUT`]); queued, unsent client messages are dropped.
        fn close(&mut self) {
            self.closed.store(true, Ordering::SeqCst);
            self.sink = None;
            self.post = None;
        }
    }

    /// A2UI JSONL over a streamed HTTP response (`application/jsonl`).
    pub struct JsonlStreamTransport(StreamTransport);

    impl JsonlStreamTransport {
        pub fn new(options: HttpTransportOptions) -> Self {
            JsonlStreamTransport(StreamTransport::new(options, Kind::Jsonl))
        }
    }

    impl Transport for JsonlStreamTransport {
        fn start(&mut self, sink: TransportSink) {
            self.0.start(sink)
        }
        fn send(&mut self, message: &Value) {
            self.0.send(message)
        }
        fn close(&mut self) {
            self.0.close()
        }
    }

    /// Server-Sent Events over a streamed GET (headers allowed).
    pub struct SseTransport(StreamTransport);

    impl SseTransport {
        pub fn new(options: HttpTransportOptions) -> Self {
            SseTransport(StreamTransport::new(options, Kind::Sse))
        }
    }

    impl Transport for SseTransport {
        fn start(&mut self, sink: TransportSink) {
            self.0.start(sink)
        }
        fn send(&mut self, message: &Value) {
            self.0.send(message)
        }
        fn close(&mut self) {
            self.0.close()
        }
    }

    /// One A2UI message (or JSONL) per text frame; client messages go back as
    /// frames. ws:// and wss:// (rustls, native roots).
    pub struct WebSocketTransport {
        url: String,
        reconnect_ms: u64,
        closed: Arc<AtomicBool>,
        out: Option<flume::Sender<Value>>,
    }

    impl WebSocketTransport {
        pub fn new(url: impl Into<String>) -> Self {
            WebSocketTransport { url: url.into(), reconnect_ms: 2000, closed: Arc::new(AtomicBool::new(false)), out: None }
        }

        /// Reconnect after a drop, ms (0 = never). Default 2000.
        pub fn reconnect_ms(mut self, ms: u64) -> Self {
            self.reconnect_ms = ms;
            self
        }
    }

    impl Transport for WebSocketTransport {
        fn start(&mut self, sink: TransportSink) {
            use futures_util::{SinkExt as _, StreamExt as _};
            use tokio_tungstenite::tungstenite::Message;

            self.close();
            let closed = Arc::new(AtomicBool::new(false));
            self.closed = closed.clone();
            let (tx, rx) = flume::unbounded::<Value>();
            self.out = Some(tx);
            let (url, reconnect_ms) = (self.url.clone(), self.reconnect_ms);
            std::thread::Builder::new()
                .name("exponential-ui websocket".into())
                .spawn(move || {
                    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                        sink.status(TransportStatus::Error, Some("no runtime".into()));
                        return;
                    };
                    rt.block_on(async move {
                        while !closed.load(Ordering::SeqCst) {
                            sink.status(TransportStatus::Connecting, None);
                            match tokio_tungstenite::connect_async(url.as_str()).await {
                                Ok((socket, _)) => {
                                    sink.status(TransportStatus::Open, None);
                                    let (mut write, mut read) = socket.split();
                                    loop {
                                        tokio::select! {
                                            frame = read.next() => match frame {
                                                Some(Ok(Message::Text(text))) => {
                                                    if !closed.load(Ordering::SeqCst) {
                                                        decode_jsonl(&text).messages.into_iter().for_each(|m| sink.message(m));
                                                    }
                                                }
                                                Some(Ok(Message::Close(_))) | None => break,
                                                Some(Err(e)) => {
                                                    sink.status(TransportStatus::Error, Some(e.to_string()));
                                                    break;
                                                }
                                                Some(Ok(_)) => {}
                                            },
                                            out = rx.recv_async() => match out {
                                                Ok(message) => {
                                                    let _ = write.send(Message::Text(message.to_string())).await;
                                                }
                                                Err(_) => {
                                                    let _ = write.send(Message::Close(None)).await;
                                                    return;
                                                }
                                            },
                                        }
                                    }
                                    if closed.load(Ordering::SeqCst) {
                                        return;
                                    }
                                    sink.status(TransportStatus::Closed, None);
                                }
                                Err(e) => {
                                    if closed.load(Ordering::SeqCst) {
                                        return;
                                    }
                                    sink.status(TransportStatus::Error, Some(e.to_string()));
                                }
                            }
                            if reconnect_ms == 0 {
                                return;
                            }
                            let mut left = reconnect_ms;
                            while left > 0 && !closed.load(Ordering::SeqCst) {
                                let step = left.min(50);
                                tokio::time::sleep(Duration::from_millis(step)).await;
                                left -= step;
                            }
                        }
                    });
                })
                .expect("spawn the websocket thread");
        }

        fn send(&mut self, message: &Value) {
            if let Some(out) = &self.out {
                let _ = out.send(message.clone());
            }
        }

        fn close(&mut self) {
            self.closed.store(true, Ordering::SeqCst);
            // Dropping the sender wakes the socket loop, which closes.
            self.out = None;
        }
    }

    /// Options of [`McpTransport`].
    #[derive(Debug, Clone)]
    pub struct McpTransportOptions {
        /// The MCP server's streamable-HTTP endpoint.
        pub url: String,
        /// The tool whose result carries the surface (called once at start).
        pub tool: String,
        pub arguments: Value,
        /// The tool client messages go to; `None` = `a2ui_event`.
        pub action_tool: Option<String>,
        pub headers: Vec<(String, String)>,
    }

    impl McpTransportOptions {
        pub fn new(url: impl Into<String>, tool: impl Into<String>) -> Self {
            McpTransportOptions { url: url.into(), tool: tool.into(), arguments: json!({}), action_tool: None, headers: Vec::new() }
        }
    }

    enum McpCommand {
        Send(Value),
    }

    /// A2UI over MCP: `initialize`, then `tools/call <tool>`; the A2UI
    /// resources of its result are delivered, and every client message goes
    /// back as a `tools/call` to the action tool (whose result may carry more
    /// messages). JSON-RPC over plain POST; an SSE reply body goes through
    /// the SSE decoder.
    pub struct McpTransport {
        options: McpTransportOptions,
        closed: Arc<AtomicBool>,
        commands: Option<flume::Sender<McpCommand>>,
    }

    impl McpTransport {
        pub fn new(options: McpTransportOptions) -> Self {
            McpTransport { options, closed: Arc::new(AtomicBool::new(false)), commands: None }
        }
    }

    struct Rpc {
        client: reqwest::blocking::Client,
        options: McpTransportOptions,
        id: u64,
        session: Option<String>,
    }

    impl Rpc {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
            self.id += 1;
            let id = self.id;
            let mut req = self.client.post(&self.options.url).header("content-type", "application/json").header("accept", "application/json, text/event-stream");
            if let Some(s) = &self.session {
                req = req.header("mcp-session-id", s.as_str());
            }
            for (k, v) in &self.options.headers {
                req = req.header(k.as_str(), v.as_str());
            }
            let res = req.body(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()).send().map_err(|e| e.to_string())?;
            if let Some(s) = res.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
                self.session = Some(s.to_string());
            }
            if !res.status().is_success() {
                return Err(format!("MCP HTTP {}", res.status().as_u16()));
            }
            let sse = res.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|t| t.contains("text/event-stream"));
            let body = res.text().map_err(|e| e.to_string())?;
            let replies: Vec<Value> = if sse {
                let mut d = SseDecoder::new();
                let mut out = d.push(&body);
                out.extend(d.end());
                out.messages
            } else {
                vec![serde_json::from_str(&body).map_err(|e| e.to_string())?]
            };
            let reply = replies.into_iter().find(|r| r.get("id").and_then(Value::as_u64) == Some(id)).ok_or_else(|| format!("MCP: no reply to {method}"))?;
            if let Some(err) = reply.get("error") {
                return Err(format!("MCP: {}", err.get("message").and_then(Value::as_str).unwrap_or("error")));
            }
            Ok(reply.get("result").cloned().unwrap_or(Value::Null))
        }
    }

    impl Transport for McpTransport {
        fn start(&mut self, sink: TransportSink) {
            self.close();
            let closed = Arc::new(AtomicBool::new(false));
            self.closed = closed.clone();
            let (tx, rx) = flume::unbounded::<McpCommand>();
            self.commands = Some(tx);
            let options = self.options.clone();
            std::thread::Builder::new()
                .name("exponential-ui mcp".into())
                .spawn(move || {
                    let mut rpc = Rpc { client: client(), options: options.clone(), id: 0, session: None };
                    let deliver = |result: &Value| {
                        if !closed.load(Ordering::SeqCst) {
                            messages_from_mcp_result(result).messages.into_iter().for_each(|m| sink.message(m));
                        }
                    };
                    sink.status(TransportStatus::Connecting, None);
                    let opened = rpc
                        .call("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "exponential-ui", "version": env!("CARGO_PKG_VERSION")}}))
                        .and_then(|_| rpc.call("tools/call", json!({"name": options.tool, "arguments": options.arguments})));
                    match opened {
                        Ok(result) => {
                            sink.status(TransportStatus::Open, None);
                            deliver(&result);
                        }
                        Err(e) => sink.status(TransportStatus::Error, Some(e)),
                    }
                    while let Ok(McpCommand::Send(message)) = rx.recv() {
                        let call = mcp_action_call(&message, options.action_tool.as_deref());
                        if let Ok(result) = rpc.call(call["method"].as_str().unwrap_or("tools/call"), call["params"].clone()) {
                            deliver(&result);
                        }
                    }
                })
                .expect("spawn the mcp thread");
        }

        fn send(&mut self, message: &Value) {
            if let Some(c) = &self.commands {
                let _ = c.send(McpCommand::Send(message.clone()));
            }
        }

        fn close(&mut self) {
            self.closed.store(true, Ordering::SeqCst);
            self.commands = None;
        }
    }

    #[cfg(test)]
    mod tests {
        use std::net::TcpListener;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        use serde_json::json;

        use super::{poster, Utf8Chunks};
        use crate::transport::{TransportEvent, TransportSink, TransportStatus};

        #[test]
        fn a_hung_post_reports_an_error_within_the_timeout_and_close_ends_the_thread() {
            // Accepts every connection and never answers (a half-open server).
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/a2ui", listener.local_addr().unwrap());
            let held = Arc::new(Mutex::new(Vec::new()));
            {
                let held = held.clone();
                std::thread::spawn(move || {
                    for conn in listener.incoming().flatten() {
                        held.lock().unwrap().push(conn);
                    }
                });
            }
            let events = Arc::new(Mutex::new(Vec::<TransportEvent>::new()));
            let sink = {
                let events = events.clone();
                TransportSink::new(move |e| events.lock().unwrap().push(e))
            };
            let total = Duration::from_millis(300);
            let (tx, handle) = poster(url, Vec::new(), Some(sink), Arc::new(AtomicBool::new(false)), Duration::from_millis(300), total);
            let began = Instant::now();
            tx.send(json!({"n": 1})).unwrap();
            tx.send(json!({"n": 2})).unwrap();
            // Both hang; the second is not stalled behind the first forever.
            let deadline = began + Duration::from_secs(5);
            loop {
                let errors = events.lock().unwrap().iter().filter(|e| matches!(e, TransportEvent::Status(TransportStatus::Error, Some(d)) if d.starts_with("send failed"))).count();
                if errors == 2 {
                    break;
                }
                assert!(Instant::now() < deadline, "the hung POSTs never timed out: {:?}", events.lock().unwrap());
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(began.elapsed() < total * 2 + Duration::from_secs(1));
            // Dropping the sender (what `close` does) ends the thread.
            tx.send(json!({"n": 3})).unwrap();
            drop(tx);
            let deadline = Instant::now() + Duration::from_secs(5);
            while !handle.is_finished() {
                assert!(Instant::now() < deadline, "the POST thread outlived close");
                std::thread::sleep(Duration::from_millis(20));
            }
            handle.join().unwrap();
        }

        #[test]
        fn a_code_point_split_across_chunks_survives() {
            let bytes = "21 °C".as_bytes();
            let mut t = Utf8Chunks::default();
            let a = t.push(&bytes[..4]);
            let b = t.push(&bytes[4..]);
            assert_eq!(format!("{a}{b}"), "21 °C");
            assert_eq!(t.end(), "");
        }
    }
}
