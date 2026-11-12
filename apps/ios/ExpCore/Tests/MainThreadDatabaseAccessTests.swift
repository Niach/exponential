import Foundation
import GRDB
import XCTest
@testable import ExpCore

// #PERF-5: a debug build flags every read or write that blocks the main thread.
final class MainThreadDatabaseAccessTests: XCTestCase {
    private var flagged: [(kind: MainThreadDatabaseAccess.Kind, sql: String)] = []
    private var pool: DatabasePool!
    private var path: URL!

    override func setUpWithError() throws {
        path = FileManager.default.temporaryDirectory.appendingPathComponent("main-thread-db-\(UUID().uuidString).sqlite")
        var config = Configuration()
        config.journalMode = .wal
        MainThreadDatabaseAccess.watch(&config)
        MainThreadDatabaseAccess.onAccess = { [unowned self] kind, sql in flagged.append((kind, sql)) }
        // Opening a pool (and migrating it) is exempt.
        pool = try MainThreadDatabaseAccess.exempting {
            let pool = try DatabasePool(path: path.path, configuration: config)
            try pool.write { db in try db.execute(sql: "CREATE TABLE t (id TEXT); INSERT INTO t VALUES ('a')") }
            return pool
        }
    }

    override func tearDownWithError() throws {
        MainThreadDatabaseAccess.onAccess = nil
        try pool.close()
        try? FileManager.default.removeItem(at: path)
    }

    private func sql(_ kind: MainThreadDatabaseAccess.Kind) -> [String] {
        flagged.filter { $0.kind == kind }.map(\.sql)
    }

    @MainActor
    func testOpeningThePoolIsExempt() {
        XCTAssertTrue(flagged.isEmpty, "\(flagged)")
    }

    @MainActor
    func testASyncReadOnTheMainThreadIsFlagged() throws {
        XCTAssertTrue(Thread.isMainThread)
        _ = try pool.read { db in try String.fetchOne(db, sql: "SELECT id FROM t") }
        XCTAssertTrue(sql(.read).contains("SELECT id FROM t"), "\(flagged)")
    }

    @MainActor
    func testASyncWriteOnTheMainThreadIsFlagged() throws {
        try pool.write { db in try db.execute(sql: "INSERT INTO t VALUES ('b')") }
        XCTAssertTrue(sql(.write).contains("INSERT INTO t VALUES ('b')"), "\(flagged)")
    }

    @MainActor
    func testAwaitedReadsAndWritesAreNot() async throws {
        try await pool.write { db in try db.execute(sql: "INSERT INTO t VALUES ('b')") }
        let count = try await pool.read { db in try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM t") }
        XCTAssertEqual(count, 2)
        XCTAssertTrue(flagged.isEmpty, "\(flagged)")
    }

    func testSyncAccessOffMainIsNot() throws {
        let done = expectation(description: "access")
        DispatchQueue.global().async { [pool] in
            try? pool!.write { db in try db.execute(sql: "INSERT INTO t VALUES ('c')") }
            _ = try? pool!.read { db in try String.fetchOne(db, sql: "SELECT id FROM t") }
            done.fulfill()
        }
        wait(for: [done], timeout: 5)
        XCTAssertTrue(flagged.isEmpty, "\(flagged)")
    }
}
