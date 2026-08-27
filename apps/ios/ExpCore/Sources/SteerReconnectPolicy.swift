import Foundation

/// EXP-621: what a viewer socket does after it closes, decided from the relay's
/// close code alone. The relay's codes live in `apps/steer-relay/src/protocol.ts`
/// — a close is NOT automatically a network fault, and treating every one as a
/// dropped connection made the screen flash "Reconnecting…" and walk up the
/// backoff curve for closes the relay expects the client to shrug off.
public enum SteerCloseCode {
    /// CLOSE_UNAUTHORIZED — the ticket was refused. Retrying re-refuses.
    public static let unauthorized = 4003
    /// CLOSE_SLOW_CONSUMER — the relay dropped this viewer because its send
    /// queue backed up (hub.ts). The room is fine and the ticket is fine: the
    /// only correct answer is to dial straight back in and replay.
    public static let slowConsumer = 4008
}

/// The outcome of a closed viewer socket.
public enum SteerCloseDecision: Equatable, Sendable {
    /// The session is over — nothing left to dial.
    case ended
    /// Redial NOW: no backoff escalation, and the caller keeps its current
    /// phase so no "Reconnecting…" banner flashes for a hiccup the viewer
    /// never needed to know about.
    case redialImmediately
    /// The ordinary drop — auto-redial on the jittered backoff curve.
    case reconnectWithBackoff
    /// A permanent no. Stay closed and stop dialing.
    case terminalClosed
}

/// The pure close-code rule, shared by the model and its tests.
public enum SteerReconnectPolicy {
    /// - Parameters:
    ///   - closeCode: the socket's close code, when one was readable. Unknown
    ///     codes (and no code at all — a socket that failed before any close
    ///     frame) take the ordinary backoff path, exactly as before EXP-621.
    ///   - sessionOver: the synced row says the session ended, or it vanished.
    public static func decide(closeCode: Int?, sessionOver: Bool) -> SteerCloseDecision {
        if sessionOver { return .ended }
        switch closeCode {
        case SteerCloseCode.unauthorized: return .terminalClosed
        case SteerCloseCode.slowConsumer: return .redialImmediately
        default: return .reconnectWithBackoff
        }
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
    /// whose dial wedged mid-`.connecting` (a hung mint, or a socket the relay
    /// never answered the join on) matched no branch, so nothing left in the
    /// process could revive it. The gate is now whether a dial is ACTUALLY
    /// alive, not what the phase claims.
    ///
    /// - Parameters:
    ///   - phase: the model's phase, flattened.
    ///   - dialActive: a dial is in flight, or a retry wait is armed. False
    ///     means nothing is going to happen on its own, whatever the phase.
    ///   - finished: shut down, or the session is over. Nothing left to dial.
    ///   - socketStale: `.live` but no frame has arrived for longer than the
    ///     staleness window, so the socket is probably dead.
    public static func revive(
        phase: SteerPhaseKind, dialActive: Bool, finished: Bool, socketStale: Bool
    ) -> SteerRevivalDecision {
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
