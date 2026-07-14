import XCTest
@testable import ExpCore

// EXP-92: the Universal-Link parser must stay the exact inverse of the URL
// builder — these are the only two path shapes the AASA claims.
final class WebLinksTests: XCTestCase {
    func testParseIssueUrl() {
        let url = URL(string: "https://app.exponential.at/w/acme/projects/web/issues/EXP-42")!
        XCTAssertEqual(
            WebLinks.parse(url),
            .issue(workspaceSlug: "acme", projectSlug: "web", identifier: "EXP-42")
        )
    }

    func testParseInviteUrl() {
        let url = URL(string: "https://app.exponential.at/invite/abc123")!
        XCTAssertEqual(WebLinks.parse(url), .invite(token: "abc123"))
    }

    func testMintParseRoundTrip() {
        let minted = WebLinks.issue(
            instanceUrl: "https://app.exponential.at/",
            workspaceSlug: "acme",
            projectSlug: "web",
            identifier: "EXP-42"
        )!
        XCTAssertEqual(
            WebLinks.parse(minted),
            .issue(workspaceSlug: "acme", projectSlug: "web", identifier: "EXP-42")
        )
    }

    func testTrailingSlashTolerated() {
        let url = URL(string: "https://app.exponential.at/w/acme/projects/web/issues/EXP-42/")!
        XCTAssertEqual(
            WebLinks.parse(url),
            .issue(workspaceSlug: "acme", projectSlug: "web", identifier: "EXP-42")
        )
    }

    func testRejectsUnclaimedPaths() {
        for path in [
            "/", "/w/acme", "/w/acme/projects/web", "/w/acme/inbox",
            "/w/acme/projects/web/issues", "/w/acme/projects/web/issues/EXP-1/changes",
            "/invite", "/auth/login",
        ] {
            let url = URL(string: "https://app.exponential.at\(path)")!
            XCTAssertNil(WebLinks.parse(url), "expected nil for \(path)")
        }
    }
}
