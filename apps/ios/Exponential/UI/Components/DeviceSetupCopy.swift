import ExpCore
import Foundation

/// The device-setup server card's words (EXP-1169), byte-identical across all
/// four clients: web's `lib/device-setup-copy.ts` is the reference, and its
/// drift test (`device-setup-copy.test.ts`) reads this file and matches each
/// value verbatim, so every literal below is a PLAIN double-quoted single-line
/// string: no interpolation, no concatenation, no escapes.
enum DeviceSetupCopy {
    static let copyCommand = "Copy install command"
    static let codeLabel = "If the CLI shows a code, enter it here"
    static let codePlaceholder = "XXXX-XXXX"
    static let approve = "Approve"
    static let approved = "Code approved. The CLI signs in within a few seconds."
    static let codeUsed = "That code has already been used. Run the login command again."
    static let codeExpired = "That code has expired. Run the login command again to get a new one."
    static let codeInvalid = "That code isn't valid. Check for typos, or run the login command again."
    static let codeOtherAccount = "This code was requested from a different account."
    static let failed = "Something went wrong. Try again."

    static func message(_ error: DeviceCodeError) -> String {
        switch error {
        case .used: return codeUsed
        case .expired: return codeExpired
        case .invalid: return codeInvalid
        case .otherAccount: return codeOtherAccount
        case .failed: return failed
        }
    }
}
