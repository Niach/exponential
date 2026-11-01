import Foundation
import os

// #EXP-1528: the sealed relay TUNNEL, eNet's fallback carrier
// (fixtures/steer-enet.json `fallback.$comment`). The same exp-steer
// envelopes as eNet's chat stream, each SEALED to the session tree key
// (fixtures/sealed-content.json): tunnel `data` = `v1:<epoch>:<b64url
// push_seal(tree_public, utf8(envelope JSON))>`, opened with
// openBody("exp-sealed:" + data). The relay sees the session id, the tunnel
// id and ciphertext, nothing else. Wire (viewer <-> relay): `{t:"sealed",
// data}` both ways, plus `{t:"tunnel_error", code}` relay -> viewer, then close.

private let logger = Logger(subsystem: "at.exponential", category: "SteerTunnel")

/// tRPC `steer.tunnelTicket {sessionId}`. Non-strict: anything that is not a
/// ticket nor `disabled` reads as `unavailable`.
public enum SteerTunnelTicket: Decodable, Equatable, Sendable {
    case disabled
    case unavailable
    case ticket(ticket: String, url: String)

    private enum Keys: String, CodingKey { case disabled, unavailable, ticket, url }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        if let url = try? c.decodeIfPresent(String.self, forKey: .url), !url.isEmpty {
            self = .ticket(ticket: (try? c.decodeIfPresent(String.self, forKey: .ticket)) ?? "", url: url)
        } else if (try? c.decodeIfPresent(Bool.self, forKey: .disabled)) == true {
            self = .disabled
        } else {
            self = .unavailable
        }
    }
}

/// The ticket call (a fake in the tests).
public protocol SteerTunnelTicketing: Sendable {
    func tunnelTicket(accountId: String, sessionId: String) async throws -> SteerTunnelTicket
}

private struct TunnelTicketInput: Encodable { let sessionId: String }

public final class SteerTunnelApi: SteerTunnelTicketing {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func tunnelTicket(accountId: String, sessionId: String) async throws -> SteerTunnelTicket {
        try await trpc.mutation(accountId: accountId, path: "steer.tunnelTicket", input: TunnelTicketInput(sessionId: sessionId))
    }
}

/// Seals / opens one tunnel `data` with the tree key (a fake in the tests).
public protocol SteerTunnelSealing: Sendable {
    /// The sealed `data` of one envelope's JSON text; nil = can't seal.
    func seal(_ plaintext: String) -> String?
    /// The envelope JSON of one `data`; nil = it does not open here.
    func open(_ data: String) -> String?
}

/// The app's sealer: the root's current public key + the secrets this
/// install holds (`SealedKeyRing`), through the UniFFI sealed-content
/// functions (`SealedCrypto`).
public struct SteerTreeSealer: SteerTunnelSealing {
    private let crypto: SealedCrypto
    private let ring: SealedKeyRing
    private let rootId: String
    private let key: TreeSealKey

    /// Nil = no relay leg: the build has no sealed crypto, the root has no
    /// seal key, or this install does not hold its current epoch's secret.
    public init?(crypto: SealedCrypto, ring: SealedKeyRing, rootId: String, key: TreeSealKey?) {
        guard crypto.isAvailable, let key, ring.holds(rootId: rootId, epoch: key.epoch) else { return nil }
        self.crypto = crypto
        self.ring = ring
        self.rootId = rootId
        self.key = key
    }

    public func seal(_ plaintext: String) -> String? {
        guard let body = try? crypto.sealBody(treePublicKey: key.publicKey, epoch: key.epoch, text: plaintext),
              body.hasPrefix(SealedEnvelope.bodyPrefix) else { return nil }
        return String(body.dropFirst(SealedEnvelope.bodyPrefix.count))
    }

    public func open(_ data: String) -> String? {
        let opened = crypto.openBody(SealedEnvelope.bodyPrefix + data, keys: ring.keys(rootId: rootId))
        return opened.status == .opened ? opened.text : nil
    }
}

