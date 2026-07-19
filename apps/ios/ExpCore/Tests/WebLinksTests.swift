import XCTest
@testable import ExpCore

// EXP-92: the Universal-Link parser must stay the exact inverse of the URL
// builder — these are the only two path shapes the AASA claims.
// EXP-180 (the great rename): the canonical web route is
// `/t/{team}/boards/{board}/issues/{identifier}`; the legacy `/w/` and
// `/projects/` forms are dead on the web (no redirects) and must not parse.
final class WebLinksTests: XCTestCase {
    func testParseIssueUrl() {
        let url = URL(string: "https://app.exponential.at/t/acme/boards/web/issues/EXP-42")!
        XCTAssertEqual(
            WebLinks.parse(url),
            .issue(teamSlug: "acme", boardSlug: "web", identifier: "EXP-42")
        )
    }

    func testParseInviteUrl() {
        let url = URL(string: "https://app.exponential.at/invite/abc123")!
        XCTAssertEqual(WebLinks.parse(url), .invite(token: "abc123"))
    }

    func testMintParseRoundTrip() {
        let minted = WebLinks.issue(
            instanceUrl: "https://app.exponential.at/",
            teamSlug: "acme",
            boardSlug: "web",
            identifier: "EXP-42"
        )!
        XCTAssertEqual(minted.absoluteString, "https://app.exponential.at/t/acme/boards/web/issues/EXP-42")
        XCTAssertEqual(
            WebLinks.parse(minted),
            .issue(teamSlug: "acme", boardSlug: "web", identifier: "EXP-42")
        )
    }

    func testTrailingSlashTolerated() {
        let url = URL(string: "https://app.exponential.at/t/acme/boards/web/issues/EXP-42/")!
        XCTAssertEqual(
            WebLinks.parse(url),
            .issue(teamSlug: "acme", boardSlug: "web", identifier: "EXP-42")
        )
    }

    func testRejectsUnclaimedPaths() {
        for path in [
            "/", "/t/acme", "/t/acme/boards/web", "/t/acme/inbox",
            "/t/acme/boards/web/issues", "/t/acme/boards/web/issues/EXP-1/changes",
            // The dead legacy forms (EXP-180): `/w/` and `/projects/` no longer
            // exist on the web, so the app must not claim them either.
            "/w/acme/projects/web/issues/EXP-42", "/w/acme/boards/web/issues/EXP-42",
            "/t/acme/projects/web/issues/EXP-42",
            "/invite", "/auth/login",
        ] {
            let url = URL(string: "https://app.exponential.at\(path)")!
            XCTAssertNil(WebLinks.parse(url), "expected nil for \(path)")
        }
    }
}
