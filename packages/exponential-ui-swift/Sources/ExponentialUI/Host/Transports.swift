import Foundation
import ExponentialUICore

// VAPP-91: the transport adapters the Swift SDK ships. Each speaks A2UI
// messages in and client messages out through the core's decoders
// (`JsonlDecoder`, `SseDecoder`, `ExpressDecoder`, `decodeJsonlJson`,
// `messagesFromMcpResultJson`, `mcpActionCallJson`); none knows the host
// behind it. URLSession only.
//
// #AXC-12: each reads the Express inference format too (catalog/host.json
// `inference`): an HTTP body by its Content-Type, `a2ui-express` SSE
// events, `{express}` WebSocket frames, Express MCP resources (the core's
// `ExpressDecoder`, `SseDecoder.withSurface`, `FrameDecoder`,
// `messagesFromMcpResultWithJson`). The prose goes to `start`'s `text`
// (the host's `onText`); the SDK draws none.

/// `connecting | open | closed | error`.
public enum TransportStatus: String, Sendable {
    case connecting, open, closed, error
}

/// #AXC-12: what an agent writes: A2UI JSON messages (the default) or
/// Express (compiled by the core against the core catalog).
public enum InferenceFormat: String, Sendable, CaseIterable {
    case json, express
}

/// #AXC-12: the `inference` keys of the core's contract, read once.
enum InferenceWire {
    static let express: JSONValue = JSONValue.parse(hostContractJson())["inference"]?["express"] ?? .null
    static let mimeType: String = express["mimeType"]?.string ?? "application/a2ui+express"
    static let defaultSurfaceId: String = express["defaultSurfaceId"]?.string ?? "main"
}

/// Messages in (one A2UI server message as JSON text per call), client
/// messages out.
@MainActor
public protocol Transport: AnyObject {
    /// Start delivering messages; report the connection state.
    func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void)
    /// #AXC-12: `start` plus `text`, the prose of an Express response piece
    /// by piece (append them). The default drops the prose.
    func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void, text: (@MainActor (String) -> Void)?)
    /// One client message (JSON text).
    func send(_ message: String) async throws
    /// Round 9 §1.3: a message with the `sendDataModel` metadata beside it.
    func send(_ message: String, metadata: String) async throws
    func close()
}

public extension Transport {
    func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void, text: (@MainActor (String) -> Void)?) {
        start(receive: receive, status: status)
    }

    /// `{"message", "metadata"}` on the wire (the MCP carrier sends it beside).
    func send(_ message: String, metadata: String) async throws {
        try await send(#"{"message":\#(message),"metadata":\#(metadata)}"#)
    }
}

/// The `messages` of a decoder's `{messages, issues}` output as JSON texts.
func decodedMessages(_ json: String) -> [String] {
    (JSONValue.parse(json)["messages"]?.array ?? []).map(\.json)
}

/// #AXC-12: the prose pieces of a decoder's output (`text`: an Express
/// response's prose around its `<a2ui>` blocks; never for JSON). Append them
/// in order; the SDK draws none.
func decodedText(_ json: String) -> [String] {
    (JSONValue.parse(json)["text"]?.array ?? []).compactMap(\.string)
}

/// One decoder output: messages to `receive`, prose to `text`.
@MainActor
func deliver(_ json: String, receive: @MainActor (String) -> Void, text: (@MainActor (String) -> Void)?) {
    let value = JSONValue.parse(json)
    (value["messages"]?.array ?? []).forEach { receive($0.json) }
    if let text { (value["text"]?.array ?? []).compactMap(\.string).forEach(text) }
}

/// In-memory: `feed` messages in, read what the host sent from `sent`.
@MainActor
public final class MemoryTransport: Transport {
    public private(set) var sent: [String] = []
    public var onSend: ((String) -> Void)?
    private var receive: ((String) -> Void)?
    private var pending: [String] = []

