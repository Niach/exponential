import Foundation
import os

// #EXP-1528: one viewer feed over its two carriers, driven by the carrier
// rule (`SteerCarrier.step`, fixtures/steer-enet.json `carrierCases`): eNet
// first with the stall timer armed; the sealed relay tunnel when eNet stalls,
// has no feed or drops; eNet redials in the background while the relay is
// active and takes over on its `activity_reset`. Frames reach the model only
// from the ACTIVE carrier; steering goes out on it.
//
// The model sees ONE feed: `.active` when a carrier becomes active, `.frames`,
// and the end of the feed: `.noFeed` (nothing ever became active: both
// failed) or `.closed` (it was active and now NEITHER carrier is, with no
// redial in flight). `reopenRelayLater` IS that `.closed`: the model's own
// backoff redial starts a fresh feed (eNet + stall + relay), which is how the
// lost-banner rule counts failed redials of BOTH carriers.

private let logger = Logger(subsystem: "at.exponential", category: "SteerFeedCarriers")

@MainActor
public final class SteerFeedCarriers {
    public enum Signal: Equatable, Sendable {
        /// A carrier became active (enet or relay). The first one ends the dial.
        case active(SteerCarrier.Active)
        /// Frames of the active carrier, in order.
        case frames([String])
        /// An AVAILABLE relay leg opened (connStats `fallbacks`; #EXP-1678: a
        /// `no_key` leg never counts).
        case fallback
        /// One carrier failed (connStats error class).
        case failure(String)
        /// Nothing became active: both carriers failed. The eNet reason when
        /// eNet had no feed.
        case noFeed(SteerEnetNoFeed?)
        /// Was active, now neither carrier is.
        case closed
    }

    public struct Timings: Sendable {
        public var stallMs: Int
        public var joinTimeoutMs: Int
        public var retryMs: [Int]

        public init(
            stallMs: Int = SteerCarrier.stallMs,
            joinTimeoutMs: Int = SteerEnet.joinTimeoutMs,
            retryMs: [Int] = SteerCarrier.relayRetryMs
        ) {
            self.stallMs = stallMs
            self.joinTimeoutMs = joinTimeoutMs
            self.retryMs = retryMs
        }
    }

    public private(set) var state = SteerCarrier.State()
    public private(set) var finished = false
    /// #EXP-1531 connStats: the node's `path_kind` for the joined device
    /// (`direct` | `iroh_relay`), read once per eNet join; nil = unknown.
    public private(set) var enetPath: String?

    private let accountId: String
    private let sessionId: String
    private let enet: SteerEnetCarrier?
    private let tunnel: SteerTunnelDialing?
    private let timings: Timings
    private let onSignal: @MainActor (Signal) -> Void

    private var everActive = false
    private var lastNoFeed: SteerEnetNoFeed?

    private var enetLink: SteerEnetLink?
    private var enetToken = 0
    private var enetDialing = false
    private var enetTask: Task<Void, Never>?
    private var enetRetry: Task<Void, Never>?
    private var enetAttempts = 0
    private var stallTask: Task<Void, Never>?

    private var relayLink: SteerTunnelLink?
    private var relayToken = 0
    private var relayDialing = false
    private var relayTask: Task<Void, Never>?
    private var relayJoinTimer: Task<Void, Never>?
    private var relayKnownUnavailable = false
    /// #EXP-1678: the leg was `no_key` (no fallback to count).
    private var relayNoKey = false

    public init(
        accountId: String,
        sessionId: String,
        enet: SteerEnetCarrier?,
        tunnel: SteerTunnelDialing?,
        timings: Timings = Timings(),
        onSignal: @escaping @MainActor (Signal) -> Void
    ) {
        self.accountId = accountId
        self.sessionId = sessionId
        self.enet = enet
        self.tunnel = tunnel
        self.timings = timings
        self.onSignal = onSignal
    }

    public func start() { handle(.start) }

    /// Steering frames go out on the ACTIVE carrier; false = none is.
    @discardableResult
    public func send(_ frames: [String]) -> Bool {
        switch state.active {
        case .enet: enetLink?.send(frames); return enetLink != nil
        case .relay: relayLink?.send(frames); return relayLink != nil
        case .none: return false
        }
    }

