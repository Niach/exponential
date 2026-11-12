import Foundation
import os

// VAPP-33: steer over eNet's chat channel. The spec is
// packages/domain-contract/fixtures/steer-enet.json (its `$comment`): one chat
// message = one envelope `{t:"exp-steer", v:1, sessionId, frames}` carrying
// the steer relay's wire frames unchanged (each with its own `t`). Mirrors the
// TS reference `@exp/domain-contract/steer-enet` and crates/steer
// `enet_steer.rs`. The viewer joins the session's device here; since VAPP-43
// the steer relay carries no session content, so a viewer that can't join has
// NO FEED (`SteerEnetNoFeed`) and shows its existing states instead.
// #EXP-1528: unless the sealed relay tunnel (SteerTunnel.swift) carries it:
// eNet stays primary, the relay is the fallback (SteerFeedCarriers).

private let logger = Logger(subsystem: "at.exponential", category: "SteerEnet")

public enum SteerEnet {
    public static let tag = "exp-steer"
    public static let version = 1
    public static let batchMaxBytes = 262_144
    public static let chatMaxBytes = 1_048_576
    public static let keepaliveMs = 30_000
    public static let staleMs = 75_000
    public static let joinTimeoutMs = 8_000
    public static let sendTimeoutMs = 20_000
    /// #PERF-9 STORED TAIL: the newest feed events a viewer keeps per session.
    public static let storedTail = 200
    /// STORED TAIL PRUNING (LRU per account): the most recently opened
    /// sessions whose tails stay, and how long an unopened one is kept.
    public static let storedTailMaxSessions = 100
    public static let storedTailMaxAgeDays = 30

    /// One parsed envelope; every frame as its own JSON text (what the steer
    /// model's reducer reads off the relay socket too).
    public struct Envelope: Equatable, Sendable {
        public let sessionId: String
        public let frames: [String]
        /// Each frame's `t`, in order.
        public let types: [String]
    }

    /// One envelope's chat body.
    public static func encode(sessionId: String, frames: [String]) -> Data {
        let id = (try? JSONEncoder().encode(sessionId)).flatMap { String(data: $0, encoding: .utf8) } ?? "\"\""
        return Data("{\"t\":\"\(tag)\",\"v\":\(version),\"sessionId\":\(id),\"frames\":[\(frames.joined(separator: ","))]}".utf8)
    }

    /// A chat body → the envelope, or nil when it is none (the fixture's
    /// `parse` rules).
    public static func parse(_ body: Data) -> Envelope? {
        guard body.first == UInt8(ascii: "{"),
              let value = try? JSONSerialization.jsonObject(with: body) else { return nil }
        return envelope(from: value)
    }

    /// The parse rules over an already-parsed JSON value.
    public static func envelope(from value: Any) -> Envelope? {
        guard let object = value as? [String: Any],
              object["t"] as? String == tag,
              let v = object["v"] as? NSNumber, !isBool(v), v.doubleValue == Double(version),
              let sessionId = object["sessionId"] as? String, !sessionId.isEmpty,
              let raw = object["frames"] as? [Any], !raw.isEmpty else { return nil }
        var frames: [String] = []
        var types: [String] = []
        for item in raw {
            guard let frame = item as? [String: Any], let type = frame["t"] as? String,
                  let data = try? JSONSerialization.data(withJSONObject: frame),
                  let text = String(data: data, encoding: .utf8) else { return nil }
            frames.append(text)
            types.append(type)
        }
        return Envelope(sessionId: sessionId, frames: frames, types: types)
    }

    /// The device's access rule for one envelope's sender (replayed here so
    /// every platform locks the same table).
    public static func viewerAllowed(viewerUserId: String?, starterUserId: String, shared: Bool) -> Bool {
        guard let viewerUserId else { return false }
        return viewerUserId == starterUserId || shared
    }

