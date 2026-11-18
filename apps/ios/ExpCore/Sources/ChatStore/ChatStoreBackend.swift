import Foundation

// #EDE-114 (chat-store.json S3 = phones): the CHAT STORE on iOS. The store is
// the `chat-store` crate behind its UniFFI facade (`chat-store-ffi`, linked
// into ExpCore with the app's one Rust library, `ENET_NODE`); this file is the
// binding-free surface the app and the tests code against, plus the adapter
// over the real binding. A build without the library has no store (nil
// factory): the app then behaves as before it, minus the stored tail.

/// The holder's access verdict for a catch-up page (the join's rule).
public enum ChatStorePageAccess: Sendable, Equatable {
    case allowed, unauthorized, noSuchSession
}

/// One account's store (the facade's `ChatStore`, a fake in the tests).
/// Blocking: every call runs off the main thread.
public protocol ChatStoreBackend: AnyObject, Sendable {
    func putFeed(rootId: String, sessionId: String, deviceId: String, eventsJson: String, nowMs: Int64) throws -> UInt32
    func putMessages(messagesJson: String) throws
    func lockedMessageIds() throws -> [String]
    func openTail(sessionId: String, deviceId: String, tail: UInt32) throws -> String
    func page(sessionId: String, deviceId: String, beforeSeq: UInt64, limit: UInt32) throws -> String
    func count(sessionId: String, deviceId: String) throws -> UInt64
    func searchChat(rootId: String, query: String, limit: UInt32) throws -> String
    func searchAll(query: String, limit: UInt32) throws -> String
    func applyRetention(nowMs: Int64, windowDays: UInt32?) throws -> UInt64
    func meta(key: String) throws -> String?
    func setMeta(key: String, value: String) throws
    func answerPage(sessionId: String, pageJson: String, access: ChatStorePageAccess) throws -> String?
}

/// One chat session the account reads (a synced `sessions` row).
public struct ChatCatchUpSession: Equatable, Sendable {
    public let sessionId: String
    public let rootId: String
    public let deviceId: String
    public let updatedAt: String?
    public let busy: Bool

    public init(sessionId: String, rootId: String, deviceId: String, updatedAt: String?, busy: Bool) {
        self.sessionId = sessionId
        self.rootId = rootId
        self.deviceId = deviceId
        self.updatedAt = updatedAt
        self.busy = busy
    }
}

/// One install the registry lists.
public struct ChatCatchUpInstall: Equatable, Sendable {
    public let endpointId: String
    public let userId: String
    public let deviceId: String?
    public let lastSeenAt: String?

    public init(endpointId: String, userId: String, deviceId: String?, lastSeenAt: String?) {
        self.endpointId = endpointId
        self.userId = userId
        self.deviceId = deviceId
        self.lastSeenAt = lastSeenAt
    }
}

/// One page to carry: `frame` to `peer` in a steer envelope of `sessionId`,
/// after `waitMs`.
public struct ChatCatchUpAsk: Equatable, Sendable {
    public let requestId: String
    public let sessionId: String
    public let peer: String
    public let frame: String
    public let waitMs: UInt64

    public init(requestId: String, sessionId: String, peer: String, frame: String, waitMs: UInt64) {
        self.requestId = requestId
        self.sessionId = sessionId
        self.peer = peer
        self.frame = frame
        self.waitMs = waitMs
    }
}

/// What one answer did.
public struct ChatCatchUpTaken: Equatable, Sendable {
    public let sessionId: String
    public let rootId: String
    public let added: UInt32

    public init(sessionId: String, rootId: String, added: UInt32) {
        self.sessionId = sessionId
        self.rootId = rootId
        self.added = added
    }
}

/// The facade's `CatchUp` walk (sans I/O; a fake in the tests).
public protocol ChatCatchUpWalker: AnyObject, Sendable {
    func setInstalls(selfEndpoint: String, installs: [ChatCatchUpInstall])
    func setWindowDays(_ windowDays: UInt32?)
    func beginPass(sessions: [ChatCatchUpSession], nowMs: Int64) -> UInt32
    func nextWalk(nowMs: Int64) -> String?
    func nextAsk(sessionId: String, nowMs: Int64) -> ChatCatchUpAsk?
    func takeAnswer(requestId: String, chunkJson: String?, nowMs: Int64) -> ChatCatchUpTaken?
    func stop()
    var concurrency: Int { get }
    var askTimeoutMs: UInt64 { get }
    var passIntervalMs: UInt64 { get }
}