/// One viewer WebSocket to the relay (a fake in the tests).
public protocol SteerTunnelSocket: AnyObject, Sendable {
    func send(_ text: String) async throws
    /// The next text frame; throws once the socket is gone.
    func receive() async throws -> String
    func close()
}

public final class URLSessionTunnelSocket: SteerTunnelSocket, @unchecked Sendable {
    private let task: URLSessionWebSocketTask

    public init(url: URL, session: URLSession = .shared) {
        task = session.webSocketTask(with: url)
        task.maximumMessageSize = SteerCarrier.tunnelMaxDataBytes + 4_096
        task.resume()
    }

    public func send(_ text: String) async throws {
        try await task.send(.string(text))
    }

    public func receive() async throws -> String {
        while true {
            switch try await task.receive() {
            case let .string(text): return text
            case let .data(data): if let text = String(data: data, encoding: .utf8) { return text }
            @unknown default: continue
            }
        }
    }

    public func close() {
        task.cancel(with: .normalClosure, reason: nil)
    }
}

/// The relay wire's frames (pure).
public enum SteerTunnelWire {
    public enum Inbound: Equatable, Sendable {
        case sealed(String)
        case tunnelError(String)
        case other
    }

    public static func sealedFrame(_ data: String) -> String {
        let quoted = (try? JSONEncoder().encode(data)).flatMap { String(data: $0, encoding: .utf8) } ?? "\"\""
        return "{\"t\":\"sealed\",\"data\":\(quoted)}"
    }

    public static func parse(_ text: String) -> Inbound {
        guard let object = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any],
              let t = object["t"] as? String else { return .other }
        switch t {
        case "sealed":
            guard let data = object["data"] as? String, data.utf8.count <= SteerCarrier.tunnelMaxDataBytes else { return .other }
            return .sealed(data)
        case "tunnel_error":
            return .tunnelError(object["code"] as? String ?? "other")
        default:
            return .other
        }
    }
}

/// One open tunnel for one session: the opened envelopes' frames in order,
/// sealed sends back. `closeReason` = the connStats error class it ended on.
public final class SteerTunnelLink: @unchecked Sendable {
    public let sessionId: String
    private let socket: SteerTunnelSocket
    private let sealer: SteerTunnelSealing
    private let lock = NSLock()
    private var closed = false
    private var reason: String?
    private var outbox: AsyncStream<[String]>.Continuation?
    private var sender: Task<Void, Never>?
    private var reader: Task<Void, Never>?
    private let inbound: AsyncStream<[String]>
    private let sink: AsyncStream<[String]>.Continuation

    public init(sessionId: String, socket: SteerTunnelSocket, sealer: SteerTunnelSealing) {
        self.sessionId = sessionId
        self.socket = socket
        self.sealer = sealer
        (inbound, sink) = AsyncStream<[String]>.makeStream()
        let (out, continuation) = AsyncStream<[String]>.makeStream()
        outbox = continuation
        sender = Task.detached { [weak self, socket, sealer] in
            for await frames in out {
                let envelope = String(decoding: SteerEnet.encode(sessionId: sessionId, frames: frames), as: UTF8.self)
                guard let data = sealer.seal(envelope) else {
                    logger.error("tunnel: an envelope did not seal")
                    continue
                }
                do {
                    try await socket.send(SteerTunnelWire.sealedFrame(data))
                } catch {
                    self?.finish(reason: "send_failed")
                    return
                }
            }
        }
        reader = Task.detached { [weak self, socket, sealer] in
            while true {
                let text: String
                do {
                    text = try await socket.receive()
                } catch {
                    self?.finish(reason: "relay_closed")
                    return
                }
                switch SteerTunnelWire.parse(text) {
                case let .sealed(data):
                    // A data that does not open or is no envelope of this
                    // tunnel's session is dropped.
                    guard let plain = sealer.open(data),
                          let envelope = SteerEnet.parse(Data(plain.utf8)),
                          envelope.sessionId == sessionId else { continue }
                    self?.sink.yield(envelope.frames)
                case let .tunnelError(code):
                    self?.finish(reason: SteerCarrier.tunnelErrorCodes.contains(code) ? code : "other")
                    return
                case .other:
                    continue
                }
            }
        }
    }

