import Foundation
import os

// #PERF-9 STORED TAIL (fixtures/steer-enet.json): a viewer keeps, per session
// it opened, the newest `SteerEnet.storedTail` activity events its feed
// applied (the `activity` wire frames as they came, seqs inside). Opening the
// session paints them at once while discovery and the join run; the join's
// replay replaces them. A tree with a key (`sessions.seal_public_key`) stores
// the tail SEALED as one body envelope over its JSON, never in clear; a
// keyless tree stores it as is. A tail that does not open is not shown. Of the
// latest-wins slot kinds (`turn`, `usage`, `queue`, `rate_limit`, `diff`…) only
// the LATEST of each is kept: a working agent's `turn` every few seconds would
// push every row out.

private let logger = Logger(subsystem: "at.exponential", category: "SteerStoredTail")

public enum SteerStoredTail {
    /// The latest-wins slot kind of an event (Android `STEER_TAIL_SLOT_KINDS`);
    /// nil = a row event, kept among the newest `SteerEnet.storedTail`.
    public static func slotKind(_ event: AgentActivityEvent) -> String? {
        switch event {
        case .turn: "turn"
        case .usage: "usage"
        case .configState: "config_state"
        case .contextLayout: "context_layout"
        case .backgroundTasks: "background_tasks"
        case .taskList: "task_list"
        case .queue: "queue"
        case .rateLimit: "rate_limit"
        case .diff: "diff"
        default: nil
        }
    }

    /// What a viewer holds of its feed for the tail: the newest
    /// `SteerEnet.storedTail` row events plus the latest event of each slot
    /// kind, the wire frame texts in the order they were applied.
    public struct Ring: Sendable {
        private var rows: [(order: Int, text: String)] = []
        private var slots: [String: (order: Int, text: String)] = [:]
        private var next = 0

        public init() {}

        public var isEmpty: Bool { rows.isEmpty && slots.isEmpty }

        public mutating func append(_ text: String, slotKind: String?) {
            defer { next += 1 }
            if let slotKind {
                slots[slotKind] = (next, text)
                return
            }
            rows.append((next, text))
            if rows.count > 2 * SteerEnet.storedTail { rows.removeFirst(rows.count - SteerEnet.storedTail) }
        }

        /// The frames to store, in applied order.
        public var frames: [String] {
            (rows.suffix(SteerEnet.storedTail) + Array(slots.values))
                .sorted { $0.order < $1.order }
                .map(\.text)
        }
    }

    /// The JSON a tail is stored as (sealed or not): `{"v":1,"frames":[…]}`.
    public static func encode(_ frames: [String]) -> String {
        let body = (try? JSONSerialization.data(withJSONObject: ["v": 1, "frames": frames]))
            .flatMap { String(data: $0, encoding: .utf8) }
        return body ?? "{\"v\":1,\"frames\":[]}"
    }

    /// The frames of a tail's JSON; nil = malformed.
    public static func decode(_ text: String) -> [String]? {
        guard let object = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any],
              object["v"] as? Int == 1,
              let frames = object["frames"] as? [String] else { return nil }
        return frames
    }

    /// What goes on disk for these frames: a sealed body with the tree's
    /// current key, the plain JSON for a keyless tree; nil = nothing may be
    /// stored (a keyed tree this build can't seal for).
    public static func body(_ frames: [String], key: TreeSealKey?, crypto: SealedCrypto) -> String? {
        let json = encode(frames)
        guard let key else { return json }
        guard let sealed = try? crypto.sealBody(treePublicKey: key.publicKey, epoch: key.epoch, text: json),
              SealedEnvelope.isSealedBody(sealed) else { return nil }
        return sealed
    }

    /// The frames of a stored body; nil = it is not shown (it does not open
    /// here, it is malformed, or a keyed tree's tail is in clear).
    public static func open(_ body: String, keyed: Bool, keys: [UInt32: Data], crypto: SealedCrypto) -> [String]? {
        if SealedEnvelope.isSealedBody(body) {
            let opened = crypto.openBody(body, keys: keys)
            guard opened.status == .opened, let text = opened.text else { return nil }
            return decode(text)
        }
        guard !keyed else { return nil }
        return decode(body)
    }
}

/// The tails on disk: `<directory>/<account>/<sessionId>.tail`. Every file
/// operation runs on one serial utility queue; `read` is the one synchronous
/// call (callers run it off the main thread).
public final class SteerStoredTailStore: @unchecked Sendable {
    public static let shared = SteerStoredTailStore(directory: defaultDirectory)

