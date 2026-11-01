import Foundation
import os

// #EXP-1528: privacy-safe connection analytics (fixtures/steer-enet.json
// `connStats`): eCore event envelopes {v:1, id, kind, at, source, data}, no
// session, user, team or device id, posted in batches to tRPC
// `connStats.record {events}`. Never blocks nor errors the UI.

private let logger = Logger(subsystem: "at.exponential", category: "SteerConnStats")

public enum SteerConnStats {
    public static let kinds = ["steer.view", "steer.serve"]
    public static let sources = ["steer-viewer", "steer-device"]
    public static let phases = ["settled", "ended"]
    public static let platforms = ["web", "desktop", "ios", "android", "cli"]
    public static let paths = ["direct", "iroh_relay", "server_fallback"]
    public static let outcomes = ["ok", "failed"]
    public static let errorClasses = [
        "no_endpoint", "no_node", "refused", "unauthorized", "join_timeout", "stale", "send_failed",
        "closed", "no_key", "device_offline", "device_unsupported", "ticket_failed", "relay_closed", "other",
    ]
    public static let settleAfterMs = 10_000
    public static let flushMs = 60_000
    public static let batchMax = 50
    public static let versionMax = 64

    private static let eventKeys: Set<String> = ["v", "id", "kind", "at", "source", "data"]
    private static let dataKeys: Set<String> = [
        "phase", "platform", "version", "outcome", "path", "firstFrameMs", "errorClass",
        "fallbacks", "reconnects", "durationMs", "failures",
    ]

    private static func isCount(_ value: Any?) -> Bool {
        guard let n = value as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID() else { return false }
        let d = n.doubleValue
        return d >= 0 && d <= 2_147_483_647 && d.rounded() == d
    }

    /// The server's validation of one posted event (`validCases`): anything
    /// off-schema, an extra key included, is refused whole.
    public static func valid(_ value: Any) -> Bool {
        guard let e = value as? [String: Any], Set(e.keys).isSubset(of: eventKeys) else { return false }
        guard let v = e["v"] as? NSNumber, CFGetTypeID(v) != CFBooleanGetTypeID(), v.intValue == 1, v.doubleValue == 1,
              let id = e["id"] as? String, !id.isEmpty, id.count <= 64,
              let kind = e["kind"] as? String, kinds.contains(kind),
              let source = e["source"] as? String, sources.contains(source),
              let at = e["at"] as? NSNumber, CFGetTypeID(at) != CFBooleanGetTypeID(),
              at.doubleValue > 0, at.doubleValue.rounded() == at.doubleValue,
              let d = e["data"] as? [String: Any], Set(d.keys).isSubset(of: dataKeys) else { return false }
        guard let phase = d["phase"] as? String, phases.contains(phase),
              let platform = d["platform"] as? String, platforms.contains(platform),
              let version = d["version"] as? String, !version.isEmpty, version.count <= versionMax,
              let outcome = d["outcome"] as? String, outcomes.contains(outcome) else { return false }
        if let path = d["path"] {
            guard let path = path as? String, paths.contains(path) else { return false }
        } else if outcome == "ok" {
            return false
        }
        if let errorClass = d["errorClass"] {
            guard let errorClass = errorClass as? String, errorClasses.contains(errorClass) else { return false }
        }
        for key in ["firstFrameMs", "fallbacks", "reconnects", "durationMs"] where d[key] != nil {
            guard isCount(d[key]) else { return false }
        }
        if let failures = d["failures"] {
            guard let f = failures as? [String: Any] else { return false }
            for (k, n) in f where !errorClasses.contains(k) || !isCount(n) { return false }
        }
        return true
    }
}

/// One posted event (`SteerConnEvent`).
public struct SteerConnEvent: Codable, Equatable, Sendable {
    public struct Fields: Codable, Equatable, Sendable {
        public var phase: String
        public var platform: String
        public var version: String
        public var outcome: String
        public var path: String?
        public var firstFrameMs: Int?
        public var errorClass: String?
        public var fallbacks: Int?
        public var reconnects: Int?
        public var durationMs: Int?
        public var failures: [String: Int]?
    }

    public var v = 1
    public var id: String
    public var kind: String
    public var at: Int64
    public var source: String
    public var data: Fields
}

/// One viewer OPEN of a session feed (`steer.view`, source `steer-viewer`):
/// `settled` once the first frame is applied or `settleAfterMs` passed
/// without one, `ended` when the viewer closes. A view closed before it
/// settled (no frame, under `settleAfterMs`) records nothing.
public struct SteerViewStats: Sendable {
    public let platform: String
    public let version: String
    private let openedAt: Date
    private var firstFrameMs: Int?
    private var path: String?
    private var settled = false
    private var fallbacks = 0
    private var reconnects = 0
    private var failures: [String: Int] = [:]
    private var lastError: String?

    public init(platform: String = "ios", version: String, openedAt: Date = Date()) {
        self.platform = platform
        self.version = String(version.prefix(SteerConnStats.versionMax))
        self.openedAt = openedAt
    }

    /// The `path` of a carrier. eNet's is the node's `path_kind` for the
    /// joined device (#EXP-1531: `direct` | `iroh_relay`), `direct` when
    /// the node does not know.
    public static func path(_ carrier: SteerCarrier.Active, enetPath: String? = nil) -> String? {
        switch carrier {
        case .enet: enetPath == "iroh_relay" ? "iroh_relay" : "direct"
        case .relay: "server_fallback"
        case .none: nil
        }
    }