    public init() {}

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void) {
        self.receive = receive
        status(.open, nil)
        let queued = pending
        pending = []
        queued.forEach(receive)
    }

    /// Messages as JSON texts.
    public func feed(_ messages: String...) {
        feed(messages)
    }

    public func feed(_ messages: [String]) {
        if let receive { messages.forEach(receive) } else { pending += messages }
    }

    public func feed(_ messages: [JSONValue]) {
        feed(messages.map(\.json))
    }

    /// JSONL text, as a stream server would send it.
    public func feedJsonl(_ text: String) {
        feed(decodedMessages(decodeJsonlJson(text: text)))
    }

    /// The sent messages parsed.
    public var sentMessages: [JSONValue] { sent.map(JSONValue.parse) }

    public func send(_ message: String) async throws {
        sent.append(message)
        onSend?(message)
    }

    public func close() {
        receive = nil
    }
}

/// The options of the HTTP stream transports.
public struct HTTPTransportOptions: Sendable {
    /// The stream (GET).
    public var url: URL
    /// Where client messages go (POST, JSON body); default `url`.
    public var postUrl: URL?
    public var headers: [String: String]
    /// Reconnect after a drop (nil = never). Default 2 s.
    public var reconnect: Duration?
    /// #AXC-12: `.express` asks for Express (`accept`). Either way an
    /// `application/a2ui+express` body (and SSE `a2ui-express` events) is
    /// read as Express: the wire decides.
    public var format: InferenceFormat
    /// The surface Express statements before any `surface("…")` target
    /// (nil = `main`).
    public var surfaceId: String?

    public init(url: URL, postUrl: URL? = nil, headers: [String: String] = [:], reconnect: Duration? = .seconds(2), format: InferenceFormat = .json, surfaceId: String? = nil) {
        self.url = url
        self.postUrl = postUrl
        self.headers = headers
        self.reconnect = reconnect
        self.format = format
        self.surfaceId = surfaceId
    }
}

/// The streamed-HTTP reader shared by JSONL and SSE.
@MainActor
public class StreamTransport: Transport {
    public let options: HTTPTransportOptions
    public var session: URLSession
    private var task: Task<Void, Never>?
    private var closed = false

    let accept: String
    /// The body's decoder; `true` = its Content-Type is Express.
    let makeDecoder: @Sendable (Bool) -> StreamDecoder

    init(options: HTTPTransportOptions, session: URLSession, accept: String, makeDecoder: @escaping @Sendable (Bool) -> StreamDecoder) {
        self.options = options
        self.session = session
        self.accept = accept
        self.makeDecoder = makeDecoder
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void) {
        start(receive: receive, status: status, text: nil)
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void, text onText: (@MainActor (String) -> Void)?) {
        closed = false
        task?.cancel()
        var request = URLRequest(url: options.url)
        request.setValue(options.format == .express ? "\(InferenceWire.mimeType), \(accept)" : accept, forHTTPHeaderField: "accept")
        for (k, v) in options.headers { request.setValue(v, forHTTPHeaderField: k) }
        request.timeoutInterval = 24 * 3600
        let session = self.session
        let reconnect = options.reconnect
        let decoderFactory = makeDecoder
        task = Task { [weak self] in
            while !Task.isCancelled {
                status(.connecting, nil)
                do {
                    let (bytes, response) = try await session.bytes(for: request)
                    if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                        throw URLError(.badServerResponse, userInfo: [NSLocalizedDescriptionKey: "HTTP \(http.statusCode)"])
                    }
                    let contentType = (response as? HTTPURLResponse)?.value(forHTTPHeaderField: "content-type") ?? ""
                    let decoder = decoderFactory(inferenceFormatOf(contentType: contentType) == InferenceFormat.express.rawValue)
                    status(.open, nil)
                    try await Self.pump(bytes, decoder: decoder, receive: receive, text: onText)
                    deliver(decoder.end(), receive: receive, text: onText)
                    status(.closed, nil)
                } catch {
                    if Task.isCancelled || self?.closed ?? true { return }
                    status(.error, error.localizedDescription)
                }
                guard let reconnect, !(self?.closed ?? true) else { return }
                try? await Task.sleep(for: reconnect)
            }
        }
    }

    /// Bytes → the decoder at line boundaries (never a split UTF-8 scalar).
    nonisolated static func pump(_ bytes: URLSession.AsyncBytes, decoder: StreamDecoder, receive: @escaping @MainActor (String) -> Void, text: (@MainActor (String) -> Void)? = nil) async throws {
        var buffer: [UInt8] = []
        buffer.reserveCapacity(4096)
        for try await byte in bytes {
            buffer.append(byte)
            if byte == 0x0A {
                let out = decoder.push(String(decoding: buffer, as: UTF8.self))
                buffer.removeAll(keepingCapacity: true)
                await hand(out, receive: receive, text: text)
            }
        }
        if !buffer.isEmpty {
            await hand(decoder.push(String(decoding: buffer, as: UTF8.self)), receive: receive, text: text)
        }
    }

    /// One push's output onto the main actor (skipped when it carries nothing).
    nonisolated static func hand(_ out: String, receive: @escaping @MainActor (String) -> Void, text: (@MainActor (String) -> Void)?) async {
        let messages = decodedMessages(out)
        let prose = text == nil ? [] : decodedText(out)
        if messages.isEmpty && prose.isEmpty { return }
        await MainActor.run {
            messages.forEach(receive)
            if let text { prose.forEach(text) }
        }
    }

    public func send(_ message: String) async throws {
        var request = URLRequest(url: options.postUrl ?? options.url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "content-type")
        for (k, v) in options.headers { request.setValue(v, forHTTPHeaderField: k) }
        request.httpBody = Data(message.utf8)
        _ = try await session.data(for: request)
    }

    public func close() {
        closed = true
        task?.cancel()
        task = nil
    }
}

