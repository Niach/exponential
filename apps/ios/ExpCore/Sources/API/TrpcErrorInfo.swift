import Foundation

// Digs a clean, user-facing message (and the tRPC error code) out of a failed
// tRPC call so surfaces can show the server's message instead of a raw
// response body and can distinguish plan-cap failures — the native
// analogue of web's isPlanLimitError / err.message. Since EXP-219 the same
// extraction also backs `TrpcError.errorDescription`, so a raw body can never
// reach the screen through plain `localizedDescription` either.

/// Prefix every plan-limit throw in the server's lib/billing.ts uses, alongside
/// the `PRECONDITION_FAILED` tRPC code. Kept in sync with the web
/// `PLAN_LIMIT_MESSAGE_PREFIX`.
public let planLimitMessagePrefix = "Your plan allows"

/// Neutral plan-cap copy shown instead of the server's message, which carries
/// purchase language ("Add seats or upgrade…") that must never render in the
/// iOS app (App Store 3.1.1 — EXP-216).
public let planLimitNeutralMessage = "This team has reached its plan limit."

/// Leading clause of the server's team-delete billing gate (REV2-55):
/// `teams.delete` refuses a team whose subscription is still live, with
/// `PRECONDITION_FAILED`. Kept in sync with the web
/// `TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE`
/// (apps/web/src/lib/billing/billing-handover.ts) — only this stable clause is
/// matched, because the server's trailing pointer names a web-only screen.
public let teamDeleteSubscriptionMessagePrefix = "This team has an active subscription"

/// Native copy for that gate. The server sends the owner to "team settings →
/// Billing", which exists on the web ONLY (the app ships no billing UI —
/// EXP-216 / App Store 3.1.1), so the refusal names the web instead of a
/// screen the user cannot reach here.
public let teamDeleteSubscriptionMessage =
    "This team has an active subscription. Cancel the subscription on the web before deleting the team."

/// The ONE sentence every client shows when a request never reached the
/// server (EXP-533). Byte-identical on web, iOS, Android and desktop: a
/// transport failure must read as "you are offline", never as Apple's
/// `URLError` text ("A server with the specified hostname could not be
/// found."), Chrome's "Failed to fetch" or reqwest's URL dump.
public let offlineErrorMessage = "You're offline. Check your connection and try again."

/// Clause of the server's pre-EXP-533 merge-conflict refusal, which shipped as
/// `PRECONDITION_FAILED` (HTTP 412) instead of a real `CONFLICT`. Only used by
/// the transitional sniff in `isMergeConflict`.
private let legacyMergeConflictClause = "has merge conflicts with"

struct TrpcErrorBody {
    let message: String
    let code: String?

    /// Parse the tRPC error envelope, tolerating the non-batched
    /// `{ "error": {...} }` form, the batched `[ { "error": {...} } ]` form,
    /// and a nested `{ "error": { "json": {...} } }` payload.
    static func parse(_ body: String) -> TrpcErrorBody? {
        guard let data = body.data(using: .utf8),
              let root = try? JSONSerialization.jsonObject(with: data) else { return nil }
        let errorObj: [String: Any]?
        if let dict = root as? [String: Any] {
            errorObj = dict["error"] as? [String: Any]
        } else if let arr = root as? [Any], let first = arr.first as? [String: Any] {
            errorObj = first["error"] as? [String: Any]
        } else {
            errorObj = nil
        }
        guard let error = errorObj else { return nil }
        let payload = (error["json"] as? [String: Any]) ?? error
        let message = (payload["message"] as? String) ?? ""
        let code = (payload["data"] as? [String: Any])?["code"] as? String
        return TrpcErrorBody(message: message, code: code)
    }

    /// The server's user-presentable `message` (billing copy swapped for the
    /// native text), or nil when the body carries no extractable message —
    /// the structural sanitizer behind `TrpcError.errorDescription` (EXP-219).
    static func userMessage(fromBody body: String) -> String? {
        guard let parsed = parse(body), !parsed.message.isEmpty else { return nil }
        return parsed.presentableMessage
    }

    /// Plan-cap detection (`PRECONDITION_FAILED` + the "Your plan allows"
    /// prefix — the code alone is shared with non-billing preconditions).
    var isPlanLimit: Bool {
        code == "PRECONDITION_FAILED" && message.hasPrefix(planLimitMessagePrefix)
    }

