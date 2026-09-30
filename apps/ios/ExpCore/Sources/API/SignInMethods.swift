import Foundation

// EXP-1126: the account's sign-in methods as ONE payload (tRPC
// `users.signInMethods`, apps/web/src/lib/auth/sign-in-methods.ts). Decoded
// permissively: the settings screen is the only reader, and a newer server
// adding a field (or a null where a string was) must never blank the list.

public struct SignInProvider: Decodable, Sendable, Identifiable, Equatable {
    /// Better Auth provider id: `google`, `apple`, an OIDC id, or `credential`
    /// (the password row).
    public let id: String
    public let name: String
    /// `apple` | `google` | `oidc` | `password` — a String so an unknown kind
    /// renders as a plain row instead of failing the decode.
    public let kind: String
    /// The instance still offers this login. A linked-but-unconfigured provider
    /// stays listed so it can be unlinked, never used or re-linked.
    public let available: Bool
    public let linked: Bool
    public let linkedAt: String?

    public init(id: String, name: String, kind: String, available: Bool, linked: Bool, linkedAt: String?) {
        self.id = id
        self.name = name
        self.kind = kind
        self.available = available
        self.linked = linked
        self.linkedAt = linkedAt
    }

    enum CodingKeys: String, CodingKey { case id, name, kind, available, linked, linkedAt }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? id
        kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? "oidc"
        available = try c.decodeIfPresent(Bool.self, forKey: .available) ?? true
        linked = try c.decodeIfPresent(Bool.self, forKey: .linked) ?? false
        linkedAt = try c.decodeIfPresent(String.self, forKey: .linkedAt)
    }

    public var isPassword: Bool { kind == "password" }
}

public struct SignInPasskey: Decodable, Sendable, Identifiable, Equatable {
    public let id: String
    public let name: String?
    public let createdAt: String?
    public let backedUp: Bool

    public init(id: String, name: String?, createdAt: String?, backedUp: Bool) {
        self.id = id
        self.name = name
        self.createdAt = createdAt
        self.backedUp = backedUp
    }

    enum CodingKeys: String, CodingKey { case id, name, createdAt, backedUp }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decodeIfPresent(String.self, forKey: .name)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
        backedUp = try c.decodeIfPresent(Bool.self, forKey: .backedUp) ?? false
    }

    /// The row title: the stored name, else the web's "Passkey".
    public var displayName: String {
        if let name, !name.trimmingCharacters(in: .whitespaces).isEmpty { return name }
        return "Passkey"
    }
}

public struct SignInMethods: Decodable, Sendable, Equatable {
    public let email: String
    public let emailVerified: Bool
    public let emailOtpEnabled: Bool
    public let passwordEnabled: Bool
    public let passkeyEnabled: Bool
    /// Render order: Apple, Google, the OIDC providers, then `credential` while
    /// a password is set, plus linked-but-unconfigured rows.
    public let providers: [SignInProvider]
    public let passkeys: [SignInPasskey]
    /// How many ways in the account has right now (server-counted).
    public let waysIn: Int

    public init(
        email: String, emailVerified: Bool, emailOtpEnabled: Bool, passwordEnabled: Bool,
        passkeyEnabled: Bool, providers: [SignInProvider], passkeys: [SignInPasskey], waysIn: Int
    ) {
        self.email = email
        self.emailVerified = emailVerified
        self.emailOtpEnabled = emailOtpEnabled
        self.passwordEnabled = passwordEnabled
        self.passkeyEnabled = passkeyEnabled
        self.providers = providers
        self.passkeys = passkeys
        self.waysIn = waysIn
    }

