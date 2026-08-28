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
                creatorId: "u1", source: nil, dueDate: nil, sortOrder: 1.0,
                completedAt: nil, duplicateOfId: nil, prUrl: nil,
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

    // EXP-481: a devices partial touching the jsonb launch_defaults column
    // (delivered as a raw JSON string on the partial wire) plus an unknown
    // future column must apply the known subset and advance — the shapes 17/18
    // decoders ride the same tolerant path as everything else.
    func testDevicePartialAppliesJsonbAndDropsUnknown() async throws {
        try await pool.write { db in
            try DeviceEntity(
                id: "row-1", userId: "u1", deviceId: "dev-1", label: "box",
                kind: "server", lastSeenAt: "2026-08-11T10:00:00Z"
            ).save(db)
        }
        let message = ShapeMessage<DeviceEntity>.partialUpdate(
            key: #""public"."devices"/"row-1""#,
            columns: columns([
                "id": "row-1",
                "launch_defaults": #"{"defaultAgent":"codex"}"#,
                "last_seen_at": "2026-08-11T10:05:00Z",
                "some_future_column": "x",
            ])
        )
        try await applyBatch(messages: [message], name: "devices", table: "devices", pool: pool)
        let row = try await pool.read { db in
            try DeviceEntity.fetchOne(db, key: "row-1")
        }
        XCTAssertEqual(row?.lastSeenAt, "2026-08-11T10:05:00Z")
        XCTAssertEqual(row?.launchDefaults, #"{"defaultAgent":"codex"}"#)
        XCTAssertEqual(row?.label, "box")
    }

    // EXP-481: the worktree `busy` BOOLEAN column takes the wire-bool mapping
    // on partials ("t" → real bool), like coding_sessions.needs_input.
    func testWorktreePartialCoercesBusyBool() async throws {
        try await pool.write { db in
            try DeviceWorktreeEntity(
                id: "wt-1", deviceRowId: "row-1", repoFullName: "acme/api",
                branch: "exp/EXP-1", busy: false
            ).save(db)
        }
        let message = ShapeMessage<DeviceWorktreeEntity>.partialUpdate(
            key: #""public"."device_worktrees"/"wt-1""#,
            columns: columns(["id": "wt-1", "busy": "t", "dirty": "tracked"])
        )
        try await applyBatch(
            messages: [message], name: "device-worktrees", table: "device_worktrees", pool: pool
        )
        let row = try await pool.read { db in
            try DeviceWorktreeEntity.fetchOne(db, key: "wt-1")
        }
        XCTAssertEqual(row?.busy, true)
        XCTAssertEqual(row?.dirty, "tracked")
    }

    func testBoardInsertPersistsRepositoryAndIcon() async throws {
        // The boards shape carries the repo ride-along + the curated icon; an
        // inserted row must persist both (EXP-364 removed `is_protected` —
        // protected boards are gone from the product and the shape).
        let board = BoardEntity(
            id: "p1", teamId: "ws1", name: "Dogfood", slug: "exponential",
            prefix: "EXP", color: "#6366f1", sortOrder: 0,
            repositoryId: "repo1", icon: "rocket",
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
        let message = ShapeMessage<BoardEntity>.insert(
            key: #""public"."boards"/"p1""#, value: board
        )
        try await applyBatch(messages: [message], name: "boards", table: "boards", pool: pool)
        let stored = try await pool.read { try BoardEntity.fetchOne($0, key: "p1") }
        XCTAssertEqual(stored?.repositoryId, "repo1")
        XCTAssertEqual(stored?.icon, "rocket")
    }

    // EXP-637: the coding-sessions shape carries the agent's own close-out.
    // An inserted row must round-trip all four columns into the v22 schema.
    func testCodingSessionInsertPersistsOutcomeAndSummary() async throws {
        let session = CodingSessionEntity(
            id: "cs1", issueId: nil, teamId: "ws1", userId: "u1",
            deviceLabel: "macbook", deviceId: "dev-1", status: "ended",
            actionName: "Refresh screenshots",
            summary: "Recaptured every store slide and opened a PR.",
            outcome: "done", endedBy: "agent", resumedFromId: "cs0",
            startedAt: "2026-08-27T09:00:00Z", endedAt: "2026-08-27T09:12:00Z",
            createdAt: "2026-08-27T09:00:00Z", updatedAt: "2026-08-27T09:12:00Z"
        )
        let message = ShapeMessage<CodingSessionEntity>.insert(
            key: #""public"."coding_sessions"/"cs1""#, value: session
        )
        try await applyBatch(
            messages: [message], name: "coding-sessions", table: "coding_sessions", pool: pool
        )
        let stored = try await pool.read { try CodingSessionEntity.fetchOne($0, key: "cs1") }
        XCTAssertEqual(stored?.summary, "Recaptured every store slide and opened a PR.")
        XCTAssertEqual(stored?.outcome, "done")
        XCTAssertEqual(stored?.endedBy, "agent")
        XCTAssertEqual(stored?.resumedFromId, "cs0")
    }

    // A pre-EXP-637 snapshot omits all four keys entirely — the decoder must
    // read them as nil rather than throw (which would brick the shape).
    func testCodingSessionDecodesWithoutOutcomeKeys() throws {
        let json = """
            {"id":"cs2","issue_id":null,"team_id":"ws1","user_id":"u1",
             "device_label":"macbook","status":"ended",
             "started_at":"2026-08-27T09:00:00Z","ended_at":"2026-08-27T09:12:00Z",
             "created_at":"2026-08-27T09:00:00Z","updated_at":"2026-08-27T09:12:00Z"}
            """
        let row = try JSONDecoder().decode(CodingSessionEntity.self, from: Data(json.utf8))
        XCTAssertNil(row.summary)
        XCTAssertNil(row.outcome)
        XCTAssertNil(row.endedBy)
        XCTAssertNil(row.resumedFromId)
        XCTAssertEqual(row.status, "ended")
    }

    // EXP-484: the coding-sessions shape carries the agent the run was
    // launched with. It round-trips into the v23 column, and a pre-EXP-484
    // snapshot omitting the key decodes nil rather than bricking the shape.
    func testCodingSessionDecodesAgentAndDefaultsNil() async throws {
        let session = CodingSessionEntity(
            id: "cs3", issueId: "i1", teamId: "ws1", userId: "u1",
            deviceLabel: "macbook", deviceId: "dev-1", status: "running", agent: "codex",
            startedAt: "2026-08-28T09:00:00Z", endedAt: nil,
            createdAt: "2026-08-28T09:00:00Z", updatedAt: "2026-08-28T09:00:00Z"
        )
        let message = ShapeMessage<CodingSessionEntity>.insert(
            key: #""public"."coding_sessions"/"cs3""#, value: session
        )
        try await applyBatch(
            messages: [message], name: "coding-sessions", table: "coding_sessions", pool: pool
        )
        let stored = try await pool.read { try CodingSessionEntity.fetchOne($0, key: "cs3") }
        XCTAssertEqual(stored?.agent, "codex")

        let json = """
            {"id":"cs4","issue_id":"i1","team_id":"ws1","user_id":"u1",
             "device_label":"macbook","status":"running",
             "started_at":"2026-08-28T09:00:00Z","ended_at":null,
             "created_at":"2026-08-28T09:00:00Z","updated_at":"2026-08-28T09:00:00Z"}
            """
        let row = try JSONDecoder().decode(CodingSessionEntity.self, from: Data(json.utf8))
        XCTAssertNil(row.agent)
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