/// How stores and walks are made (the real binding, or the tests' fakes).
public struct ChatStoreFactory: Sendable {
    public let open: @Sendable (_ dataDir: URL, _ accountId: String) throws -> ChatStoreBackend
    public let catchUp: @Sendable (_ store: ChatStoreBackend, _ ownDeviceId: String, _ userId: String, _ windowDays: UInt32?) -> ChatCatchUpWalker?
    /// #EDE-120 SIGN-OUT: deletes the account's store files (never another file).
    public let deleteAccount: @Sendable (_ dataDir: URL, _ accountId: String) throws -> Void
    /// Is this frame a catch-up page (a `history_page` naming a device)?
    public let isCatchUpPage: @Sendable (_ frameJson: String) -> Bool

    public init(
        open: @escaping @Sendable (URL, String) throws -> ChatStoreBackend,
        catchUp: @escaping @Sendable (ChatStoreBackend, String, String, UInt32?) -> ChatCatchUpWalker?,
        deleteAccount: @escaping @Sendable (URL, String) throws -> Void,
        isCatchUpPage: @escaping @Sendable (String) -> Bool = ChatStoreWire.isCatchUpPage
    ) {
        self.open = open
        self.catchUp = catchUp
        self.deleteAccount = deleteAccount
        self.isCatchUpPage = isCatchUpPage
    }

    /// The real store, or nil when this build does not link the library.
    public static var linked: ChatStoreFactory? {
        #if ENET_NODE
        ChatStoreFactory(
            open: { dir, accountId in try ChatStore.open(dataDir: dir.path, accountId: accountId) },
            catchUp: { store, ownDeviceId, userId, windowDays in
                guard let store = store as? ChatStore else { return nil }
                return ChatStoreCatchUpAdapter(CatchUp(store: store, ownDeviceId: ownDeviceId, userId: userId, windowDays: windowDays))
            },
            deleteAccount: { dir, accountId in _ = try deleteAccountStore(dataDir: dir.path, accountId: accountId) },
            isCatchUpPage: { ExpCore.isCatchUpPage(frameJson: $0) }
        )
        #else
        nil
        #endif
    }
}

#if ENET_NODE
extension ChatStore: ChatStoreBackend {
    public func answerPage(sessionId: String, pageJson: String, access: ChatStorePageAccess) throws -> String? {
        let verdict: PageAccess = switch access {
        case .allowed: .allowed
        case .unauthorized: .unauthorized
        case .noSuchSession: .noSuchSession
        }
        return try answerPage(sessionId: sessionId, pageJson: pageJson, access: verdict)
    }
}

/// The facade's `CatchUp` as a `ChatCatchUpWalker`.
final class ChatStoreCatchUpAdapter: ChatCatchUpWalker, @unchecked Sendable {
    private let walk: CatchUp

    init(_ walk: CatchUp) { self.walk = walk }

    func setInstalls(selfEndpoint: String, installs: [ChatCatchUpInstall]) {
        walk.setInstalls(selfEndpoint: selfEndpoint, installs: installs.map {
            CatchUpInstall(endpointId: $0.endpointId, userId: $0.userId, deviceId: $0.deviceId, lastSeenAt: $0.lastSeenAt)
        })
    }

    func setWindowDays(_ windowDays: UInt32?) { walk.setWindowDays(windowDays: windowDays) }

    func beginPass(sessions: [ChatCatchUpSession], nowMs: Int64) -> UInt32 {
        walk.beginPass(sessions: sessions.map {
            CatchUpSession(sessionId: $0.sessionId, rootId: $0.rootId, deviceId: $0.deviceId, updatedAt: $0.updatedAt, busy: $0.busy)
        }, nowMs: nowMs)
    }

    func nextWalk(nowMs: Int64) -> String? { walk.nextWalk(nowMs: nowMs) }

    func nextAsk(sessionId: String, nowMs: Int64) -> ChatCatchUpAsk? {
        walk.nextAsk(sessionId: sessionId, nowMs: nowMs).map {
            ChatCatchUpAsk(requestId: $0.requestId, sessionId: $0.sessionId, peer: $0.peer, frame: $0.frame, waitMs: $0.waitMs)
        }
    }

