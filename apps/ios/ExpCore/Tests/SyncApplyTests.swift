import GRDB
import XCTest
@testable import ExpCore

// Tolerant partial-apply gate (the native sync-brick fix). A partial update that
// touches a column this build's schema doesn't have (e.g. the users shape
// delivering `onboarding_completed_at`) must NOT throw `no such column`, abort
// the batch transaction before the offset save, and refail forever. It must
// drop the unknown columns, apply the known subset, and — for a pure-unknown or
// composite-PK partial — no-op so the batch commits and the offset advances.
final class SyncApplyTests: XCTestCase {
    private var tempDir: URL!
    private var pool: DatabasePool!

    override func setUpWithError() throws {
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("sync-apply-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        pool = try DatabasePool(path: tempDir.appendingPathComponent("t.sqlite").path)
        try DatabaseManager.runMigrations(on: pool)
    }

    override func tearDownWithError() throws {
        try? pool.close()
        pool = nil
        try? FileManager.default.removeItem(at: tempDir)
        tempDir = nil
    }

    // MARK: - Helpers

    private func seedUser(id: String, name: String?) throws {
        try pool.write { db in
            try UserEntity(
                id: id, name: name, email: "\(id)@example.com", image: nil,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            ).save(db)
        }
    }

    private func userKey(_ id: String) -> String { #""public"."users"/"\#(id)""# }

    private func columns(_ dict: [String: Any]) -> Data {
        try! JSONSerialization.data(withJSONObject: dict)
    }

    private func fetchUser(_ id: String) throws -> UserEntity? {
        try pool.read { try UserEntity.fetchOne($0, key: id) }
    }

    private func seedIssue(id: String, title: String) throws {
        try pool.write { db in
            try IssueEntity(
                id: id, boardId: "p1", number: 1, identifier: "EXP-1", title: title,
                description: nil, status: "todo", priority: "none", assigneeId: nil,
                creatorId: "u1", source: nil, dueDate: nil, dueTime: nil, endTime: nil, sortOrder: 1.0,
                completedAt: nil, archivedAt: nil, duplicateOfId: nil, prUrl: nil,
                prNumber: nil, prState: nil, branch: nil, prMergedAt: nil,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            ).save(db)
        }
    }

    private func issueKey(_ id: String) -> String { #""public"."issues"/"\#(id)""# }

    private func fetchIssue(_ id: String) throws -> IssueEntity? {
        try pool.read { try IssueEntity.fetchOne($0, key: id) }
    }

    private func seedSubscriber(id: String, unsubscribed: Bool) throws {
        try pool.write { db in
            try IssueSubscriberEntity(
                id: id, issueId: "i1", userId: "u1", email: nil, teamId: "ws1",
                source: "manual", unsubscribed: unsubscribed,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            ).save(db)
        }
    }

    private func subscriberKey(_ id: String) -> String { #""public"."issue_subscribers"/"\#(id)""# }

    private func fetchSubscriber(_ id: String) throws -> IssueSubscriberEntity? {
        try pool.read { try IssueSubscriberEntity.fetchOne($0, key: id) }
    }

    // MARK: - Tests

    func testUnknownColumnPartialAppliesKnownSubset() async throws {
        try seedUser(id: "u1", name: "Old")
        // The users shape can deliver server-only columns absent from the native
        // schema — the known column must still apply, the unknown one drop.
        let message = ShapeMessage<UserEntity>.partialUpdate(
            key: userKey("u1"),
            columns: columns(["id": "u1", "name": "New", "onboarding_completed_at": "2026-05-01T00:00:00Z"])
        )
        try await applyBatch(messages: [message], name: "users", table: "users", pool: pool)
        XCTAssertEqual(try fetchUser("u1")?.name, "New")
    }

    func testPureUnknownPartialIsNoOpAndDoesNotThrow() async throws {
        try seedUser(id: "u2", name: "Keep")
        let message = ShapeMessage<UserEntity>.partialUpdate(
            key: userKey("u2"),
            columns: columns(["id": "u2", "onboarding_completed_at": "x", "had_trial": "true", "is_admin": "false"])
        )
        // Every SET column is unknown → nothing to update → no throw, row intact.
        try await applyBatch(messages: [message], name: "users", table: "users", pool: pool)
        XCTAssertEqual(try fetchUser("u2")?.name, "Keep")
    }

    func testCompositePkPartialIsSkipped() async throws {
        try await pool.write { db in
            try IssueLabelEntity(issueId: "i1", labelId: "l1", teamId: "ws1").save(db)
        }
        // issue_labels has a composite PK — a partial would emit a `WHERE id`
        // the table doesn't have. It must be skipped, not throw.
        let message = ShapeMessage<IssueLabelEntity>.partialUpdate(
            key: #""public"."issue_labels"/"i1"/"l1""#,
            columns: columns(["team_id": "ws2"])
        )
        try await applyBatch(messages: [message], name: "issue-labels", table: "issue_labels", pool: pool)
        let teamId = try await pool.read { db in
            try IssueLabelEntity.filter(Column("issue_id") == "i1").fetchOne(db)?.teamId
        }
        XCTAssertEqual(teamId, "ws1")
    }

    func testBoardInsertPopulatesIsProtected() async throws {
        // The boards shape now carries is_protected; an inserted row must
        // persist it into the v5 column (not silently drop it).
        let board = BoardEntity(
            id: "p1", teamId: "ws1", name: "Dogfood", slug: "exponential",
            prefix: "EXP", color: "#6366f1", sortOrder: 0, archivedAt: nil,
            repositoryId: "repo1", isProtected: true,
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
        let message = ShapeMessage<BoardEntity>.insert(
            key: #""public"."boards"/"p1""#, value: board
        )
        try await applyBatch(messages: [message], name: "boards", table: "boards", pool: pool)
        let stored = try await pool.read { try BoardEntity.fetchOne($0, key: "p1") }
        XCTAssertEqual(stored?.isProtected, true)
    }

    func testSupportReplyNotificationInsertPersistsTeamId() async throws {
        // The notifications shape now carries team_id — set on issue-less
        // support_reply rows (the helpdesk ticket's team). An inserted row must
        // round-trip both the NULL issue_id and the team_id into the v2 column.
        let notification = NotificationEntity(
            id: "n1", userId: "u1", issueId: nil, teamId: "ws1",
            type: "support_reply", title: "New reply on ticket",
            body: "A customer replied", readAt: nil, pushedAt: nil,
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
        let message = ShapeMessage<NotificationEntity>.insert(
            key: #""public"."notifications"/"n1""#, value: notification
        )
        try await applyBatch(messages: [message], name: "notifications", table: "notifications", pool: pool)
        let stored = try await pool.read { try NotificationEntity.fetchOne($0, key: "n1") }
        XCTAssertNil(stored?.issueId)
        XCTAssertEqual(stored?.teamId, "ws1")
        XCTAssertEqual(stored?.type, "support_reply")
    }

    func testPoisonedPartialDoesNotAbortBatch() async throws {
        try seedUser(id: "target", name: "Old")
        // A batch that used to abort at the poisoned partial (unknown column)
        // must now commit every message: both inserts land and the known part
        // of the partial applies.
        let messages: [ShapeMessage<UserEntity>] = [
            .insert(key: userKey("a"), value: UserEntity(
                id: "a", name: "A", email: "a@example.com", image: nil,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            )),
            .partialUpdate(key: userKey("target"), columns: columns(["name": "Renamed", "bogus_col": "x"])),
            .insert(key: userKey("b"), value: UserEntity(
                id: "b", name: "B", email: "b@example.com", image: nil,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            )),
        ]
        try await applyBatch(messages: messages, name: "users", table: "users", pool: pool)
        XCTAssertNotNil(try fetchUser("a"))
        XCTAssertNotNil(try fetchUser("b"))
        XCTAssertEqual(try fetchUser("target")?.name, "Renamed")
    }

    // MARK: - Type-aware partial apply (the wire-decoding fix's apply half)

    func testPartialUpdateKeepsBooleanLookingTitleText() async throws {
        // Raw wire partials carry strings; a TEXT column must keep the exact
        // bytes. The old coercing pipeline turned "true" into the integer 1 and
        // "404" into the number 404 before binding — corrupting the title.
        for raw in ["true", "404"] {
            try seedIssue(id: "i1", title: "seed")
            let message = ShapeMessage<IssueEntity>.partialUpdate(
                key: issueKey("i1"), columns: columns(["title": raw])
            )
            try await applyBatch(messages: [message], name: "issues", table: "issues", pool: pool)
            XCTAssertEqual(try fetchIssue("i1")?.title, raw)
        }
    }

    func testPartialUpdateCoercesBooleanColumnFromWireString() async throws {
        // BOOLEAN columns are the affinity exception: a wire "true"/"t" must map
        // to a real Bool binding so GRDB's Bool read doesn't fail on TEXT.
        for raw in ["true", "t"] {
            try seedSubscriber(id: "s1", unsubscribed: false)
            let message = ShapeMessage<IssueSubscriberEntity>.partialUpdate(
                key: subscriberKey("s1"), columns: columns(["unsubscribed": raw])
            )
            try await applyBatch(
                messages: [message], name: "issue-subscribers",
                table: "issue_subscribers", pool: pool
            )
            XCTAssertEqual(try fetchSubscriber("s1")?.unsubscribed, true)
        }
    }

    func testPartialUpdateNumericStringsUseColumnAffinity() async throws {
        // INTEGER/REAL columns rely on SQLite affinity to convert numeric text.
        try seedIssue(id: "i2", title: "seed")
        let message = ShapeMessage<IssueEntity>.partialUpdate(
            key: issueKey("i2"), columns: columns(["number": "7", "sort_order": "3.5"])
        )
        try await applyBatch(messages: [message], name: "issues", table: "issues", pool: pool)
        let issue = try fetchIssue("i2")
        XCTAssertEqual(issue?.number, 7)
        XCTAssertEqual(issue?.sortOrder, 3.5)
    }
}
