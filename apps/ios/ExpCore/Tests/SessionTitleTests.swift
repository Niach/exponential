import Foundation
import XCTest
@testable import ExpCore

// EXP-1341: THE session display title, locked ×4 against
// `packages/domain-contract/fixtures/session-title.json`.
final class SessionTitleTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/session-title.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testCopyMatchesTheFixture() throws {
        let fixture = try fixture()
        let titles = try XCTUnwrap(fixture["titles"] as? [String: String])
        XCTAssertEqual(titles["issue"], SessionTitle.Copy.issue)
        XCTAssertEqual(titles["issues"], SessionTitle.Copy.issues)
        XCTAssertNil(titles["action"])
        XCTAssertEqual(fixture["titleMax"] as? Int, SessionTitle.Copy.titleMax)
        XCTAssertEqual(fixture["untitled"] as? String, SessionTitle.Copy.untitled)
    }

    func testReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let row = try XCTUnwrap(input["session"] as? [String: Any])
            let session = SessionTitle.Session(
                id: try XCTUnwrap(row["id"] as? String),
                rootId: try XCTUnwrap(row["rootId"] as? String),
                title: row["title"] as? String
            )
            let links = try XCTUnwrap(input["links"] as? [[String: Any]]).map {
                TreeSessionIssue(
                    sessionId: try XCTUnwrap($0["sessionId"] as? String),
                    issueId: try XCTUnwrap($0["issueId"] as? String),
                    sortOrder: try XCTUnwrap($0["sortOrder"] as? Double)
                )
            }
            let issues = try XCTUnwrap(input["issues"] as? [[String: Any]]).map {
                TreeIssue(
                    id: try XCTUnwrap($0["id"] as? String),
                    identifier: try XCTUnwrap($0["identifier"] as? String),
                    title: try XCTUnwrap($0["title"] as? String)
                )
            }
            let messages = try XCTUnwrap(input["messages"] as? [[String: Any]]).map {
                SessionTitle.Message(
                    id: try XCTUnwrap($0["id"] as? String),
                    sessionId: try XCTUnwrap($0["sessionId"] as? String),
                    authorKind: try XCTUnwrap($0["authorKind"] as? String),
                    authorSessionId: $0["authorSessionId"] as? String,
                    body: try XCTUnwrap($0["body"] as? String),
                    hasCard: $0["card"] is [String: Any],
                    createdAt: try XCTUnwrap($0["createdAt"] as? String)
                )
            }
            XCTAssertEqual(
                SessionTitle.title(session, links: links, issues: issues, messages: messages),
                testCase["expected"] as? String,
                name
            )
        }
    }
}
