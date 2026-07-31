import Foundation
import XCTest
@testable import ExpCore

// The dead-session classification (AuthApi.classifySessionRead). Better Auth
// answers a DEAD bearer with 200 and no session rather than a 401, so
// "2xx + no user + a token was presented" is the only session-read shape that
// proves the credential is gone. Everything else stays indeterminate on purpose:
// collapsing offline into "signed out" is exactly the conservatism this keeps.
final class AuthSessionReadTests: XCTestCase {
    private func classify(
        status: Int, body: String, tokenPresented: Bool
    ) -> SessionReadOutcome {
        AuthApi.classifySessionRead(
            statusCode: status, body: Data(body.utf8), tokenPresented: tokenPresented
        )
    }

    private func isInvalidated(_ outcome: SessionReadOutcome) -> Bool {
        if case .invalidated = outcome { return true }
        return false
    }

    private func isIndeterminate(_ outcome: SessionReadOutcome) -> Bool {
        if case .indeterminate = outcome { return true }
        return false
    }

    func testLiveSessionResolvesTheUser() {
        let outcome = classify(
            status: 200,
            body: #"{"user":{"id":"u1","email":"jane@example.com","onboardingCompletedAt":null}}"#,
            tokenPresented: true
        )
        guard case let .user(user) = outcome else {
            return XCTFail("expected a resolved user, got \(outcome)")
        }
        XCTAssertEqual(user.id, "u1")
        XCTAssertEqual(user.email, "jane@example.com")
    }

    // The account-deleted case: the bearer is presented, the server answers 200
    // with an explicit null session.
    func testNullSessionWithTokenIsDefinitivelyDead() {
        XCTAssertTrue(isInvalidated(classify(status: 200, body: "null", tokenPresented: true)))
        XCTAssertTrue(isInvalidated(
            classify(status: 200, body: #"{"session":null,"user":null}"#, tokenPresented: true)
        ))
        XCTAssertTrue(isInvalidated(classify(status: 200, body: "{}", tokenPresented: true)))
    }

    // A tokenless read is legitimately userless (the pre-login probe) — there is
    // no credential to have died.
    func testNullSessionWithoutTokenIsIndeterminate() {
        XCTAssertTrue(isIndeterminate(classify(status: 200, body: "null", tokenPresented: false)))
        XCTAssertTrue(isIndeterminate(
            classify(status: 200, body: #"{"user":null}"#, tokenPresented: false)
        ))
    }

    // Non-2xx never signs anyone out: a 500/502/504 from a proxy is a server
    // having a bad day, not a revoked session.
    func testNon2xxIsAlwaysIndeterminate() {
        for status in [401, 403, 500, 502, 503, 504] {
            XCTAssertTrue(
                isIndeterminate(classify(status: status, body: "null", tokenPresented: true)),
                "HTTP \(status) must not count as a dead session"
            )
        }
    }

    // A 2xx body we cannot parse (captive portal HTML, empty response) says
    // nothing about the session.
    func testUnparseableBodyIsIndeterminate() {
        XCTAssertTrue(isIndeterminate(
            classify(status: 200, body: "<html>sign in to the wifi</html>", tokenPresented: true)
        ))
        XCTAssertTrue(isIndeterminate(classify(status: 200, body: "", tokenPresented: true)))
    }
}
