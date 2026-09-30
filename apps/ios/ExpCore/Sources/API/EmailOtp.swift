import Foundation

/// Did the instance accept a request for a one-time sign-in code? The send call
/// deliberately says nothing about whether the address exists (the server
/// answers 200 either way), so the only failures worth surfacing are transport
/// and rate limits.
public enum SendCodeResult: Sendable, Equatable {
    case success
    case failure(message: String)
}

/// Login copy for the one-time code errors (and the EXP-1126 email change).
/// The codes below get our own wording (byte-identical across the four
/// clients); everything else falls through to the server's own `message`, which is already user-facing.
public enum EmailCodeCopy {
    public static func message(code: String?, serverMessage: String?) -> String {
        switch code?.uppercased() {
        case "INVALID_OTP":
            return "That code is not right. Check the email and try again."
        case "OTP_EXPIRED":
            return "That code expired. Request a new one."
        case "TOO_MANY_ATTEMPTS":
            return "Too many attempts. Request a new code."
        case "INVALID_EMAIL":
            return "Enter a valid email address."
        default:
            if let serverMessage, !serverMessage.isEmpty { return serverMessage }
            return "Couldn't check that code. Please try again."
        }
    }
}

/// EXP-1026: a one-time code for an address with no account creates one, and
/// the account needs a name. The natives ASK for it (`X-Exp-Ask-Name: 1` on
/// every `/sign-in/email-otp`): the server then answers an unknown address
/// sent without a `name` with 400 `code: "NAME_REQUIRED"` WITHOUT consuming the
/// code, the login screen shows the name step, and the SAME code is resubmitted
/// as `{email, otp, name}`. Existing accounts never see the step.
public enum EmailCodeSignUp {
    public static let askNameHeader = "X-Exp-Ask-Name"
    public static let askNameValue = "1"
    public static let nameRequiredCode = "NAME_REQUIRED"

    /// The `/sign-in/email-otp` body; `name` rides only once the user gave one.
    public static func requestBody(email: String, otp: String, name: String?) -> [String: String] {
        var body = ["email": email, "otp": otp]
        if let name = name?.trimmingCharacters(in: .whitespacesAndNewlines), !name.isEmpty {
            body["name"] = name
        }
        return body
    }

    /// Whether a failed verify is the server asking for the name step.
    public static func isNameRequired(status: Int, code: String?) -> Bool {
        status == 400 && code?.uppercased() == nameRequiredCode
    }
}

/// EXP-1026: the name step's copy, byte-identical across the natives (Android
/// `EmailCodeSignUpCopy`); `EmailCodeSignUpTests` locks the literals.
public enum EmailCodeSignUpCopy {
    public static let title = "What should we call you?"
    public static let body = "This is how your teammates see you."
    public static let fieldLabel = "Name"
    public static let placeholder = "Your name"
    public static let button = "Create account"
    public static let emptyNameError = "Enter your name to continue."
}