/// The core's chunk decoders behind one shape.
protocol StreamDecoder: Sendable {
    func push(_ chunk: String) -> String
    func end() -> String
}

extension JsonlDecoder: StreamDecoder {
    func push(_ chunk: String) -> String { push(chunk: chunk) }
}

extension SseDecoder: StreamDecoder {
    func push(_ chunk: String) -> String { push(chunk: chunk) }
}

/// #AXC-12: Express (model output; prose comes back in `text`).
extension ExpressDecoder: StreamDecoder {
    func push(_ chunk: String) -> String { push(chunk: chunk) }
}

extension FrameDecoder: StreamDecoder {
    func push(_ chunk: String) -> String { push(frame: chunk) }
}

/// A2UI JSONL over a streamed HTTP response (`application/jsonl`); an
/// `application/a2ui+express` response is ONE Express response (prose +
/// `<a2ui>` blocks).
public final class JSONLStreamTransport: StreamTransport {
    public init(_ options: HTTPTransportOptions, session: URLSession = .shared) {
        let surfaceId = options.surfaceId ?? InferenceWire.defaultSurfaceId
        super.init(options: options, session: session, accept: "application/jsonl, application/x-ndjson", makeDecoder: { express in
            express ? ExpressDecoder(wrapped: true, surfaceId: surfaceId) : JsonlDecoder()
        })
    }

    public convenience init(url: URL, postUrl: URL? = nil, headers: [String: String] = [:], reconnect: Duration? = .seconds(2), format: InferenceFormat = .json, surfaceId: String? = nil) {
        self.init(HTTPTransportOptions(url: url, postUrl: postUrl, headers: headers, reconnect: reconnect, format: format, surfaceId: surfaceId))
    }
}

/// Server-Sent Events over a streamed HTTP response (headers allowed);
/// `a2ui-express` events are the chunks of the stream's ONE Express
/// response.
public final class SSETransport: StreamTransport {
    public init(_ options: HTTPTransportOptions, session: URLSession = .shared) {
        let surfaceId = options.surfaceId
        super.init(options: options, session: session, accept: "text/event-stream", makeDecoder: { _ in SseDecoder.withSurface(surfaceId: surfaceId) })
    }

