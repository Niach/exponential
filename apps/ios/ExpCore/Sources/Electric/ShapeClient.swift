import Foundation
import GRDB
import os

private let initialOffset = "-1"
private let liveTimeoutSeconds: TimeInterval = 60
private let logger = Logger(subsystem: "at.exponential", category: "ShapeClient")
// EXP-304: flat retry budget for "the network isn't up yet" failures, before
// the exponential ladder takes over. Android uses the same figures.
private let networkUnreadyRetryMs: UInt64 = 750
private let networkUnreadyBurst = 6

/// The session every shape of ONE account shares (EXP-304).
///
/// It used to be one session per shape, which meant 15 separate connections to
/// the same host: app start fired 15 simultaneous cold DNS lookups and TLS
/// handshakes, and that storm — not the volume of data — is what put ~10s
/// between launching the app and seeing current data. One session lets
/// URLSession negotiate HTTP/2 and multiplex all 15 long-polls over a single
/// connection: one lookup, one handshake.
///
/// Per ACCOUNT, never one global session: the cookie-off stance below is a
/// cross-account leak guard and must stay scoped the same way the bearer is.
public func makeShapeSession() -> URLSession {
    let config = URLSessionConfiguration.default
    config.timeoutIntervalForRequest = liveTimeoutSeconds + 30
    // 15 concurrent long-polls share this session, and the HTTP/1.1 fallback
    // default is 6 — which would queue 9 of them behind holds that only end
    // when the server has something to say.
    config.httpMaximumConnectionsPerHost = 32
    // Never let URLCache answer a shape request. Electric snapshots ship
    // `cache-control: public, max-age=604800`, so a cached (possibly empty,
    // anonymously-authed) snapshot replayed on a bare offset=-1 refetch would
    // wipe every local row and re-save a stale handle — the poisoned-cache 409
    // loop. Shape reads must always hit the server.
    config.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData
    config.urlCache = nil
    // Bearer auth is the ONLY credential a shape request carries; a cookie jar
    // is not just unnecessary but actively harmful across accounts on the same
    // host. Better Auth sets a signed `__Secure-better-auth.session_data`
    // cookie on every authenticated response (a 5-minute session-cache
    // snapshot) and getSession trusts that cookie OVER the bearer. With the
    // default shared jar, a cookie left over from a previously signed-in user
    // rides along on this account's requests and the server resolves — and
    // syncs — them as the PREVIOUS user (the "Apple login shows the Google
    // account's data" cross-account leak). Kill the jar so shapes are always
    // scoped by the bearer (mirrors HTTPClient's cookie-off stance; the server
    // also ignores the cache on bearer requests, this is defense-in-depth +
    // stops us hoarding it).
    config.httpShouldSetCookies = false
    config.httpCookieStorage = nil
    return URLSession(configuration: config)
}

public final class ShapeClient<T: Codable & Sendable>: Sendable {
    private let shapeName: String
    private let urlPath: String
    // The owning account — reports are keyed by it so the offline banner only
    // reflects the ACTIVE account's health.
    private let accountId: String
    private let baseUrlProvider: @Sendable () -> String?
    private let tokenProvider: @Sendable () -> String?
    private let pool: DatabasePool
    private let onMessages: @Sendable ([ShapeMessage<T>]) async throws -> Void

    private let session: URLSession

    public init(
        shapeName: String,
        urlPath: String,
        accountId: String,
        baseUrlProvider: @escaping @Sendable () -> String?,
        tokenProvider: @escaping @Sendable () -> String?,
        pool: DatabasePool,
        session: URLSession,
        onMessages: @escaping @Sendable ([ShapeMessage<T>]) async throws -> Void
    ) {
        self.shapeName = shapeName
        self.urlPath = urlPath
        self.accountId = accountId
        self.baseUrlProvider = baseUrlProvider
        self.tokenProvider = tokenProvider
        self.pool = pool
        self.session = session
        self.onMessages = onMessages
    }