    func takeAnswer(requestId: String, chunkJson: String?, nowMs: Int64) -> ChatCatchUpTaken? {
        walk.takeAnswer(requestId: requestId, chunkJson: chunkJson, nowMs: nowMs).map {
            ChatCatchUpTaken(sessionId: $0.sessionId, rootId: $0.rootId, added: $0.added)
        }
    }

    func stop() { walk.stop() }
    var concurrency: Int { Int(walk.concurrency()) }
    var askTimeoutMs: UInt64 { walk.askTimeoutMs() }
    var passIntervalMs: UInt64 { walk.passIntervalMs() }
}
#endif

/// The wire shapes the store takes and gives (pure; the tests read them).
public enum ChatStoreWire {
    /// `[{seq, event}]` (the store's `putFeed` input) out of received frame
    /// texts: `activity {event, seq}` frames and `history_chunk {events,
    /// seqs}` pages; anything else, and an event without a seq, drops out.
    public static func feedEvents(_ frames: [String]) -> [[String: Any]] {
        var out: [[String: Any]] = []
        for text in frames {
            guard let object = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] else { continue }
            switch object["t"] as? String {
            case "activity":
                guard let seq = object["seq"] as? NSNumber, let event = object["event"] as? [String: Any] else { continue }
                out.append(["seq": seq, "event": event])
            case "history_chunk":
                let events = object["events"] as? [Any] ?? []
                let seqs = object["seqs"] as? [NSNumber] ?? []
                for (index, event) in events.enumerated() where index < seqs.count {
                    guard let event = event as? [String: Any] else { continue }
                    out.append(["seq": seqs[index], "event": event])
                }
            default:
                continue
            }
        }
        return out
    }

    /// The JSON text of `feedEvents`; nil = nothing to store.
    public static func feedJson(_ frames: [String]) -> String? {
        let events = feedEvents(frames)
        guard !events.isEmpty, let data = try? JSONSerialization.data(withJSONObject: events) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// OPEN: `openTail`'s `{events: [{seq, event}], truncated}` as the
    /// `activity` frame texts a feed applies (`{t, seq, event}`), in order.
    public static func tailFrames(_ json: String) -> (frames: [String], truncated: Bool) {
        guard let object = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any] else { return ([], false) }
        let events = object["events"] as? [[String: Any]] ?? []
        let frames: [String] = events.compactMap { row in
            guard let seq = row["seq"], let event = row["event"],
                  let data = try? JSONSerialization.data(withJSONObject: ["t": "activity", "seq": seq, "event": event]) else { return nil }
            return String(data: data, encoding: .utf8)
        }
        return (frames, object["truncated"] as? Bool == true)
    }

    /// An older page off the store (`page`'s `{events, seqs}`) as the
    /// `history_chunk` text a feed applies.
    public static func pageChunk(_ json: String) -> String? {
        guard var object = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any],
              let events = object["events"] as? [Any], !events.isEmpty else { return nil }
        object["t"] = "history_chunk"
        object["done"] = true
        guard let data = try? JSONSerialization.data(withJSONObject: object) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// `history_page` naming a `deviceId` (the facade's `is_page`, for a
    /// build without the library).
    public static func isCatchUpPage(_ frame: String) -> Bool {
        guard let object = (try? JSONSerialization.jsonObject(with: Data(frame.utf8))) as? [String: Any] else { return false }
        return object["t"] as? String == "history_page" && (object["deviceId"] as? String).map { !$0.isEmpty } == true
            && object["requestId"] is String
    }

    /// A frame's `t` and `requestId`.
    public static func typeAndRequest(_ frame: String) -> (t: String?, requestId: String?) {
        guard let object = (try? JSONSerialization.jsonObject(with: Data(frame.utf8))) as? [String: Any] else { return (nil, nil) }
        return (object["t"] as? String, object["requestId"] as? String)
    }

    /// An ISO-8601 stamp as epoch ms.
    public static func isoMs(_ value: String) -> Int64? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        guard let date = fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value) else { return nil }
        return Int64((date.timeIntervalSince1970 * 1000).rounded())
    }
}
