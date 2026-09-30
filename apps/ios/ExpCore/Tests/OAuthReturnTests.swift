import Foundation
import XCTest
@testable import ExpCore

// EXP-1126: the ONE oauth-return parser (login code, link-mode `linked`, and
// the failure twin) + the link-mode error copy, byte-locked to the web's
// `oauthLinkErrorMessage` (apps/web/src/lib/deep-link.ts).
final class OAuthReturnTests: XCTestCase {
    private func url(_ string: String) -> URL { URL(string: string)! }

    func testLinkedParses() {
        XCTAssertEqual(
            OAuthReturn.parse(url("exponential://oauth-return?linked=google#linked=google")),
            .linked("google")
        )
        XCTAssertEqual(OAuthReturn.parse(url("exponential://oauth-return?linked=my-oidc")), .linked("my-oidc"))
    }

    func testCodeParses() {
        XCTAssertEqual(OAuthReturn.parse(url("exponential://oauth-return?code=abc#code=abc")), .code("abc"))
    }

    func testErrorWinsOverLinkedAndCode() {
        XCTAssertEqual(
            OAuthReturn.parse(url("exponential://oauth-return?linked=google&code=c#error=access_denied")),
            .error("access_denied")
        )
        XCTAssertEqual(
            OAuthReturn.parse(url("exponential://oauth-return?code=c&linked=apple")),
            .linked("apple"),
            "linked beats code"
        )
    }

    func testFragmentBeatsQuery() {
        XCTAssertEqual(
            OAuthReturn.parse(url("exponential://oauth-return?code=fromQuery#code=fromFragment")),
            .code("fromFragment")
        )
    }

    func testEmptyCallbackIsNone() {
        XCTAssertEqual(OAuthReturn.parse(url("exponential://oauth-return")), OAuthReturn.none)
        XCTAssertEqual(OAuthReturn.parse(url("exponential://oauth-return?code=")), OAuthReturn.none)
    }

    func testLinkErrorCopy() {
        XCTAssertEqual(OAuthReturn.linkErrorMessage("access_denied"), "Linking was cancelled.")
        for reason in [
            "link_ticket_invalid", "state_missing", "state_invalid", "state_mismatch",
            "state_not_found", "please_restart_the_process",
        ] {
            XCTAssertEqual(
                OAuthReturn.linkErrorMessage(reason),
                "That link request expired. Please try again.",
                reason
            )
        }
        XCTAssertEqual(
            OAuthReturn.linkErrorMessage("account_already_linked_to_different_user"),
            "That account is already linked to a different user."
        )
        XCTAssertEqual(
            OAuthReturn.linkErrorMessage("unable_to_link_account"),
            "Couldn't link that account. Please try again."
        )
        XCTAssertEqual(
            OAuthReturn.linkErrorMessage("anything_else"),
            "Couldn't link that account. Please try again."
        )
    }

    func testLinkStartUrl() throws {
        let google = try XCTUnwrap(AuthApi.linkStartUrl(
            instanceUrl: "https://exp.example.com", ticket: "T_k-1", provider: "google", codeChallenge: "ch"
        ))
        XCTAssertEqual(
            google.absoluteString,
            "https://exp.example.com/api/mobile-oauth-start?link=T_k-1&provider=google&code_challenge=ch"
        )
        let oidc = try XCTUnwrap(AuthApi.linkStartUrl(
            instanceUrl: "https://exp.example.com", ticket: "T", provider: "corp-sso", codeChallenge: "ch"
        ))
        XCTAssertEqual(
            oidc.absoluteString,
            "https://exp.example.com/api/mobile-oauth-start?link=T&providerId=corp-sso&code_challenge=ch"
        )
    }
}
