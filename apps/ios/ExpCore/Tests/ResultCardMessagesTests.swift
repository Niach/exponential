import Foundation
import GRDB
import XCTest

@testable import ExpCore

/// results-tab.json + session-screen.json `count` (Android
/// `ChatResultsCardsTest`): the Results tab and its count fold the root's
/// session-authored card rows, never the chat's loaded window (#EXP-1380),
/// so a result older than the window still counts.
final class ResultCardMessagesTests: XCTestCase {
    func testAResultOlderThanTheLoadedWindowStillCounts() throws {
        let queue = try DatabaseQueue()
        try queue.write { db in
            try db.execute(sql: """
                CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, root_id TEXT NOT NULL,
                  team_id TEXT, author_kind TEXT, author_id TEXT, body TEXT, card TEXT,
                  answered_at TEXT, consumed_at TEXT, created_at TEXT, updated_at TEXT,
                  author_session_id TEXT, conversation_id TEXT, attachment_ids TEXT, issue_brief TEXT)
                """)
            let result = #"{"kind":"surface","title":"Shot","result":true}"#
            for i in 0..<500 {
                let id = String(format: "m%04d", i)
                let stamp = String(format: "2026-11-12T10:%02d:%02d.000Z", i / 60, i % 60)
                // m0003 = the old result; m0004 = a card no session wrote;
                // m0005 = a JSON `null` card; m0006 = another root's result.
                let card: String? = [3, 4, 6].contains(i) ? result : (i == 5 ? "null" : nil)
                let author: String? = i == 4 ? nil : "s"
                try db.execute(sql: """
                    INSERT INTO messages (id, session_id, root_id, team_id, author_kind, body, card,
                      created_at, updated_at, author_session_id, conversation_id)
                    VALUES (?, 's', ?, 't', 'agent', 'b', ?, ?, ?, ?, 'c')
                    """, arguments: [id, i == 6 ? "other" : "root", card, stamp, stamp, author])
            }
        }
        let window = try queue.read { db in try SessionQueries.newestMessages(db: db, rootId: "root", limit: 200) }
        XCTAssertFalse(window.contains { $0.id == "m0003" })

        let cards = try queue.read { db in try SessionQueries.resultCardMessages(db: db, rootId: "root") }
        XCTAssertEqual(cards.map(\.id), ["m0003"])

        let messages = cards.map { row in
            ResultsTab.MessageInput(
                id: row.id, sessionId: row.sessionId, authorSessionId: row.authorSessionId,
                createdAt: SessionTree.ms(row.createdAt) ?? 0, card: TreeMessageCard.parse(row.card)
            )
        }
        let results = ResultsTab.results(
            now: Date().timeIntervalSince1970 * 1000,
            rootId: "root",
            sessions: [ResultsTab.SessionInput(id: "s", rootId: "root", name: "Chat")],
            messages: messages
        )
        XCTAssertEqual(results.map(\.id), ["m0003"])
    }

    /// Large chats: a session's dock cards and composer replies read its
    /// card rows only (a JSON `null` card is none), never its conversation.
    func testCardRowsAreTheSessionsOwnCardRowsOnly() throws {
        let queue = try DatabaseQueue()
        try queue.write { db in
            try db.execute(sql: """
                CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, root_id TEXT NOT NULL,
                  team_id TEXT, author_kind TEXT, author_id TEXT, body TEXT, card TEXT,
                  answered_at TEXT, consumed_at TEXT, created_at TEXT, updated_at TEXT,
                  author_session_id TEXT, conversation_id TEXT, attachment_ids TEXT, issue_brief TEXT)
                """)
            let rows: [(String, String, String?)] = [
                ("a", "s", nil), ("b", "s", #"{"kind":"replies","items":[]}"#), ("c", "s", "null"),
                ("d", "other", #"{"kind":"question"}"#), ("e", "s", #"{"sealed":"v1:1:x"}"#),
            ]
            for (index, row) in rows.enumerated() {
                let stamp = "2026-11-12T10:00:0\(index).000Z"
                try db.execute(sql: """
                    INSERT INTO messages (id, session_id, root_id, team_id, author_kind, body, card,
                      created_at, updated_at, conversation_id)
                    VALUES (?, ?, 'root', 't', 'agent', 'b', ?, ?, ?, 'c')
                    """, arguments: [row.0, row.1, row.2, stamp, stamp])
            }
        }
        let cards = try queue.read { db in try SessionQueries.cardRows(db: db, sessionId: "s") }
        XCTAssertEqual(cards.map(\.id), ["b", "e"])
    }
}