    /// The stale rule's first step: re-send `join` on the active carrier.
    public func rejoin() {
        switch state.active {
        case .enet: enetLink?.rejoin()
        case .relay: relayLink?.rejoin()
        case .none: break
        }
    }

    /// The stale rule's second step: the active carrier is dead.
    public func dropActive() {
        switch state.active {
        case .enet: enetDropped()
        case .relay: relayEnded(token: relayToken)
        case .none: break
        }
    }

    /// A wake signal (foreground, network back, the screen opened): a
    /// background eNet redial waiting out its backoff dials now.
    public func wakeup() {
        guard !finished, enetLink == nil, !enetDialing else { return }
        logger.info("wakeup: eNet redial now")
        enetAttempts = 0
        dialEnet()
    }

    /// Tear down without a signal (the model closes the feed).
    public func close() {
        guard !finished else { return }
        finished = true
        teardown()
    }

    // MARK: - The rule

    private func handle(_ event: SteerCarrier.Event, frames: [String] = []) {
        guard !finished else { return }
        let (next, actions) = SteerCarrier.step(state, event)
        let was = state.active
        state = next
        if next.active != was, next.active != .none {
            everActive = true
            logger.info("active carrier \(next.active.rawValue, privacy: .public)")
            onSignal(.active(next.active))
        }
        for action in actions {
            guard !finished else { return }
            perform(action, frames: frames)
        }
        settleIfDown()
    }

    private func perform(_ action: SteerCarrier.Action, frames: [String]) {
        switch action {
        case .dialEnet: dialEnet()
        case .armStall: armStall()
        case .openRelay:
            openRelay()
        case .rejoinRelay:
            if let relayLink {
                onSignal(.fallback)
                relayLink.rejoin()
                armRelayJoinTimer(token: relayToken)
            } else if !relayDialing {
                openRelay()
            }
        case .closeRelay: closeRelay()
        case .redialEnetLater: redialEnetLater()
        case .reopenRelayLater: break // `.closed`: the model's backoff redial (see the header)
        case .apply: if !frames.isEmpty { onSignal(.frames(frames)) }
        case .drop: break
        }
    }

    /// Both carriers down with nothing in flight = the end of this feed.
    private func settleIfDown() {
        guard !finished, state.active == .none, state.relay == .closed,
              !enetDialing, enetLink == nil, !relayDialing, relayLink == nil else { return }
        finished = true
        teardown()
        onSignal(everActive ? .closed : .noFeed(lastNoFeed))
    }

    private func teardown() {
        stallTask?.cancel()
        enetRetry?.cancel()
        enetTask?.cancel()
        enetToken += 1
        enetDialing = false
        enetLink?.close(sendLeave: true)
        enetLink = nil
        closeRelay()
    }

    // MARK: - eNet