    /// A carrier became active. The first one is the first frame (and the
    /// settle); every later one is a feed restart.
    public mutating func active(_ carrier: SteerCarrier.Active, enetPath: String? = nil, at: Date = Date()) -> SteerConnEvent? {
        guard let path = Self.path(carrier, enetPath: enetPath) else { return nil }
        if firstFrameMs != nil {
            reconnects += 1
            return nil
        }
        self.path = path
        firstFrameMs = max(0, Int((at.timeIntervalSince(openedAt) * 1000).rounded()))
        return settle(at: at)
    }

    public mutating func fallback() { fallbacks += 1 }

    public mutating func failure(_ errorClass: String) {
        let cls = SteerConnStats.errorClasses.contains(errorClass) ? errorClass : "other"
        failures[cls, default: 0] += 1
        lastError = cls
    }

    /// `settleAfterMs` passed: a view with no frame settles `failed`.
    public mutating func settleTimeout(at: Date = Date()) -> SteerConnEvent? {
        settle(at: at)
    }

    private mutating func settle(at: Date) -> SteerConnEvent? {
        guard !settled else { return nil }
        settled = true
        var data = base(phase: "settled")
        if path == nil { data.errorClass = lastError ?? "other" }
        return event(data, at: at)
    }

    /// The viewer closed.
    public mutating func end(at: Date = Date()) -> [SteerConnEvent] {
        var out: [SteerConnEvent] = []
        if !settled {
            guard at.timeIntervalSince(openedAt) * 1000 >= Double(SteerConnStats.settleAfterMs) else { return [] }
            if let e = settle(at: at) { out.append(e) }
        }
        var data = base(phase: "ended")
        data.durationMs = max(0, Int((at.timeIntervalSince(openedAt) * 1000).rounded()))
        data.fallbacks = fallbacks
        data.reconnects = reconnects
        if !failures.isEmpty { data.failures = failures }
        if path == nil || !failures.isEmpty { data.errorClass = lastError }
        out.append(event(data, at: at))
        return out
    }

    private func base(phase: String) -> SteerConnEvent.Fields {
        SteerConnEvent.Fields(
            phase: phase,
            platform: platform,
            version: version,
            outcome: path == nil ? "failed" : "ok",
            path: path,
            firstFrameMs: firstFrameMs
        )
    }

    private func event(_ data: SteerConnEvent.Fields, at: Date) -> SteerConnEvent {
        SteerConnEvent(
            id: UUID().uuidString.lowercased(),
            kind: "steer.view",
            at: Int64(at.timeIntervalSince1970 * 1000),
            source: "steer-viewer",
            data: data
        )
    }
}

/// tRPC `connStats.record` (a fake in the tests).
public protocol SteerConnStatsSink: Sendable {
    /// The number of events the server accepted.
    func record(accountId: String, events: [SteerConnEvent]) async throws -> Int
}

private struct RecordInput: Encodable { let events: [SteerConnEvent] }
private struct RecordOutput: Decodable { let accepted: Int? }

public final class SteerConnStatsApi: SteerConnStatsSink {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func record(accountId: String, events: [SteerConnEvent]) async throws -> Int {
        let out: RecordOutput = try await trpc.mutation(accountId: accountId, path: "connStats.record", input: RecordInput(events: events))
        return out.accepted ?? 0
    }
}

/// The client buffer: events per account, flushed every `flushMs` and on
/// app background, in batches of at most `batchMax`. A failed post keeps
/// the batch for the next flush (bounded); nothing ever surfaces.
public final class SteerConnStatsRecorder: @unchecked Sendable {
    private static let bufferCap = 500
    private let sink: SteerConnStatsSink
    private let lock = NSLock()
    private var buffer: [String: [SteerConnEvent]] = [:]
    private var timer: Task<Void, Never>?
    private var flushing = false

    public init(sink: SteerConnStatsSink) {
        self.sink = sink
    }

    deinit { timer?.cancel() }

    public func record(accountId: String, _ events: [SteerConnEvent]) {
        guard !events.isEmpty else { return }
        lock.withLock {
            var list = buffer[accountId] ?? []
            list.append(contentsOf: events)
            if list.count > Self.bufferCap { list.removeFirst(list.count - Self.bufferCap) }
            buffer[accountId] = list
            if timer == nil {
                let ms = SteerConnStats.flushMs
                timer = Task.detached { [weak self] in
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .milliseconds(ms))
                        guard let self, !Task.isCancelled else { return }
                        await self.flush()
                    }
                }
            }
        }
    }

    public var pending: Int { lock.withLock { buffer.values.reduce(0) { $0 + $1.count } } }

    /// Post everything buffered (the timer, and app background).
    public func flush() async {
        let taken = lock.withLock { () -> [String: [SteerConnEvent]]? in
            guard !flushing else { return nil }
            flushing = true
            defer { buffer = [:] }
            return buffer
        }
        guard let taken else { return }
        var failed: [String: [SteerConnEvent]] = [:]
        for (accountId, events) in taken {
            var index = 0
            while index < events.count {
                let batch = Array(events[index ..< min(index + SteerConnStats.batchMax, events.count)])
                index += batch.count
                do {
                    _ = try await sink.record(accountId: accountId, events: batch)
                } catch {
                    logger.info("connStats.record failed: \(String(describing: error), privacy: .public)")
                    failed[accountId, default: []].append(contentsOf: batch)
                }
            }
        }
        lock.withLock {
            flushing = false
            for (accountId, events) in failed {
                var list = events + (buffer[accountId] ?? [])
                if list.count > Self.bufferCap { list.removeFirst(list.count - Self.bufferCap) }
                buffer[accountId] = list
            }
        }
    }
}