    public func run() async throws {
        var backoffMs: UInt64 = 500
        // H2 (§9.1): a nil baseUrl/token provider silently spins here every 2s
        // and looks identical to the migration blackout ("no shape activity, no
        // log entries"). Log the transition into and out of the paused state
        // (once each — not every tick — so the 50-line ring buffer stays useful).
        var loggedMissingCreds = false
        // Auto-recovery: after 3 consecutive schema-class apply errors, mark the
        // shape needs_refetch ONCE per run so the next poll re-fetches
        // atomically (no UI blackout). `didAutoReset` gates the one-shot;
        // `pendingRecoveryReport` flips the diagnostics badge to `.recovered` on
        // the first clean poll after the reset.
        var consecutiveSchemaErrors = 0
        var didAutoReset = false
        var pendingRecoveryReport = false
        // EXP-304: the FIRST poll of this run must come back rather than
        // disappear into a live hold. A resumed shape's cursor is already live,
        // so it would otherwise send `live=true` and Electric would hold the
        // request open by design — nothing observes whether we caught up, and
        // the app sits on stale rows until something unrelated changes on the
        // server. One non-live poll answers in a single round-trip with the
        // pending delta (or a bare up-to-date); after that we go live as usual.
        // Every wake path — scenePhase .active, network regain, pull-to-refresh
        // — restarts the pipeline, so a fresh `run()` is exactly the moment
        // this needs to be true.
        var confirmFreshness = true
        var consecutiveNetworkErrors = 0
        while !Task.isCancelled {
            do {
                guard let baseUrl = baseUrlProvider(), let token = tokenProvider() else {
                    if !loggedMissingCreds {
                        SyncDebug.shared.log("[\(shapeName)] paused: awaiting baseUrl/token")
                        loggedMissingCreds = true
                    }
                    try await Task.sleep(for: .seconds(2))
                    continue
                }
                if loggedMissingCreds {
                    SyncDebug.shared.log("[\(shapeName)] resumed: credentials available")
                    loggedMissingCreds = false
                }
                let shouldPause = try await pollOnce(
                    baseUrl: baseUrl, token: token, confirmFreshness: confirmFreshness
                )
                // Freshness is established — this poll returned from the current
                // offset — so the next one may take the live hold.
                confirmFreshness = false
                // A clean poll supersedes any pending error: reset the schema
                // escalation counter, clear the per-shape error, and report
                // `recovered` once if we had auto-reset this run.
                consecutiveSchemaErrors = 0
                consecutiveNetworkErrors = 0
                SyncDebug.shared.clearShapeError(name: shapeName)
                if pendingRecoveryReport {
                    pendingRecoveryReport = false
                    SyncDebug.shared.reportRecovery(name: shapeName, .recovered)
                }
                backoffMs = 500
                if shouldPause {
                    try await Task.sleep(for: .milliseconds(500))
                }
            } catch is CancellationError {
                throw CancellationError()
            } catch ShapeError.upgradeRequired {
                // Client-version gate (EXP-104): below THIS server's minimum.
                // Stop this loop permanently — no backoff, no retry. Only this
                // account's shapes stop (the gate is keyed by accountId,
                // REV2-43); other signed-in servers keep syncing, and sync here
                // restarts on the next app launch (with, presumably, an updated
                // build). Logged like the other terminal failures so the
                // timeline shows why it stopped.
                logger.warning("[\(self.shapeName)] stopped: client upgrade required")
                SyncDebug.shared.log("[\(shapeName)] STOP: client upgrade required")
                return
            } catch {
                let message = Self.describe(error)
                let isSchema = Self.isSchemaError(error)
                logger.warning("[\(self.shapeName)] error: \(message)")
                SyncDebug.shared.log("[\(shapeName)] ERR: \(message)")
                SyncDebug.shared.reportApplyError(name: shapeName, message: message, isSchema: isSchema)
                // HTTP-level failures already went through reportShape; this
                // also catches transport errors so the sync banner can react.
                if !(error is ShapeError) {
                    SyncDebug.shared.reportTransportError(name: shapeName, accountId: accountId)
                }
                if isSchema {
                    consecutiveSchemaErrors += 1
                    if consecutiveSchemaErrors >= 3 && !didAutoReset {
                        didAutoReset = true
                        pendingRecoveryReport = true
                        await persistNeedsRefetch()
                        SyncDebug.shared.reportRecovery(name: shapeName, .recovering)
                    }
                }
                // EXP-304: DNS/connect-class failures mean "the network isn't
                // usable yet" — a radio waking, a VPN establishing its tunnel,
                // private DNS resolving — not "the server is unhappy". They
                // clear on their own in a second or two, so the first few
                // retries stay flat and short instead of climbing the
                // 500ms→30s ladder and leaving the shape parked long after
                // connectivity came back. Past the burst the ladder takes over,
                // so a genuine outage still can't be hammered.
                if Self.isNetworkUnready(error) {
                    consecutiveNetworkErrors += 1
                } else {
                    consecutiveNetworkErrors = 0
                }
                if consecutiveNetworkErrors > 0, consecutiveNetworkErrors <= networkUnreadyBurst {
                    // The connection we come back on may be a different one, so
                    // re-prove freshness rather than reopening a live hold.
                    confirmFreshness = true
                    try await Task.sleep(for: .milliseconds(networkUnreadyRetryMs))
                    backoffMs = 500
                } else {
                    try await Task.sleep(for: .milliseconds(backoffMs))
                    backoffMs = min(backoffMs * 2, 30_000)
                }
            }
        }
    }

