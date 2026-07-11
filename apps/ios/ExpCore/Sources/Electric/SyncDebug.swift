import Foundation

public enum RecoveryState: Sendable, Equatable {
    case none
    case recovering
    case recovered
}

/// Per-account aggregate sync health (drives the offline/expired banner).
public struct AccountHealth: Sendable {
    public var lastSuccessAt: Date?
    public var lastErrorAt: Date?
    public var lastErrorWasUnauthorized = false
    /// Start of the CURRENT uninterrupted failure streak: set on the first
    /// failure after a success (or ever), left alone while failures repeat,
    /// cleared by ANY 2xx, and RESTARTED when a failure lands after a long
    /// quiet gap (the retry loops weren't running — app suspended mid-outage).
    /// The banner alarms only once a streak has PERSISTED (EXP-44) — a single
    /// failed long-poll or the app-wake burst (all 15 shapes dying
    /// simultaneously on resume) must never flash it.
    public var failureStreakStartedAt: Date?
    public init() {}
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
    // Per-account health for the UI's sync banner: any 2xx counts as a success;
    // transport failures (no HTTP response) and non-2xx mark the last error.
    // Keyed by accountId — every signed-in account's ShapeClients report here,
    // so the banner must read only the ACTIVE account's entry (one account's
    // outage must never alarm while the active account syncs fine).
    public private(set) var accountHealth: [String: AccountHealth] = [:]
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

    /// How long a failure streak must persist (with no intervening success)
    /// before the banner may alarm. TIME-based by design: on app wake all 14
    /// shape long-polls fail simultaneously before the first fresh success, so
    /// any consecutive-failure COUNT would trip instantly on healthy servers.
    /// The same grace also absorbs one-off 401 token-refresh races.
    private static let failureStreakGrace: TimeInterval = 12

    /// An error older than this no longer alarms (health()'s staleness guard),
    /// and a failure GAP this long breaks the streak's continuity. While
    /// genuinely failing, the retry loops report at most ~30s apart
    /// (ShapeClient's backoff cap) — a far longer gap means they weren't
    /// running (app suspended mid-outage), so the wake burst's first fresh
    /// failure must RESTART the EXP-44 debounce instead of inheriting an
    /// hours-old streak start (which would flash the banner immediately on
    /// resume). Deliberately much larger than failureStreakGrace: during a
    /// real outage the capped backoff keeps gaps well under it, so the streak
    /// — and the banner — stay solid.
    private static let errorStalenessWindow: TimeInterval = 300

    /// Whether a fresh failure starts a NEW streak instead of extending the
    /// current one (see errorStalenessWindow). Shared by both failure paths.
    private static func streakBroken(previousErrorAt: Date?, streakStartedAt: Date?) -> Bool {
        guard streakStartedAt != nil, let previousErrorAt else { return true }
        return Date().timeIntervalSince(previousErrorAt) >= errorStalenessWindow
    }

    /// Health for one account's pipelines (the active account drives the
    /// banner). PURE READ — all state mutation stays in the report* methods
    /// (this is re-evaluated in view bodies on observation ticks).
    public func health(forAccountId accountId: String?) -> Health {
        guard let accountId, let h = accountHealth[accountId], let err = h.lastErrorAt else { return .ok }
        // ANY success after the last failure clears instantly.
        if let ok = h.lastSuccessAt, ok > err { return .ok }
        // Staleness guard: an error that stopped repeating long ago (the retry
        // loops died with the app suspended) mustn't alarm on wake.
        guard Date().timeIntervalSince(err) < Self.errorStalenessWindow else { return .ok }
        // Alarm only once the failure streak has persisted through the grace
        // window — the wake-up burst resolves via a 2xx (streak cleared) well
        // inside it, while a genuine outage keeps the streak alive.
        guard let streakStart = h.failureStreakStartedAt,
              Date().timeIntervalSince(streakStart) >= Self.failureStreakGrace else { return .ok }
        return h.lastErrorWasUnauthorized ? .unauthorized : .offline
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

    public func reportShape(name: String, httpStatus: Int, isLive: Bool, accountId: String) {
        Task { @MainActor in
            var status = self.shapes[name] ?? ShapeStatus()
            status.lastHttpStatus = httpStatus
            status.requestCount += 1
            status.lastActivityAt = .now
            status.isLive = isLive
            if !(200...299).contains(httpStatus) {
                status.errorCount += 1
            }
            self.shapes[name] = status

            var h = self.accountHealth[accountId] ?? AccountHealth()
            if (200...299).contains(httpStatus) {
                h.lastSuccessAt = .now
                h.failureStreakStartedAt = nil // any 2xx clears the streak
            } else {
                let previousErrorAt = h.lastErrorAt
                h.lastErrorAt = .now
                h.lastErrorWasUnauthorized = httpStatus == 401
                if Self.streakBroken(previousErrorAt: previousErrorAt, streakStartedAt: h.failureStreakStartedAt) {
                    h.failureStreakStartedAt = .now
                }
            }
            self.accountHealth[accountId] = h
        }
    }

    /// A request that never produced an HTTP response (network down, DNS, TLS).
    public func reportTransportError(name _: String, accountId: String) {
        Task { @MainActor in
            var h = self.accountHealth[accountId] ?? AccountHealth()
            let previousErrorAt = h.lastErrorAt
            h.lastErrorAt = .now
            h.lastErrorWasUnauthorized = false
            if Self.streakBroken(previousErrorAt: previousErrorAt, streakStartedAt: h.failureStreakStartedAt) {
                h.failureStreakStartedAt = .now
            }
            self.accountHealth[accountId] = h
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
