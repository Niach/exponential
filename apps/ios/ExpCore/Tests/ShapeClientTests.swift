import GRDB
import XCTest
@testable import ExpCore

// Hardening gate: locks the Electric wire-format mapping (controls including
// snapshot-end, type-aware entity decoding, raw partial updates) and the
// persisted 409-refetch state (needs_refetch / is_live on electric_offsets,
// including the additive v2 migration an upgrading install runs).
final class ShapeClientTests: XCTestCase {
    private struct Row: Codable, Equatable, Sendable {
        let id: String
        let title: String
        let done: Bool
        let count: Int

        init(id: String, title: String, done: Bool, count: Int) {
            self.id = id
            self.title = title
            self.done = done
            self.count = count
        }

        enum CodingKeys: String, CodingKey { case id, title, done, count }

        // Mirrors the real entities: `title` is a required String (its absence in
        // a changed-columns-only update is what forces the .partialUpdate
        // fallback), while `done`/`count` go through the type-aware wire helpers —
        // so this fixture locks the helpers and the client mapping together.
        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decode(String.self, forKey: .id)
            title = try c.decode(String.self, forKey: .title)
            done = c.decodeWireBool(forKey: .done, default: false)
            count = try c.decodeWireInt(forKey: .count) ?? 0
        }
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
            accountId: "test-account",
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

    func testDecodesInsertWithWireStringValues() {
        // Electric sends all column values as strings; type-aware decoding maps
        // the numeric/boolean fields while keeping String fields verbatim.
        let messages = decode(#"""
        [
          {
            "headers": {"operation": "insert"},
            "key": "\"public\".\"test\"/\"a1\"",
            "value": {"id": "a1", "title": "Hello", "done": "true", "count": "3"}
          }
        ]
        """#)
        XCTAssertEqual(messages.count, 1)
        guard case let .insert(key, value) = messages[0] else { return XCTFail("expected insert") }
        XCTAssertEqual(key, #""public"."test"/"a1""#)
        XCTAssertEqual(value, Row(id: "a1", title: "Hello", done: true, count: 3))
    }

    func testStringFieldsKeepNumericAndBooleanLookingText() {
        // THE regression for the permanent-drop bug: a String field whose value
        // LOOKS numeric/boolean must decode as the verbatim string, never coerced
        // to Int/Bool/Double (which then failed the String re-decode and dropped
        // the row forever).
        for raw in ["404", "true", "3.5"] {
            let messages = decode(#"""
            [
              {
                "headers": {"operation": "insert"},
                "key": "\"public\".\"test\"/\"a1\"",
                "value": {"id": "a1", "title": "\#(raw)", "done": "false", "count": "0"}
              }
            ]
            """#)
            XCTAssertEqual(messages.count, 1, "expected one message for title \(raw)")
            guard case let .insert(_, value) = messages[0] else {
                return XCTFail("expected insert for title \(raw)")
            }
            XCTAssertEqual(value.title, raw)
        }
    }

    func testUpdateWithPartialColumnsFallsBackToPartialUpdate() {
        // An update carrying only changed columns can't decode to a full Row
        // (required `title` is absent) — it surfaces as .partialUpdate. The
        // payload now carries the RAW wire values (strings); the apply side
        // (SQLite affinity + boolCols) owns conversion, so nothing is coerced
        // here.
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
        XCTAssertEqual(decoded?["done"] as? String, "false")
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

    func testFreshInstallOffsetsCarryRefetchState() throws {
        // EXP-180 collapsed the migration list to a single v1_initial (the -v5
        // filename bump wiped every old snapshot, so the additive v2 upgrade
        // path is gone). The refetch-state columns must now be baked directly
        // into the collapsed create — a DatabaseManager-opened pool decodes
        // rows with the inert defaults straight away.
        let accountId = "refetch-fresh-\(UUID().uuidString)"
        let manager = DatabaseManager()
        defer {
            manager.closePool(forAccountId: accountId)
            DatabaseManager.deleteFiles(forAccountId: accountId)
        }
        let pool = try manager.pool(forAccountId: accountId)
        try pool.write { db in
            try db.execute(
                sql: "INSERT INTO electric_offsets (shape, handle, offset) VALUES (?, ?, ?)",
                arguments: ["issues", "h1", "42_0"]
            )
        }
        let saved = try pool.read { try ElectricOffset.fetchOne($0, key: "issues") }
        XCTAssertEqual(saved?.handle, "h1")
        XCTAssertEqual(saved?.offset, "42_0")
        XCTAssertEqual(saved?.needsRefetch, false)
        XCTAssertEqual(saved?.isLive, false)
    }
}
