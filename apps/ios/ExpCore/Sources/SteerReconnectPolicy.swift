import Foundation

/// The pure revival rule (EXP-625), shared by the model and its tests. The
/// relay close-code rule (EXP-621: slow consumer, unauthorized) is gone with
/// the relay viewer (VAPP-43): an eNet link carries no close code.
public enum SteerReconnectPolicy {}

/// The steer protocol words that must never be shown to a human.
public enum SteerOutcome {
    /// EXP-773: `bye` outcomes (and error codes) that carry NO caption of
    /// their own. `ended` is the ordinary close; `history` is the device's
    /// stored replay of an ENDED run closing itself out once the transcript
    /// has been delivered (VAPP-43), so the feed simply stays with the plain
    /// ended caption; `history_unavailable` and `device_offline` were the
    /// retired relay's codes, kept in the set for cross-client parity.
    /// Anything else a peer coins is better shown than swallowed. Byte-equal on Android
    /// (`SILENT_END_OUTCOMES`) and the web (`steer-session-store.ts`).
    public static let silentEndOutcomes: Set<String> = [
        "ended", "history", "history_unavailable", "device_offline",
    ]

    /// The end banner's caption for a `bye` outcome or error code — nil means
    /// the plain "Session ended".
    public static func endDetail(_ outcome: String?) -> String? {
        guard let outcome, !outcome.isEmpty, !silentEndOutcomes.contains(outcome) else {
            return nil
        }
        return outcome
    }
}

/// EXP-625: the phase a viewer model is in, flattened to what a revival
/// decision actually cares about. The app target's own `AgentSessionModel.Phase`
/// carries detail strings and a reconnecting flag; this is the shape the pure
/// rule sees.
public enum SteerPhaseKind: Equatable, Sendable {
    case idle
    case connecting
    case starting
    case live
    /// `.closed(reconnecting: true)`: a backoff wait is (or should be) armed.
    case closedReconnecting
    /// Ended, or closed with no retry left (steer disabled, unauthorized).
    case final
}

/// What a wake signal (foreground, screen attach, network return) should do to
/// a viewer model.
public enum SteerRevivalDecision: Equatable, Sendable {
    /// Full connect: reset the dial state and dial from scratch.
    case dial
    /// A retry that should already be running never fired, or its wait is still
    /// parked: cancel the wait and dial NOW without touching the phase, so
    /// `.starting` and the "Reconnecting…" banner hold steady.
    case wakeRetry
    /// Nominally live but the socket has gone quiet: redial silently, keeping
    /// the phase, so no banner flashes for a socket the OS killed while the app
    /// was suspended.
    case redialSilently
    /// Leave it alone.
    case nothing
}

extension SteerReconnectPolicy {
    /// EXP-625: mobile viewers got stuck on "Connecting…" forever after a
    /// background because every revival entry point was PHASE-gated. A model
    /// whose dial wedged mid-`.connecting` (a hung lookup, or a join the device
    /// never answered) matched no branch, so nothing left in the
    /// process could revive it. The gate is now whether a dial is ACTUALLY
    /// alive, not what the phase claims.
    ///
    /// - Parameters:
    ///   - phase: the model's phase, flattened.
    ///   - dialInFlight: a dial is between its entry and its outcome — it owns
    ///     a link (or an endpoint lookup) right now.
    ///   - retryArmed: a retry wait is parked, waiting to start a dial.
    ///     Neither flag means nothing is going to happen on its own, whatever
    ///     the phase.
    ///   - finished: shut down, or the session is over. Nothing left to dial.
    ///   - socketStale: `.live` but no frame has arrived for longer than the
    ///     staleness window, so the socket is probably dead.
    public static func revive(
        phase: SteerPhaseKind, dialInFlight: Bool, retryArmed: Bool, finished: Bool,
        socketStale: Bool
    ) -> SteerRevivalDecision {
        let dialActive = dialInFlight || retryArmed
        if finished { return .nothing }
        // Checked before `dialActive` so a terminal close stays terminal: it
        // has no dial and never wants one.
        if phase == .final { return .nothing }
        // `.live` is decided by the SOCKET, not by the dial: a dial that ends
        // in a live room has done its job, so `dialActive` is false for every
        // healthy live session and reading it as "the dial died" would redial
        // one on every foreground. Silence past the staleness window is the
        // only honest evidence a live-looking socket is a corpse, and the
        // silent redial keeps the phase so no banner flashes.
        if phase == .live { return socketStale ? .redialSilently : .nothing }
        // The stuck case: nothing is in flight and no wait is armed, so only an
        // explicit connect can get this model moving again. This is the wedge
        // EXP-625 is about, and it wears whatever phase it wedged in.
        if !dialActive { return .dial }
        // Only an ARMED wait may be cut short. A dial IN FLIGHT owns a socket
        // whose join is still pending — `.starting` spends most of each retry
        // cycle right here — and starting a second one alongside it leaves the
        // first socket joined and unowned: its close then runs against the new
        // dial, orphaning the live socket and regressing the phase. The
        // eNet join timeout already bounds an in-flight dial, so waiting it out
        // is both safe and correct.
        if dialInFlight { return .nothing }
        switch phase {
        case .closedReconnecting, .starting:
            // A wait is armed but the user is asking now; skip the rest of it.
            return .wakeRetry
        case .idle, .connecting, .live, .final:
            // `.connecting` with a live, join-ack-bounded dial: let it finish.
            return .nothing
        }
    }
}