    /// The device's batching over serialized frame sizes: groups of frame
    /// indexes (one envelope each) plus the frames too big for any envelope.
    public static func batches(
        sizes: [Int], overhead: Int, batchMaxBytes: Int = batchMaxBytes, chatMaxBytes: Int = chatMaxBytes
    ) -> (groups: [[Int]], dropped: [Int]) {
        var groups: [[Int]] = []
        var dropped: [Int] = []
        var current: [Int] = []
        var bytes = overhead
        for (index, size) in sizes.enumerated() {
            if overhead + size > chatMaxBytes {
                dropped.append(index)
                continue
            }
            let next = bytes + size + (current.isEmpty ? 0 : 1)
            if !current.isEmpty, next > batchMaxBytes {
                groups.append(current)
                current = [index]
                bytes = overhead + size
                continue
            }
            current.append(index)
            bytes = next
        }
        if !current.isEmpty { groups.append(current) }
        return (groups, dropped)
    }

    /// The `t` of one frame's JSON text.
    public static func frameType(_ frame: String) -> String? {
        guard let data = frame.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return object["t"] as? String
    }

    static func errorCode(_ frame: String) -> String? {
        guard let data = frame.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              object["t"] as? String == "error" else { return nil }
        return object["code"] as? String
    }

    private static func isBool(_ number: NSNumber) -> Bool {
        CFGetTypeID(number) == CFBooleanGetTypeID()
    }

    static let joinFrame = #"{"t":"join","channel":"activity"}"#
    static let leaveFrame = #"{"t":"leave"}"#
}

/// The device side of `installs.forSession`: one install of the machine that
/// runs the session, newest first.
public struct SteerEndpoint: Decodable, Equatable, Sendable {
    public let endpointId: String
    public let kind: String

    public init(endpointId: String, kind: String) {
        self.endpointId = endpointId
        self.kind = kind
    }
}

/// What a steer viewer needs from this install's eNet node (a fake in the
/// tests): discovery, one link per (session, endpoint), chat sends.
public protocol SteerEnetCarrier: AnyObject, Sendable {
    /// False when no node runs (eNet off in this build, signed out, failed).
    func ready() async -> Bool
    /// `installs.forSession`, in the server's order.
    func endpoints(accountId: String, sessionId: String) async throws -> [String]
    /// Subscribes to the envelopes `peer` sends for `sessionId` (and admits
    /// `peer` on the node: it dials us back).
    func open(sessionId: String, peer: String) async -> SteerEnetLink
    /// One chat message to `peer`, returned once acked.
    func send(peer: String, body: Data) async throws
    /// EXP-1531 connStats: how `peer` is reached right now, `direct` |
    /// `iroh_relay`; nil = no open connection (or no node).
    func pathKind(peer: String) async -> String?
}

extension SteerEnetCarrier {
    public func pathKind(peer: String) async -> String? { nil }
}

/// The frames one joined device sends for one session, plus the ordered
/// sends back to it. Buffers until someone reads: the joiner while joining
/// (`next`), then the model (`stream`).
public final class SteerEnetLink: @unchecked Sendable {
    public let sessionId: String
    public let peer: String
    private let lock = NSLock()
    private var closeHooks: [@Sendable () -> Void] = []
    private var queue: [[String]] = []
    private var waiter: CheckedContinuation<[String]?, Never>?
    private var sink: AsyncStream<[String]>.Continuation?
    private var closed = false
    private var outbox: AsyncStream<[String]>.Continuation?
    private var sender: Task<Void, Never>?
    private var onSendFailure: (@Sendable () -> Void)?
    /// EXP-1531: the peer's last connection closed (`peerDisconnected`)
    /// before anyone read the stream: `stream` reports the drop at once.
    private var dropped = false
    /// An envelope arrived: this link rode a live connection (a
    /// `peerDisconnected` the pump hands over before that is about an older
    /// connection, not this one).
    private var received = false
    /// SUBSCRIPTIONS: the links this one shares its device feed with.
    private let inbox: SteerEnetInbox?