    private func armStall() {
        stallTask?.cancel()
        let ms = timings.stallMs
        stallTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(ms))
            guard let self, !Task.isCancelled else { return }
            logger.info("eNet stalled, opening the relay")
            self.handle(.stall)
        }
    }

    private func dialEnet() {
        guard !finished, enetLink == nil, !enetDialing else { return }
        enetRetry?.cancel()
        enetRetry = nil
        enetDialing = true
        enetToken += 1
        let token = enetToken
        guard let enet else {
            Task { @MainActor [weak self] in self?.enetDialed(.noFeed(.noNode), token: token) }
            return
        }
        let accountId = accountId, sessionId = sessionId
        let timeout = Duration.milliseconds(timings.joinTimeoutMs)
        let firstFrame: @Sendable () -> Void = { [weak self] in
            Task { @MainActor [weak self] in
                guard let self, token == self.enetToken else { return }
                self.stallTask?.cancel()
            }
        }
        enetTask = Task { @MainActor [weak self] in
            let outcome = await SteerEnetJoiner.join(
                carrier: enet,
                accountId: accountId,
                sessionId: sessionId,
                timeout: timeout,
                onFirstFrame: firstFrame
            )
            var path: String?
            if case let .joined(link, _) = outcome { path = await enet.pathKind(peer: link.peer) }
            guard let self else {
                if case let .joined(link, _) = outcome { link.close(sendLeave: true) }
                return
            }
            self.enetDialed(outcome, token: token, path: path)
        }
    }

    private func enetDialed(_ outcome: SteerEnetJoinOutcome, token: Int, path: String? = nil) {
        guard !finished, token == enetToken else {
            // EXP-1487 / #EXP-1531: a dial superseded by a newer one sends no
            // `leave` (its `join` may already have landed and the device keys
            // viewers by endpoint); a finished feed leaves, unless another
            // link of this install still reads it (`SteerEnetLink.close`).
            if case let .joined(link, _) = outcome { link.close(sendLeave: finished) }
            return
        }
        enetDialing = false
        switch outcome {
        case let .joined(link, replay):
            stallTask?.cancel()
            enetAttempts = 0
            enetLink = link
            enetPath = path
            let stream = link.stream { [weak self] in
                Task { @MainActor in
                    guard let self, token == self.enetToken else { return }
                    self.enetDropped()
                }
            }
            // The join replay (`activity_reset` .. `activity_synced`) IS the
            // reset; it ended on the marker, so a stand-by relay closes.
            handle(.enetReset, frames: replay)
            if state.active == .enet, state.relay == .open { handle(.enetSynced) }
            enetTask = Task { @MainActor [weak self] in
                for await frames in stream {
                    guard let self, token == self.enetToken else { return }
                    self.route(frames, reset: .enetReset, rest: .enetSynced)
                }
                guard let self, token == self.enetToken else { return }
                self.enetDropped()
            }
        case let .noFeed(reason):
            logger.info("eNet no feed \(reason.rawValue, privacy: .public)")
            lastNoFeed = reason
            onSignal(.failure(Self.errorClass(reason)))
            // #EXP-1678: the device refused every join: no relay leg (it reaches the same device).
            handle(reason == .allRefused ? .enetRefused : .enetNoFeed)
        }
    }

    private func enetDropped() {
        guard !finished, let link = enetLink else { return }
        logger.info("eNet feed dropped")
        enetToken += 1
        enetTask?.cancel()
        enetLink = nil
        link.close()
        onSignal(.failure("closed"))
        handle(.enetDropped)
    }

    private func redialEnetLater() {
        guard !finished, enetLink == nil, !enetDialing else { return }
        enetRetry?.cancel()
        let curve = timings.retryMs
        let ms = curve.isEmpty ? 1_000 : curve[min(enetAttempts, curve.count - 1)]
        enetAttempts += 1
        enetRetry = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(ms))
            guard let self, !Task.isCancelled else { return }
            self.enetRetry = nil
            self.dialEnet()
        }
    }

    /// One batch of a carrier's frames through the rule: each
    /// `activity_reset` is that carrier's reset, every other frame its
    /// `*Synced` (runs of them applied together).
    private func route(_ frames: [String], reset: SteerCarrier.Event, rest: SteerCarrier.Event) {
        var run: [String] = []
        for frame in frames {
            if SteerEnet.frameType(frame) == "activity_reset" {
                if !run.isEmpty { handle(rest, frames: run) }
                run = []
                handle(reset, frames: [frame])
            } else {
                run.append(frame)
            }
            if finished { return }
        }
        if !run.isEmpty { handle(rest, frames: run) }
    }

    static func errorClass(_ reason: SteerEnetNoFeed) -> String {
        switch reason {
        case .noNode: "no_node"
        case .lookupFailed: "other"
        case .noEndpoint: "no_endpoint"
        case .allRefused: "refused"
        case .timeout: "join_timeout"
        case .sendFailed: "send_failed"
        }
    }

    // MARK: - Relay

    private func openRelay() {
        guard !finished, relayLink == nil, !relayDialing else { return }
        guard let tunnel, !relayKnownUnavailable else {
            // #EXP-1678: a leg known unavailable for any reason but `no_key` was available.
            if tunnel != nil, relayKnownUnavailable, !relayNoKey { onSignal(.fallback) }
            Task { @MainActor [weak self] in self?.handle(.relayUnavailable) }
            return
        }
        relayDialing = true
        relayToken += 1
        let token = relayToken
        let sessionId = sessionId
        relayTask = Task { @MainActor [weak self] in
            let opened = await tunnel.open(sessionId: sessionId)
            guard let self, !self.finished, token == self.relayToken else {
                if case let .link(link) = opened { link.close() }
                return
            }
            self.relayOpened(opened, token: token)
        }
    }

    private func relayOpened(_ opened: SteerTunnelOpen, token: Int) {
        relayDialing = false
        // #EXP-1678: count the leg only once it is available (a key held here).
        if case .unavailable("no_key") = opened {
            relayNoKey = true
        } else {
            onSignal(.fallback)
        }
        switch opened {
        case let .unavailable(errorClass):
            logger.info("relay unavailable \(errorClass, privacy: .public)")
            relayKnownUnavailable = true
            onSignal(.failure(errorClass))
            handle(.relayUnavailable)
        case let .link(link):
            guard state.relay == .open else {
                link.close()
                settleIfDown()
                return
            }
            relayLink = link
            link.send([SteerEnet.joinFrame])
            armRelayJoinTimer(token: token)
            relayTask = Task { @MainActor [weak self] in
                for await frames in link.frames {
                    guard let self, token == self.relayToken else { return }
                    self.relayFrames(frames, token: token)
                    if self.finished || token != self.relayToken { return }
                }
                self?.relayEnded(token: token)
            }
        }
    }

    private var relayAnswered = false

    private func relayFrames(_ frames: [String], token: Int) {
        if !relayAnswered {
            // Before the device's reset: a refusal means it does not hold
            // (or won't show us) the session: no relay leg.
            for frame in frames {
                if let code = SteerEnet.errorCode(frame), code == "no_such_session" || code == "unauthorized" {
                    relayKnownUnavailable = true
                    onSignal(.failure(code == "unauthorized" ? "unauthorized" : "refused"))
                    dropRelayLink()
                    handle(.relayUnavailable)
                    return
                }
            }
            if frames.contains(where: { SteerEnet.frameType($0) == "activity_reset" }) {
                relayAnswered = true
                relayJoinTimer?.cancel()
            }
        } else if frames.contains(where: { SteerEnet.frameType($0) == "activity_reset" }) {
            relayJoinTimer?.cancel()
        }
        route(frames, reset: .relayReset, rest: .relaySynced)
    }

    private func armRelayJoinTimer(token: Int) {
        relayJoinTimer?.cancel()
        let ms = timings.joinTimeoutMs
        relayJoinTimer = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(ms))
            guard let self, !Task.isCancelled, token == self.relayToken, self.relayLink != nil else { return }
            // Not synced in time: no relay leg for this feed.
            guard self.state.active != .relay else { return }
            logger.info("relay join timed out")
            self.relayKnownUnavailable = true
            self.onSignal(.failure("join_timeout"))
            self.dropRelayLink()
            self.handle(.relayUnavailable)
        }
    }

    /// The tunnel closed under us.
    private func relayEnded(token: Int) {
        guard !finished, token == relayToken, let link = relayLink else { return }
        let reason = link.closeReason ?? "relay_closed"
        let answered = relayAnswered
        dropRelayLink()
        onSignal(.failure(reason))
        if state.active != .relay, !answered || SteerCarrier.tunnelErrorCodes.contains(reason) {
            relayKnownUnavailable = true
            handle(.relayUnavailable)
        } else {
            handle(.relayDropped)
        }
    }

    private func dropRelayLink() {
        relayToken += 1
        relayJoinTimer?.cancel()
        relayTask?.cancel()
        relayDialing = false
        relayAnswered = false
        relayLink?.close()
        relayLink = nil
    }

    private func closeRelay() {
        let link = relayLink
        relayLink = nil
        dropRelayLink()
        link?.close(sendLeave: true)
    }
}