    /// Does this failure mean "the network isn't usable yet" rather than "the
    /// server said no"? These all clear by themselves once the radio, the VPN
    /// tunnel or the DNS resolver is up.
    private static func isNetworkUnready(_ error: Error) -> Bool {
        guard let urlError = error as? URLError else { return false }
        switch urlError.code {
        case .cannotFindHost, .dnsLookupFailed, .cannotConnectToHost,
             .notConnectedToInternet, .networkConnectionLost, .timedOut,
             .internationalRoamingOff, .callIsActive, .dataNotAllowed:
            return true
        default:
            return false
        }
    }

    /// Mark the shape needs_refetch so the next poll does the atomic
    /// DELETE+reinsert refetch — identical to the inline must-refetch handler,
    /// so rows stay on screen until replaced (no blackout). No handle: the old
    /// one is presumed dead after a run of schema-class failures.
    private func persistNeedsRefetch() async {
        try? await pool.write { db in
            try ElectricOffset(
                shape: self.shapeName, handle: "", offset: initialOffset,
                needsRefetch: true, isLive: false
            ).save(db)
        }
    }

    /// The SQLite message for a GRDB failure (its `localizedDescription` is a
    /// generic NSError string that hides the cause), else the plain description.
    private static func describe(_ error: Error) -> String {
        if let dbError = error as? DatabaseError {
            return dbError.message ?? "\(dbError)"
        }
        return error.localizedDescription
    }

    /// A schema-drift failure the tolerant-apply path couldn't absorb — the
    /// signal that triggers a one-shot refetch of the shape.
    private static func isSchemaError(_ error: Error) -> Bool {
        guard let dbError = error as? DatabaseError, let message = dbError.message?.lowercased() else {
            return false
        }
        return message.contains("no such column")
            || message.contains("no such table")
            || message.contains("has no column")
    }

    /// Returns `true` when the next poll should be paced (a refetch is pending
    /// after a 409 / inline must-refetch, or a non-live poll made no progress).
    private func pollOnce(
        baseUrl: String, token: String, confirmFreshness: Bool
    ) async throws -> Bool {
        let saved = try await pool.read { db in
            try ElectricOffset.fetchOne(db, key: shapeName)
        }
        // A persisted needs_refetch row survives a quit between the 409 and the
        // refetch, so the atomic DELETE+reinsert still happens after relaunch.
        let refetching = saved?.needsRefetch ?? false
        let wasLive = saved?.isLive ?? false
        // A live cursor may still take ONE non-live poll first, so this run's
        // first request comes back with an answer instead of parking in a hold
        // (EXP-304 — see `confirmFreshness` in `run()`).
        let goLive = wasLive && !confirmFreshness

        var components = URLComponents(string: "\(baseUrl)\(urlPath)")!
        if saved == nil || refetching {
            // Initial snapshot / post-409 refetch: offset=-1, plus the
            // replacement handle Electric sent on the 409 (when we have one).
            var query = [URLQueryItem(name: "offset", value: initialOffset)]
            if let handle = saved?.handle, !handle.isEmpty {
                query.append(URLQueryItem(name: "handle", value: handle))
            }
            components.queryItems = query
        } else {
            var query = [
                URLQueryItem(name: "offset", value: saved!.offset),
                URLQueryItem(name: "handle", value: saved!.handle),
            ]
            // Only long-poll live once the snapshot completed (up-to-date
            // seen); catch-up polls stay non-live per the Electric protocol.
            if goLive {
                query.append(URLQueryItem(name: "live", value: "true"))
            }
            components.queryItems = query
        }

        var request = URLRequest(url: components.url!)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        // Client-version gate (EXP-104): shape reads carry the build version too,
        // so an out-of-date client's sync loop gets 426'd like everything else.
        request.setValue(AppConstants.clientVersionHeaderValue, forHTTPHeaderField: "x-client-version")

        let startedAt = DispatchTime.now().uptimeNanoseconds
        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw ShapeError.invalidResponse
        }
        // EXP-304: how long the request actually took. A live hold legitimately
        // sits at ~60s, but a snapshot/catch-up/confirm poll that takes seconds
        // is the whole story of "sync is slow" — and without this the debug log
        // could say a shape errored, but never that it was slow.
        let elapsedMs = (DispatchTime.now().uptimeNanoseconds - startedAt) / 1_000_000
        let kind = (saved == nil || refetching) ? "snapshot" : (goLive ? "live" : (wasLive ? "confirm" : "catchup"))