    /// - Parameter inbox: where the carrier registers this link; a `leave`
    ///   goes out only while no other live link of the same (session, peer)
    ///   is registered there (the device keys viewers by endpoint, so it
    ///   would end THAT link's feed too).
    public init(
        sessionId: String,
        peer: String,
        inbox: SteerEnetInbox? = nil,
        send: @escaping @Sendable (Data) async throws -> Void
    ) {
        self.sessionId = sessionId
        self.peer = peer
        self.inbox = inbox
        let (stream, continuation) = AsyncStream<[String]>.makeStream()
        outbox = continuation
        let sendBody = send
        let sessionId = sessionId
        let me = ObjectIdentifier(self)
        sender = Task.detached { [weak self] in
            // ONE ordered stream of sends: each awaited (acked) before the next.
            for await frames in stream {
                // EXP-1487: re-checked at send time, a newer link of the same
                // (session, peer) may have joined since `close` queued it.
                if frames == [SteerEnet.leaveFrame],
                   inbox?.sharesFeed(sessionId: sessionId, peer: peer, besides: me) == true {
                    logger.info("eNet steer leave skipped: another link reads this feed")
                    continue
                }
                do {
                    try await SteerEnetLink.withTimeout(.milliseconds(SteerEnet.sendTimeoutMs)) {
                        try await sendBody(SteerEnet.encode(sessionId: sessionId, frames: frames))
                    }
                } catch {
                    logger.info("eNet steer send failed: \(String(describing: error), privacy: .public)")
                    self?.failed()
                    return
                }
            }
        }
    }

    deinit { sender?.cancel() }

    /// The inbox hands over one envelope's frames.
    func receive(_ frames: [String]) {
        lock.lock()
        if !closed { received = true }
        if closed {
            lock.unlock()
        } else if let sink {
            lock.unlock()
            sink.yield(frames)
        } else if let waiter {
            self.waiter = nil
            lock.unlock()
            waiter.resume(returning: frames)
        } else {
            queue.append(frames)
            lock.unlock()
        }
    }

    /// The next envelope's frames while joining; nil once closed or cancelled.
    func next() async -> [String]? {
        await withTaskCancellationHandler {
            await withCheckedContinuation { (continuation: CheckedContinuation<[String]?, Never>) in
                lock.lock()
                if !queue.isEmpty {
                    let frames = queue.removeFirst()
                    lock.unlock()
                    continuation.resume(returning: frames)
                } else if closed || dropped || Task.isCancelled {
                    lock.unlock()
                    continuation.resume(returning: nil)
                } else {
                    waiter = continuation
                    lock.unlock()
                }
            }
        } onCancel: {
            lock.lock()
            let pending = waiter
            waiter = nil
            lock.unlock()
            pending?.resume(returning: nil)
        }
    }

    /// Every envelope from here on (anything buffered first), in order. One
    /// reader; `onSendFailure` fires once if a send fails or times out, or
    /// the peer's last connection closed (the device dropped us: redial).
    public func stream(onSendFailure: @escaping @Sendable () -> Void) -> AsyncStream<[String]> {
        let (stream, continuation) = AsyncStream<[String]>.makeStream()
        lock.lock()
        let droppedAlready = dropped && !closed
        self.onSendFailure = droppedAlready ? nil : onSendFailure
        for frames in queue { continuation.yield(frames) }
        queue = []
        if closed { continuation.finish() } else { sink = continuation }
        lock.unlock()
        if droppedAlready { onSendFailure() }
        return stream
    }

    /// Queue frames (each one frame's JSON text) for the device.
    public func send(_ frames: [String]) {
        lock.withLock { closed ? nil : outbox }?.yield(frames)
    }

    /// Re-send `join` (the stale rule): the device restarts the feed.
    public func rejoin() { send([SteerEnet.joinFrame]) }

    /// Unsubscribe; `leave` goes out first when asked (best effort), and
    /// only when no other live link reads the same (session, peer) feed
    /// (SUBSCRIPTIONS, the desktop's `ViewerSubscription::shared`).
    public func close(sendLeave: Bool = false) {
        let shared = sendLeave
            && inbox?.sharesFeed(sessionId: sessionId, peer: peer, besides: ObjectIdentifier(self)) == true
        lock.lock()
        guard !closed else { lock.unlock(); return }
        if sendLeave, !shared { outbox?.yield([SteerEnet.leaveFrame]) }
        closed = true
        outbox?.finish()
        let sink = self.sink, waiter = self.waiter, hooks = closeHooks
        self.sink = nil
        self.waiter = nil
        queue = []
        closeHooks = []
        lock.unlock()
        sink?.finish()
        waiter?.resume(returning: nil)
        for hook in hooks { hook() }
    }

    /// Runs once on `close` (the carrier unregisters the link from its inbox).
    public func onClose(_ hook: @escaping @Sendable () -> Void) {
        lock.withLock { closeHooks.append(hook) }
    }

