//! VAPP-91: the transports an [`crate::runtime::ExponentialHost`] reads A2UI
//! messages from and sends client messages to (the Rust mirror of
//! `packages/exponential-ui/src/host/transports.ts`). Every adapter decodes
//! through the core's decoders (`eui_core::host::{JsonlDecoder,
//! SseDecoder, messages_from_mcp_result, mcp_action_call}`), never its own
//! parsing.
//!
//! Threading: a transport may deliver from ANY thread. It hands messages and
//! status changes to its [`TransportSink`]; the host's pump lands them on
//! the gpui main thread. The `net` adapters run their I/O on their own std
//! threads (the blocking reqwest client, a private current-thread tokio
//! runtime per WebSocket) and never touch gpui's executors.
//!
//! #AXC-12: every adapter speaks one of two [`WireFormat`]s: A2UI JSON (the
//! default) or Express, the compact notation an agent writes, compiled by
//! the core's [`ExpressDecoder`] into the same A2UI messages. Express prose
//! (the text around a response's `<a2ui>` blocks) reaches the host as
//! [`TransportEvent::Text`]; the SDK draws none of it.

use std::sync::{Arc, Mutex};

use eui_core::host::{decode_jsonl, DecodeIssue, Decoded, ExpressDecoder};
use serde_json::Value;

/// #AXC-12: the Express wire constants (catalog/host.json `inference`).
pub use eui_core::host::contract::{EXPRESS_DEFAULT_SURFACE_ID, EXPRESS_MIME_TYPE, EXPRESS_SSE_EVENT, EXPRESS_WS_KEY};

/// #AXC-12: what a transport's server speaks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum WireFormat {
    /// A2UI JSON messages are what the transport asks for. An Express
    /// answer is still read (an `application/a2ui+express` body, the SSE
    /// event `a2ui-express`, WebSocket `{"express": …}` frames), its
    /// statements before any `surface("…")` line targeting
    /// [`EXPRESS_DEFAULT_SURFACE_ID`].
    #[default]
    Json,
    /// Express is what the transport asks for (the HTTP `accept`), compiled
    /// into A2UI messages as each statement becomes ready.
    Express {
        /// A model's whole response: prose with `<a2ui>…</a2ui>` blocks
        /// (the prose comes out as [`TransportEvent::Text`]). `false` = the
        /// stream is ONE Express body without tags.
        wrapped: bool,
        /// The surface of statements before any `surface("…")` line.
        surface_id: String,
    },
}

impl WireFormat {
    /// Express as a model writes it: prose around `<a2ui>` blocks.
    pub fn express(surface_id: impl Into<String>) -> Self {
        WireFormat::Express { wrapped: true, surface_id: surface_id.into() }
    }

    /// Express as one bare body (no `<a2ui>` tags, no prose).
    pub fn express_body(surface_id: impl Into<String>) -> Self {
        WireFormat::Express { wrapped: false, surface_id: surface_id.into() }
    }

    pub fn is_express(&self) -> bool {
        matches!(self, WireFormat::Express { .. })
    }

    /// The surface of statements before any `surface("…")` line (`None` =
    /// the contract default).
    pub fn surface_id(&self) -> Option<&str> {
        match self {
            WireFormat::Json => None,
            WireFormat::Express { surface_id, .. } => Some(surface_id),
        }
    }

    /// A fresh decoder for one Express body: the format's, or for
    /// [`WireFormat::Json`] a model's response on the default surface.
    pub fn express_decoder(&self) -> ExpressDecoder {
        match self {
            WireFormat::Json => ExpressDecoder::new(true, EXPRESS_DEFAULT_SURFACE_ID),
            WireFormat::Express { wrapped, surface_id } => ExpressDecoder::new(*wrapped, surface_id),
        }
    }
}

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
    /// #AXC-12: a piece of an Express response's prose, as it streamed in
    /// (append the pieces; a run after a block starts with `"\n\n"`).
    Text(String),
    /// #AXC-12: a decoder issue: an Express block that does not compile, a
    /// line / event / frame that is not JSON, an oversized message. The
    /// stream reads on past it; the host records it (`HostOptions::on_issue`).
    Issue(DecodeIssue),
    /// #VAPP-300: the surfaces whose Express block is still open (the
    /// decoder's `open`), around each decoded batch: `settled: false` BEFORE
    /// its messages (those surfaces stream as they arrive), `true` after
    /// (every other surface's block closed). The host reduces them
    /// `streaming`: a child not streamed yet renders as nothing.
    Open { surface_ids: Vec<String>, settled: bool },
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

    /// #AXC-12: a piece of Express prose.
    pub fn text(&self, text: String) {
        (self.deliver)(TransportEvent::Text(text))
    }

    /// One decoder output: its open block around its messages, its prose,
    /// then its issues.
    pub fn decoded(&self, decoded: Decoded) {
        decoded_events(decoded).into_iter().for_each(|e| self.event(e));
    }

    fn event(&self, event: TransportEvent) {
        (self.deliver)(event)
    }

    /// #VAPP-300: no Express block is open any more (a dropped stream).
    pub fn settle(&self) {
        self.event(TransportEvent::Open { surface_ids: Vec::new(), settled: true })
    }

    pub fn status(&self, status: TransportStatus, detail: Option<String>) {
        (self.deliver)(TransportEvent::Status(status, detail))
    }
}

