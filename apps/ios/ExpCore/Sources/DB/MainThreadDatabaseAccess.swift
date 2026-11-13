import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "MainThreadDatabaseAccess")

/// #PERF-5: a synchronous `DatabasePool.read` on the main thread waits for one
/// of the pool's reader connections, a synchronous `write` for the writer, and
/// with every connection busy mid-sync that froze the app for 11 s (GlitchTip
/// IOS-1). Debug builds trace every connection: a statement that runs on the
/// main thread is reported here (a sync read or write runs its block on the
/// calling thread; `await pool.read` / `await pool.write` never do). Opening
/// a pool (its migrations) is exempt. Set `EXP_TRAP_MAIN_THREAD_DB=1` to stop
/// on the first one.
public enum MainThreadDatabaseAccess {
    public enum Kind: String, Sendable {
        case read
        case write
    }

    /// Installs the trace on a pool's connections (debug builds only).
    public static func watch(_ config: inout Configuration) {
        #if DEBUG
        config.prepareDatabase { db in
            let kind: Kind = db.configuration.readonly ? .read : .write
            db.trace(options: .statement) { event in
                guard Thread.isMainThread, !MainThreadDatabaseAccess.exempt else { return }
                MainThreadDatabaseAccess.report(kind, event.description)
            }
        }
        #endif
    }

    /// Runs `body` (a pool's open and migrations) without reports.
    public static func exempting<T>(_ body: () throws -> T) rethrows -> T {
        guard Thread.isMainThread else { return try body() }
        let outer = exempt
        exempt = true
        defer { exempt = outer }
        return try body()
    }

    /// Only touched on the main thread.
    nonisolated(unsafe) private static var exempt = false

    /// Tests: called with every main-thread statement.
    nonisolated(unsafe) public static var onAccess: ((Kind, String) -> Void)?

    private static let trap = ProcessInfo.processInfo.environment["EXP_TRAP_MAIN_THREAD_DB"] == "1"

    static func report(_ kind: Kind, _ sql: String) {
        if let onAccess {
            onAccess(kind, sql)
            return
        }
        logger.fault("Main-thread database \(kind.rawValue, privacy: .public): \(sql, privacy: .public)")
        if trap { assertionFailure("#PERF-5: main-thread database \(kind.rawValue): \(sql)") }
    }
}
