import Foundation

public enum RecoveryState: Sendable, Equatable {
    case none
    case recovering
    case recovered
}

public struct ShapeStatus: Sendable {
    public var lastHttpStatus: Int = 0
    public var requestCount: Int = 0
    public var errorCount: Int = 0
    public var lastActivityAt: Date = .now
    public var isLive: Bool = false
    // The most recent apply/transport error message (verbatim SQLite text for a
    // GRDB failure), how many errors have arrived in a row without an
    // intervening clean poll, whether the last one was a schema-class failure
    // (no such column/table — the drift the tolerant-apply guards against), and
    // the auto-recovery state for this shape.
    public var lastErrorMessage: String?
    public var consecutiveErrors: Int = 0
    public var isSchemaError: Bool = false
    public var recoveryState: RecoveryState = .none

    public init(
        lastHttpStatus: Int = 0,
        requestCount: Int = 0,
        errorCount: Int = 0,
        lastActivityAt: Date = .now,
        isLive: Bool = false,
        lastErrorMessage: String? = nil,
        consecutiveErrors: Int = 0,
        isSchemaError: Bool = false,
        recoveryState: RecoveryState = .none
    ) {
        self.lastHttpStatus = lastHttpStatus
        self.requestCount = requestCount
        self.errorCount = errorCount
        self.lastActivityAt = lastActivityAt
        self.isLive = isLive
        self.lastErrorMessage = lastErrorMessage
        self.consecutiveErrors = consecutiveErrors
        self.isSchemaError = isSchemaError
        self.recoveryState = recoveryState
    }
}

@Observable
public final class SyncDebug: @unchecked Sendable {
    public static let shared = SyncDebug()

    public var shapes: [String: ShapeStatus] = [:]
    public var lastMessages: [String] = []
    // (shape|sorted-columns) keys already logged, so a repeating dropped-column
    // partial is logged once per run instead of on every poll. Main-actor only.
    @ObservationIgnored private var reportedDroppedColumnSets: Set<String> = []
    // Aggregate health for the UI's sync banner: any 2xx response counts as a
    // success; transport failures (no HTTP response at all) and non-2xx mark
    // the last error. Healthy = the most recent signal was a success.
    public var lastSuccessAt: Date?
    public var lastErrorAt: Date?
    public var lastErrorWasUnauthorized = false
    // A hard, non-transient failure that stops sync from ever starting for an
    // account: DB pool open / GRDB migration failure, or a resync that can't
    // relaunch (no token, pool throws). Surfaced prominently in SyncDebugView
    // so the "no shape activity + no log entries + resync no-op" blackout can
    // never again be diagnosed only from os.Logger. Cleared on a good launch.
    public var lastFatalError: String?

    /// What the sync banner should say, if anything. Degraded only after the
    /// errors persist a moment (a single failed long-poll mustn't flash a
    /// banner — ShapeClient retries with backoff).
    public enum Health: Equatable, Sendable {
        case ok
        case offline
        case unauthorized
    }

    public var health: Health {
        guard let err = lastErrorAt else { return .ok }
        if let ok = lastSuccessAt, ok > err { return .ok }
        // Require a few seconds of sustained failure before alarming.
        guard Date().timeIntervalSince(err) < 300 else { return .ok }
        if let ok = lastSuccessAt, Date().timeIntervalSince(ok) < 8 { return .ok }
        return lastErrorWasUnauthorized ? .unauthorized : .offline
    }

    /// Record a hard failure that prevents sync from starting for an account.
    /// Also mirrored into the log ring so the timeline shows it in context.
    public func reportFatal(_ message: String) {
        Task { @MainActor in
            self.lastFatalError = message
        }
        log("FATAL: \(message)")
    }

    /// Clear the fatal banner — called once a pipeline successfully launches,
    /// which supersedes any earlier open/migration failure for that account.
    public func clearFatal() {
        Task { @MainActor in
            self.lastFatalError = nil
        }
    }

