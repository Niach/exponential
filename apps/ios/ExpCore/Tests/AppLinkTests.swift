import Foundation
import XCTest
@testable import ExpCore

// EXP-1188: the agent-prose link rule, locked ×4 (web `app-link.test.ts`,
// desktop `domain::app_link`, Android `AppLinkTest`) against the ONE contract
// fixture.
final class AppLinkTests: XCTestCase {
    private var repoRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
    }

    private func fixture() throws -> [String: Any] {
        let url = repoRoot.appendingPathComponent("packages/domain-contract/fixtures/app-link.json")
        let data = try Data(contentsOf: url)
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func expected(_ raw: [String: Any]) throws -> AppLink {
        switch try XCTUnwrap(raw["kind"] as? String) {
        case "issue":
            return .issue(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                boardSlug: try XCTUnwrap(raw["boardSlug"] as? String),
                identifier: try XCTUnwrap(raw["identifier"] as? String)
            )
        case "session":
            return .session(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                sessionId: try XCTUnwrap(raw["sessionId"] as? String)
            )
        case "app":
            return .app(path: try XCTUnwrap(raw["path"] as? String))
        case "external":
            return .external(url: try XCTUnwrap(raw["url"] as? String))
        case "ignore":
            return .ignore
        case let other:
            XCTFail("unknown kind \(other)")
            return .ignore
        }
    }

    func testEveryFixtureCaseClassifiesTheSame() throws {
        let root = try fixture()
        let origin = try XCTUnwrap(root["origin"] as? String)
        let cases = try XCTUnwrap(root["cases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 23)
        for object in cases {
            let name = try XCTUnwrap(object["name"] as? String)
            let href = try XCTUnwrap(object["href"] as? String)
            let link = try expected(try XCTUnwrap(object["link"] as? [String: Any]))
            XCTAssertEqual(AppLink.classify(href, origin: origin), link, name)
        }
    }

    func testOriginTrailingSlashIsTolerated() {
        XCTAssertEqual(
            AppLink.classify("https://app.exponential.at/t/acme/sessions/abc", origin: "https://app.exponential.at/"),
            .session(teamSlug: "acme", sessionId: "abc")
        )
    }
}