fn decoded_events(decoded: Decoded) -> Vec<TransportEvent> {
    // #VAPP-300: the open block around the messages.
    let opening = TransportEvent::Open { surface_ids: decoded.open.clone(), settled: false };
    let settled = TransportEvent::Open { surface_ids: decoded.open, settled: true };
    let messages = decoded.messages.into_iter().map(TransportEvent::Message);
    let text = decoded.text.into_iter().map(TransportEvent::Text);
    std::iter::once(opening).chain(messages).chain(std::iter::once(settled)).chain(text).chain(decoded.issues.into_iter().map(TransportEvent::Issue)).collect()
}

/// Messages in, client messages out.
pub trait Transport: 'static {
    /// Start delivering messages and status changes to `sink`.
    fn start(&mut self, sink: TransportSink);
    /// One client message (A2UI `action` / `error`) to the server.
    fn send(&mut self, message: &Value);
    /// Round 9 §1.3: a message with the `sendDataModel` metadata beside it:
    /// `{"message", "metadata"}` on the wire (the MCP carrier unwraps it).
    fn send_with_metadata(&mut self, message: &Value, metadata: &Value) {
        self.send(&with_metadata(message, metadata));
    }
    fn close(&mut self);
}

/// §1.3: the WebSocket / JSONL wrap of a message with metadata.
pub fn with_metadata(message: &Value, metadata: &Value) -> Value {
    serde_json::json!({"message": message, "metadata": metadata})
}

/// The MCP `tools/call` of what [`Transport::send_with_metadata`] sent: the
/// wrap is opened, the metadata rides beside the message (§10).
pub fn mcp_call_of(sent: &Value, tool: Option<&str>) -> Value {
    match (sent.get("message"), sent.get("metadata")) {
        (Some(inner), Some(metadata)) => eui_core::host::mcp_action_call_with(inner, tool, Some(metadata)),
        _ => eui_core::host::mcp_action_call(sent, tool),
    }
}

