import GRDB
import XCTest
@testable import ExpCore

// #ERR-26: the memory budget of a whole-table read. However many models
// observe a table, ONE read runs per change window and every one of them
// holds the SAME value (one copy of the table, not one per observer).
@MainActor
final class SharedReadTests: XCTestCase {
    private var tempDir: URL!
    private var pool: DatabasePool!

    override func setUp() async throws {
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("shared-read-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        pool = try DatabasePool(path: tempDir.appendingPathComponent("t.sqlite").path)
        try DatabaseManager.runMigrations(on: pool)
    }

    override func tearDown() async throws {
        try? pool.close()
        pool = nil
        try? FileManager.default.removeItem(at: tempDir)
        tempDir = nil
    }

    private func issue(_ n: Int, description: String? = nil) -> IssueEntity {
        IssueEntity(
            id: "i\(n)", boardId: "b1", number: n, identifier: "EXP-\(n)", title: "Issue \(n)",
            description: description, status: "in_progress", priority: "none", assigneeId: nil,
            creatorId: "u1", source: nil, dueDate: nil, sortOrder: Double(n),
            completedAt: nil, duplicateOfId: nil,
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
    }

    private func insert(_ n: Int, description: String? = nil) async throws {
        let row = issue(n, description: description)
        try await pool.write { try row.insert($0) }
    }

    private func read(minInterval: Duration = .milliseconds(300), stopAfter: Duration = .milliseconds(100)) -> SharedRead<[IssueEntity]> {
        SharedRead(writer: pool, tables: ["issues"], minInterval: minInterval, stopAfter: stopAfter, fetch: {
            try IssueEntity.fetchAll($0)
        })
    }

    /// Each observer's last value, by index.
    private final class Seen {
        var values: [[IssueEntity]?]
        init(_ count: Int) { values = Array(repeating: nil, count: count) }
    }

    private func observe(_ read: SharedRead<[IssueEntity]>, count: Int) -> (Seen, [Task<Void, Never>]) {
        let seen = Seen(count)
        let tasks = (0..<count).map { index in
            let stream = read.values()
            return Task { @MainActor in
                for await value in stream { seen.values[index] = value }
            }
        }
        return (seen, tasks)
    }

    private func waitUntil(_ timeout: Duration = .seconds(5), _ condition: () -> Bool) async throws {
        let deadline = ContinuousClock.now + timeout
        while !condition() {
            guard ContinuousClock.now < deadline else { return XCTFail("timed out") }
            try await Task.sleep(for: .milliseconds(10))
        }
    }

    func testTenObserversShareOneReadAndOneCopy() async throws {
        try await insert(0)
        let read = read()
        let (seen, tasks) = observe(read, count: 10)
        defer { tasks.forEach { $0.cancel() } }
        try await waitUntil { seen.values.allSatisfy { $0?.count == 1 } }
        XCTAssertEqual(read.reads, 1)
        let storage = Set(seen.values.map { $0!.withUnsafeBufferPointer { Int(bitPattern: $0.baseAddress) } })
        XCTAssertEqual(storage.count, 1, "every observer holds the same rows, not a copy")
    }

    func testABurstOfWritesFoldsIntoAtMostOneReadPerInterval() async throws {
        try await insert(0)
        let read = read(minInterval: .milliseconds(500))
        let (seen, tasks) = observe(read, count: 10)
        defer { tasks.forEach { $0.cancel() } }
        try await waitUntil { seen.values.allSatisfy { $0?.count == 1 } }
        for n in 1...20 { try await insert(n) }
        try await waitUntil { seen.values.allSatisfy { $0?.count == 21 } }
        // The first read, at most one while the burst lands, then the folded
        // rest: never one per write, never one per observer.
        XCTAssertLessThanOrEqual(read.reads, 3)
    }

    func testTheReadStopsAfterTheLastObserverLeavesAndRestartsFresh() async throws {
        try await insert(0)
        let read = read()
        let (seen, tasks) = observe(read, count: 2)
        try await waitUntil { seen.values.allSatisfy { $0 != nil } }
        tasks.forEach { $0.cancel() }
        try await waitUntil { !read.isLive }
        let reads = read.reads
        try await insert(1)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(read.reads, reads, "an idle read never re-reads")

        let (again, more) = observe(read, count: 1)
        defer { more.forEach { $0.cancel() } }
        try await waitUntil { again.values[0]?.count == 2 }
    }

    func testAnEqualReReadKeepsTheHeldValue() async throws {
        try await insert(0)
        let read = SharedRead<[IssueEntity]>(
            writer: pool, tables: ["issues"], minInterval: .milliseconds(50), same: { $0 == $1 },
            fetch: { try IssueEntity.fetchAll($0) }
        )
        var deliveries = 0
        var held: [IssueEntity] = []
        let stream = read.values()
        let task = Task { @MainActor in
            for await value in stream {
                deliveries += 1
                held = value
            }
        }
        defer { task.cancel() }
        try await waitUntil { deliveries == 1 }
        let before = held.withUnsafeBufferPointer { Int(bitPattern: $0.baseAddress) }
        try await pool.write { try $0.execute(sql: #"UPDATE "issues" SET "title" = "title""#) }
        try await waitUntil { read.reads == 2 }
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(deliveries, 1, "an equal re-read is no emission")
        XCTAssertEqual(held.withUnsafeBufferPointer { Int(bitPattern: $0.baseAddress) }, before)
    }

    func testOneAccountDatabaseHasOneIssuesRead() {
        let reads = SharedReads(writer: pool)
        XCTAssertTrue(reads.issues === reads.issues)
    }

    func testTheListingReadLeavesTheDescriptionOut() async throws {
        try await insert(1, description: String(repeating: "x", count: 10_000))
        let rows = try await pool.read { try IssueEntity.withoutDescription($0).fetchAll($0) }
        XCTAssertEqual(rows.map(\.title), ["Issue 1"])
        XCTAssertNil(rows[0].description)
        XCTAssertEqual(rows[0].identifier, "EXP-1")
    }
}
