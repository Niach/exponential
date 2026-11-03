import Foundation

// P2P stabilisation (#EXP-1528): eNet stays the PRIMARY steer carrier; the
// steer relay comes back as a FALLBACK carrying ciphertext only (the sealed
// tunnel, `SteerTunnel.swift`). The rule every viewer replays is
// fixtures/steer-enet.json `fallback` + `carrierCases`; this is the Swift
// twin of `steerCarrierStep` in packages/domain-contract/src/steer-enet.ts.

public enum SteerCarrier {
    /// `fallback.stallMs`: no eNet frame by then opens the relay.
    public static let stallMs = 2_500
    /// `fallback.tunnelMaxDataBytes`: a longer tunnel `data` is dropped.
    public static let tunnelMaxDataBytes = 1_572_864
    /// `fallback.relayRetryMs`: the background redial curve (eNet under the
    /// relay, the relay after a drop).
    public static let relayRetryMs = [1_000, 3_000, 10_000, 30_000]
    /// `fallback.tunnelErrorCodes`.
    public static let tunnelErrorCodes = ["device_offline", "device_unsupported"]
    /// `fallback.startingRetry` (#EXP-1678): the NO FEED redial backoff.
    public static let startingRetryBaseMs = 3_000
    public static let startingRetryMaxMs = 30_000

    /// #EXP-1678 (`startingRetryCases`): the cap of the n-th consecutive NO
    /// FEED redial, min(base * 2^n, max).
    public static func startingRetryCapMs(retries: Int) -> Int {
        // Past 2^4 the cap is max already; clamping the shift keeps it from overflowing.
        let shift = min(max(retries, 0), 20)
        return min(startingRetryBaseMs << shift, startingRetryMaxMs)
    }

    /// #EXP-1678: the wait itself, equal jitter in [cap/2, cap); `random` in [0, 1).
    public static func startingRetryDelayMs(retries: Int, random: Double = Double.random(in: 0 ..< 1)) -> Double {
        let cap = Double(startingRetryCapMs(retries: retries))
        return cap / 2 + random * (cap / 2)
    }

    public enum Active: String, Equatable, Sendable { case none, enet, relay }
    public enum Relay: String, Equatable, Sendable { case closed, open }

    public struct State: Equatable, Sendable {
        public var active: Active
        public var relay: Relay

        public init(active: Active = .none, relay: Relay = .closed) {
            self.active = active
            self.relay = relay
        }
    }

    public enum Event: String, Equatable, Sendable {
        case start, stall, enetNoFeed, enetRefused, enetReset, enetSynced, enetDropped
        case relayReset, relaySynced, relayDropped, relayUnavailable
    }

    public enum Action: String, Equatable, Sendable {
        case dialEnet, armStall, openRelay, rejoinRelay, closeRelay
        case redialEnetLater, reopenRelayLater, apply, drop
    }

    /// The pure carrier rule (`carrierCases`). `enetReset`/`relayReset` = that
    /// carrier's `activity_reset`; the `*Synced` events stand for every later
    /// frame of that carrier (synced, live, bye).
    public static func step(_ state: State, _ event: Event) -> (state: State, actions: [Action]) {
        let active = state.active, relay = state.relay
        func at(_ a: Active, _ r: Relay, _ actions: [Action]) -> (State, [Action]) {
            (State(active: a, relay: r), actions)
        }
        switch event {
        case .start:
            return at(.none, .closed, [.dialEnet, .armStall])
        case .stall:
            return active == .none && relay == .closed ? at(.none, .open, [.openRelay]) : at(active, relay, [])
        case .enetNoFeed:
            if active == .none, relay == .closed { return at(.none, .open, [.openRelay, .redialEnetLater]) }
            return at(active, relay, [.redialEnetLater])
        case .enetRefused:
            // #EXP-1678: the device itself said no; the relay reaches the same device.
            return at(active, relay, active == .relay ? [.redialEnetLater] : [])
        case .enetReset:
            return at(.enet, relay, [.apply])
        case .enetSynced:
            if active != .enet { return at(active, relay, [.drop]) }
            return relay == .open ? at(.enet, .closed, [.apply, .closeRelay]) : at(.enet, relay, [.apply])
        case .enetDropped:
            if active != .enet { return at(active, relay, [.redialEnetLater]) }
            return relay == .open
                ? at(.none, .open, [.rejoinRelay, .redialEnetLater])
                : at(.none, .open, [.openRelay, .redialEnetLater])
        case .relayReset:
            return active == .enet ? at(active, relay, [.drop]) : at(.relay, relay, [.apply])
        case .relaySynced:
            return active == .relay ? at(active, relay, [.apply]) : at(active, relay, [.drop])
        case .relayDropped:
            return active == .enet ? at(.enet, .closed, []) : at(.none, .closed, [.reopenRelayLater])
        case .relayUnavailable:
            return at(active, .closed, [])
        }
    }
}
