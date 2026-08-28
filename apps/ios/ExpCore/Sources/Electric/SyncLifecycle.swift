import Foundation

/// EXP-656: what a lifecycle edge does to the shape pipelines, as a pure rule
/// so `bun run ios:test` covers it without URLProtocol stubs (SyncManager owns
/// the Tasks and the URLSessions; this owns the decision).
///
/// The behavior it encodes is Electric's own client contract (see
/// `packages/electric-protocol/README.md` §4): background ⇒ cancel the
/// in-flight long-polls immediately, foreground ⇒ resume from the persisted
/// cursor with a non-live catch-up poll. Immediately, because a "park after N
/// seconds" timer runs on a clock that does not advance while the phone
/// sleeps — it simply never fires, and the 19 long-polls are carried into
/// suspension on sockets the OS has already killed.
public enum SyncLifecycleAction: Equatable, Sendable {
    /// Cancel every account's shape tasks and invalidate their URLSessions.
    case park
    /// Relaunch every signed-in account's pipeline now, on a fresh session.
    case relaunchAll
    /// Leave the pipelines alone.
    case ignore
}

/// The lifecycle state SyncManager drives: parked-ness plus the stamp that
/// collapses a foreground and a co-firing network edge into ONE restart.
public struct SyncLifecycleState: Equatable, Sendable {
    /// The scene is in the background and the pipelines are cancelled.
    public private(set) var parked: Bool
    /// When the last all-account relaunch ran — the floor a network edge
    /// respects (a foreground never does; see `onForeground`).
    public private(set) var lastRestartAllAt: Date?

    public init(parked: Bool = false, lastRestartAllAt: Date? = nil) {
        self.parked = parked
        self.lastRestartAllAt = lastRestartAllAt
    }

    /// A decision plus the state it leaves behind.
    public struct Outcome: Equatable, Sendable {
        public let state: SyncLifecycleState
        public let action: SyncLifecycleAction

        public init(state: SyncLifecycleState, action: SyncLifecycleAction) {
            self.state = state
            self.action = action
        }
    }

    /// The scene left the foreground. Always parks — there is no grace window
    /// to wait out, and re-parking an already parked manager is a no-op on
    /// tasks that are already cancelled.
    public func onBackground() -> Outcome {
        Outcome(
            state: SyncLifecycleState(parked: true, lastRestartAllAt: lastRestartAllAt),
            action: .park
        )
    }

    /// The scene became active. A parked manager ALWAYS relaunches, however
    /// short the trip was: the pipelines were cancelled on the way out, so
    /// there is nothing left to ride out, and the floor is deliberately
    /// bypassed (it exists to collapse duplicate restarts, not to delay the
    /// one the user is waiting for). The stamp still moves so a network edge
    /// firing alongside the resume coalesces into this restart.
    ///
    /// Not parked = a cold launch, or an `.inactive`-only flip that never
    /// reached `.background`: the pipelines are already running and a restart
    /// would only throw away the snapshot that is landing.
    public func onForeground(now: Date = Date()) -> Outcome {
        guard parked else { return Outcome(state: self, action: .ignore) }
        return Outcome(
            state: SyncLifecycleState(parked: false, lastRestartAllAt: now),
            action: .relaunchAll
        )
    }

    /// Connectivity returned or the path changed (NetworkPathWatcher). Ignored
    /// while parked — the app is in the background and the resume will do the
    /// relaunch — and rate-limited by `floor` otherwise, so a burst of path
    /// updates during a VPN handshake costs one restart.
    public func onNetworkEdge(now: Date = Date(), floor: TimeInterval) -> Outcome {
        guard !parked else { return Outcome(state: self, action: .ignore) }
        if let last = lastRestartAllAt, now.timeIntervalSince(last) < floor {
            return Outcome(state: self, action: .ignore)
        }
        return Outcome(
            state: SyncLifecycleState(parked: false, lastRestartAllAt: now),
            action: .relaunchAll
        )
    }
}