    private func failed() {
        let callback = lock.withLock { () -> (@Sendable () -> Void)? in
            defer { onSendFailure = nil }
            return closed ? nil : onSendFailure
        }
        callback?()
    }

    /// EXP-1531: the peer's LAST connection closed (hung up, or the QUIC
    /// idle timeout found it dead): ends the link at once exactly like a
    /// failed send, instead of waiting out `staleMs`. A joiner still reading
    /// (`next`) wakes with nil and moves on. Ignored until an envelope
    /// arrived: the pump is ordered, so an earlier disconnect is a previous
    /// connection's (a redial's link must not die of its predecessor's drop).
    func peerDisconnected() {
        let (callback, pending) = lock.withLock { () -> ((@Sendable () -> Void)?, CheckedContinuation<[String]?, Never>?) in
            guard !closed, received else { return (nil, nil) }
            dropped = true
            let pending = waiter
            waiter = nil
            defer { onSendFailure = nil }
            return (onSendFailure, pending)
        }
        pending?.resume(returning: nil)
        callback?()
    }

    /// Whether the peer's connection closed under this link.
    var isDropped: Bool { lock.withLock { dropped } }

    /// #EXP-1623: a RACE, not a task group: a group waits for every child,
    /// and a node call that ignores cancellation (the uniffi `send_chat`
    /// future while iroh dials) held the caller far past its deadline. The
    /// loser is cancelled and never awaited.
    static func withTimeout<T: Sendable>(
        _ timeout: Duration, _ body: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        let race = SteerEnetRace<T>()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                race.start(continuation, work: Task { try await body() }, timeout: timeout)
            }
        } onCancel: {
            race.finish(.failure(CancellationError()))
        }
    }
}

/// #EXP-1623: `withTimeout`'s one-shot finish line: the work, the clock or
/// the caller's cancellation, whichever comes first, resumes the caller.
private final class SteerEnetRace<T: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<T, Error>?
    private var work: Task<T, Error>?
    private var clock: Task<Void, Never>?
    private var outcome: Result<T, Error>?

    func start(_ continuation: CheckedContinuation<T, Error>, work: Task<T, Error>, timeout: Duration) {
        let early = lock.withLock { () -> Result<T, Error>? in
            self.continuation = continuation
            self.work = work
            return outcome
        }
        if let early {
            work.cancel()
            lock.withLock { self.continuation = nil }
            continuation.resume(with: early)
            return
        }
        let clock = Task { [weak self] in
            try? await Task.sleep(for: timeout)
            guard !Task.isCancelled else { return }
            self?.finish(.failure(SteerEnetTimeout()))
        }
        lock.withLock { self.clock = clock }
        Task { [weak self] in
            let result = await work.result
            self?.finish(result)
        }
    }

    func finish(_ result: Result<T, Error>) {
        let (continuation, work, clock) = lock.withLock { () -> (CheckedContinuation<T, Error>?, Task<T, Error>?, Task<Void, Never>?) in
            guard case .none = outcome else { return (nil, nil, nil) }
            outcome = result
            defer { self.continuation = nil }
            return (self.continuation, self.work, self.clock)
        }
        work?.cancel()
        clock?.cancel()
        continuation?.resume(with: result)
    }
}

struct SteerEnetTimeout: Error {}

/// Routes incoming chat to the open links: only envelopes whose sessionId a
/// link joined AND whose sender is the endpoint it joined (VIEWER SIDE);
/// every other steer envelope is dropped.
public final class SteerEnetInbox: @unchecked Sendable {
    private let lock = NSLock()
    private var links: [String: [ObjectIdentifier: WeakLink]] = [:]

    private struct WeakLink { weak var link: SteerEnetLink? }

    public init() {}

    private static func key(_ sessionId: String, _ peer: String) -> String {
        "\(sessionId)\n\(peer.lowercased())"
    }

    public func register(_ link: SteerEnetLink) {
        lock.withLock {
            links[Self.key(link.sessionId, link.peer), default: [:]][ObjectIdentifier(link)] = WeakLink(link: link)
        }
    }