    public func log(_ message: String) {
        Task { @MainActor in
            self.lastMessages.append(message)
            if self.lastMessages.count > 50 {
                self.lastMessages.removeFirst()
            }
        }
    }

    public func reportShape(name: String, httpStatus: Int, isLive: Bool) {
        Task { @MainActor in
            var status = self.shapes[name] ?? ShapeStatus()
            status.lastHttpStatus = httpStatus
            status.requestCount += 1
            status.lastActivityAt = .now
            status.isLive = isLive
            if (200...299).contains(httpStatus) {
                self.lastSuccessAt = .now
            } else {
                status.errorCount += 1
                self.lastErrorAt = .now
                self.lastErrorWasUnauthorized = httpStatus == 401
            }
            self.shapes[name] = status
        }
    }

    /// A request that never produced an HTTP response (network down, DNS, TLS).
    public func reportTransportError(name _: String) {
        Task { @MainActor in
            self.lastErrorAt = .now
            self.lastErrorWasUnauthorized = false
        }
    }

    /// Record a per-shape apply/transport error message so diagnostics shows
    /// *why* a shape is stuck (previously only a count was visible). Aggregate
    /// health (lastErrorAt / unauthorized) stays owned by reportShape /
    /// reportTransportError — this only annotates the per-shape row.
    public func reportApplyError(name: String, message: String, isSchema: Bool) {
        Task { @MainActor in
            var status = self.shapes[name] ?? ShapeStatus()
            status.lastErrorMessage = message
            status.consecutiveErrors += 1
            status.isSchemaError = isSchema
            status.lastActivityAt = .now
            self.shapes[name] = status
        }
    }

    /// A partial update referenced columns this build's schema doesn't have;
    /// the tolerant-apply path dropped them and applied the rest. Logged (not a
    /// per-shape error) so the drift is visible without alarming the banner.
    /// Throttled to once per (shape, column-set) per run — the same partial
    /// arrives on every poll and would otherwise flood the 50-line log.
    public func reportDroppedColumns(name: String, columns: [String]) {
        guard !columns.isEmpty else { return }
        let sorted = columns.sorted()
        let key = "\(name)|\(sorted.joined(separator: ","))"
        Task { @MainActor in
            guard !self.reportedDroppedColumnSets.contains(key) else { return }
            self.reportedDroppedColumnSets.insert(key)
            self.lastMessages.append("[\(name)] dropped unknown columns: \(sorted.joined(separator: ", "))")
            if self.lastMessages.count > 50 { self.lastMessages.removeFirst() }
        }
    }

    /// A full-row insert whose entity failed to decode was dropped; the row
    /// arrives again on the next refetch. Surfaced so silent data loss is
    /// visible in diagnostics.
    public func reportDecodeDrop(name: String, key: String) {
        log("[\(name)] dropped undecodable insert: \(key)")
    }

    /// Auto-recovery transitions for a shape (schema drift → atomic refetch).
    public func reportRecovery(name: String, _ state: RecoveryState) {
        Task { @MainActor in
            var status = self.shapes[name] ?? ShapeStatus()
            status.recoveryState = state
            self.shapes[name] = status
        }
        log("[\(name)] \(state == .recovering ? "auto-recovering (schema drift)" : "recovered")")
    }

    /// A clean poll supersedes any pending per-shape error. Resets the error
    /// counters/message but leaves the recovery state for ShapeClient to flip to
    /// `.recovered` once the refetch reaches up-to-date.
    public func clearShapeError(name: String) {
        Task { @MainActor in
            guard var status = self.shapes[name] else { return }
            if status.consecutiveErrors == 0 && status.lastErrorMessage == nil { return }
            status.consecutiveErrors = 0
            status.lastErrorMessage = nil
            status.isSchemaError = false
            self.shapes[name] = status
        }
    }
}
