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

    // EXP-1126: the email change's bad-address refusal.
    func testInvalidEmailGetsOurOwnCopy() {
        XCTAssertEqual(
            EmailCodeCopy.message(code: "INVALID_EMAIL", serverMessage: "Invalid email"),
            "Enter a valid email address."
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

// EXP-1596: the pure halves of Add passkey — the creation options the server
// serves and the RegistrationResponseJSON it verifies.
final class PasskeyRegistrationWireTests: XCTestCase {
    // MARK: - Creation options

    func testParsesServedCreationOptions() {
        let challenge = Data([1, 2, 3, 4])
        let userId = Data("user-1".utf8)
        let existing = Data([9, 9, 9])
        let json = """
        {"challenge":"\(Base64URL.encode(challenge))",
         "rp":{"name":"Exponential","id":"app.exponential.at"},
         "user":{"id":"\(Base64URL.encode(userId))","name":"dev@example.com","displayName":"Dev"},
         "pubKeyCredParams":[{"alg":-8,"type":"public-key"},{"alg":-7,"type":"public-key"}],
         "timeout":60000,"attestation":"none",
         "excludeCredentials":[{"id":"\(Base64URL.encode(existing))","type":"public-key","transports":["internal"]}],
         "authenticatorSelection":{"residentKey":"preferred","userVerification":"preferred"}}
        """
        let options = PasskeyWire.parseRegistrationOptions(Data(json.utf8), cookieHeader: "a=b")
        XCTAssertEqual(options?.challenge, challenge)
        XCTAssertEqual(options?.rpId, "app.exponential.at")
        XCTAssertEqual(options?.userId, userId)
        XCTAssertEqual(options?.userName, "dev@example.com")
        XCTAssertEqual(options?.excludedCredentialIds, [existing])
        XCTAssertEqual(options?.cookieHeader, "a=b")
    }

    // The account's first passkey: no excludeCredentials is not a failure.
    func testParsesCreationOptionsWithoutExcludeCredentials() {
        let json = #"{"challenge":"AQID","rp":{"id":"localhost"},"user":{"id":"dXNlcg","name":"u@x.at"}}"#
        let options = PasskeyWire.parseRegistrationOptions(Data(json.utf8))
        XCTAssertEqual(options?.excludedCredentialIds, [])
        XCTAssertEqual(options?.rpId, "localhost")
        XCTAssertEqual(options?.userId, Data("user".utf8))
    }

    func testCreationOptionsMissingAPieceAreUnusable() {
        let cases = [
            #"{"rp":{"id":"x"},"user":{"id":"dXNlcg","name":"u"}}"#,
            #"{"challenge":"AQID","user":{"id":"dXNlcg","name":"u"}}"#,
            #"{"challenge":"AQID","rp":{"name":"no id"},"user":{"id":"dXNlcg","name":"u"}}"#,
            #"{"challenge":"AQID","rp":{"id":"x"}}"#,
            #"{"challenge":"AQID","rp":{"id":"x"},"user":{"name":"u"}}"#,
            #"{"challenge":"AQID","rp":{"id":"x"},"user":{"id":"dXNlcg"}}"#,
            #"{"challenge":"a","rp":{"id":"x"},"user":{"id":"dXNlcg","name":"u"}}"#,
            "not json",
        ]
        for json in cases {
            XCTAssertNil(PasskeyWire.parseRegistrationOptions(Data(json.utf8)), "parsed \(json)")
        }
    }

    // MARK: - RegistrationResponseJSON

    func testAttestationEncodesTheWebAuthnRegistrationShape() throws {
        let attestation = PasskeyAttestation(
            credentialId: Data([1, 2]),
            clientDataJSON: Data("{}".utf8),
            attestationObject: Data([0xa3, 0x63])
        )
        let body = try PasskeyWire.registerRequestBody(attestation, name: "iPhone")
        let root = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(root["name"] as? String, "iPhone")
        let response = try XCTUnwrap(root["response"] as? [String: Any])

        XCTAssertEqual(response["id"] as? String, Base64URL.encode(Data([1, 2])))
        XCTAssertEqual(response["rawId"] as? String, Base64URL.encode(Data([1, 2])))
        XCTAssertEqual(response["type"] as? String, "public-key")
        XCTAssertEqual(response["authenticatorAttachment"] as? String, "platform")
        XCTAssertEqual(response["clientExtensionResults"] as? [String: String], [:])

        let inner = try XCTUnwrap(response["response"] as? [String: Any])
        XCTAssertEqual(inner["clientDataJSON"] as? String, Base64URL.encode(Data("{}".utf8)))
        XCTAssertEqual(inner["attestationObject"] as? String, Base64URL.encode(Data([0xa3, 0x63])))
        XCTAssertEqual(inner.count, 2, "only the two members the attestation carries")
    }

    func testNilAttachmentIsOmitted() throws {
        let attestation = PasskeyAttestation(
            credentialId: Data([1]), clientDataJSON: Data([2]), attestationObject: Data([3]), attachment: nil
        )
        let body = try PasskeyWire.registerRequestBody(attestation, name: "iPhone")
        let root = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let response = try XCTUnwrap(root["response"] as? [String: Any])
        XCTAssertNil(response["authenticatorAttachment"])
    }

    // MARK: - Error copy

    // #EXP-2143 follow-up: a session older than a day is no sentence but the
    // step-up sheet's cue; anything else is the server's own message.
    func testStaleSessionIsTheStepUpCue() {
        let stale = Data(#"{"code":"SESSION_NOT_FRESH","message":"Session is not fresh"}"#.utf8)
        XCTAssertEqual(AuthApi.passkeyRegistrationFailure(from: stale), .notFresh)
        let other = Data(#"{"code":"FAILED_TO_VERIFY_REGISTRATION","message":"Failed to verify registration"}"#.utf8)
        XCTAssertEqual(AuthApi.passkeyRegistrationFailure(from: other), .message("Failed to verify registration"))
        XCTAssertNil(AuthApi.passkeyRegistrationFailure(from: Data("oops".utf8)))
    }
}

// EXP-857: the code + passkey flags gate their two buttons.
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

    func testFlagsCanBeIndividuallyOff() throws {
        let config = try decode("""
        {"passwordEnabled":false,"signupEnabled":false,"passwordResetEnabled":false,"oidcProviders":[],
         "googleLoginEnabled":false,"appleLoginEnabled":false,"emailOtpEnabled":true,"passkeyEnabled":false}
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

// #EXP-2143 follow-up: "Confirm it's you" on the wire — the step-up endpoints
// (bearer + Origin), the passkey proof's challenge-cookie replay, and the
// server `code` every refusal carries (`StepUp.errorCopy` reads it).
final class StepUpWireTests: XCTestCase {
    private struct Seen {
        let method: String
        let path: String
        let headers: [String: String]
        let body: [String: Any]?
    }

    /// Answers by path from `Self.answers`, recording every request.
    private final class Server: URLProtocol {
        static let seen = Locked<[Seen]>([])
        static let answers = Locked<[String: (Int, String, [String: String])]>([:])

        override class func canInit(with request: URLRequest) -> Bool { true }
        override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

        override func startLoading() {
            let path = request.url?.path ?? ""
            let data = request.httpBody ?? request.httpBodyStream.map(Self.read) ?? Data()
            Self.seen.mutate {
                $0.append(Seen(
                    method: request.httpMethod ?? "GET",
                    path: path,
                    headers: request.allHTTPHeaderFields ?? [:],
                    body: (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
                ))
            }
            let (status, body, headers) = Self.answers.value[path] ?? (404, "{}", [:])
            let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: headers)!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: Data(body.utf8))
            client?.urlProtocolDidFinishLoading(self)
        }

        override func stopLoading() {}

        private static func read(_ stream: InputStream) -> Data {
            stream.open()
            defer { stream.close() }
            var data = Data()
            var buffer = [UInt8](repeating: 0, count: 4096)
            while stream.hasBytesAvailable {
                let n = stream.read(&buffer, maxLength: buffer.count)
                if n <= 0 { break }
                data.append(buffer, count: n)
            }
            return data
        }
    }

    private final class MemoryKeychain: KeychainStoring, @unchecked Sendable {
        private let lock = NSLock()
        private var storage: [String: String] = [:]
        func get(_ key: String) throws -> String? { lock.withLock { storage[key] } }
        @discardableResult
        func set(_ key: String, value: String?) -> Bool {
            lock.withLock { storage[key] = value }
            return true
        }
        func delete(_ key: String) { lock.withLock { storage[key] = nil } }
    }

    private let instance = "https://exp.example.com"

    private func api() -> AuthApi {
        let auth = AuthRepository(accountStore: AccountStore(keychain: MemoryKeychain()))
        return AuthApi(httpClient: HTTPClient(auth: auth, protocolClasses: [Server.self]), auth: auth)
    }

    override func setUp() {
        super.setUp()
        Server.seen.mutate { $0 = [] }
        Server.answers.mutate { $0 = [:] }
    }

    private func answer(_ path: String, _ status: Int, _ body: String, headers: [String: String] = [:]) {
        Server.answers.mutate { $0[path] = (status, body, headers) }
    }

    func testRegistrationOptionsSurfaceTheStaleSessionDistinctly() async {
        answer("/api/auth/passkey/generate-register-options", 403,
               #"{"code":"SESSION_NOT_FRESH","message":"Session is not fresh"}"#)
        guard case .notFresh = await api().passkeyRegistrationOptions(
            instanceUrl: instance, token: "tok", name: "iPhone"
        ) else { return XCTFail("SESSION_NOT_FRESH must not become a sentence") }
    }

    func testRegistrationVerifySurfacesTheStaleSessionDistinctly() async {
        answer("/api/auth/passkey/verify-registration", 403, #"{"code":"SESSION_NOT_FRESH","message":"x"}"#)
        let attestation = PasskeyAttestation(credentialId: Data([1]), clientDataJSON: Data([2]), attestationObject: Data([3]))
        let failure = await api().verifyPasskeyRegistration(
            instanceUrl: instance, token: "tok", cookieHeader: "", attestation: attestation, name: "iPhone"
        )
        XCTAssertEqual(failure, .notFresh)
    }

    func testMethodsReadTheAccountsWaysInWithTheBearer() async throws {
        answer("/api/auth/step-up/methods", 200, #"{"methods":["passkey","email","fax"],"email":"dev@example.com"}"#)
        let result = try await api().stepUpMethods(instanceUrl: instance, token: "tok").get()
        XCTAssertEqual(result, StepUpMethods(methods: [.passkey, .email], email: "dev@example.com"))
        let request = try XCTUnwrap(Server.seen.value.first)
        XCTAssertEqual(request.method, "GET")
        XCTAssertEqual(request.headers["Authorization"], "Bearer tok")
    }

    func testCodeAndPasswordPostTheirBodies() async throws {
        for path in ["email/send", "email/verify", "password"] {
            answer("/api/auth/step-up/\(path)", 200, #"{"fresh":true}"#)
        }
        let api = api()
        _ = try await api.stepUpSendCode(instanceUrl: instance, token: "tok").get()
        _ = try await api.stepUpVerifyCode(instanceUrl: instance, token: "tok", code: "123456").get()
        _ = try await api.stepUpPassword(instanceUrl: instance, token: "tok", password: "hunter2").get()
        let seen = Server.seen.value
        XCTAssertEqual(seen.map(\.path), [
            "/api/auth/step-up/email/send", "/api/auth/step-up/email/verify", "/api/auth/step-up/password",
        ])
        XCTAssertEqual(seen.map(\.method), ["POST", "POST", "POST"])
        for request in seen {
            XCTAssertEqual(request.headers["Authorization"], "Bearer tok")
            // Better Auth's CSRF check 403s an Origin-less POST.
            XCTAssertEqual(request.headers["Origin"], instance)
        }
        XCTAssertEqual(seen[1].body?["code"] as? String, "123456")
        XCTAssertEqual(seen[2].body?["password"] as? String, "hunter2")
    }

    func testARefusalCarriesTheServerCode() async {
        answer("/api/auth/step-up/password", 401, #"{"code":"INVALID_PASSWORD","message":"Invalid password"}"#)
        answer("/api/auth/step-up/email/verify", 500, "oops")
        let api = api()
        let password = await api.stepUpPassword(instanceUrl: instance, token: "tok", password: "no")
        XCTAssertEqual(password.failureCode, "INVALID_PASSWORD")
        XCTAssertEqual(StepUp.errorCopy(password.failureCode ?? ""), "That password is not right.")
        let code = await api.stepUpVerifyCode(instanceUrl: instance, token: "tok", code: "1")
        XCTAssertEqual(code.failureCode, "HTTP_500")
    }

    /// The options come WITH the bearer (this account's passkeys); their
    /// challenge cookie rides back on the proof, whose body is the login's.
    func testPasskeyProofReplaysTheChallengeCookie() async throws {
        let credential = Data([9, 9])
        answer(
            "/api/auth/passkey/generate-authenticate-options", 200,
            #"{"challenge":"AQID","rpId":"exp.example.com","allowCredentials":[{"id":"\#(Base64URL.encode(credential))","type":"public-key"}]}"#,
            headers: ["Set-Cookie": "better-auth-passkey=chal.sig; Path=/; HttpOnly"]
        )
        answer("/api/auth/step-up/passkey", 200, #"{"fresh":true}"#)
        let api = api()
        let options = try await api.stepUpPasskeyOptions(instanceUrl: instance, token: "tok").get()
        XCTAssertEqual(options.allowedCredentialIds, [credential])
        XCTAssertEqual(options.cookieHeader, "better-auth-passkey=chal.sig")
        let assertion = PasskeyAssertion(
            credentialId: credential, clientDataJSON: Data([1]), authenticatorData: Data([2]),
            signature: Data([3]), userHandle: nil
        )
        _ = try await api.stepUpPasskey(
            instanceUrl: instance, token: "tok", cookieHeader: options.cookieHeader, assertion: assertion
        ).get()
        let seen = Server.seen.value
        XCTAssertEqual(seen[0].headers["Authorization"], "Bearer tok")
        XCTAssertEqual(seen[1].path, "/api/auth/step-up/passkey")
        XCTAssertEqual(seen[1].headers["Cookie"], "better-auth-passkey=chal.sig")
        let response = try XCTUnwrap(seen[1].body?["response"] as? [String: Any])
        XCTAssertEqual(response["id"] as? String, Base64URL.encode(credential))
    }

    func testAForeignPasskeyIsRefused() async {
        answer("/api/auth/step-up/passkey", 401, #"{"code":"PASSKEY_FAILED","message":"x"}"#)
        let assertion = PasskeyAssertion(
            credentialId: Data([1]), clientDataJSON: Data([1]), authenticatorData: Data([2]), signature: Data([3]), userHandle: nil
        )
        let result = await api().stepUpPasskey(instanceUrl: instance, token: "tok", cookieHeader: "", assertion: assertion)
        XCTAssertEqual(result.failureCode, "PASSKEY_FAILED")
    }
}

private extension Result where Failure == StepUpRefused {
    var failureCode: String? {
        if case let .failure(refused) = self { return refused.code }
        return nil
    }
}
