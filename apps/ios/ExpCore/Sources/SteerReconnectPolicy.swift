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
