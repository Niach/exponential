import Foundation

/// EXP-1169: the device-setup server card's pure rules, the same on all four
/// clients. Web's twins: `buildServerInstallSnippet` / `displayedSnippet`
/// (`components/device-setup.tsx`) and `normalizeUserCode` /
/// `isCompleteUserCode` / `deviceErrorMessage` (`lib/auth/device-code.ts`).
public enum ServerInstallCommand {
    /// What the copy control puts on the pasteboard: always ONE line. The
    /// one-time install token rides along as `EXP_INSTALL_TOKEN` when there is
    /// one, so the new daemon signs itself in.
    public static func copied(origin: String, token: String?) -> String {
        AppConstants.serverInstallSnippet(origin: origin, token: token)
    }

    /// What the command box shows: the same command on fixed lines (three
    /// with a token, two without), so the box reads like a shell snippet.
    public static func displayed(origin: String, token: String?) -> String {
        let tokenPart = token.map { " \\\n  EXP_INSTALL_TOKEN=\($0)" } ?? ""
        return "curl -fsSL https://exponential.at/install.sh |\n  EXP_INSTANCE=\(origin)\(tokenPart) sh"
    }
}

/// RFC 8628 user codes (EXP-403): always 8 characters, printed `XXXX-XXXX`.
public enum DeviceUserCode {
    /// Uppercase, keep only A-Z and 0-9, at most 8 characters, and insert the
    /// dash only once a 5th character exists, so backspacing over it deletes
    /// instead of fighting the formatter and a pasted dashed code stays stable.
    public static func normalize(_ input: String) -> String {
        let bare = String(
            input.uppercased().unicodeScalars
                .filter { ("A"..."Z").contains($0) || ("0"..."9").contains($0) }
                .map(Character.init)
                .prefix(8)
        )
        guard bare.count >= 5 else { return bare }
        return "\(bare.prefix(4))-\(bare.dropFirst(4))"
    }

    /// A complete code: 8 characters once the dash is stripped.
    public static func isComplete(_ code: String) -> Bool {
        code.replacingOccurrences(of: "-", with: "").count == 8
    }
}

/// Why approving a CLI device code in place did not work. The view maps each
/// case to its `DeviceSetupCopy` line.
public enum DeviceCodeError: Equatable, Sendable {
    case used
    case expired
    case invalid
    case otherAccount
    case failed

    /// Better Auth's RFC 8628 `error` field. Anything unknown is `failed`.
    public static func from(errorCode: String?) -> DeviceCodeError {
        switch errorCode {
        case "expired_token": return .expired
        case "invalid_request", "invalid_grant": return .invalid
        case "access_denied": return .otherAccount
        default: return .failed
        }
    }
}

/// `devices.createInstallToken`'s result: a one-time token for the install
/// command and when it lapses (the card remints then).
public struct CliInstallToken: Decodable, Sendable, Equatable {
    public let token: String
    public let expiresAt: String

    public init(token: String, expiresAt: String) {
        self.token = token
        self.expiresAt = expiresAt
    }

    public var expiresAtDate: Date? { WireTimestamps.parse(expiresAt) }
}
