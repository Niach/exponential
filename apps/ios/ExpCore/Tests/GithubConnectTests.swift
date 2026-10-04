import XCTest
@testable import ExpCore

// SLOP-26: the prerequisite ladder every GitHub surface names (not configured
// → not linked → expired → not installed → the live list), the
// github-connected return's slug, and the mint refusal that falls back to the
// guided page. Android's `GithubConnectTest` locks the same rules.
final class GithubConnectTests: XCTestCase {
    func testPrerequisiteLadder() {
        XCTAssertEqual(
            GithubConnect.prerequisite(configured: false, linked: true, needsReconnect: false, installed: true),
            .notConfigured
        )
        XCTAssertEqual(
            GithubConnect.prerequisite(configured: true, linked: false, needsReconnect: false, installed: false),
            .notLinked
        )
        XCTAssertEqual(
            GithubConnect.prerequisite(configured: true, linked: true, needsReconnect: true, installed: false),
            .expired
        )
        XCTAssertEqual(
            GithubConnect.prerequisite(configured: true, linked: true, needsReconnect: false, installed: false),
            .notInstalled
        )
        XCTAssertNil(
            GithubConnect.prerequisite(configured: true, linked: true, needsReconnect: false, installed: true)
        )
    }

    func testStatusDecodesTheConnectionFields() throws {
        let json = """
        {"configured":true,"connectConfigured":true,"linked":true,"needsReconnect":false,"login":"octocat",
         "installed":true,"installUrl":"https://github.com/apps/exp/installations/new",
         "connectUrl":"https://app.test/integrations/github?team=t&return=app",
         "installations":[{"installationId":7,"accountLogin":"acme","accountType":"Organization",
           "manageUrl":"https://github.com/organizations/acme/settings/installations/7",
           "suspended":false,"needsReauth":false,"stale":false}]}
        """
        let status = try JSONDecoder().decode(GithubStatusResult.self, from: Data(json.utf8))
        XCTAssertTrue(status.linked)
        XCTAssertFalse(status.needsReconnect)
        XCTAssertEqual(status.login, "octocat")
        XCTAssertNil(status.prerequisite)
        XCTAssertEqual(status.installations.first?.accountLogin, "acme")
        XCTAssertFalse(status.installations.first?.isSuspended ?? true)
    }

    func testReposWithoutTheNewFieldsFallsBackToInstalled() throws {
        let json = """
        {"configured":true,"installed":false,"installUrl":null,"repos":[],"hasMore":false}
        """
        let repos = try JSONDecoder().decode(GithubReposResult.self, from: Data(json.utf8))
        XCTAssertFalse(repos.linked)
        XCTAssertFalse(repos.needsReconnect)
        XCTAssertNil(repos.login)
        XCTAssertEqual(repos.prerequisite, .notLinked)
    }

    func testExpiredOutranksNotInstalled() throws {
        let json = """
        {"configured":true,"linked":true,"needsReconnect":true,"login":null,"installed":false,
         "installUrl":"https://github.com/apps/exp/installations/new","repos":[],"hasMore":false,"installations":[]}
        """
        let repos = try JSONDecoder().decode(GithubReposResult.self, from: Data(json.utf8))
        XCTAssertEqual(repos.prerequisite, .expired)
    }

    func testErrorSlugParsesTheErrorQuery() {
        let url = URL(string: "exponential://github-connected?error=session")!
        XCTAssertEqual(GithubConnect.errorSlug(from: url), "session")
    }

    func testSuccessFormsHaveNoSlug() {
        for raw in [
            "exponential://github-connected",
            "exponential://github-connected/",
            "exponential://github-connected?",
            "exponential://github-connected?error=",
            "exponential://github-connected#frag",
        ] {
            XCTAssertNil(GithubConnect.errorSlug(from: URL(string: raw)!), raw)
        }
    }

    func testOnlyBadRequestFallsBackToTheGuidedPage() {
        XCTAssertTrue(GithubConnect.linkNotOffered(code: "BAD_REQUEST"))
        XCTAssertFalse(GithubConnect.linkNotOffered(code: "UNAUTHORIZED"))
        XCTAssertFalse(GithubConnect.linkNotOffered(code: nil))
    }
}