        logger.info("[\(self.shapeName)] HTTP \(httpResponse.statusCode), \(data.count) bytes, \(kind), \(elapsedMs)ms")
        SyncDebug.shared.log("[\(shapeName)] HTTP \(httpResponse.statusCode), \(data.count)B, \(kind) \(elapsedMs)ms")
        SyncDebug.shared.reportShape(name: shapeName, httpStatus: httpResponse.statusCode, isLive: wasLive, accountId: accountId)

        if httpResponse.statusCode == 409 {
            // The shape rotated. Electric sends the replacement handle in the
            // response header — persist it with a needs_refetch marker instead
            // of deleting the row, so the refetch targets the new handle and a
            // quit before it lands still resumes into the atomic replacement.
            // Don't delete table data yet — leave stale rows visible until the
            // next poll re-fetches and replaces them atomically.
            let newHandle = httpResponse.value(forHTTPHeaderField: "electric-handle") ?? ""
            try await pool.write { db in
                try ElectricOffset(
                    shape: shapeName, handle: newHandle, offset: initialOffset,
                    needsRefetch: true, isLive: false
                ).save(db)
            }
            return true
        }

        if httpResponse.statusCode == 401 {
            throw ShapeError.unauthorized
        }

        // Client-version gate (EXP-104): this build is below THIS server's
        // minimum. Trip the update gate for the owning account only (REV2-43 —
        // defensive decode, min/latest may be absent) and throw so run() can
        // stop this shape's loop permanently.
        if httpResponse.statusCode == 426 {
            let info = try? JSONDecoder().decode(ClientUpgradeResponse.self, from: data)
            await UpdateGate.shared.trigger(accountId: accountId, min: info?.min, latest: info?.latest)
            throw ShapeError.upgradeRequired
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw ShapeError.httpError(httpResponse.statusCode)
        }

        let handle = httpResponse.value(forHTTPHeaderField: "electric-handle")
        let offset = httpResponse.value(forHTTPHeaderField: "electric-offset")

        var messages = decodeMessages(data)

        // Handle inline must-refetch control message in the response body:
        // strip it, mark the row needs_refetch (no handle — the old one is
        // dead), and signal a pending refetch so the next poll replaces all
        // rows atomically.
        let hasInlineMustRefetch = messages.contains { if case .mustRefetch = $0 { true } else { false } }
        if hasInlineMustRefetch {
            messages.removeAll { if case .mustRefetch = $0 { true } else { false } }
            try await pool.write { db in
                try ElectricOffset(
                    shape: shapeName, handle: "", offset: initialOffset,
                    needsRefetch: true, isLive: false
                ).save(db)
            }
            if !messages.isEmpty {
                try await onMessages(messages)
            }
            return true
        }

        // Only up-to-date flips the shape live — snapshot-end merely closes a
        // snapshot chunk and catch-up polls must stay non-live until Electric
        // says we reached head.
        let sawUpToDate = messages.contains { if case .upToDate = $0 { true } else { false } }

        // After a 409 / inline must-refetch, this is the re-fetch with fresh
        // data. Prepend .mustRefetch so applyBatch does DELETE + INSERTs in
        // one transaction — ValueObservation never sees an empty table.
        if refetching {
            messages.insert(.mustRefetch, at: 0)
        }