    public func unregister(_ link: SteerEnetLink) {
        lock.withLock {
            let key = Self.key(link.sessionId, link.peer)
            links[key]?.removeValue(forKey: ObjectIdentifier(link))
            if links[key]?.isEmpty == true { links.removeValue(forKey: key) }
        }
    }

    /// SUBSCRIPTIONS: whether a live link other than `besides` reads
    /// `sessionId` from `peer` (then a `leave` would end its feed too).
    public func sharesFeed(sessionId: String, peer: String, besides: ObjectIdentifier) -> Bool {
        lock.withLock {
            (links[Self.key(sessionId, peer)] ?? [:]).contains { $0.key != besides && $0.value.link != nil }
        }
    }

    /// EXP-1531 `peerDisconnected`: the last connection to `peer` closed,
    /// so every link joined to it ends at once (the send-failure path).
    public func peerDisconnected(_ peer: String) {
        let peer = peer.lowercased()
        let targets = lock.withLock {
            links.values.flatMap { $0.values.compactMap(\.link) }.filter { $0.peer.lowercased() == peer }
        }
        for link in targets { link.peerDisconnected() }
    }

    /// One chat message from `from`. True when it was a steer envelope
    /// (applied or ignored), false when it is someone else's message.
    @discardableResult
    public func deliver(from: String, body: Data) -> Bool {
        guard let envelope = SteerEnet.parse(body) else { return false }
        let targets = lock.withLock {
            (links[Self.key(envelope.sessionId, from)] ?? [:]).values.compactMap(\.link)
        }
        for link in targets { link.receive(envelope.frames) }
        return true
    }
}

/// Why a viewer has no feed (VAPP-43: there is no relay to fall back to).
public enum SteerEnetNoFeed: String, Equatable, Sendable {
    case noNode, lookupFailed, noEndpoint, allRefused, timeout
    /// #EXP-1678: a join could not be sent (or the link died mid-join) on an
    /// endpoint; `allRefused` = every endpoint ANSWERED error / bye.
    case sendFailed
}

public enum SteerEnetJoinOutcome: Sendable {
    /// Joined: `replay` = every frame up to and including `activity_synced`,
    /// plus whatever followed it in the same envelope.
    case joined(SteerEnetLink, replay: [String])
    case noFeed(SteerEnetNoFeed)
}

/// What a viewer with NO FEED shows (the fixture's `$comment`): its existing
/// states with their existing copy, whatever the reason.
public enum SteerNoFeedState: Equatable, Sendable {
    /// The row is still alive: "The live stream isn't up yet. The desktop may
    /// still be connecting." — and redial on the starting backoff.
    case starting
    /// The row ended: `ended` with the device-offline copy ("{device} is
    /// offline. The transcript lives on that machine."). NOT terminal
    /// (VAPP-169): the viewer redials per `SteerOfflineRedial`.
    case endedDeviceOffline

    /// - Parameter sessionOver: the synced row ended, or it vanished.
    public static func decide(sessionOver: Bool) -> SteerNoFeedState {
        sessionOver ? .endedDeviceOffline : .starting
    }
}

/// OFFLINE REDIAL (VAPP-169, the fixture's `offlineRedial` cases): which
/// signal redials a viewer showing no transcript. The redial holds the phase.
public enum SteerOfflineRedial {
    public enum Phase: String, Sendable {
        /// The device-offline `ended` (NO FEED on an ended row).
        case offline
        /// Ended by `bye`: the transcript is on screen.
        case ended
        /// A live row nobody feeds yet (redials on its own backoff).
        case starting
    }

    public enum Trigger: String, Sendable {
        /// The session's device flipped online in presence.
        case presenceOnline
        case presenceOffline
        /// Every `SteerEnet.staleMs` while the offline copy shows.
        case staleTimer
        /// App foreground, network back, the screen opened.
        case wakeup
    }

    public static func decide(phase: Phase, trigger: Trigger) -> Bool {
        if trigger == .presenceOffline || phase == .ended { return false }
        if phase == .offline { return true }
        return trigger != .staleTimer
    }
}

/// LOST BANNER (EXP-1487, the fixture's `lostBanner` + `lostBannerCases`): a
/// feed that was live and drops redials SILENTLY (the phase holds, no
/// "Connection lost"); the banner shows only once the feed has been down at
/// least `graceMs` since the drop AND at least `failedRedials` redials since
/// the drop failed. A successful join resets both.
public enum SteerLostBanner {
    public static let graceMs = 5_000
    public static let failedRedials = 2