#[derive(Default)]
struct MemoryInner {
    sink: Option<TransportSink>,
    pending: Vec<TransportEvent>,
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
        self.feed_events(messages.into_iter().map(TransportEvent::Message));
    }

    fn feed_events(&self, events: impl IntoIterator<Item = TransportEvent>) {
        let mut inner = self.inner.lock().unwrap();
        match inner.sink.clone() {
            Some(sink) => {
                drop(inner);
                for e in events {
                    sink.event(e);
                }
            }
            None => inner.pending.extend(events),
        }
    }

    /// #AXC-12: one decoder output (an [`ExpressDecoder`]'s `push`/`end`):
    /// its messages, its prose as [`TransportEvent::Text`], its issues as
    /// [`TransportEvent::Issue`].
    pub fn feed_decoded(&self, decoded: Decoded) {
        self.feed_events(decoded_events(decoded));
    }

    /// #AXC-12: a whole Express stream in `format`, as a server would send
    /// it in `chunks` (each delivered as it decodes).
    pub fn feed_express<'a>(&self, format: &WireFormat, chunks: impl IntoIterator<Item = &'a str>) {
        let mut decoder = format.express_decoder();
        for chunk in chunks {
            self.feed_decoded(decoder.push(chunk));
        }
        self.feed_decoded(decoder.end());
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
        for e in pending {
            sink.event(e);
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
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use eui_core::host::{inference_format_of, messages_from_mcp_result, Decoded, ExpressDecoder, FrameDecoder, JsonlDecoder, SseDecoder};
    use serde_json::{json, Value};

    use super::{Transport, TransportSink, TransportStatus, WireFormat};

    /// Options of the HTTP stream transports (JSONL, SSE).
    #[derive(Debug, Clone)]
    pub struct HttpTransportOptions {
        /// The stream to read (GET).
        pub url: String,
        /// Where client messages go (POST, JSON body); `None` = `url`.
        pub post_url: Option<String>,
        pub headers: Vec<(String, String)>,
        /// Reconnect after a drop (an error or an aborted stream), ms (0 =
        /// never). `None` = 2000, or the SSE `retry:` value when the stream
        /// sent one.
        pub reconnect_ms: Option<u64>,
        /// Reconnect after a CLEAN end of the stream too (the server resumes
        /// it). Default false: a stream that ends is done (status `Closed`).
        /// An SSE stream that sent a `retry:` field signals this itself.
        pub resumable: bool,
        /// #AXC-12: JSON (default) or Express: what the JSONL transport
        /// asks for, and the Express decoder's surface (and, for a body,
        /// wrapping). A body is read by its content type either way
        /// (`application/a2ui+express` = Express, else JSONL); SSE
        /// `a2ui-express` events are Express either way.
        pub format: WireFormat,
    }

    impl HttpTransportOptions {
        pub fn new(url: impl Into<String>) -> Self {
            HttpTransportOptions { url: url.into(), post_url: None, headers: Vec::new(), reconnect_ms: None, resumable: false, format: WireFormat::Json }
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
            self.reconnect_ms = Some(ms);
            self
        }

        pub fn resumable(mut self, resumable: bool) -> Self {
            self.resumable = resumable;
            self
        }

        pub fn format(mut self, format: WireFormat) -> Self {
            self.format = format;
            self
        }
    }

    /// THE builder of every eui-gpui reqwest client: reqwest 0.13 without a
    /// bundled provider builds its TLS from the process default and panics
    /// without one, so ring goes in first unless the host installed one
    /// already (the workspace's `exp-http` gate allowlists only this).
    pub(crate) fn http_client() -> reqwest::blocking::ClientBuilder {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::blocking::Client::builder()
    }

    fn client() -> reqwest::blocking::Client {
        // No total timeout: a live stream stays open for as long as the
        // server keeps it.
        http_client().timeout(None::<Duration>).build().unwrap_or_else(|_| http_client().build().expect("an HTTP client"))
    }

    /// A stream transport's generation, bumped by every `start`/`close`
    /// (the TS `generation`): a loop whose generation is stale exits, so an
    /// old loop asleep in its reconnect wait never runs beside a new one.
    #[derive(Clone)]
    struct Live {
        generation: Arc<AtomicU64>,
        at: u64,
    }

    impl Live {
        /// A fresh generation of `generation`.
        fn next(generation: &Arc<AtomicU64>) -> Live {
            Live { generation: generation.clone(), at: generation.fetch_add(1, Ordering::SeqCst) + 1 }
        }

        fn is(&self) -> bool {
            self.generation.load(Ordering::SeqCst) == self.at
        }
    }

    /// Sleep up to `ms`, waking early when `live` goes stale.
    fn wait(ms: u64, live: &Live) {
        let mut left = ms;
        while left > 0 && live.is() {
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
        http_client().connect_timeout(connect).timeout(total).build().unwrap_or_else(|_| http_client().build().expect("an HTTP client"))
    }

    /// The POST side: one thread sends client messages in order. Like the
    /// TS `StreamTransport.send` (a rejected fetch, no retry), a failed or
    /// timed-out POST drops that message; it reports `Error` (detail = why)
    /// to `sink` while `live`, and the next successful POST reports
    /// `Open` again. The thread exits once every [`flume::Sender`] is
    /// dropped or `live` goes stale (`close`/`start`; still-queued messages
    /// are dropped) and its in-flight request (bounded by `total`) ends;
    /// it is detached, never joined (a join could block the main thread
    /// for `total`).
    fn poster(url: String, headers: Vec<(String, String)>, sink: Option<TransportSink>, live: Live, connect: Duration, total: Duration) -> (flume::Sender<Value>, std::thread::JoinHandle<()>) {
        let (tx, rx) = flume::unbounded::<Value>();
        let handle = std::thread::Builder::new()
            .name("exponential-ui post".into())
            .spawn(move || {
                let client = post_client(connect, total);
                let report = |status: TransportStatus, detail: Option<String>| {
                    if let Some(sink) = &sink {
                        if live.is() {
                            sink.status(status, detail);
                        }
                    }
                };
                let mut failed = false;
                while let Ok(message) = rx.recv() {
                    if !live.is() {
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
        /// SSE: the core's decoder reads the `a2ui-express` events as one
        /// Express response beside the JSON events.
        Sse(SseDecoder),
        /// #AXC-12: an `application/a2ui+express` body.
        Express(ExpressDecoder),
    }

    impl Wire {
        /// The decoder of one stream response (`content_type` = the
        /// response's, sniffed by the core's `inference_format_of`).
        fn of(kind: Kind, format: &WireFormat, content_type: &str) -> Wire {
            match kind {
                Kind::Jsonl if inference_format_of(content_type) == "express" => Wire::Express(format.express_decoder()),
                Kind::Jsonl => Wire::Jsonl(JsonlDecoder::new()),
                Kind::Sse => Wire::Sse(SseDecoder::with_surface(format.surface_id())),
            }
        }

        fn push(&mut self, chunk: &str) -> Decoded {
            match self {
                Wire::Jsonl(d) => d.push(chunk),
                Wire::Sse(d) => d.push(chunk),
                Wire::Express(d) => d.push(chunk),
            }
        }

        fn end(&mut self) -> Decoded {
            match self {
                Wire::Jsonl(d) => d.end(),
                Wire::Sse(d) => d.end(),
                Wire::Express(d) => d.end(),
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
        generation: Arc<AtomicU64>,
        /// The running generation (`None` before `start` / after `close`).
        live: Option<Live>,
        sink: Option<TransportSink>,
        post: Option<flume::Sender<Value>>,
    }

    impl StreamTransport {
        fn new(options: HttpTransportOptions, kind: Kind) -> Self {
            StreamTransport { options, kind, generation: Arc::new(AtomicU64::new(0)), live: None, sink: None, post: None }
        }

        fn accept(&self) -> &'static str {
            match (self.kind, self.options.format.is_express()) {
                (Kind::Jsonl, false) => "application/jsonl, application/x-ndjson",
                (Kind::Jsonl, true) => "application/a2ui+express, application/jsonl;q=0.5, application/x-ndjson;q=0.5",
                (Kind::Sse, _) => "text/event-stream",
            }
        }

        fn start(&mut self, sink: TransportSink) {
            self.close();
            let live = Live::next(&self.generation);
            self.live = Some(live.clone());
            self.sink = Some(sink.clone());
            let options = self.options.clone();
            let accept = self.accept();
            let kind = self.kind;
            std::thread::Builder::new()
                .name("exponential-ui stream".into())
                .spawn(move || run_stream(options, accept, kind, sink, live))
                .expect("spawn the stream thread");
        }

        fn send(&mut self, message: &Value) {
            let post = self.post.get_or_insert_with(|| {
                let url = self.options.post_url.clone().unwrap_or_else(|| self.options.url.clone());
                // Before `start`: a generation of its own, gone at the next start/close.
                let live = self.live.clone().unwrap_or_else(|| Live::next(&self.generation));
                poster(url, self.options.headers.clone(), self.sink.clone(), live, POST_CONNECT_TIMEOUT, POST_TIMEOUT).0
            });
            let _ = post.send(message.clone());
        }

        /// Stops delivery at once (the generation goes stale); the reader
        /// thread exits at its next chunk (a blocking read cannot be
        /// interrupted). Dropping the POST sender ends the POST thread after
        /// its in-flight request (bounded by [`POST_TIMEOUT`]); queued,
        /// unsent client messages are dropped.
        fn close(&mut self) {
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.live = None;
            self.sink = None;
            self.post = None;
        }
    }

    /// How one stream request ended.
    enum StreamEnd {
        /// A clean end of the body; `retry_ms` = the SSE `retry:` field.
        Clean { retry_ms: Option<u64> },
        /// The generation went stale mid-stream.
        Stale,
    }

    /// The stream loop (the TS `StreamTransport.run`): connect, decode,
    /// deliver. An error reconnects after `reconnect_ms`; a CLEAN end stops
    /// (status `Closed`) unless `resumable` or the SSE stream sent `retry:`.
    /// `live` is checked after every blocking step.
    fn run_stream(options: HttpTransportOptions, accept: &'static str, kind: Kind, sink: TransportSink, live: Live) {
        let client = client();
        let deliver = |d: Decoded| {
            if live.is() {
                sink.decoded(d);
            }
        };
        while live.is() {
            sink.status(TransportStatus::Connecting, None);
            let mut req = client.get(&options.url).header("accept", accept);
            for (k, v) in &options.headers {
                req = req.header(k.as_str(), v.as_str());
            }
            let outcome: Result<StreamEnd, String> = (|| {
                let mut res = req.send().map_err(|e| e.to_string())?;
                if !live.is() {
                    return Ok(StreamEnd::Stale);
                }
                if !res.status().is_success() {
                    return Err(format!("HTTP {}", res.status().as_u16()));
                }
                sink.status(TransportStatus::Open, None);
                let content_type = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                let mut wire = Wire::of(kind, &options.format, &content_type);
                let mut text = Utf8Chunks::default();
                let mut buf = [0u8; 16 * 1024];
                loop {
                    let n = res.read(&mut buf).map_err(|e| e.to_string())?;
                    if !live.is() {
                        return Ok(StreamEnd::Stale);
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
                let retry_ms = match &wire {
                    Wire::Sse(d) => d.retry_ms(),
                    Wire::Jsonl(_) | Wire::Express(_) => None,
                };
                Ok(StreamEnd::Clean { retry_ms })
            })();
            if !live.is() {
                return;
            }
            let mut wait_ms = options.reconnect_ms.unwrap_or(2000);
            match outcome {
                Ok(StreamEnd::Stale) => return,
                Ok(StreamEnd::Clean { retry_ms }) => {
                    sink.status(TransportStatus::Closed, None);
                    // A clean end: done, unless the server said it resumes the stream.
                    if !options.resumable && retry_ms.is_none() {
                        return;
                    }
                    if let (Some(ms), None) = (retry_ms, options.reconnect_ms) {
                        wait_ms = ms;
                    }
                }
                Err(e) => {
                    // #VAPP-300: a dropped stream's open block never closes: settle it.
                    sink.settle();
                    sink.status(TransportStatus::Error, Some(e))
                }
            }
            if wait_ms == 0 {
                return;
            }
            wait(wait_ms, &live);
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
    /// frames. ws:// and wss:// (rustls, native roots). #AXC-12: a
    /// `{"express": "<chunk>"}` frame is a chunk of the connection's ONE
    /// Express response (the core's `FrameDecoder`; it ends with the
    /// connection).
    pub struct WebSocketTransport {
        url: String,
        reconnect_ms: u64,
        format: WireFormat,
        closed: Arc<AtomicBool>,
        out: Option<flume::Sender<Value>>,
    }

    impl WebSocketTransport {
        pub fn new(url: impl Into<String>) -> Self {
            WebSocketTransport { url: url.into(), reconnect_ms: 2000, format: WireFormat::Json, closed: Arc::new(AtomicBool::new(false)), out: None }
        }

        /// Reconnect after a drop, ms (0 = never). Default 2000.
        pub fn reconnect_ms(mut self, ms: u64) -> Self {
            self.reconnect_ms = ms;
            self
        }

        /// #AXC-12: JSON (default) or Express.
        pub fn format(mut self, format: WireFormat) -> Self {
            self.format = format;
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
            let (url, reconnect_ms, format) = (self.url.clone(), self.reconnect_ms, self.format.clone());
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
                                    let mut frames = FrameDecoder::new(format.surface_id());
                                    loop {
                                        tokio::select! {
                                            frame = read.next() => match frame {
                                                Some(Ok(Message::Text(text))) => {
                                                    let decoded = frames.push(&text);
                                                    if !closed.load(Ordering::SeqCst) {
                                                        sink.decoded(decoded);
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
                                                    let _ = write.send(Message::text(message.to_string())).await;
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
                                    // The connection's Express response ends with it.
                                    sink.decoded(frames.end());
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

    /// An MCP `initialize` / `tools/call` may take this long in total
    /// (connect bounded by [`POST_CONNECT_TIMEOUT`]). Longer than a plain
    /// client-message POST: a tool may legitimately run for a while. The
    /// MCP thread serves calls in order, so past this the call fails (its
    /// error is reported) instead of stalling every later client message.
    const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(120);

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
                    let mut rpc = Rpc { client: post_client(POST_CONNECT_TIMEOUT, MCP_CALL_TIMEOUT), options: options.clone(), id: 0, session: None };
                    let deliver = |result: &Value| {
                        if !closed.load(Ordering::SeqCst) {
                            sink.decoded(messages_from_mcp_result(result));
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
                        let call = super::mcp_call_of(&message, options.action_tool.as_deref());
                        // A failed or timed-out call drops that message and
                        // reports why (the TS `McpTransport.send` rejects).
                        match rpc.call(call["method"].as_str().unwrap_or("tools/call"), call["params"].clone()) {
                            Ok(result) => deliver(&result),
                            Err(e) if !closed.load(Ordering::SeqCst) => sink.status(TransportStatus::Error, Some(e)),
                            Err(_) => {}
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
        use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        use serde_json::json;

        use super::{poster, post_client, HttpTransportOptions, JsonlStreamTransport, Live, McpTransportOptions, Rpc, SseTransport, Utf8Chunks};
        use crate::transport::{Transport, WireFormat};
        use crate::transport::{TransportEvent, TransportSink, TransportStatus};

        /// Answers every GET with `status` + `body` (then closes); counts them.
        fn stream_server(status: &'static str, content_type: &'static str, body: String) -> (String, Arc<AtomicUsize>) {
            use std::io::{Read as _, Write as _};
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/stream", listener.local_addr().unwrap());
            let gets = Arc::new(AtomicUsize::new(0));
            let counter = gets.clone();
            std::thread::spawn(move || {
                for mut conn in listener.incoming().flatten() {
                    let mut req = Vec::new();
                    let mut byte = [0u8; 1];
                    while !req.ends_with(b"\r\n\r\n") && conn.read(&mut byte).map(|n| n == 1).unwrap_or(false) {
                        req.push(byte[0]);
                    }
                    counter.fetch_add(1, AtomicOrdering::SeqCst);
                    let head = format!("HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
                    let _ = conn.write_all(head.as_bytes());
                    let _ = conn.write_all(body.as_bytes());
                }
            });
            (url, gets)
        }

        fn recorder() -> (TransportSink, Arc<Mutex<Vec<TransportEvent>>>) {
            let events = Arc::new(Mutex::new(Vec::<TransportEvent>::new()));
            let e = events.clone();
            (TransportSink::new(move |ev| e.lock().unwrap().push(ev)), events)
        }

        fn messages(events: &Mutex<Vec<TransportEvent>>) -> usize {
            events.lock().unwrap().iter().filter(|e| matches!(e, TransportEvent::Message(_))).count()
        }

        fn wait_until(what: &str, cond: impl Fn() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !cond() {
                assert!(Instant::now() < deadline, "timed out waiting for {what}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        const LINE: &str = "{\"version\":\"1.0\",\"createSurface\":{\"surfaceId\":\"s1\",\"catalogId\":\"c\"}}\n";

        #[test]
        fn a_clean_end_of_stream_closes_the_transport_without_reconnecting() {
            let (url, gets) = stream_server("200 OK", "application/jsonl", LINE.to_string());
            let (sink, events) = recorder();
            let mut t = JsonlStreamTransport::new(HttpTransportOptions::new(url).reconnect_ms(5));
            t.start(sink);
            wait_until("the Closed status", || events.lock().unwrap().iter().any(|e| matches!(e, TransportEvent::Status(TransportStatus::Closed, _))));
            std::thread::sleep(Duration::from_millis(200));
            assert_eq!(gets.load(AtomicOrdering::SeqCst), 1, "no reconnect after a clean end");
            assert_eq!(messages(&events), 1, "no re-delivery");
            assert!(matches!(events.lock().unwrap().last(), Some(TransportEvent::Status(TransportStatus::Closed, None))));
            t.close();
        }

        #[test]
        fn an_error_or_a_resumable_stream_reconnects_and_close_stops_the_loop() {
            // An HTTP error reconnects.
            let (url, gets) = stream_server("500 Internal Server Error", "text/plain", String::new());
            let (sink, events) = recorder();
            let mut t = JsonlStreamTransport::new(HttpTransportOptions::new(url).reconnect_ms(5));
            t.start(sink);
            wait_until("a reconnect after an error", || gets.load(AtomicOrdering::SeqCst) > 1);
            assert!(events.lock().unwrap().iter().any(|e| matches!(e, TransportEvent::Status(TransportStatus::Error, Some(d)) if d == "HTTP 500")));
            t.close();

            // `resumable: true` reconnects after a clean end.
            let (url, gets) = stream_server("200 OK", "application/jsonl", LINE.to_string());
            let (sink, _) = recorder();
            let mut t = JsonlStreamTransport::new(HttpTransportOptions::new(url).reconnect_ms(5).resumable(true));
            t.start(sink);
            wait_until("a resumed stream", || gets.load(AtomicOrdering::SeqCst) > 1);
            // close(): the stale loop stops (at most the in-flight request lands).
            t.close();
            std::thread::sleep(Duration::from_millis(100));
            let after = gets.load(AtomicOrdering::SeqCst);
            std::thread::sleep(Duration::from_millis(200));
            assert_eq!(gets.load(AtomicOrdering::SeqCst), after, "a closed transport kept reconnecting");

            // An SSE `retry:` field resumes too (its value = the wait).
            let (url, gets) = stream_server("200 OK", "text/event-stream", format!("retry: 5\ndata: {LINE}\n"));
            let (sink, events) = recorder();
            let mut t = SseTransport::new(HttpTransportOptions::new(url));
            t.start(sink);
            wait_until("an SSE retry reconnect", || gets.load(AtomicOrdering::SeqCst) > 1);
            assert!(messages(&events) >= 1);
            // A restart leaves exactly one live loop: the old one exits.
            let (sink, _) = recorder();
            t.start(sink);
            t.close();
            std::thread::sleep(Duration::from_millis(100));
            let after = gets.load(AtomicOrdering::SeqCst);
            std::thread::sleep(Duration::from_millis(200));
            assert_eq!(gets.load(AtomicOrdering::SeqCst), after, "a stale loop survived start + close");
        }

        /// Accepts every connection and never answers (a half-open server).
        fn silent_server() -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/mcp", listener.local_addr().unwrap());
            std::thread::spawn(move || {
                let mut held = Vec::new();
                for conn in listener.incoming().flatten() {
                    held.push(conn);
                }
            });
            url
        }

        #[test]
        fn a_hung_mcp_call_errors_within_the_timeout() {
            let total = Duration::from_millis(300);
            let mut rpc = Rpc { client: post_client(Duration::from_millis(300), total), options: McpTransportOptions::new(silent_server(), "surface"), id: 0, session: None };
            let began = Instant::now();
            let result = rpc.call("tools/call", json!({"name": "a2ui_event", "arguments": {}}));
            assert!(result.is_err(), "a never-answering server returned {result:?}");
            assert!(began.elapsed() < total + Duration::from_secs(2), "the call hung for {:?}", began.elapsed());
        }

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
            // Each event with the moment it arrived.
            let events = Arc::new(Mutex::new(Vec::<(Instant, TransportEvent)>::new()));
            let sink = {
                let events = events.clone();
                TransportSink::new(move |e| events.lock().unwrap().push((Instant::now(), e)))
            };
            let total = Duration::from_millis(300);
            let (tx, handle) = poster(url, Vec::new(), Some(sink), Live::next(&Arc::new(AtomicU64::new(0))), Duration::from_millis(300), total);
            let began = Instant::now();
            tx.send(json!({"n": 1})).unwrap();
            tx.send(json!({"n": 2})).unwrap();
            // Both hang; the second is not stalled behind the first forever.
            let deadline = began + Duration::from_secs(5);
            let failed_at = loop {
                let failed_at: Vec<Instant> = events.lock().unwrap().iter().filter(|(_, e)| matches!(e, TransportEvent::Status(TransportStatus::Error, Some(d)) if d.starts_with("send failed"))).map(|(at, _)| *at).collect();
                if failed_at.len() == 2 {
                    break failed_at;
                }
                assert!(Instant::now() < deadline, "the hung POSTs never timed out: {:?}", events.lock().unwrap());
                std::thread::sleep(Duration::from_millis(20));
            };
            // #EXP-1869: timed from the first failure, not from `began` (that
            // counted the post thread's start and client build, which a loaded
            // runner stretched past the budget); the second POST is bounded by
            // `total` on its own, with the slack of the MCP call test above.
            let gap = failed_at[1] - failed_at[0];
            assert!(gap < total + Duration::from_secs(2), "the second POST waited {gap:?} behind the first");
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

        /// #AXC-12: a model's response: prose around one Express block.
        const EXPRESS: &str = "Here it is.\n<a2ui>\nroot = Text(text=\"ok\")\n</a2ui>\nAnything else?";

        fn texts(events: &Mutex<Vec<TransportEvent>>) -> String {
            events.lock().unwrap().iter().filter_map(|e| if let TransportEvent::Text(t) = e { Some(t.as_str()) } else { None }).collect()
        }

        fn delivered(events: &Mutex<Vec<TransportEvent>>) -> Vec<serde_json::Value> {
            events.lock().unwrap().iter().filter_map(|e| if let TransportEvent::Message(m) = e { Some(m.clone()) } else { None }).collect()
        }

        fn root_text(messages: &[serde_json::Value]) -> Option<String> {
            messages.iter().find_map(|m| {
                let list = m.get("createSurface").or_else(|| m.get("updateComponents"))?.get("components")?.as_array()?;
                list.iter().find(|c| c["id"] == "root").and_then(|c| c["text"].as_str()).map(str::to_string)
            })
        }

        #[test]
        fn an_express_body_streams_messages_and_prose_and_asks_for_express() {
            let (url, _) = stream_server("200 OK", "application/a2ui+express; charset=utf-8", EXPRESS.to_string());
            let (sink, events) = recorder();
            let mut t = JsonlStreamTransport::new(HttpTransportOptions::new(url).format(WireFormat::express("main")));
            assert!(t.0.accept().starts_with("application/a2ui+express"));
            t.start(sink);
            wait_until("the Closed status", || events.lock().unwrap().iter().any(|e| matches!(e, TransportEvent::Status(TransportStatus::Closed, _))));
            let messages = delivered(&events);
            assert_eq!(messages[0]["createSurface"]["surfaceId"], "main", "{messages:?}");
            assert_eq!(root_text(&messages).as_deref(), Some("ok"));
            assert_eq!(texts(&events), "Here it is.\n\nAnything else?");
            t.close();

            // The Express format still reads a JSONL answer as JSONL.
            let (url, _) = stream_server("200 OK", "application/jsonl", LINE.to_string());
            let (sink, events) = recorder();
            let mut t = JsonlStreamTransport::new(HttpTransportOptions::new(url).format(WireFormat::express("main")));
            t.start(sink);
            wait_until("the Closed status", || events.lock().unwrap().iter().any(|e| matches!(e, TransportEvent::Status(TransportStatus::Closed, _))));
            assert_eq!(delivered(&events).len(), 1);
            assert_eq!(texts(&events), "");
            t.close();

            // The JSON format reads an Express body by its content type; a
            // block that does not compile is an issue, the stream reads on.
            let body = format!("{EXPRESS}\n<a2ui>\nroot = Sparkle(text=\"x\")\n</a2ui>\n");
            let (url, _) = stream_server("200 OK", "Application/A2UI+Express", body);
            let (sink, events) = recorder();
            let mut t = JsonlStreamTransport::new(HttpTransportOptions::new(url));
            assert!(t.0.accept().starts_with("application/jsonl"));
            t.start(sink);
            wait_until("the Closed status", || events.lock().unwrap().iter().any(|e| matches!(e, TransportEvent::Status(TransportStatus::Closed, _))));
            assert_eq!(root_text(&delivered(&events)).as_deref(), Some("ok"));
            let issues: Vec<String> = events.lock().unwrap().iter().filter_map(|e| if let TransportEvent::Issue(i) = e { Some(i.message.clone()) } else { None }).collect();
            assert!(issues.len() == 1 && issues[0].contains("Sparkle"), "{issues:?}");
            t.close();
        }

        #[test]
        fn express_sse_events_are_one_stream_beside_the_json_events() {
            // The Express stream split mid-statement across events (a
            // multi-line chunk = several data lines), a JSON event between.
            let (a, b) = EXPRESS.split_at(30);
            let data = |chunk: &str| chunk.split('\n').map(|l| format!("data: {l}\n")).collect::<String>();
            let body = format!("event: a2ui-express\n{}\ndata: {LINE}\nevent: a2ui-express\n{}\n", data(a), data(b));
            let (url, _) = stream_server("200 OK", "text/event-stream", body);
            let (sink, events) = recorder();
            let mut t = SseTransport::new(HttpTransportOptions::new(url).format(WireFormat::express("main")));
            t.start(sink);
            wait_until("the Closed status", || events.lock().unwrap().iter().any(|e| matches!(e, TransportEvent::Status(TransportStatus::Closed, _))));
            let messages = delivered(&events);
            assert!(messages.iter().any(|m| m["createSurface"]["surfaceId"] == "s1"), "the JSON event: {messages:?}");
            assert_eq!(root_text(&messages).as_deref(), Some("ok"), "{messages:?}");
            assert_eq!(texts(&events), "Here it is.\n\nAnything else?");
            t.close();

            // The JSON format reads them too, on the contract's default surface.
            let (url, _) = stream_server("200 OK", "text/event-stream", format!("event: a2ui-express\n{}\n", data(EXPRESS)));
            let (sink, events) = recorder();
            let mut t = SseTransport::new(HttpTransportOptions::new(url));
            t.start(sink);
            wait_until("the Closed status", || events.lock().unwrap().iter().any(|e| matches!(e, TransportEvent::Status(TransportStatus::Closed, _))));
            let messages = delivered(&events);
            assert_eq!(messages[0]["createSurface"]["surfaceId"], "main", "{messages:?}");
            assert_eq!(texts(&events), "Here it is.\n\nAnything else?");
            t.close();
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
