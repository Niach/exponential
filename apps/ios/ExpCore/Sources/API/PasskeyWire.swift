import Foundation

// The pure wire pieces of passkey sign-in (EXP-857), kept out of AuthApi so
// they can be unit-tested without a server: base64url, the Set-Cookie replay
// the WebAuthn challenge cookie needs, and the AuthenticationResponseJSON the
// server verifies. The ceremony itself (ASAuthorization) lives in the app.

/// base64url without padding — the encoding every WebAuthn field travels in.
public enum Base64URL {
    public static func encode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    public static func decode(_ string: String) -> Data? {
        var base64 = string
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        // Re-pad to a multiple of 4; anything else is not base64 at all.
        let remainder = base64.count % 4
        if remainder == 1 { return nil }
        if remainder > 0 {
            base64 += String(repeating: "=", count: 4 - remainder)
        }
        return Data(base64Encoded: base64)
    }
}

/// `GET /api/auth/passkey/generate-authenticate-options`, parsed.
/// `cookieHeader` is the response's Set-Cookie pairs pre-joined for replay:
/// the signed challenge cookie rides there and the verify call is rejected
/// without it (HTTPClient deliberately runs without a cookie jar).
public struct PasskeyAuthenticationOptions: Sendable, Equatable {
    public let challenge: Data
    public let rpId: String
    public let allowedCredentialIds: [Data]
    public let cookieHeader: String

    public init(
        challenge: Data,
        rpId: String,
        allowedCredentialIds: [Data] = [],
        cookieHeader: String = ""
    ) {
        self.challenge = challenge
        self.rpId = rpId
        self.allowedCredentialIds = allowedCredentialIds
        self.cookieHeader = cookieHeader
    }
}

/// What the platform authenticator handed back, in raw bytes.
public struct PasskeyAssertion: Sendable, Equatable {
    public let credentialId: Data
    public let clientDataJSON: Data
    public let authenticatorData: Data
    public let signature: Data
    public let userHandle: Data?
    public let attachment: String?

    public init(
        credentialId: Data,
        clientDataJSON: Data,
        authenticatorData: Data,
        signature: Data,
        userHandle: Data? = nil,
        attachment: String? = "platform"
    ) {
        self.credentialId = credentialId
        self.clientDataJSON = clientDataJSON
        self.authenticatorData = authenticatorData
        self.signature = signature
        self.userHandle = userHandle
        self.attachment = attachment
    }
}

/// The WebAuthn `AuthenticationResponseJSON` the server verifies, built from
/// the raw assertion bytes. Optional members are omitted when nil, exactly as
/// the browser form does.
public struct AuthenticationResponseJSON: Encodable, Sendable {
    public struct AssertionResponse: Encodable, Sendable {
        public let clientDataJSON: String
        public let authenticatorData: String
        public let signature: String
        public let userHandle: String?
    }

    public let id: String
    public let rawId: String
    public let type: String
    public let authenticatorAttachment: String?
    public let response: AssertionResponse
    public let clientExtensionResults: [String: String]

    public init(assertion: PasskeyAssertion) {
        let credentialId = Base64URL.encode(assertion.credentialId)
        self.id = credentialId
        self.rawId = credentialId
        self.type = "public-key"
        self.authenticatorAttachment = assertion.attachment
        self.response = AssertionResponse(
            clientDataJSON: Base64URL.encode(assertion.clientDataJSON),
            authenticatorData: Base64URL.encode(assertion.authenticatorData),
            signature: Base64URL.encode(assertion.signature),
            // An empty userHandle is no userHandle — a zero-length string
            // fails verification where an absent key passes.
            userHandle: (assertion.userHandle?.isEmpty ?? true)
                ? nil
                : Base64URL.encode(assertion.userHandle!)
        )
        self.clientExtensionResults = [:]
    }
}

public enum PasskeyWire {
    /// `{"response": <AuthenticationResponseJSON>}` — the verify-authentication body.
    public static func verifyRequestBody(_ assertion: PasskeyAssertion) throws -> Data {
        try JSONEncoder().encode(VerifyBody(response: AuthenticationResponseJSON(assertion: assertion)))
    }

    private struct VerifyBody: Encodable {
        let response: AuthenticationResponseJSON
    }

    /// Parse the served `PublicKeyCredentialRequestOptionsJSON`. Nil when the
    /// challenge or rpId is missing/undecodable — there is no ceremony without
    /// both. `allowCredentials` is absent for a signed-out client (discoverable
    /// credentials), which is the normal login case.
    public static func parseAuthenticationOptions(
        _ data: Data, cookieHeader: String = ""
    ) -> PasskeyAuthenticationOptions? {
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let challengeText = root["challenge"] as? String,
              let challenge = Base64URL.decode(challengeText),
              let rpId = root["rpId"] as? String, !rpId.isEmpty else { return nil }
        let allowed = (root["allowCredentials"] as? [[String: Any]] ?? [])
            .compactMap { $0["id"] as? String }
            .compactMap { Base64URL.decode($0) }
        return PasskeyAuthenticationOptions(
            challenge: challenge,
            rpId: rpId,
            allowedCredentialIds: allowed,
            cookieHeader: cookieHeader
        )
    }

    /// Every `name=value` pair of a Set-Cookie response header.
    ///
    /// Foundation merges repeated Set-Cookie headers into ONE comma-joined
    /// string, and an `Expires=Wed, 21 Oct …` attribute carries a comma of its
    /// own — so a segment only starts a new cookie when its first `=` is
    /// preceded by a bare token (no spaces, no `;`).
    public static func cookiePairs(fromSetCookieHeader header: String) -> [String] {
        var cookies: [String] = []
        var current = ""
        for piece in header.split(separator: ",", omittingEmptySubsequences: false) {
            let segment = String(piece)
            if current.isEmpty {
                current = segment
            } else if startsNewCookie(segment) {
                cookies.append(current)
                current = segment
            } else {
                current += "," + segment
            }
        }
        if !current.isEmpty { cookies.append(current) }
        return cookies.compactMap(pair(from:))
    }

    /// The pairs joined as one `Cookie:` request header value.
    public static func cookieHeaderValue(fromSetCookieHeader header: String?) -> String {
        guard let header else { return "" }
        return cookiePairs(fromSetCookieHeader: header).joined(separator: "; ")
    }

    private static func startsNewCookie(_ segment: String) -> Bool {
        let trimmed = segment.trimmingCharacters(in: .whitespaces)
        guard let equals = trimmed.firstIndex(of: "=") else { return false }
        let name = trimmed[trimmed.startIndex..<equals]
        return !name.isEmpty && !name.contains(" ") && !name.contains(";")
    }

    private static func pair(from cookie: String) -> String? {
        let head = cookie.split(separator: ";", maxSplits: 1, omittingEmptySubsequences: false)[0]
            .trimmingCharacters(in: .whitespaces)
        guard let equals = head.firstIndex(of: "=") else { return nil }
        let name = String(head[head.startIndex..<equals]).trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return nil }
        let value = String(head[head.index(after: equals)...])
        return "\(name)=\(value)"
    }
}
