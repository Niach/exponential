import Foundation
import GRDB
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
        case "device": return .device(id: try XCTUnwrap(object["deviceId"] as? String))
        case "none": return .none
        case let other: throw NSError(domain: "unknown kind \(other)", code: 1)
        }
    }

    private func sessionTarget(_ raw: Any?) throws -> InboxRoute.SessionTarget {
        let object = try XCTUnwrap(raw as? [String: Any])
        switch try XCTUnwrap(object["kind"] as? String) {
        case "agent": return .agent(agentId: try XCTUnwrap(object["agentId"] as? String))
        case "chat":
            return .chat(
                chatId: try XCTUnwrap(object["chatId"] as? String),
                sessionId: object["sessionId"] as? String
            )
        case "session": return .session(id: try XCTUnwrap(object["sessionId"] as? String))
        case let other: throw NSError(domain: "unknown kind \(other)", code: 1)
        }
    }

    // #EXP-1987: which team's inbox lists a row (`listCases`).
    func testReplaysEveryListCase() throws {
        let cases = try XCTUnwrap(try fixture()["listCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let row = try XCTUnwrap(testCase["row"] as? [String: Any], name)
            XCTAssertEqual(
                InboxRoute.inboxRowListed(
                    type: try XCTUnwrap(row["type"] as? String, name),
                    rowTeamId: row["teamId"] as? String,
                    issueId: row["issueId"] as? String,
                    teamId: testCase["teamId"] as? String,
                    teamIds: try XCTUnwrap(testCase["teamIds"] as? [String], name)
                ),
                try XCTUnwrap(testCase["expected"] as? Bool, name),
                name
            )
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
                    sessionId: row["sessionId"] as? String,
                    deviceId: row["deviceId"] as? String
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
                    viewerId: testCase["viewerId"] as? String,
                    groupChat: session["groupChat"] as? Bool ?? false
                ),
                try sessionTarget(testCase["expected"]),
                name
            )
        }
    }

    /// #EXP-1990: over the store, a team agent's group chat (its agent's
    /// `group_chat_id`) opens the agent's screen for every member; a child
    /// of it, or another member's private conversation, stays a session.
    func testTheStoreRoutesAGroupChatToTheAgentForEveryMember() throws {
        let pool = try DatabasePool(path: NSTemporaryDirectory() + "exp1990-inbox-route-\(UUID().uuidString).sqlite")
        try DatabaseManager.runMigrations(on: pool)
        let rows = [
            #"{"id":"g1","team_id":"t","root_id":"g1","user_id":"owner","agent_id":"ag1","shared":true,"device_id":"d","created_at":"x","updated_at":"x"}"#,
            #"{"id":"k1","team_id":"t","root_id":"g1","parent_id":"g1","depth":1,"user_id":"owner","device_id":"d","created_at":"x","updated_at":"x"}"#,
            #"{"id":"c2","team_id":"t","root_id":"c2","user_id":"owner","agent_id":"ag2","device_id":"d","created_at":"x","updated_at":"x"}"#,
        ]
        try pool.write { db in
            for row in rows { try JSONDecoder().decode(SessionEntity.self, from: Data(row.utf8)).save(db) }
            try AgentDefinitionEntity(id: "ag1", teamId: "t", name: "Release", createdAt: "x", updatedAt: "x", groupChatId: "g1").save(db)
            try AgentDefinitionEntity(id: "ag2", teamId: "t", name: "Triage", createdAt: "x", updatedAt: "x").save(db)
        }
        try pool.read { db in
            XCTAssertEqual(try InboxRoute.sessionTarget(db: db, sessionId: "g1", viewerId: "me"), .agent(agentId: "ag1"))
            XCTAssertEqual(try InboxRoute.sessionTarget(db: db, sessionId: "k1", viewerId: "me"), .session(id: "k1"))
            XCTAssertEqual(try InboxRoute.sessionTarget(db: db, sessionId: "c2", viewerId: "me"), .session(id: "c2"))
            XCTAssertEqual(try InboxRoute.sessionTarget(db: db, sessionId: "c2", viewerId: "owner"), .agent(agentId: "ag2"))
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
