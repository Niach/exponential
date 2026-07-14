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
            try IssueLabelEntity(issueId: "i1", labelId: "l1", workspaceId: "ws1").save(db)
        }
        // issue_labels has a composite PK — a partial would emit a `WHERE id`
        // the table doesn't have. It must be skipped, not throw.
        let message = ShapeMessage<IssueLabelEntity>.partialUpdate(
            key: #""public"."issue_labels"/"i1"/"l1""#,
            columns: columns(["workspace_id": "ws2"])
        )
        try await applyBatch(messages: [message], name: "issue-labels", table: "issue_labels", pool: pool)
        let workspaceId = try await pool.read { db in
            try IssueLabelEntity.filter(Column("issue_id") == "i1").fetchOne(db)?.workspaceId
        }
        XCTAssertEqual(workspaceId, "ws1")
    }

    func testProjectInsertPopulatesIsProtected() async throws {
        // The projects shape now carries is_protected; an inserted row must
        // persist it into the v5 column (not silently drop it).
        let project = ProjectEntity(
            id: "p1", workspaceId: "ws1", name: "Dogfood", slug: "exponential",
            prefix: "EXP", color: "#6366f1", sortOrder: 0, archivedAt: nil,
            githubRepo: nil, repositoryId: "repo1", type: "feedback",
            publicShowComments: true, publicShowActivity: false,
            isProtected: true, previewConfig: nil,
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
        let message = ShapeMessage<ProjectEntity>.insert(
            key: #""public"."projects"/"p1""#, value: project
        )
        try await applyBatch(messages: [message], name: "projects", table: "projects", pool: pool)
        let stored = try await pool.read { try ProjectEntity.fetchOne($0, key: "p1") }
        XCTAssertEqual(stored?.isProtected, true)
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
}
