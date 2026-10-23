import Foundation
import GRDB
import XCTest
@testable import ExpCore

// NEW-8: messages are keyed through their conversation, locked ×4 against
// `packages/domain-contract/fixtures/message-conversation.json`.
final class MessageConversationTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/message-conversation.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private struct FixtureMessage {
        let id: String
        let conversationId: String
    }

    private func conversations(_ raw: Any?) throws -> [MessageConversation] {
        let rows = try XCTUnwrap(raw as? [[String: Any]])
        return try rows.map {
            MessageConversation(id: try XCTUnwrap($0["id"] as? String), sessionId: $0["sessionId"] as? String)
        }
    }

    private func cases() throws -> (defaults: [MessageConversation], cases: [[String: Any]]) {
        let fixture = try fixture()
        let cases = try XCTUnwrap(fixture["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        return (try conversations(fixture["conversations"]), cases)
    }

    private func expected(_ testCase: [String: Any]) throws -> [[String]] {
        try XCTUnwrap(testCase["expected"] as? [[String: Any]]).map {
            [try XCTUnwrap($0["id"] as? String), try XCTUnwrap($0["sessionId"] as? String)]
        }
    }

    func testReplaysEveryCase() throws {
        let (defaults, cases) = try cases()
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let conversations = testCase["conversations"] != nil
                ? try conversations(testCase["conversations"])
                : defaults
            let messages = try XCTUnwrap(testCase["messages"] as? [[String: Any]]).map {
                FixtureMessage(
                    id: try XCTUnwrap($0["id"] as? String),
                    conversationId: try XCTUnwrap($0["conversationId"] as? String)
                )
            }
            let keyed = MessageConversations.keyMessagesBySession(
                messages,
                conversationId: \.conversationId,
                conversations: conversations
            )
            XCTAssertEqual(keyed.map { [$0.message.id, $0.sessionId] }, try expected(testCase), name)
        }
    }

    /// The local store keys the same way: `SessionQueries.messages` joins the
    /// synced `conversations`, so every case replays against GRDB too.
    func testSessionQueriesReplayEveryCase() throws {
        let (defaults, cases) = try cases()
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let conversations = testCase["conversations"] != nil
                ? try conversations(testCase["conversations"])
                : defaults
            let queue = try DatabaseQueue()
            try DatabaseManager.makeMigrator().migrate(queue)
            let rows = try XCTUnwrap(testCase["messages"] as? [[String: Any]])
            try queue.write { db in
                for conversation in conversations {
                    try ConversationEntity(
                        id: conversation.id,
                        teamId: "t",
                        kind: conversation.sessionId == nil ? "dm" : "session",
                        sessionId: conversation.sessionId,
                        createdAt: "2026-01-01T00:00:00Z",
                        updatedAt: "2026-01-01T00:00:00Z"
                    ).insert(db)
                }
                for (index, row) in rows.enumerated() {
                    try MessageEntity(
                        id: try XCTUnwrap(row["id"] as? String),
                        conversationId: try XCTUnwrap(row["conversationId"] as? String),
                        rootId: "s-root",
                        teamId: "t",
                        authorKind: "user",
                        // Input order = creation order.
                        createdAt: "2026-01-01T00:00:0\(index)Z",
                        updatedAt: "2026-01-01T00:00:00Z"
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
