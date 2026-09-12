import Foundation
import XCTest
@testable import ExpCore

// The pure halves of passkey + one-time-code sign-in (EXP-857): everything the
// login screen can get wrong without a server. The WebAuthn ceremony itself is
// system UI and stays untested here.
final class PasskeyWireTests: XCTestCase {
    // MARK: - base64url

    func testBase64UrlRoundTripsEveryByte() {
        let data = Data((0...255).map { UInt8($0) })
        let encoded = Base64URL.encode(data)
        XCTAssertFalse(encoded.contains("+"), "base64url never uses +")
        XCTAssertFalse(encoded.contains("/"), "base64url never uses /")
        XCTAssertFalse(encoded.contains("="), "the WebAuthn form is unpadded")
        XCTAssertEqual(Base64URL.decode(encoded), data)
    }

    func testBase64UrlDecodesUnpaddedInput() {
        // "hi" → "aGk" (one padding char short of base64).
        XCTAssertEqual(Base64URL.decode("aGk"), Data("hi".utf8))
        XCTAssertEqual(Base64URL.decode(""), Data())
    }

    func testBase64UrlRejectsGarbage() {
        XCTAssertNil(Base64URL.decode("a"), "a lone char can never be base64")
        XCTAssertNil(Base64URL.decode("!!!!"))
    }

    // MARK: - Set-Cookie replay

    func testCookiePairsKeepsTheSecurePrefixedName() {
        let header = "__Secure-better-auth.better-auth-passkey=chal.sig; Path=/; HttpOnly; Secure; SameSite=Lax"
        XCTAssertEqual(
            PasskeyWire.cookiePairs(fromSetCookieHeader: header),
            ["__Secure-better-auth.better-auth-passkey=chal.sig"]
        )
    }

    // Foundation merges repeated Set-Cookie headers into one comma-joined
    // string — and an Expires attribute contains a comma of its own.
    func testCookiePairsSplitsMergedHeadersAroundExpiresCommas() {
        let header = [
            "__Secure-better-auth.better-auth-passkey=abc; Path=/; Expires=Wed, 21 Oct 2026 07:28:00 GMT; HttpOnly",
            "better-auth.session_token=tok123; Path=/; HttpOnly",
        ].joined(separator: ", ")
        XCTAssertEqual(
            PasskeyWire.cookiePairs(fromSetCookieHeader: header),
            [
                "__Secure-better-auth.better-auth-passkey=abc",
                "better-auth.session_token=tok123",
            ]
        )
        XCTAssertEqual(
            PasskeyWire.cookieHeaderValue(fromSetCookieHeader: header),
            "__Secure-better-auth.better-auth-passkey=abc; better-auth.session_token=tok123"
        )
    }

    func testCookieHeaderValueIsEmptyWithoutASetCookie() {
        XCTAssertEqual(PasskeyWire.cookieHeaderValue(fromSetCookieHeader: nil), "")
        XCTAssertEqual(PasskeyWire.cookiePairs(fromSetCookieHeader: ""), [])
    }

    // MARK: - Options

    func testParsesServedAuthenticationOptions() {
        let challenge = Data([1, 2, 3, 4])
        let credential = Data([9, 9, 9])
        let json = """
        {"challenge":"\(Base64URL.encode(challenge))","rpId":"app.exponential.at","timeout":60000,
         "userVerification":"preferred",
         "allowCredentials":[{"id":"\(Base64URL.encode(credential))","type":"public-key"}]}
        """
        let options = PasskeyWire.parseAuthenticationOptions(Data(json.utf8), cookieHeader: "a=b")
        XCTAssertEqual(options?.challenge, challenge)
        XCTAssertEqual(options?.rpId, "app.exponential.at")
        XCTAssertEqual(options?.allowedCredentialIds, [credential])
        XCTAssertEqual(options?.cookieHeader, "a=b")
    }

