import Foundation
import ExponentialUICore

// VAPP-91: the transport adapters the Swift SDK ships. Each speaks A2UI
// messages in and client messages out through the core's decoders
// (`JsonlDecoder`, `SseDecoder`, `decodeJsonlJson`,
// `messagesFromMcpResultJson`, `mcpActionCallJson`); none knows the host
// behind it. URLSession only.

/// `connecting | open | closed | error`.
public enum TransportStatus: String, Sendable {
    case connecting, open, closed, error
}

/// Messages in (one A2UI server message as JSON text per call), client
/// messages out.
@MainActor
public protocol Transport: AnyObject {
    /// Start delivering messages; report the connection state.
    func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void)
    /// One client message (JSON text).
    func send(_ message: String) async throws
    func close()
}

/// The `messages` of a decoder's `{messages, issues}` output as JSON texts.
func decodedMessages(_ json: String) -> [String] {
    (JSONValue.parse(json)["messages"]?.array ?? []).map(\.json)
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

    public init(url: URL, postUrl: URL? = nil, headers: [String: String] = [:], reconnect: Duration? = .seconds(2)) {
        self.url = url
        self.postUrl = postUrl
        self.headers = headers
        self.reconnect = reconnect
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
    let makeDecoder: @Sendable () -> StreamDecoder

    init(options: HTTPTransportOptions, session: URLSession, accept: String, makeDecoder: @escaping @Sendable () -> StreamDecoder) {
        self.options = options
        self.session = session
        self.accept = accept
        self.makeDecoder = makeDecoder
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void) {
        closed = false
        task?.cancel()
        var request = URLRequest(url: options.url)
        request.setValue(accept, forHTTPHeaderField: "accept")
        for (k, v) in options.headers { request.setValue(v, forHTTPHeaderField: k) }
        request.timeoutInterval = 24 * 3600
        let session = self.session
        let reconnect = options.reconnect
        let decoderFactory = makeDecoder
        task = Task { [weak self] in
            while !Task.isCancelled {
                status(.connecting, nil)
                let decoder = decoderFactory()
                do {
                    let (bytes, response) = try await session.bytes(for: request)
                    if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                        throw URLError(.badServerResponse, userInfo: [NSLocalizedDescriptionKey: "HTTP \(http.statusCode)"])
                    }
                    status(.open, nil)
                    try await Self.pump(bytes, decoder: decoder, receive: receive)
                    decodedMessages(decoder.end()).forEach(receive)
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
    nonisolated static func pump(_ bytes: URLSession.AsyncBytes, decoder: StreamDecoder, receive: @escaping @MainActor (String) -> Void) async throws {
        var buffer: [UInt8] = []
        buffer.reserveCapacity(4096)
        for try await byte in bytes {
            buffer.append(byte)
            if byte == 0x0A {
                let out = decoder.push(String(decoding: buffer, as: UTF8.self))
                buffer.removeAll(keepingCapacity: true)
                let messages = decodedMessages(out)
                if !messages.isEmpty { await MainActor.run { messages.forEach(receive) } }
            }
        }
        if !buffer.isEmpty {
            let messages = decodedMessages(decoder.push(String(decoding: buffer, as: UTF8.self)))
            if !messages.isEmpty { await MainActor.run { messages.forEach(receive) } }
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

/// A2UI JSONL over a streamed HTTP response (`application/jsonl`).
public final class JSONLStreamTransport: StreamTransport {
    public init(_ options: HTTPTransportOptions, session: URLSession = .shared) {
        super.init(options: options, session: session, accept: "application/jsonl, application/x-ndjson", makeDecoder: { JsonlDecoder() })
    }

    public convenience init(url: URL, postUrl: URL? = nil, headers: [String: String] = [:], reconnect: Duration? = .seconds(2)) {
        self.init(HTTPTransportOptions(url: url, postUrl: postUrl, headers: headers, reconnect: reconnect))
    }
}

/// Server-Sent Events over a streamed HTTP response (headers allowed).
public final class SSETransport: StreamTransport {
    public init(_ options: HTTPTransportOptions, session: URLSession = .shared) {
        super.init(options: options, session: session, accept: "text/event-stream", makeDecoder: { SseDecoder() })
    }

    public convenience init(url: URL, postUrl: URL? = nil, headers: [String: String] = [:], reconnect: Duration? = .seconds(2)) {
        self.init(HTTPTransportOptions(url: url, postUrl: postUrl, headers: headers, reconnect: reconnect))
    }
}

/// One A2UI message (or JSONL) per frame; client messages go back as text
/// frames.
@MainActor
public final class WebSocketTransport: Transport {
    public let url: URL
    public let protocols: [String]
    public let headers: [String: String]
    public let reconnect: Duration?
    public var session: URLSession
    private var socket: URLSessionWebSocketTask?
    private var loop: Task<Void, Never>?
    private var closed = false

    public init(url: URL, protocols: [String] = [], headers: [String: String] = [:], reconnect: Duration? = .seconds(2), session: URLSession = .shared) {
        self.url = url
        self.protocols = protocols
        self.headers = headers
        self.reconnect = reconnect
        self.session = session
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void) {
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
                        decodedMessages(decodeJsonlJson(text: text)).forEach(receive)
                    }
                } catch {
                    if self.closed || Task.isCancelled { return }
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
    public var session: URLSession
    private var id = 0
    private var mcpSession: String?
    private var receive: ((String) -> Void)?

    public init(url: URL, tool: String, arguments: JSONValue = .object([:]), actionTool: String? = nil, headers: [String: String] = [:], session: URLSession = .shared) {
        self.url = url
        self.tool = tool
        self.arguments = arguments
        self.actionTool = actionTool
        self.headers = headers
        self.session = session
    }

    public func start(receive: @escaping @MainActor (String) -> Void, status: @escaping @MainActor (TransportStatus, String?) -> Void) {
        self.receive = receive
        status(.connecting, nil)
        Task {
            do {
                _ = try await rpc("initialize", .object(["protocolVersion": .string("2025-06-18"), "capabilities": .object([:]), "clientInfo": .object(["name": .string("exponential-ui"), "version": .string(ExponentialUI.coreVersion)])]))
                let result = try await rpc("tools/call", .object(["name": .string(tool), "arguments": arguments]))
                status(.open, nil)
                let messages = decodedMessages(try messagesFromMcpResultJson(resultJson: result.json))
                messages.forEach(receive)
            } catch {
                status(.error, error.localizedDescription)
            }
        }
    }

    public func send(_ message: String) async throws {
        let call = JSONValue.parse(try mcpActionCallJson(messageJson: message, tool: actionTool))
        let result = try await rpc(call["method"]?.string ?? "tools/call", call["params"] ?? .object([:]))
        if let receive {
            decodedMessages(try messagesFromMcpResultJson(resultJson: result.json)).forEach(receive)
        }
    }

    public func close() {
        receive = nil
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