    /// - Parameters:
    ///   - downMs: milliseconds since the drop.
    ///   - failedRedials: redials since the drop that failed.
    public static func show(downMs: Int, failedRedials: Int) -> Bool {
        downMs >= graceMs && failedRedials >= Self.failedRedials
    }
}

/// NO FEED, EMPTY TRANSCRIPT (EXP-1487, the fixture's `noFeed` cases +
/// `noFeedIdle`): what a viewer with nothing to show says while NO FEED
/// holds, keyed on the synced row. An ended row = the device-offline copy;
/// #EXP-1703: a live row whose device is offline = `away`; a busy row =
/// `starting`; a row that is neither (idle, or waiting on an open
/// question) is never `starting`: the `idle` empty state (no spinner) while
/// it keeps redialing on its backoff.
public enum SteerNoFeed {
    public enum State: String, Equatable, Sendable {
        case offline, away, starting, idle
    }

    /// `noFeedIdle.title` / `noFeedIdle.text`.
    public static let idleTitle = "No live transcript"
    public static let idleText = "The device holding this session isn't publishing right now. Its conversation stays in the chat."

    /// #EXP-1703: `noFeedAway` — `{device}` = the device's name (nil/empty
    /// = `awayDevice`).
    public static let awayTitle = "{device} is offline"
    public static let awayText = "This session continues when it's back. Its transcript loads then."
    public static let awayDevice = "Its device"

    public static func away(deviceName: String?) -> (title: String, text: String) {
        let device = (deviceName ?? "").isEmpty ? awayDevice : deviceName ?? ""
        return (awayTitle.replacingOccurrences(of: "{device}", with: device), awayText)
    }

    public static func state(ended: Bool, busy: Bool, deviceOffline: Bool = false) -> State {
        if ended { return .offline }
        if deviceOffline { return .away }
        return busy ? .starting : .idle
    }
}

/// The transport choice: `installs.forSession` → `join` the first endpoint →
/// on `error no_such_session` (or `unauthorized`, or a failed send) the next
/// → no node / no endpoint / every endpoint refused / a failed send /
/// no `activity_synced` within `joinTimeoutMs` = no feed.
public enum SteerEnetJoiner {
    public static func join(
        carrier: SteerEnetCarrier,
        accountId: String,
        sessionId: String,
        timeout: Duration = .milliseconds(SteerEnet.joinTimeoutMs),
        onFirstFrame: (@Sendable () -> Void)? = nil
    ) async -> SteerEnetJoinOutcome {
        guard await carrier.ready() else { return .noFeed(.noNode) }
        let endpoints: [String]
        do {
            endpoints = try await carrier.endpoints(accountId: accountId, sessionId: sessionId)
        } catch {
            logger.info("installs.forSession failed: \(error.localizedDescription, privacy: .public)")
            return .noFeed(.lookupFailed)
        }
        guard !endpoints.isEmpty else { return .noFeed(.noEndpoint) }
        let deadline = ContinuousClock.now + timeout
        var sendFailed = false
        for peer in endpoints {
            let link = await carrier.open(sessionId: sessionId, peer: peer)
            let result = await attempt(link: link, carrier: carrier, deadline: deadline, onFirstFrame: onFirstFrame)
            switch result {
            case let .joined(replay):
                return .joined(link, replay: replay)
            case .refused:
                link.close()
                continue
            case .sendFailed:
                link.close()
                sendFailed = true
                continue
            case .timedOut:
                link.close()
                return .noFeed(.timeout)
            }
        }
        // #EXP-1678: `allRefused` only when the devices themselves said no.
        return .noFeed(sendFailed ? .sendFailed : .allRefused)
    }

    private enum Attempt: Sendable { case joined([String]), refused, sendFailed, timedOut }