    public convenience init(url: URL, postUrl: URL? = nil, headers: [String: String] = [:], reconnect: Duration? = .seconds(2), format: InferenceFormat = .json, surfaceId: String? = nil) {
        self.init(HTTPTransportOptions(url: url, postUrl: postUrl, headers: headers, reconnect: reconnect, format: format, surfaceId: surfaceId))
    }
}

/// One A2UI message (or JSONL) per frame; client messages go back as text
/// frames. #AXC-12: `{"express": "…"}` frames are the chunks of the
/// connection's ONE Express response (flushed at close).
@MainActor
public final class WebSocketTransport: Transport {
    public let url: URL
    public let protocols: [String]
    public let headers: [String: String]
    public let reconnect: Duration?
    /// #AXC-12: `.express` = the agent writes Express; the frames decide
    /// either way (nothing to negotiate on a socket).
    public let format: InferenceFormat
    /// The surface Express statements before any `surface("…")` target
    /// (nil = `main`).
    public let surfaceId: String?
    public var session: URLSession
    private var socket: URLSessionWebSocketTask?
    private var loop: Task<Void, Never>?
    private var closed = false

    public init(url: URL, protocols: [String] = [], headers: [String: String] = [:], reconnect: Duration? = .seconds(2), format: InferenceFormat = .json, surfaceId: String? = nil, session: URLSession = .shared) {
        self.url = url
        self.protocols = protocols
        self.headers = headers
        self.reconnect = reconnect
        self.format = format
        self.surfaceId = surfaceId
        self.session = session
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void) {
        start(receive: receive, status: status, text: nil)
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void, text onText: (@MainActor (String) -> Void)?) {
        closed = false
        loop?.cancel()
        loop = Task { [weak self] in
            while let self, !self.closed, !Task.isCancelled {
                var request = URLRequest(url: self.url)
                for (k, v) in self.headers { request.setValue(v, forHTTPHeaderField: k) }
                if !self.protocols.isEmpty { request.setValue(self.protocols.joined(separator: ", "), forHTTPHeaderField: "Sec-WebSocket-Protocol") }
                let socket = self.session.webSocketTask(with: request)
                self.socket = socket
                status(.connecting, nil)
                socket.resume()
                var opened = false
                // One decoder per connection: its `{express}` frames are ONE response.
                let frames = FrameDecoder(surfaceId: self.surfaceId)
                do {
                    while true {
                        let frame = try await socket.receive()
                        if !opened {
                            opened = true
                            status(.open, nil)
                        }
                        let text: String
                        switch frame {
                        case let .string(s): text = s
                        case let .data(d): text = String(decoding: d, as: UTF8.self)
                        @unknown default: continue
                        }
                        deliver(frames.push(frame: text), receive: receive, text: onText)
                    }
                } catch {
                    if self.closed || Task.isCancelled { return }
                    // The connection's Express response ends with it.
                    deliver(frames.end(), receive: receive, text: onText)
                    status(opened ? .closed : .error, opened ? nil : error.localizedDescription)
                }
                guard let wait = self.reconnect, !self.closed else { return }
                try? await Task.sleep(for: wait)
            }
        }
    }

    public func send(_ message: String) async throws {
        try await socket?.send(.string(message))
    }

    public func close() {
        closed = true
        loop?.cancel()
        socket?.cancel(with: .normalClosure, reason: nil)
        socket = nil
    }
}

/// A2UI over MCP: calls `tool` once at start, delivers the A2UI resources of
/// its result, and sends every client message as a `tools/call` to
/// `actionTool` (default `a2ui_event`; its result may carry more messages).
/// JSON-RPC over plain POST; an SSE response body is read through the SSE
/// decoder.
@MainActor
public final class MCPTransport: Transport {
    public let url: URL
    public let tool: String
    public let arguments: JSONValue
    public let actionTool: String?
    public let headers: [String: String]
    /// #AXC-12: `.express` = the agent writes Express; the result decides
    /// either way (an `application/a2ui+express` resource,
    /// `structuredContent.a2uiExpress`).
    public let format: InferenceFormat
    /// The surface Express statements before any `surface("…")` target
    /// (nil = `main`).
    public let surfaceId: String?
    public var session: URLSession
    private var id = 0
    private var mcpSession: String?
    private var receive: ((String) -> Void)?
    private var text: (@MainActor (String) -> Void)?

