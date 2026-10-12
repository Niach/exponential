import Foundation

/// Recheck R3 (feedback-states.json `starting.maybeDelivered`): what a FAILED
/// `steer.startSession` says about the start. A request whose fate is unknown
/// (a network or transport error, a client-side timeout, the server's
/// `relay_timeout` answer) may have reached the device: the host shows the
/// failure toast with Retry (same `startId`) AND follows the start quietly
/// for `starting.watchMs`, so a late session still lands. A response with a
/// definite error (any 4xx, or a 5xx whose error body is not
/// `relay_timeout`) is a definite failure: toast + Retry, no follower. One
/// rule ×4 (web, desktop `may_have_reached_device`, Android `StartDelivery`).
public enum StartDelivery {

    public enum Fate: Equatable, Sendable {
        /// The device may have the start: toast + Retry + the quiet follow.
        case maybeDelivered
        /// Refused: toast + Retry, nothing to follow.
        case failed
    }

    public static func fate(of error: Error) -> Fate {
        switch error {
        case is URLError, is DecodingError:
            // No answer, or a 2xx we could not read: the frame may be out.
            return .maybeDelivered
        case let error as HTTPError:
            guard case let .httpError(status, body) = error else { return .maybeDelivered }
            return fate(status: status, message: SteerApi.trpcErrorMessage(fromBody: body))
        case let error as TrpcError:
            guard case let .httpError(status, body) = error else { return .failed }
            return fate(status: status, message: SteerApi.trpcErrorMessage(fromBody: body))
        case let error as SteerStartError:
            switch error {
            case .rejected: return .failed
            case let .serverError(status, message): return fate(status: status, message: message)
            }
        default:
            return .failed
        }
    }

    /// An HTTP answer: a 4xx refused it; a 5xx refused it only when its error
    /// body names something other than the relay timing out (a 5xx with no
    /// error body, a proxy's, says nothing about the start).
    public static func fate(status: Int, message: String?) -> Fate {
        guard status >= 500 else { return .failed }
        guard let message, !message.isEmpty else { return .maybeDelivered }
        return isRelayTimeout(message) ? .maybeDelivered : .failed
    }

    /// The server's relay timeout (REV-34): the relay's `relay_timeout`
    /// reason, which `steer.startSession` reports as the relay's 504.
    public static func isRelayTimeout(_ message: String) -> Bool {
        message.contains("relay_timeout") || message.contains("Steer relay error (504)")
    }
}
