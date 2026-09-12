import Foundation

/// Did the instance accept a request for a one-time sign-in code? The send call
/// deliberately says nothing about whether the address exists (the server
/// answers 200 either way), so the only failures worth surfacing are transport
/// and rate limits.
public enum SendCodeResult: Sendable, Equatable {
    case success
    case failure(message: String)
}

/// Login copy for the one-time code errors. The three codes below get our own
/// wording (byte-identical across the four clients); everything else falls
/// through to the server's own `message`, which is already user-facing.
public enum EmailCodeCopy {
    public static func message(code: String?, serverMessage: String?) -> String {
        switch code?.uppercased() {
        case "INVALID_OTP":
            return "That code is not right. Check the email and try again."
        case "OTP_EXPIRED":
            return "That code expired. Request a new one."
        case "TOO_MANY_ATTEMPTS":
            return "Too many attempts. Request a new code."
        default:
            if let serverMessage, !serverMessage.isEmpty { return serverMessage }
            return "Couldn't check that code. Please try again."
        }
    }
}