    public static var defaultDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return base.appendingPathComponent("Exponential/feed-tails", isDirectory: true)
    }

    private let directory: URL
    private let queue = DispatchQueue(label: "at.exponential.stored-tail", qos: .utility)
    private let lock = NSLock()
    /// Bumped per account by `removeAccounts`: a write already on its way
    /// for a signed-out account lands nowhere.
    private var generations: [String: Int] = [:]

    public func generation(accountId: String) -> Int {
        lock.withLock { generations[accountId] ?? 0 }
    }

    public init(directory: URL) {
        self.directory = directory
    }

    /// Seal (when keyed) and write one session's tail, off the caller's
    /// thread. A keyed tree that can't be sealed for drops its old file
    /// instead: nothing is ever written in clear for it.
    public func write(
        accountId: String, sessionId: String, frames: [String], key: TreeSealKey?, crypto: SealedCrypto, generation: Int
    ) {
        let file = url(accountId: accountId, sessionId: sessionId)
        queue.async { [directory] in
            guard self.generation(accountId: accountId) == generation else { return }
            guard !frames.isEmpty, let body = SteerStoredTail.body(frames, key: key, crypto: crypto) else {
                try? FileManager.default.removeItem(at: file)
                return
            }
            do {
                let folder = file.deletingLastPathComponent()
                if !FileManager.default.fileExists(atPath: folder.path) {
                    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                    var root = directory
                    var values = URLResourceValues()
                    values.isExcludedFromBackup = true
                    try? root.setResourceValues(values)
                }
                try Data(body.utf8).write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            } catch {
                logger.error("tail write failed: \(error.localizedDescription, privacy: .public)")
            }
            Self.prune(file.deletingLastPathComponent(), now: Date())
        }
    }

    /// PRUNING (LRU per account): a tail's last use is its file's
    /// modification date (stamped when the session opens, and by every
    /// write). One not opened within `storedTailMaxAgeDays` goes, and beyond
    /// the `storedTailMaxSessions` most recently used only those stay.
    static func prune(_ folder: URL, now: Date) {
        let fm = FileManager.default
        guard let files = try? fm.contentsOfDirectory(
            at: folder, includingPropertiesForKeys: [.contentModificationDateKey], options: [.skipsHiddenFiles]
        ) else { return }
        let maxAge = TimeInterval(SteerEnet.storedTailMaxAgeDays) * 86_400
        let used = files
            .filter { $0.pathExtension == "tail" }
            .map { ($0, (try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast) }
            .sorted { $0.1 > $1.1 }
        for (index, (file, at)) in used.enumerated()
        where index >= SteerEnet.storedTailMaxSessions || now.timeIntervalSince(at) > maxAge {
            try? fm.removeItem(at: file)
        }
    }

    /// One session's stored body, nil when there is none. Synchronous: run
    /// it off the main thread. Waits for the writes queued before it.
    public func read(accountId: String, sessionId: String) -> String? {
        let file = url(accountId: accountId, sessionId: sessionId)
        return queue.sync {
            guard let body = (try? Data(contentsOf: file)).flatMap({ String(data: $0, encoding: .utf8) }) else { return nil }
            // Opening the session stamps its tail's last use (PRUNING).
            try? FileManager.default.setAttributes([.modificationDate: Date()], ofItemAtPath: file.path)
            return body
        }
    }

    /// Forget one session's tail (it did not open).
    public func remove(accountId: String, sessionId: String) {
        let file = url(accountId: accountId, sessionId: sessionId)
        queue.async { try? FileManager.default.removeItem(at: file) }
    }

    /// Sign-out: every tail of these accounts goes.
    public func removeAccounts(_ accountIds: Set<String>) {
        guard !accountIds.isEmpty else { return }
        lock.withLock {
            for accountId in accountIds { generations[accountId, default: 0] += 1 }
        }
        let folders = accountIds.map(folder(accountId:))
        queue.async {
            for folder in folders { try? FileManager.default.removeItem(at: folder) }
        }
    }

    private func folder(accountId: String) -> URL {
        directory.appendingPathComponent(Self.safe(accountId), isDirectory: true)
    }

    private func url(accountId: String, sessionId: String) -> URL {
        folder(accountId: accountId).appendingPathComponent("\(Self.safe(sessionId)).tail")
    }

    private static func safe(_ name: String) -> String {
        String(name.map { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "." || $0 == "_" ? $0 : "_" })
    }
}
