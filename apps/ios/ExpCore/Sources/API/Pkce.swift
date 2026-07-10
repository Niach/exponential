import CryptoKit
import Foundation

/// PKCE for the mobile OAuth handoff (REV-13). The app sends
/// `code_challenge = S256(verifier)` to /api/mobile-oauth-start and keeps the
/// verifier in memory; the `exponential://oauth-return` deep link then carries
/// only a single-use code the view model redeems via
/// POST /api/mobile-oauth-exchange with the verifier — the raw session token
/// never rides a deep link (ASWebAuthenticationSession delivery is app-bound
/// on iOS, but the code flow keeps the wire contract identical across all
/// three native clients).
public struct Pkce: Sendable {
    public let verifier: String
    public let challenge: String

    /// Derive the S256 challenge for a known verifier (unit-testable form —
    /// RFC 7636 §4.2: base64url_no_pad(SHA-256(ASCII(verifier)))).
    public init(verifier: String) {
        self.verifier = verifier
        let digest = SHA256.hash(data: Data(verifier.utf8))
        self.challenge = Data(digest).base64URLNoPad()
    }

    /// Fresh attempt: 32 random bytes → 43-char base64url verifier
    /// (RFC 7636 §4.1 minimum entropy, charset ⊂ unreserved).
    public static func generate() -> Pkce {
        var bytes = [UInt8](repeating: 0, count: 32)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        if status != errSecSuccess {
            // SecRandomCopyBytes practically never fails; fall back to the
            // system RNG rather than crash the login flow.
            bytes = (0..<32).map { _ in UInt8.random(in: UInt8.min...UInt8.max) }
        }
        return Pkce(verifier: Data(bytes).base64URLNoPad())
    }
}

private extension Data {
    func base64URLNoPad() -> String {
        base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}