    // A signed-out client gets no allowCredentials — discoverable credentials
    // are the normal login case, not a parse failure.
    func testParsesOptionsWithoutAllowCredentials() {
        let json = #"{"challenge":"AQID","rpId":"app.exponential.at"}"#
        let options = PasskeyWire.parseAuthenticationOptions(Data(json.utf8))
        XCTAssertEqual(options?.allowedCredentialIds, [])
        XCTAssertEqual(options?.rpId, "app.exponential.at")
    }

    func testOptionsWithoutAChallengeAreUnusable() {
        XCTAssertNil(PasskeyWire.parseAuthenticationOptions(Data(#"{"rpId":"x"}"#.utf8)))
        XCTAssertNil(PasskeyWire.parseAuthenticationOptions(Data(#"{"challenge":"AQID"}"#.utf8)))
        XCTAssertNil(PasskeyWire.parseAuthenticationOptions(Data("not json".utf8)))
    }

    // MARK: - AuthenticationResponseJSON

    func testAssertionEncodesTheWebAuthnResponseShape() throws {
        let assertion = PasskeyAssertion(
            credentialId: Data([1, 2]),
            clientDataJSON: Data("{}".utf8),
            authenticatorData: Data([3, 4]),
            signature: Data([5, 6]),
            userHandle: Data([7])
        )
        let body = try PasskeyWire.verifyRequestBody(assertion)
        let root = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let response = try XCTUnwrap(root["response"] as? [String: Any])

        XCTAssertEqual(response["id"] as? String, Base64URL.encode(Data([1, 2])))
        XCTAssertEqual(response["rawId"] as? String, Base64URL.encode(Data([1, 2])))
        XCTAssertEqual(response["type"] as? String, "public-key")
        XCTAssertEqual(response["authenticatorAttachment"] as? String, "platform")
        XCTAssertEqual(response["clientExtensionResults"] as? [String: String], [:])

        let inner = try XCTUnwrap(response["response"] as? [String: Any])
        XCTAssertEqual(inner["clientDataJSON"] as? String, Base64URL.encode(Data("{}".utf8)))
        XCTAssertEqual(inner["authenticatorData"] as? String, Base64URL.encode(Data([3, 4])))
        XCTAssertEqual(inner["signature"] as? String, Base64URL.encode(Data([5, 6])))
        XCTAssertEqual(inner["userHandle"] as? String, Base64URL.encode(Data([7])))
    }

    // An empty userHandle is no userHandle: a zero-length string fails
    // verification where an absent key passes.
    func testEmptyUserHandleIsOmitted() throws {
        let assertion = PasskeyAssertion(
            credentialId: Data([1]),
            clientDataJSON: Data([2]),
            authenticatorData: Data([3]),
            signature: Data([4]),
            userHandle: Data(),
            attachment: nil
        )
        let body = try PasskeyWire.verifyRequestBody(assertion)
        let root = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let response = try XCTUnwrap(root["response"] as? [String: Any])
        let inner = try XCTUnwrap(response["response"] as? [String: Any])
        XCTAssertNil(inner["userHandle"])
        XCTAssertNil(response["authenticatorAttachment"])
    }

    // MARK: - One-time code copy

    func testKnownCodesGetOurOwnCopy() {
        XCTAssertEqual(
            EmailCodeCopy.message(code: "INVALID_OTP", serverMessage: "Invalid OTP"),
            "That code is not right. Check the email and try again."
        )
        XCTAssertEqual(
            EmailCodeCopy.message(code: "OTP_EXPIRED", serverMessage: "expired"),
            "That code expired. Request a new one."
        )
        XCTAssertEqual(
            EmailCodeCopy.message(code: "TOO_MANY_ATTEMPTS", serverMessage: nil),
            "Too many attempts. Request a new code."
        )
    }

    func testUnknownCodesFallBackToTheServerMessage() {
        XCTAssertEqual(
            EmailCodeCopy.message(code: "SOMETHING_ELSE", serverMessage: "Nope."),
            "Nope."
        )
        XCTAssertEqual(
            EmailCodeCopy.message(code: nil, serverMessage: ""),
            "Couldn't check that code. Please try again."
        )
    }
}

// The login screen is the first thing to touch an arbitrary instance URL, so a
// server that predates EXP-857 must decode — with both new affordances off.
final class AuthConfigDecodeTests: XCTestCase {
    private func decode(_ json: String) throws -> AuthConfig {
        try JSONDecoder().decode(AuthConfig.self, from: Data(json.utf8))
    }

    func testDecodesTheNewLoginFlags() throws {
        let config = try decode("""
        {"passwordEnabled":true,"signupEnabled":true,"passwordResetEnabled":true,"oidcProviders":[],
         "googleLoginEnabled":true,"appleLoginEnabled":true,"emailOtpEnabled":true,"passkeyEnabled":true}
        """)
        XCTAssertTrue(config.emailOtpEnabled)
        XCTAssertTrue(config.passkeyEnabled)
    }

    func testOlderServersDecodeWithBothAffordancesOff() throws {
        let config = try decode("""
        {"passwordEnabled":true,"oidcProviders":[],"googleLoginEnabled":false}
        """)
        XCTAssertFalse(config.emailOtpEnabled)
        XCTAssertFalse(config.passkeyEnabled)
        XCTAssertTrue(config.passwordEnabled)
        XCTAssertFalse(config.appleLoginEnabled)
    }

    func testFlagsCanBeIndividuallyOff() throws {
        let config = try decode("""
        {"passwordEnabled":false,"oidcProviders":[],"googleLoginEnabled":false,"emailOtpEnabled":true}
        """)
        XCTAssertTrue(config.emailOtpEnabled)
        XCTAssertFalse(config.passkeyEnabled)
        XCTAssertFalse(config.passwordEnabled)
    }
}

// EXP-857 (security): the assertion's relying party comes from whatever server
// the instance field points at, and iOS scopes a passkey by rpId alone. A
// hostile instance must not be able to name someone else's host and relay the
// signed challenge.
final class PasskeyRelyingPartyGuardTests: XCTestCase {
    func testAcceptsTheInstancesOwnHost() {
        XCTAssertTrue(PasskeyWire.relyingPartyMatchesInstance(
            rpId: "app.exponential.at", instanceUrl: "https://app.exponential.at"
        ))
        // Case and the root dot are noise; a port on the instance URL is not
        // part of the rpId at all (a dev instance on :3000 still matches).
        XCTAssertTrue(PasskeyWire.relyingPartyMatchesInstance(
            rpId: "APP.Exponential.AT.", instanceUrl: "https://app.exponential.at"
        ))
        XCTAssertTrue(PasskeyWire.relyingPartyMatchesInstance(
            rpId: "localhost", instanceUrl: "https://localhost:3000"
        ))
    }

    func testRejectsAnotherHostsRelyingParty() {
        // The whole attack: a self-hosted instance claiming the cloud's rpId.
        XCTAssertFalse(PasskeyWire.relyingPartyMatchesInstance(
            rpId: "app.exponential.at", instanceUrl: "https://evil.example.com"
        ))
        // A registrable parent is still someone else's namespace.
        XCTAssertFalse(PasskeyWire.relyingPartyMatchesInstance(
            rpId: "exponential.at", instanceUrl: "https://app.exponential.at"
        ))
        XCTAssertFalse(PasskeyWire.relyingPartyMatchesInstance(
            rpId: "evil.app.exponential.at", instanceUrl: "https://app.exponential.at"
        ))
    }

    func testRejectsMalformedInput() {
        for rpId in ["", " ", "app.exponential.at:443", "app.exponential.at/x", "https://app.exponential.at"] {
            XCTAssertFalse(
                PasskeyWire.relyingPartyMatchesInstance(
                    rpId: rpId, instanceUrl: "https://app.exponential.at"
                ),
                "accepted \(rpId)"
            )
        }
        XCTAssertFalse(PasskeyWire.relyingPartyMatchesInstance(
            rpId: "app.exponential.at", instanceUrl: "not a url"
        ))
    }
}