    /// Team-delete billing gate detection (`PRECONDITION_FAILED` + the
    /// "This team has an active subscription" clause — REV2-55).
    var isTeamDeleteSubscriptionGate: Bool {
        code == "PRECONDITION_FAILED" && message.hasPrefix(teamDeleteSubscriptionMessagePrefix)
    }

    /// The one place that decides which server messages render verbatim and
    /// which are replaced: everything passes through except the two billing
    /// messages, whose web-only wording must never reach an iOS surface.
    var presentableMessage: String {
        if isPlanLimit { return planLimitNeutralMessage }
        if isTeamDeleteSubscriptionGate { return teamDeleteSubscriptionMessage }
        return message
    }
}

public extension Error {
    /// True when the request never reached the server: DNS, connect, TLS-less
    /// connectivity and roaming/data failures from `URLSession`, which
    /// `HTTPClient.perform` rethrows unwrapped. A `TrpcError` is NEVER offline
    /// — it means the server answered.
    var isOfflineError: Bool {
        guard let urlError = self as? URLError else { return false }
        switch urlError.code {
        case .notConnectedToInternet,
             .dnsLookupFailed,
             .cannotFindHost,
             .cannotConnectToHost,
             .networkConnectionLost,
             .timedOut,
             .internationalRoamingOff,
             .dataNotAllowed:
            return true
        default:
            return false
        }
    }

    /// A clean, user-facing message (EXP-533). A transport failure reads as the
    /// shared offline sentence; for `TrpcError.httpError` the tRPC error
    /// `message` is extracted from the JSON body; otherwise the localized
    /// description. Plan-cap and team-delete billing-gate messages are replaced
    /// with native copy — the server's wording is written for the web, where
    /// billing lives.
    var userFacingMessage: String {
        if isOfflineError { return offlineErrorMessage }
        guard let trpcError = self as? TrpcError,
              case let .httpError(_, body) = trpcError,
              let parsed = TrpcErrorBody.parse(body),
              !parsed.message.isEmpty
        else { return localizedDescription }
        return parsed.presentableMessage
    }

    /// Historical name for `userFacingMessage`, kept so the existing call sites
    /// pick the offline sentence up unchanged.
    var trpcUserMessage: String { userFacingMessage }

    /// True only for a REAL content conflict on a PR merge (EXP-533): the
    /// server answers `CONFLICT` / HTTP 409 for a conflict it diagnosed, and
    /// `PRECONDITION_FAILED` for everything else it refused (stale head,
    /// branch protection, GitHub App misconfiguration) — offering the
    /// "Fix conflicts" recovery run there would only waste an agent run.
    var isMergeConflict: Bool {
        guard let trpcError = self as? TrpcError,
              case let .httpError(status, body) = trpcError else { return false }
        if status == 409 { return true }
        // TRANSITIONAL (EXP-533): remove once every server answers a real conflict with 409
        guard status == 412, let parsed = TrpcErrorBody.parse(body) else { return false }
        return parsed.message.contains(legacyMergeConflictClause)
    }

    /// The tRPC error `code` (`NOT_FOUND`, `FORBIDDEN`, `PRECONDITION_FAILED`,
    /// …) of a failed call, or nil for anything that isn't a tRPC error
    /// envelope (transport failures, non-tRPC errors). Lets a caller tell a
    /// permanent server "no" from a transient network failure without matching
    /// on message text.
    var trpcErrorCode: String? {
        guard let trpcError = self as? TrpcError,
              case let .httpError(_, body) = trpcError else { return nil }
        return TrpcErrorBody.parse(body)?.code
    }

    /// True when a tRPC failure is a plan-cap (`PRECONDITION_FAILED` + the
    /// "Your plan allows" message) — mirrors web `isPlanLimitError`.
    var isPlanLimitError: Bool {
        guard let trpcError = self as? TrpcError,
              case let .httpError(_, body) = trpcError,
              let parsed = TrpcErrorBody.parse(body) else { return false }
        return parsed.isPlanLimit
    }
}

/// A failed PR merge, as every merge surface renders it (EXP-533): the caption
/// text plus whether the refusal was a real content conflict, which is the only
/// case where the "Fix conflicts" recovery run can help. Mirrors web
/// `lib/merge-failure.ts`, Android `domain/MergeFailure.kt` and desktop
/// `ui/src/pr_merge.rs`.
public struct MergeFailure: Sendable, Equatable {
    public let message: String
    public let isConflict: Bool

    public init(message: String, isConflict: Bool) {
        self.message = message
        self.isConflict = isConflict
    }

    public init(error: Error) {
        self.init(message: error.userFacingMessage, isConflict: error.isMergeConflict)
    }
}
