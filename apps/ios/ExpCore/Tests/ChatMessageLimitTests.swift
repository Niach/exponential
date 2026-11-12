import Foundation
import GRDB
import XCTest

@testable import ExpCore

/// #EXP-1380 (TS `useNewestMessages`, Android `ChatMessageLimitTest`): a chat
/// loads its NEWEST messages, Load earlier widens by `windowStep`, waiting
/// cards load however old.
final class ChatMessageLimitTests: XCTestCase {
    private struct Row: Equatable {
        let id: String
        let createdAt: String
    }

    private func row(_ i: Int) -> Row {
        Row(id: String(format: "m%05d", i), createdAt: String(format: "2026-11-12T%08d", i))
    }

    private func newest(_ all: [Row], _ limit: Int) -> [Row] {
        Array(all.sorted { $0.createdAt != $1.createdAt ? $0.createdAt < $1.createdAt : $0.id < $1.id }.suffix(limit))
    }

    func testTheLimitIsTheWindowPlusOneStepAndLoadEarlierWidensByAStep() {
        XCTAssertEqual(Chats.chatMessageLimit, Chats.Transcript.window + Chats.Transcript.windowStep)
        XCTAssertEqual(Chats.widenedMessageLimit(Chats.chatMessageLimit), Chats.chatMessageLimit + Chats.Transcript.windowStep)
        let limits = Chats.MessageLimits()
        let key = "chat-limit-test"
        XCTAssertEqual(limits.limit(key), Chats.chatMessageLimit)
        limits.loadEarlier(key)
        limits.loadEarlier(key)
        XCTAssertEqual(limits.limit(key), Chats.chatMessageLimit + 2 * Chats.Transcript.windowStep)
        // Per chat: another key is untouched.
        XCTAssertEqual(limits.limit("\(key)-other"), Chats.chatMessageLimit)
    }

    func testALargeTreeLoadsOnlyItsNewestPagePlusEveryWaitingCard() {
        let all = (0..<8_492).map(row)
        let limit = Chats.chatMessageLimit
        let page = Chats.newestMessagesPage(
            loaded: newest(all, limit), waiting: [all[3], all[8_491], all[3]], limit: limit,
            id: \.id, createdAt: \.createdAt
        )
        XCTAssertEqual(page.rows.count, limit + 1)
        XCTAssertTrue(page.more)
        // The old waiting card joins in time order; neither card is doubled.
        XCTAssertEqual(page.rows.first, all[3])
        XCTAssertEqual(page.rows.last, all.last)
        XCTAssertEqual(Set(page.rows.map(\.id)).count, page.rows.count)
        // A short tree loads whole: nothing more.
        let short = Chats.newestMessagesPage(
            loaded: Array(all.prefix(50)), waiting: [], limit: limit, id: \.id, createdAt: \.createdAt
        )
        XCTAssertFalse(short.more)
        XCTAssertEqual(short.rows.count, 50)
    }

    func testTheWaitingIdsAreEveryDigestsWaitingAndWaitsOnceEachSorted() {
        let ids = Chats.digestWaitingIds([
            #"{"waiting":{"id":"b"},"waits":[{"id":"c"},{"id":"b"}]}"#,
            nil,
            #"{"waits":[{"id":"a"},{"nope":1}]}"#,
            #"{"first":{"id":"z"}}"#,
            "not json",
        ])
        XCTAssertEqual(ids, ["a", "b", "c"])
    }

    /// The SQL: the newest `limit` rows of the root, oldest first, keyed
    /// rows only, never another root's.
    func testNewestMessagesReadsTheRootsNewestPageOldestFirst() throws {
        let queue = try DatabaseQueue()
        try queue.write { db in
            try db.execute(sql: """
                CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, root_id TEXT NOT NULL,
                  team_id TEXT, author_kind TEXT, author_id TEXT, body TEXT, card TEXT,
                  answered_at TEXT, consumed_at TEXT, created_at TEXT, updated_at TEXT,
                  author_session_id TEXT, conversation_id TEXT, attachment_ids TEXT, issue_brief TEXT)
                """)
            for i in 0..<500 {
                let id = String(format: "m%04d", i)
                let stamp = String(format: "2026-11-12 10:%02d:%02d+00", i / 60, i % 60)
                try db.execute(sql: """
                    INSERT INTO messages (id, session_id, root_id, team_id, author_kind, body, created_at, updated_at, conversation_id)
                    VALUES (?, 's', ?, 't', 'agent', 'b', ?, ?, 'c')
                    """, arguments: [id, i % 5 == 0 ? "other" : "root", stamp, stamp])
            }
        }
        let page = try queue.read { db in try SessionQueries.newestMessages(db: db, rootId: "root", limit: 200) }
        XCTAssertEqual(page.count, 200)
        XCTAssertEqual(page.last?.id, "m0499")
        XCTAssertEqual(page.map(\.id), page.map(\.id).sorted())
        XCTAssertTrue(page.allSatisfy { $0.message.rootId == "root" })
    }
}