    private static func attempt(
        link: SteerEnetLink,
        carrier: SteerEnetCarrier,
        deadline: ContinuousClock.Instant,
        onFirstFrame: (@Sendable () -> Void)?
    ) async -> Attempt {
        let peer = link.peer
        let body = SteerEnet.encode(sessionId: link.sessionId, frames: [SteerEnet.joinFrame])
        return await withTaskGroup(of: Attempt?.self) { group in
            group.addTask {
                // The join goes out on its own (acked) so a peer we can't reach
                // reads as refused right away, not as a timeout.
                // #EXP-1623: bounded by the deadline itself, so a dial that
                // ignores cancellation cannot hold the group past it.
                do {
                    let left = max(deadline - ContinuousClock.now, .milliseconds(1))
                    try await SteerEnetLink.withTimeout(left) {
                        try await carrier.send(peer: peer, body: body)
                    }
                } catch is SteerEnetTimeout {
                    return .timedOut
                } catch {
                    logger.info("eNet steer join to \(peer.prefix(12), privacy: .public) failed: \(String(describing: error), privacy: .public)")
                    return .sendFailed
                }
                var replay: [String] = []
                while let frames = await link.next() {
                    // #EXP-1528: the device answers (the stall timer stops).
                    if replay.isEmpty { onFirstFrame?() }
                    for (index, frame) in frames.enumerated() {
                        if let code = SteerEnet.errorCode(frame),
                           code == "no_such_session" || code == "unauthorized" {
                            return .refused
                        }
                        // #EXP-1678: a `bye` before the replay began = the device said no.
                        if replay.isEmpty, SteerEnet.frameType(frame) == "bye" { return .refused }
                        replay.append(frame)
                        if SteerEnet.frameType(frame) == "activity_synced" {
                            // The rest of this envelope is the live feed's
                            // first frames (a stored replay batches its
                            // `bye {history}` right after the marker): keep
                            // them, in order.
                            replay.append(contentsOf: frames[(index + 1)...])
                            return .joined(replay)
                        }
                    }
                }
                // EXP-1531: the peer hung up mid-join: the next endpoint
                // (#EXP-1678: no answer, so not a refusal).
                return link.isDropped ? .sendFailed : nil
            }
            group.addTask {
                try? await Task.sleep(until: deadline, clock: .continuous)
                return .timedOut
            }
            defer { group.cancelAll() }
            while let result = await group.next() {
                if let result { return result }
            }
            return .timedOut
        }
    }
}

/// The app's carrier: this install's node (`InstallNodeService`) plus the
/// registry's `installs.forSession`.
public final class InstallSteerCarrier: SteerEnetCarrier {
    private let service: InstallNodeService
    private let lookup: SteerEndpointLookup

    public init(service: InstallNodeService, lookup: SteerEndpointLookup) {
        self.service = service
        self.lookup = lookup
    }

    public func ready() async -> Bool { await service.endpointId != nil }

    public func endpoints(accountId: String, sessionId: String) async throws -> [String] {
        let own = await service.endpointId
        return try await lookup.forSession(accountId: accountId, sessionId: sessionId)
            .map(\.endpointId)
            .filter { $0 != own }
    }

    public func open(sessionId: String, peer: String) async -> SteerEnetLink {
        let inbox = service.steerInbox
        let service = service
        let link = SteerEnetLink(
            sessionId: sessionId,
            peer: peer,
            inbox: inbox,
            send: { try await service.sendChat(peer: peer, body: $0) }
        )
        inbox.register(link)
        // The device dials us back on its own stream: admit it.
        await service.admit(peer: peer)
        link.onClose { [weak link] in
            if let link { inbox.unregister(link) }
        }
        return link
    }

    public func send(peer: String, body: Data) async throws {
        try await service.sendChat(peer: peer, body: body)
    }

    public func pathKind(peer: String) async -> String? {
        await service.pathKind(peer: peer)
    }
}

/// `installs.forSession` (a fake in the tests).
public protocol SteerEndpointLookup: Sendable {
    func forSession(accountId: String, sessionId: String) async throws -> [SteerEndpoint]
}

private struct InstallsSessionInput: Encodable { let sessionId: String }

// Here, not in `InstallsApi.swift`: the share extension compiles that file
// without eNet.
extension InstallsApi: SteerEndpointLookup {
    /// VAPP-33: the installs of the machine running `sessionId`, newest first.
    public func forSession(accountId: String, sessionId: String) async throws -> [SteerEndpoint] {
        try await trpc.query(accountId: accountId, path: "installs.forSession", input: InstallsSessionInput(sessionId: sessionId))
    }
}
