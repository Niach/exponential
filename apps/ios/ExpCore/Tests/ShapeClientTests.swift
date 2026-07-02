import GRDB
import XCTest
@testable import ExpCore

// EXP-1#13 hardening gate: locks the Electric wire-format mapping (controls
// including snapshot-end, string-coerced values, partial updates) and the
// persisted 409-refetch state (needs_refetch / is_live on electric_offsets,
// including the additive v2 migration an upgrading install runs).
final class ShapeClientTests: XCTestCase {
    private struct Row: Codable, Equatable, Sendable {
        let id: String
        let done: Bool
        let count: Int
    }

    private var tempDir: URL!
    private var pool: DatabasePool!

    override func setUpWithError() throws {
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("shape-client-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        pool = try DatabasePool(path: tempDir.appendingPathComponent("t.sqlite").path)
    }

    override func tearDownWithError() throws {
        try? pool.close()
        pool = nil
        try? FileManager.default.removeItem(at: tempDir)
        tempDir = nil
    }

    private func makeClient() -> ShapeClient<Row> {
        ShapeClient<Row>(
            shapeName: "test",
            urlPath: "/api/shapes/test",
            baseUrlProvider: { nil },
            tokenProvider: { nil },
            pool: pool,
            onMessages: { _ in }
        )
    }

    private func decode(_ json: String) -> [ShapeMessage<Row>] {
        makeClient().decodeMessages(Data(json.utf8))
    }

    // MARK: - Control messages

    func testDecodesUpToDateAndMustRefetchControls() {
        let messages = decode(#"""
        [
          {"headers": {"control": "up-to-date"}},
          {"headers": {"control": "must-refetch"}}
        ]
        """#)
        XCTAssertEqual(messages.count, 2)
        guard case .upToDate = messages[0] else { return XCTFail("expected upToDate") }
        guard case .mustRefetch = messages[1] else { return XCTFail("expected mustRefetch") }
    }

    func testSnapshotEndIsRecognizedButCarriesNoMessage() {
        // Chunk boundary of a multi-response snapshot: recognized, dropped.
        // Liveness gates on up-to-date, never on snapshot-end.
        let messages = decode(#"""
        [
          {"headers": {"control": "snapshot-end"}},
          {"headers": {"control": "up-to-date"}}
        ]
        """#)
        XCTAssertEqual(messages.count, 1)
        guard case .upToDate = messages[0] else { return XCTFail("expected upToDate only") }
    }

    // MARK: - Operations

    func testDecodesInsertWithStringCoercedValues() {
        // Electric sends all column values as strings; decode must coerce.
        let messages = decode(#"""
        [
          {
            "headers": {"operation": "insert"},
            "key": "\"public\".\"test\"/\"a1\"",
            "value": {"id": "a1", "done": "true", "count": "3"}
          }
        ]
        """#)
        XCTAssertEqual(messages.count, 1)
        guard case let .insert(key, value) = messages[0] else { return XCTFail("expected insert") }
        XCTAssertEqual(key, #""public"."test"/"a1""#)
        XCTAssertEqual(value, Row(id: "a1", done: true, count: 3))
    }

    func testUpdateWithPartialColumnsFallsBackToPartialUpdate() {
        // An update carrying only changed columns can't decode to a full Row —
        // it must surface as .partialUpdate with the coerced column payload.
        let messages = decode(#"""
        [
          {
            "headers": {"operation": "update"},
            "key": "\"public\".\"test\"/\"a1\"",
            "value": {"id": "a1", "done": "false"}
          }
        ]
        """#)
        XCTAssertEqual(messages.count, 1)
        guard case let .partialUpdate(key, columns) = messages[0] else {
            return XCTFail("expected partialUpdate")
        }
        XCTAssertEqual(key, #""public"."test"/"a1""#)
        let decoded = try? JSONSerialization.jsonObject(with: columns) as? [String: Any]
        XCTAssertEqual(decoded?["id"] as? String, "a1")
        XCTAssertEqual(decoded?["done"] as? Bool, false)
    }

    func testDecodesDeleteWithoutValue() {
        let messages = decode(#"""
        [
          {
            "headers": {"operation": "delete"},
            "key": "\"public\".\"test\"/\"a1\""
          }
        ]
        """#)
        XCTAssertEqual(messages.count, 1)
        guard case let .delete(key, value) = messages[0] else { return XCTFail("expected delete") }
        XCTAssertEqual(key, #""public"."test"/"a1""#)
        XCTAssertNil(value)
    }

    func testUnknownControlAndMalformedEntriesAreDropped() {
        let messages = decode(#"""
        [
          {"headers": {"control": "some-future-control"}},
          {"headers": {"operation": "insert"}},
          {"key": "no-headers"}
        ]
        """#)
        XCTAssertTrue(messages.isEmpty)
    }
}

// MARK: - Refetch state persistence (EXP-1#13)

final class OffsetRefetchStateTests: XCTestCase {
    func testElectricOffsetRefetchStateRoundTrips() throws {
        let accountId = "refetch-roundtrip-\(UUID().uuidString)"
        let manager = DatabaseManager()
        defer {
            manager.closePool(forAccountId: accountId)
            DatabaseManager.deleteFiles(forAccountId: accountId)
        }
        let pool = try manager.pool(forAccountId: accountId)

        // A 409 persists the replacement handle with needs_refetch — a quit
        // between the 409 and the refetch must resume into the atomic refetch.
        try pool.write { db in
            try ElectricOffset(
                shape: "issues", handle: "new-handle", offset: "-1",
                needsRefetch: true, isLive: false
            ).save(db)
        }
        var saved = try pool.read { try ElectricOffset.fetchOne($0, key: "issues") }
        XCTAssertEqual(saved?.handle, "new-handle")
        XCTAssertEqual(saved?.offset, "-1")
        XCTAssertEqual(saved?.needsRefetch, true)
        XCTAssertEqual(saved?.isLive, false)

        // After up-to-date the shape flips live and the marker clears.
        try pool.write { db in
            try ElectricOffset(
                shape: "issues", handle: "new-handle", offset: "42_0",
                needsRefetch: false, isLive: true
            ).save(db)
        }
        saved = try pool.read { try ElectricOffset.fetchOne($0, key: "issues") }
        XCTAssertEqual(saved?.needsRefetch, false)
        XCTAssertEqual(saved?.isLive, true)

        // Legacy 3-column call sites keep compiling and default to inert state.
        XCTAssertEqual(ElectricOffset(shape: "s", handle: "h", offset: "o").needsRefetch, false)
        XCTAssertEqual(ElectricOffset(shape: "s", handle: "h", offset: "o").isLive, false)
    }

    func testV2MigrationIsAdditiveAndPreservesExistingRows() throws {
        // Simulate a pre-v2 install: a DB whose migration record contains only
        // v1_initial and whose electric_offsets rows use the old 3-column
        // schema. Opening through DatabaseManager must run ONLY the additive
        // v2 migration — rows survive with needs_refetch/is_live = false.
        // (A -v4 filename bump would instead wipe the local snapshot; the
        // whole point of v2 being ALTER TABLE is that it never does.)
        let accountId = "refetch-migration-\(UUID().uuidString)"
        defer { DatabaseManager.deleteFiles(forAccountId: accountId) }

        let url = try DatabaseManager.fileURL(for: accountId)
        do {
            var migrator = DatabaseMigrator()
            migrator.registerMigration("v1_initial") { db in
                try db.create(table: "electric_offsets", ifNotExists: true) { t in
                    t.primaryKey("shape", .text)
                    t.column("handle", .text).notNull()
                    t.column("offset", .text).notNull()
                }
            }
            let pool = try DatabasePool(path: url.path)
            try migrator.migrate(pool)
            try pool.write { db in
                try db.execute(
                    sql: "INSERT INTO electric_offsets (shape, handle, offset) VALUES (?, ?, ?)",
                    arguments: ["issues", "h1", "42_0"]
                )
            }
            try pool.close()
        }

        let manager = DatabaseManager()
        defer { manager.closePool(forAccountId: accountId) }
        let pool = try manager.pool(forAccountId: accountId)
        let saved = try pool.read { try ElectricOffset.fetchOne($0, key: "issues") }
        XCTAssertEqual(saved?.handle, "h1")
        XCTAssertEqual(saved?.offset, "42_0")
        XCTAssertEqual(saved?.needsRefetch, false)
        XCTAssertEqual(saved?.isLive, false)
    }
}