    deinit {
        sender?.cancel()
        reader?.cancel()
    }

    /// Every opened envelope's frames; finishes when the tunnel closes.
    public var frames: AsyncStream<[String]> { inbound }

    /// Why it closed (nil while open, or when we closed it).
    public var closeReason: String? { lock.withLock { reason } }

    public func send(_ frames: [String]) {
        lock.withLock { closed ? nil : outbox }?.yield(frames)
    }

    public func rejoin() { send([SteerEnet.joinFrame]) }

    public func close(sendLeave: Bool = false) {
        let pending = lock.withLock { () -> AsyncStream<[String]>.Continuation? in
            guard !closed else { return nil }
            if sendLeave { outbox?.yield([SteerEnet.leaveFrame]) }
            return outbox
        }
        pending?.finish()
        finish(reason: nil, closeSocket: !sendLeave)
        if sendLeave {
            // Let the `leave` out first (best effort).
            let socket = socket
            Task.detached {
                try? await Task.sleep(for: .milliseconds(200))
                socket.close()
            }
        }
    }

    private func finish(reason: String?, closeSocket: Bool = true) {
        let first = lock.withLock { () -> Bool in
            guard !closed else { return false }
            closed = true
            self.reason = reason
            return true
        }
        guard first else { return }
        if reason != nil { logger.info("tunnel closed: \(reason ?? "", privacy: .public)") }
        outbox?.finish()
        reader?.cancel()
        sink.finish()
        if closeSocket { socket.close() }
    }
}

/// What opening the relay leg gave.
public enum SteerTunnelOpen: Sendable {
    case link(SteerTunnelLink)
    /// `relayUnavailable`, with its connStats error class.
    case unavailable(String)
}

/// Opens the relay leg of one session (a fake in the tests).
public protocol SteerTunnelDialing: Sendable {
    func open(sessionId: String) async -> SteerTunnelOpen
}

/// The app's dialer: the availability rule (a sealer = a keyed root + the
/// secret held here), then `steer.tunnelTicket`, then the socket.
public final class SteerTunnelDialer: SteerTunnelDialing {
    private let accountId: String
    private let api: SteerTunnelTicketing
    private let sealer: @Sendable () async -> SteerTunnelSealing?
    private let socket: @Sendable (URL) -> SteerTunnelSocket

    public init(
        accountId: String,
        api: SteerTunnelTicketing,
        sealer: @escaping @Sendable () async -> SteerTunnelSealing?,
        socket: @escaping @Sendable (URL) -> SteerTunnelSocket = { URLSessionTunnelSocket(url: $0) }
    ) {
        self.accountId = accountId
        self.api = api
        self.sealer = sealer
        self.socket = socket
    }

    public func open(sessionId: String) async -> SteerTunnelOpen {
        guard let sealer = await sealer() else { return .unavailable("no_key") }
        let ticket: SteerTunnelTicket
        do {
            ticket = try await api.tunnelTicket(accountId: accountId, sessionId: sessionId)
        } catch {
            logger.info("steer.tunnelTicket failed: \(String(describing: error), privacy: .public)")
            return .unavailable("ticket_failed")
        }
        switch ticket {
        case .disabled:
            return .unavailable("ticket_failed")
        case .unavailable:
            return .unavailable("device_offline")
        case let .ticket(_, url):
            guard let url = URL(string: url) else { return .unavailable("ticket_failed") }
            return .link(SteerTunnelLink(sessionId: sessionId, socket: socket(url), sealer: sealer))
        }
    }
}
