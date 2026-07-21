import Foundation

// Digs a clean, user-facing message (and the tRPC error code) out of a failed
// tRPC call so surfaces can show the server's message instead of the raw
// "tRPC HTTP 412: {json}" and can distinguish plan-cap failures — the native
// analogue of web's isPlanLimitError / err.message.

/// Prefix every plan-limit throw in the server's lib/billing.ts uses, alongside
/// the `PRECONDITION_FAILED` tRPC code. Kept in sync with the web
/// `PLAN_LIMIT_MESSAGE_PREFIX`.
public let planLimitMessagePrefix = "Your plan allows"

/// Neutral plan-cap copy shown instead of the server's message, which carries
/// purchase language ("Add seats or upgrade…") that must never render in the
/// iOS app (App Store 3.1.1 — EXP-216).
public let planLimitNeutralMessage = "This team has reached its plan limit."

private struct TrpcErrorBody {
    let message: String
    let code: String?

    /// Parse the tRPC error envelope, tolerating both the non-batched
    /// `{ "error": {...} }` form and the batched `[ { "error": {...} } ]` form.
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
        let message = (error["message"] as? String) ?? ""
        let code = (error["data"] as? [String: Any])?["code"] as? String
        return TrpcErrorBody(message: message, code: code)
    }

    /// Plan-cap detection (`PRECONDITION_FAILED` + the "Your plan allows"
    /// prefix — the code alone is shared with non-billing preconditions).
    var isPlanLimit: Bool {
        code == "PRECONDITION_FAILED" && message.hasPrefix(planLimitMessagePrefix)
    }
}

public extension Error {
    /// A clean, user-facing message. For `TrpcError.httpError` it extracts the
    /// tRPC error `message` from the JSON body; otherwise the localized
    /// description. Plan-cap messages are replaced with neutral copy — the
    /// server's wording is written for the web, where billing lives.
    var trpcUserMessage: String {
        guard let trpcError = self as? TrpcError,
              case let .httpError(_, body) = trpcError,
              let parsed = TrpcErrorBody.parse(body),
              !parsed.message.isEmpty
        else { return localizedDescription }
        return parsed.isPlanLimit ? planLimitNeutralMessage : parsed.message
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