        let inserts = messages.filter { if case .insert = $0 { true } else { false } }.count
        let updates = messages.filter { if case .update = $0 { true } else { false } }.count
        let partials = messages.filter { if case .partialUpdate = $0 { true } else { false } }.count
        if !messages.isEmpty {
            logger.info("[\(self.shapeName)] \(messages.count) msgs (\(inserts) ins, \(updates) upd, \(partials) partial)")
            SyncDebug.shared.log("[\(shapeName)] \(inserts) ins, \(updates) upd, \(partials) partial")
            try await onMessages(messages)
        }

        if let handle, let offset {
            let live = sawUpToDate || (wasLive && !refetching)
            try await pool.write { db in
                try ElectricOffset(
                    shape: shapeName, handle: handle, offset: offset,
                    needsRefetch: false, isLive: live
                ).save(db)
            }
        }

        // Pace the loop when a non-live poll made no progress, so a response
        // that never reaches up-to-date can't spin-request.
        // Keyed on goLive, not wasLive: a freshness-confirming poll is non-live
        // too, so if one ever came back empty without up-to-date it must be
        // paced like any other no-progress poll rather than spin-requesting.
        return !goLive && !sawUpToDate && messages.isEmpty
    }

    // Internal (not private) so ExpCoreTests can lock the wire-format mapping
    // (controls incl. snapshot-end, type-aware entity decoding, raw partial
    // updates).
    func decodeMessages(_ data: Data) -> [ShapeMessage<T>] {
        guard !data.isEmpty else { return [] }

        // Electric sends a JSON array of message objects. Parse with
        // JSONSerialization, then re-encode each value for type-aware Codable
        // decoding: every column arrives as a JSON string, and the entities' wire
        // decoders accept both the string and native-scalar forms per field (see
        // WireDecoding.swift) — no whole-row blind coercion.
        guard let array = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            logger.warning("[\(self.shapeName)] top-level decode failed")
            return []
        }
        return array.compactMap { mapRawDict($0) }
    }

    private func mapRawDict(_ dict: [String: Any]) -> ShapeMessage<T>? {
        let headers = dict["headers"] as? [String: Any] ?? [:]

        if let control = headers["control"] as? String {
            switch control {
            case "up-to-date": return .upToDate
            case "must-refetch": return .mustRefetch
            // Chunk boundary of a multi-response snapshot — recognized but
            // carries no data; liveness is gated on up-to-date, never on
            // snapshot-end.
            case "snapshot-end": return nil
            default: return nil
            }
        }

        guard let operation = headers["operation"] as? String,
              let key = dict["key"] as? String else {
            return nil
        }

        let rawValue = dict["value"] as? [String: Any]

        let decodedValue: T? = {
            guard let value = rawValue else { return nil }
            guard let jsonData = try? JSONSerialization.data(withJSONObject: value) else { return nil }
            // Single strict decode: the entities' per-field wire decoders accept
            // both string and native-scalar column values, so a well-formed full
            // row always decodes. A failure here is a genuine bad/partial row —
            // surfaced as a loud decode drop (reportDecodeDrop) rather than
            // silently coerced, which matches Android.
            return try? JSONDecoder().decode(T.self, from: jsonData)
        }()

        switch operation {
        case "insert":
            guard let value = decodedValue else {
                // A full-row insert that failed to decode used to vanish
                // silently. Surface it (the row re-arrives on the next refetch).
                if rawValue != nil {
                    SyncDebug.shared.reportDecodeDrop(name: shapeName, key: key)
                }
                return nil
            }
            return .insert(key: key, value: value)
        case "update":
            if let value = decodedValue {
                return .update(key: key, value: value)
            }
            // A partial (changed-columns-only) update can't decode to a full T.
            // Carry the RAW wire dict as the partial payload — the apply side
            // owns type conversion (SQLite affinity for numerics, boolCols for
            // booleans). Coercing here is what used to corrupt TEXT columns
            // (a title "true" stored as the integer 1).
            guard let rawValue else { return nil }
            guard let columnData = try? JSONSerialization.data(withJSONObject: rawValue) else { return nil }
            return .partialUpdate(key: key, columns: columnData)
        case "delete":
            return .delete(key: key, value: decodedValue)
        default:
            return nil
        }
    }
}

public enum ShapeError: Error {
    case invalidResponse
    case unauthorized
    /// The server rejected this client version (HTTP 426, EXP-104). run() stops
    /// the shape loop for good on this case.
    case upgradeRequired
    case httpError(Int)
}
