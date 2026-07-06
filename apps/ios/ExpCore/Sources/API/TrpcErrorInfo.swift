import Foundation

// Digs a clean, user-facing message (and the tRPC error code) out of a failed
// tRPC call so surfaces can show the server's message instead of the raw
// "tRPC HTTP 412: {json}" and can distinguish plan-cap failures — the native
// analogue of web's isPlanLimitError / err.message.

/// Prefix every plan-limit throw in the server's lib/billing.ts uses, alongside
/// the `PRECONDITION_FAILED` tRPC code. Kept in sync with the web
/// `PLAN_LIMIT_MESSAGE_PREFIX`.
public let planLimitMessagePrefix = "Your plan allows"

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
}

public extension Error {
    /// A clean, user-facing message. For `TrpcError.httpError` it extracts the
    /// tRPC error `message` from the JSON body; otherwise the localized
    /// description.
    var trpcUserMessage: String {
        guard case let TrpcError.httpError(_, body) = self,
              let parsed = TrpcErrorBody.parse(body),
              !parsed.message.isEmpty
        else { return localizedDescription }
        return parsed.message
    }

    /// True when a tRPC failure is a plan-cap (`PRECONDITION_FAILED` + the
    /// "Your plan allows" message) — mirrors web `isPlanLimitError`.
    var isPlanLimitError: Bool {
        guard case let TrpcError.httpError(_, body) = self,
              let parsed = TrpcErrorBody.parse(body) else { return false }
        return parsed.code == "PRECONDITION_FAILED"
            && parsed.message.hasPrefix(planLimitMessagePrefix)
    }
}
