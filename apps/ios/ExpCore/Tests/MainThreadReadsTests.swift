import Foundation
import GRDB
import XCTest
@testable import ExpCore

// #PERF-5: a debug build flags every read that blocks the main thread.
final class MainThreadReadsTests: XCTestCase {
    private var flagged: [String] = []
    private var pool: DatabasePool!
    private var path: URL!

    override func setUpWithError() throws {
        path = FileManager.default.temporaryDirectory.appendingPathComponent("main-thread-reads-\(UUID().uuidString).sqlite")
        var config = Configuration()
        config.journalMode = .wal
        MainThreadReads.watch(&config)
        pool = try DatabasePool(path: path.path, configuration: config)
        try pool.write { db in try db.execute(sql: "CREATE TABLE t (id TEXT); INSERT INTO t VALUES ('a')") }
        MainThreadReads.onRead = { [unowned self] sql in flagged.append(sql) }
    }

    override func tearDownWithError() throws {
        MainThreadReads.onRead = nil
        try pool.close()
        try? FileManager.default.removeItem(at: path)
    }

    @MainActor
    func testASyncReadOnTheMainThreadIsFlagged() throws {
        XCTAssertTrue(Thread.isMainThread)
        _ = try pool.read { db in try String.fetchOne(db, sql: "SELECT id FROM t") }
        XCTAssertTrue(flagged.contains("SELECT id FROM t"), "\(flagged)")
    }

    @MainActor
    func testAnAwaitedReadIsNot() async throws {
        let id = try await pool.read { db in try String.fetchOne(db, sql: "SELECT id FROM t") }
        XCTAssertEqual(id, "a")
        XCTAssertEqual(flagged, [])
    }

    func testASyncReadOffMainIsNot() throws {
        let done = expectation(description: "read")
        DispatchQueue.global().async { [pool] in
            _ = try? pool!.read { db in try String.fetchOne(db, sql: "SELECT id FROM t") }
            done.fulfill()
        }
        wait(for: [done], timeout: 5)
        XCTAssertEqual(flagged, [])
    }
}