    public init(url: URL, tool: String, arguments: JSONValue = .object([:]), actionTool: String? = nil, headers: [String: String] = [:], format: InferenceFormat = .json, surfaceId: String? = nil, session: URLSession = .shared) {
        self.url = url
        self.tool = tool
        self.arguments = arguments
        self.actionTool = actionTool
        self.headers = headers
        self.format = format
        self.surfaceId = surfaceId
        self.session = session
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void) {
        start(receive: receive, status: status, text: nil)
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void, text: (@MainActor (String) -> Void)?) {
        self.receive = receive
        self.text = text
        status(.connecting, nil)
        Task {
            do {
                _ = try await rpc("initialize", .object(["protocolVersion": .string("2025-06-18"), "capabilities": .object([:]), "clientInfo": .object(["name": .string("exponential-ui"), "version": .string(ExponentialUI.coreVersion)])]))
                let result = try await rpc("tools/call", .object(["name": .string(tool), "arguments": arguments]))
                status(.open, nil)
                try self.deliverResult(result)
            } catch {
                status(.error, error.localizedDescription)
            }
        }
    }

    public func send(_ message: String) async throws {
        try await call(message, metadata: nil)
    }

    public func send(_ message: String, metadata: String) async throws {
        try await call(message, metadata: metadata)
    }

    private func call(_ message: String, metadata: String?) async throws {
        let call = JSONValue.parse(try mcpActionCallJson(messageJson: message, tool: actionTool, metadataJson: metadata))
        let result = try await rpc(call["method"]?.string ?? "tools/call", call["params"] ?? .object([:]))
        try deliverResult(result)
    }

    /// A tool result's messages (Express resources compiled) and prose.
    private func deliverResult(_ result: JSONValue) throws {
        guard let receive else { return }
        deliver(try messagesFromMcpResultWithJson(resultJson: result.json, surfaceId: surfaceId), receive: receive, text: text)
    }

    public func close() {
        receive = nil
        text = nil
    }

    public struct MCPError: LocalizedError {
        public let message: String
        public var errorDescription: String? { message }
    }

    private func rpc(_ method: String, _ params: JSONValue) async throws -> JSONValue {
        id += 1
        let id = self.id
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "content-type")
        request.setValue("application/json, text/event-stream", forHTTPHeaderField: "accept")
        if let mcpSession { request.setValue(mcpSession, forHTTPHeaderField: "mcp-session-id") }
        for (k, v) in headers { request.setValue(v, forHTTPHeaderField: k) }
        request.httpBody = Data(JSONValue.object(["jsonrpc": .string("2.0"), "id": .number(Double(id)), "method": .string(method), "params": params]).json.utf8)
        let (data, response) = try await session.data(for: request)
        let http = response as? HTTPURLResponse
        if let s = http?.value(forHTTPHeaderField: "mcp-session-id") { mcpSession = s }
        if let code = http?.statusCode, !(200..<300).contains(code) { throw MCPError(message: "MCP HTTP \(code)") }
        let body = String(decoding: data, as: UTF8.self)
        let replies: [JSONValue]
        if (http?.value(forHTTPHeaderField: "content-type") ?? "").contains("text/event-stream") {
            let d = SseDecoder()
            replies = (decodedMessages(d.push(chunk: body)) + decodedMessages(d.end())).map(JSONValue.parse)
        } else {
            replies = [JSONValue.parse(body)]
        }
        guard let reply = replies.first(where: { $0["id"]?.number == Double(id) }) else { throw MCPError(message: "MCP: no reply to \(method)") }
        if let error = reply["error"], !error.isNull { throw MCPError(message: "MCP: \(error["message"]?.string ?? "error")") }
        return reply["result"] ?? .null
    }
}