    enum CodingKeys: String, CodingKey {
        case email, emailVerified, emailOtpEnabled, passwordEnabled, passkeyEnabled
        case providers, passkeys, waysIn
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        email = try c.decodeIfPresent(String.self, forKey: .email) ?? ""
        emailVerified = try c.decodeIfPresent(Bool.self, forKey: .emailVerified) ?? false
        emailOtpEnabled = try c.decodeIfPresent(Bool.self, forKey: .emailOtpEnabled) ?? false
        passwordEnabled = try c.decodeIfPresent(Bool.self, forKey: .passwordEnabled) ?? false
        passkeyEnabled = try c.decodeIfPresent(Bool.self, forKey: .passkeyEnabled) ?? false
        providers = try c.decodeIfPresent([SignInProvider].self, forKey: .providers) ?? []
        passkeys = try c.decodeIfPresent([SignInPasskey].self, forKey: .passkeys) ?? []
        waysIn = try c.decodeIfPresent(Int.self, forKey: .waysIn) ?? 0
    }

    /// Nothing may remove the last way in: a linked row can go only while
    /// another way remains (the server enforces the same rule and answers
    /// `PRECONDITION_FAILED` otherwise).
    public static func canUnlink(_ provider: SignInProvider, in methods: SignInMethods) -> Bool {
        provider.linked && methods.waysIn > 1
    }

    /// The same rule for a passkey row.
    public var canRemovePasskey: Bool { waysIn > 1 }

    /// The ONE refusal copy the server sends for the last way in.
    public static let lastWayInMessage =
        "This is your only way to sign in. Add another method before removing it."
}

/// `users.mintSignInLinkTicket`: the single-use ticket the link-mode browser
/// handoff carries (`/api/mobile-oauth-start?link=`).
public struct SignInLinkTicket: Decodable, Sendable {
    public let ticket: String
    public let expiresInSeconds: Int

    public init(ticket: String, expiresInSeconds: Int) {
        self.ticket = ticket
        self.expiresInSeconds = expiresInSeconds
    }

    enum CodingKeys: String, CodingKey { case ticket, expiresInSeconds }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        ticket = try c.decode(String.self, forKey: .ticket)
        expiresInSeconds = try c.decodeIfPresent(Int.self, forKey: .expiresInSeconds) ?? 120
    }
}

/// What an `exponential://oauth-return` callback says. One parser for the
/// login hop (a PKCE `code`), the link hop (`linked=<providerId>`) and the
/// failure twin of both (`error=<reason>`). The server doubles the payload
/// into the query AND the fragment (ASWebAuthenticationSession keeps the whole
/// URL; the query is the EXP-21 fallback form); the fragment wins on a key
/// collision. Precedence: error > linked > code.
public enum OAuthReturn: Equatable, Sendable {
    case error(String)
    case linked(String)
    case code(String)
    case none

    public static func parse(_ url: URL) -> OAuthReturn {
        let params = params(url)
        if let reason = params["error"] { return .error(reason) }
        if let provider = params["linked"] { return .linked(provider) }
        if let code = params["code"] { return .code(code) }
        return .none
    }

    /// Callback params from the fragment merged over the query.
    public static func params(_ url: URL) -> [String: String] {
        var params = [String: String]()
        if let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems {
            for item in items where item.value?.isEmpty == false {
                params[item.name] = item.value
            }
        }
        if let fragment = url.fragment {
            for pair in fragment.split(separator: "&") {
                let parts = pair.split(separator: "=", maxSplits: 1)
                if parts.count == 2, !parts[1].isEmpty {
                    let value = String(parts[1])
                    params[String(parts[0])] = value.removingPercentEncoding ?? value
                }
            }
        }
        return params
    }

    /// The LINK-mode failure copy — byte-identical to the web's
    /// `oauthLinkErrorMessage` (apps/web/src/lib/deep-link.ts).
    public static func linkErrorMessage(_ reason: String) -> String {
        switch reason {
        case "access_denied":
            return "Linking was cancelled."
        case "link_ticket_invalid", "state_missing", "state_invalid", "state_mismatch",
             "state_not_found", "please_restart_the_process":
            return "That link request expired. Please try again."
        case "account_already_linked_to_different_user":
            return "That account is already linked to a different user."
        default:
            return "Couldn't link that account. Please try again."
        }
    }
}
