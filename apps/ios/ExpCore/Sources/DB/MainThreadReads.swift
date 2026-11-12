import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "MainThreadReads")

/// #PERF-5: a synchronous `DatabasePool.read` on the main thread waits for one
/// of the pool's reader connections, and with every reader busy mid-sync that
/// froze the app for 11 s (GlitchTip IOS-1). Debug builds trace every reader
/// connection: a statement that runs on the main thread is reported here (a
/// sync read runs its block on the calling thread; `await pool.read` never
/// does). Set `EXP_TRAP_MAIN_THREAD_READS=1` to stop on the first one.
public enum MainThreadReads {
    /// Installs the trace on a pool's reader connections (debug builds only).
    public static func watch(_ config: inout Configuration) {
        #if DEBUG
        config.prepareDatabase { db in
            guard db.configuration.readonly else { return }
            db.trace(options: .statement) { event in
                guard Thread.isMainThread else { return }
                MainThreadReads.report(event.description)
            }
        }
        #endif
    }

    /// Tests: called with the SQL of every main-thread read.
    nonisolated(unsafe) public static var onRead: ((String) -> Void)?

    private static let trap = ProcessInfo.processInfo.environment["EXP_TRAP_MAIN_THREAD_READS"] == "1"

    static func report(_ sql: String) {
        if let onRead {
            onRead(sql)
            return
        }
        logger.fault("Main-thread database read: \(sql, privacy: .public)")
        if trap { assertionFailure("#PERF-5: main-thread database read: \(sql)") }
    }
}
