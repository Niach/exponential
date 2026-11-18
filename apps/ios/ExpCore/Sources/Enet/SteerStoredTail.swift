import Foundation

// #PERF-9 STORED TAIL (fixtures/steer-enet.json), READ ONLY since #EDE-114:
// the chat store (`ChatStoreHub`) replaced it. A tail an older build wrote
// (the newest activity frames a viewer applied, SEALED as one body envelope
// for a tree with a key, as is for a keyless one) is read into the store once
// and then deleted (chat-store.json S3); nothing writes one any more.

public enum SteerStoredTail {
    /// The JSON a tail is stored as (sealed or not): `{"frames":[…],"v":1}`,
    /// keys sorted: a Swift Dictionary's order changes per instance (#EXP-1853).
    public static func encode(_ frames: [String]) -> String {
        let body = (try? JSONSerialization.data(withJSONObject: ["v": 1, "frames": frames], options: [.sortedKeys]))
            .flatMap { String(data: $0, encoding: .utf8) }
        return body ?? "{\"frames\":[],\"v\":1}"
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

    public init(directory: URL) {
        self.directory = directory
    }

    /// Writes one session's stored body as an older build did (the tests'
    /// fixtures for the read-in).
    public func writeBody(accountId: String, sessionId: String, body: String) throws {
        let file = url(accountId: accountId, sessionId: sessionId)
        try queue.sync {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(body.utf8).write(to: file, options: [.atomic])
        }
    }

    /// One session's stored body, nil when there is none. Synchronous: run
    /// it off the main thread.
    public func read(accountId: String, sessionId: String) -> String? {
        let file = url(accountId: accountId, sessionId: sessionId)
        return queue.sync {
            (try? Data(contentsOf: file)).flatMap { String(data: $0, encoding: .utf8) }
        }
    }

    /// #EDE-114: the sessions of `accountId` with a tail on disk (each read
    /// into the chat store once, then deleted).
    public func sessionIds(accountId: String) -> [String] {
        let folder = folder(accountId: accountId)
        return queue.sync {
            ((try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? [])
                .filter { $0.hasSuffix(".tail") }
                .map { String($0.dropLast(5)) }
                .sorted()
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
