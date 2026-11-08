import Foundation
import GRDB
import XCTest
@testable import ExpCore

// #EXP-1730: a message keys to its own session, locked ×4 against
// `packages/domain-contract/fixtures/message-session.json`.
final class MessageSessionTests: XCTestCase {
    private func cases() throws -> [[String: Any]] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/message-session.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        let fixture = try XCTUnwrap(json as? [String: Any])
        let cases = try XCTUnwrap(fixture["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        return cases
    }

    private struct FixtureMessage {
        let id: String
        let sessionId: String?
        let conversationId: String?
    }

    private func messages(_ testCase: [String: Any]) throws -> [FixtureMessage] {
        try XCTUnwrap(testCase["messages"] as? [[String: Any]]).map {
            FixtureMessage(
                id: try XCTUnwrap($0["id"] as? String),
                sessionId: $0["sessionId"] as? String,
                conversationId: $0["conversationId"] as? String
            )
        }
    }

    private func expected(_ testCase: [String: Any]) throws -> [[String]] {
        try XCTUnwrap(testCase["expected"] as? [[String: Any]]).map {
            [try XCTUnwrap($0["id"] as? String), try XCTUnwrap($0["sessionId"] as? String)]
        }
    }

    func testReplaysEveryCase() throws {
        for testCase in try cases() {
            let name = testCase["name"] as? String ?? ""
            let keyed = MessageSessions.keyMessagesBySession(try messages(testCase), sessionId: \.sessionId)
            XCTAssertEqual(keyed.map { [$0.message.id, $0.sessionId] }, try expected(testCase), name)
        }
    }

    /// The local store keys the same way: `SessionQueries.messages` reads
    /// `messages.session_id`, so every case replays against GRDB too.
    func testSessionQueriesReplayEveryCase() throws {
        for testCase in try cases() {
            let name = testCase["name"] as? String ?? ""
            let queue = try DatabaseQueue()
            try DatabaseManager.makeMigrator().migrate(queue)
            let rows = try messages(testCase)
            try queue.write { db in
                for (index, row) in rows.enumerated() {
                    try MessageEntity(
                        id: row.id,
                        conversationId: row.conversationId ?? "c-\(row.id)",
                        rootId: "s-root",
                        teamId: "t",
                        authorKind: "user",
                        // Input order = creation order.
                        createdAt: "2026-01-01T00:00:0\(index)Z",
                        updatedAt: "2026-01-01T00:00:00Z",
                        sessionId: row.sessionId
                    ).insert(db)
                }
            }
            let keyed = try queue.read { try SessionQueries.messages(db: $0, rootId: "s-root") }
            XCTAssertEqual(keyed.map { [$0.id, $0.sessionId] }, try expected(testCase), name)
            for pair in try expected(testCase) {
                let own = try queue.read { try SessionQueries.messages(db: $0, sessionId: pair[1]) }
                XCTAssertTrue(own.contains { $0.id == pair[0] }, name)
            }
        }
    }
}
