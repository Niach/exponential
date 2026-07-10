import XCTest
@testable import ExpCore

// PKCE derivation for the mobile OAuth handoff (REV-13). The S256 vector is
// RFC 7636 Appendix B — the same pair is asserted by the web
// (mobile-oauth-code.test.ts), Android (OauthPkceTest), and desktop
// (login.rs) tests so all four implementations provably agree.
final class PkceTests: XCTestCase {
    func testChallengeS256MatchesRfc7636Vector() {
        let pkce = Pkce(verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
        XCTAssertEqual(pkce.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
    }

    func testGenerateProducesBase64UrlVerifierOfExpectedLength() {
        let pkce = Pkce.generate()
        // 32 random bytes → 43 base64url chars, no padding (RFC 7636 §4.1 valid).
        XCTAssertEqual(pkce.verifier.count, 43)
        XCTAssertNotNil(pkce.verifier.range(of: "^[A-Za-z0-9_-]+$", options: .regularExpression))
        // Challenge is a SHA-256 digest → also 43 base64url chars.
        XCTAssertEqual(pkce.challenge.count, 43)
        XCTAssertNotNil(pkce.challenge.range(of: "^[A-Za-z0-9_-]+$", options: .regularExpression))
    }

    func testGenerateDoesNotRepeat() {
        XCTAssertNotEqual(Pkce.generate().verifier, Pkce.generate().verifier)
    }
}
