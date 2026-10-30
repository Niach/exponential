import Foundation
import XCTest
@testable import ExpCore

// #EXP-1456: where an inbox row opens, locked ×4 against
// `packages/domain-contract/fixtures/inbox-route.json`.
final class InboxRouteTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/inbox-route.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func rowTarget(_ raw: Any?) throws -> InboxRoute.RowTarget {
        let object = try XCTUnwrap(raw as? [String: Any])
        switch try XCTUnwrap(object["kind"] as? String) {
        case "issue":
            return .issue(
                id: try XCTUnwrap(object["issueId"] as? String),
                resultsFace: (object["face"] as? String) == "results"
            )
        case "session": return .session(id: try XCTUnwrap(object["sessionId"] as? String))
        case "none": return .none
        case let other: throw NSError(domain: "unknown kind \(other)", code: 1)
        }
    }

    private func sessionTarget(_ raw: Any?) throws -> InboxRoute.SessionTarget {
        let object = try XCTUnwrap(raw as? [String: Any])
        switch try XCTUnwrap(object["kind"] as? String) {
        case "agent": return .agent(agentId: try XCTUnwrap(object["agentId"] as? String))
        case "thread":
            return .thread(
                threadId: try XCTUnwrap(object["threadId"] as? String),
                sessionId: object["sessionId"] as? String
            )
        case "session": return .session(id: try XCTUnwrap(object["sessionId"] as? String))
        case let other: throw NSError(domain: "unknown kind \(other)", code: 1)
        }
    }

    func testReplaysEveryRowCase() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let row = try XCTUnwrap(testCase["row"] as? [String: Any])
            XCTAssertEqual(
                InboxRoute.inboxRowTarget(
                    type: try XCTUnwrap(row["type"] as? String),
                    issueId: row["issueId"] as? String,
                    sessionId: row["sessionId"] as? String
                ),
                try rowTarget(testCase["expected"]),
                name
            )
        }
    }

    func testReplaysEverySessionCase() throws {
        let cases = try XCTUnwrap(try fixture()["sessionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let session = try XCTUnwrap(testCase["session"] as? [String: Any])
            XCTAssertEqual(
                InboxRoute.sessionTarget(
                    id: try XCTUnwrap(session["id"] as? String),
                    rootId: session["rootId"] as? String,
                    agentId: session["agentId"] as? String,
                    userId: session["userId"] as? String,
                    viewerId: testCase["viewerId"] as? String
                ),
                try sessionTarget(testCase["expected"]),
                name
            )
        }
    }

    /// The push payload reads the same rule: a `sessionId` routes to the
    /// session whatever the type.
    func testPushWithASessionOpensItWhateverItsType() {
        XCTAssertEqual(
            NotificationRouting.pushTarget(["type": "agent_message", "sessionId": "s1"]),
            .session(id: "s1")
        )
        XCTAssertEqual(
            NotificationRouting.pushTarget(["type": "session_blocked", "sessionId": "s1"]),
            .session(id: "s1")
        )
        XCTAssertEqual(
            NotificationRouting.pushTarget(["type": "agent_message", "issueId": "i1", "sessionId": "s1"]),
            .issue(id: "i1")
        )
    }
}
