import Foundation

// EXP-1459: the tree secrets this install holds, in memory (opening runs on
// every row render) over the persistent store the app plugs in (the shared
// keychain, ExpPush `TreeKeyStore`), plus the opener every view reads
// sealed content through.

/// The persistent side of the key ring (the keychain in the app).
public struct TreeKeyPersistence: Sendable {
    public let load: @Sendable () -> [String: [UInt32: Data]]
    public let save: @Sendable (_ rootId: String, _ epoch: UInt32, _ secret: Data) -> Void
    public let removeAll: @Sendable () -> Void

    public init(
        load: @escaping @Sendable () -> [String: [UInt32: Data]],
        save: @escaping @Sendable (String, UInt32, Data) -> Void,
        removeAll: @escaping @Sendable () -> Void
    ) {
        self.load = load
        self.save = save
        self.removeAll = removeAll
    }

    public static let memory = TreeKeyPersistence(load: { [:] }, save: { _, _, _ in }, removeAll: {})
}

extension Notification.Name {
    /// Posted (any thread) when the ring gained or dropped secrets: rows that
    /// showed the locked placeholder re-open.
    public static let sealedKeysChanged = Notification.Name("at.exponential.sealedKeysChanged")
}

public final class SealedKeyRing: @unchecked Sendable {
    /// `sealedKeysChanged` userInfo: the roots that gained a secret ([String]);
    /// absent = every root (the ring was cleared).
    public static let rootIdsKey = "rootIds"
    private let lock = NSLock()
    private let persistence: TreeKeyPersistence
    private var loaded = false
    private var keys: [String: [UInt32: Data]] = [:]
    private var _version = 0

    public init(persistence: TreeKeyPersistence) {
        self.persistence = persistence
    }

    /// Bumps whenever the ring changes (cache key for opened content).
    public var version: Int { lock.withLock { _version } }

    private func ensureLoaded() {
        guard !loaded else { return }
        loaded = true
        for (root, epochs) in persistence.load() { keys[root.lowercased()] = epochs }
    }

    public func keys(rootId: String) -> [UInt32: Data] {
        lock.withLock {
            ensureLoaded()
            return keys[rootId.lowercased()] ?? [:]
        }
    }

    public func holds(rootId: String, epoch: UInt32) -> Bool {
        keys(rootId: rootId)[epoch] != nil
    }

    /// Adds secrets; persists and notifies only what is new.
    public func add(_ entries: [(rootId: String, epoch: UInt32, secret: Data)]) {
        let fresh: [(String, UInt32, Data)] = lock.withLock {
            ensureLoaded()
            var out: [(String, UInt32, Data)] = []
            for entry in entries where entry.secret.count == 32 {
                let root = entry.rootId.lowercased()
                if keys[root]?[entry.epoch] == entry.secret { continue }
                keys[root, default: [:]][entry.epoch] = entry.secret
                out.append((root, entry.epoch, entry.secret))
            }
            if !out.isEmpty { _version += 1 }
            return out
        }
        guard !fresh.isEmpty else { return }
        for (root, epoch, secret) in fresh { persistence.save(root, epoch, secret) }
        NotificationCenter.default.post(
            name: .sealedKeysChanged,
            object: self,
            userInfo: [SealedKeyRing.rootIdsKey: Array(Set(fresh.map(\.0)))]
        )
    }

    /// Forgets every secret (the install identity rotated: the wraps were
    /// for the old key).
    public func removeAll() {
        lock.withLock {
            loaded = true
            keys = [:]
            _version += 1
        }
        persistence.removeAll()
        NotificationCenter.default.post(name: .sealedKeysChanged, object: self)
    }
}

/// Opens sealed bodies, cards and titles for display, memoized (an envelope
/// opens once per key-ring version). A row this install holds no key for
/// asks the holder for it (`onMissingKey`, throttled there) and shows the
/// locked placeholder until the wrap lands.
public final class SealedOpener: @unchecked Sendable {
    /// The app's opener (set once at launch by AppDependencies); the default
    /// opens plaintext only.
    nonisolated(unsafe) public static var shared = SealedOpener(
        crypto: UnavailableSealedCrypto(),
        ring: SealedKeyRing(persistence: .memory)
    )

    public let crypto: SealedCrypto
    public let ring: SealedKeyRing
    private let lock = NSLock()
    private var bodies: [String: OpenedText] = [:]
    private var cards: [String: OpenedCardJson] = [:]
    private var cacheVersion = -1
    private var missing: (@Sendable (String) -> Void)?

    public init(crypto: SealedCrypto, ring: SealedKeyRing) {
        self.crypto = crypto
        self.ring = ring
    }

    /// Called with a root id whose content this install can't open yet.
    public func onMissingKey(_ handler: @escaping @Sendable (String) -> Void) {
        lock.withLock { missing = handler }
    }

    private func cacheReset() {
        let version = ring.version
        if version != cacheVersion || bodies.count + cards.count > 4000 {
            bodies.removeAll(keepingCapacity: true)
            cards.removeAll(keepingCapacity: true)
            cacheVersion = version
        }
    }

    private func report(_ status: SealStatus, rootId: String?) {
        guard status == .noKey, let rootId, let handler = lock.withLock({ missing }) else { return }
        handler(rootId)
    }

    /// A body / sealed title / digest preview. Plaintext passes untouched.
    public func body(_ body: String, rootId: String?) -> OpenedText {
        guard SealedEnvelope.isSealedBody(body) else { return .plain(body) }
        let key = "\(rootId ?? "")|\(body)"
        if let hit = lock.withLock({ () -> OpenedText? in
            cacheReset()
            return bodies[key]
        }) { return hit }
        let opened = crypto.openBody(body, keys: rootId.map { ring.keys(rootId: $0) } ?? [:])
        lock.withLock { bodies[key] = opened }
        report(opened.status, rootId: rootId)
        return opened
    }

    /// The readable text of a body, or nil when it is locked.
    public func text(_ body: String?, rootId: String?) -> String? {
        guard let body else { return nil }
        return self.body(body, rootId: rootId).text
    }

    /// A `messages.card` JSON: the full card, or the clear part when locked.
    public func card(_ json: String, rootId: String?) -> OpenedCardJson {
        guard SealedEnvelope.mayBeSealedCard(json) else { return OpenedCardJson(status: .plain, epoch: nil, json: json) }
        let key = "\(rootId ?? "")|\(json)"
        if let hit = lock.withLock({ () -> OpenedCardJson? in
            cacheReset()
            return cards[key]
        }) { return hit }
        let opened = crypto.openCard(json, keys: rootId.map { ring.keys(rootId: $0) } ?? [:])
        lock.withLock { cards[key] = opened }
        report(opened.status, rootId: rootId)
        return opened
    }

    /// A session's display title: the opened `sealed_title`, else the clear
    /// `title` (a neutral fallback once titles are sealed).
    public func title(sealedTitle: String?, title: String?, rootId: String?) -> String? {
        if let sealedTitle, let text = body(sealedTitle, rootId: rootId).text, !text.isEmpty { return text }
        return title
    }
}

/// One message's content as the views read it (EXP-1459).
public struct SealedMessage {
    public let body: String
    public let status: SealStatus

    /// A row's body + card JSON TEXT: the opened body ("" when locked), the
    /// opened card JSON (or its clear part) and the worse of the two statuses.
    public static func open(body: String, card: String?, rootId: String?, opener: SealedOpener = .shared) -> (body: String, card: String?, status: SealStatus) {
        let text = opener.body(body, rootId: rootId)
        let opened = card.map { opener.card($0, rootId: rootId) }
        return (text.text ?? "", opened?.json ?? card, worse(text.status, opened?.status))
    }

    /// A digest entry (its card is already an object).
    public static func open(body: String, card: [String: Any]?, rootId: String?, opener: SealedOpener = .shared) -> (body: String, card: [String: Any]?, status: SealStatus) {
        guard let card, card["sealed"] != nil,
              let data = try? JSONSerialization.data(withJSONObject: card, options: [.sortedKeys]),
              let json = String(data: data, encoding: .utf8) else {
            let text = opener.body(body, rootId: rootId)
            return (text.text ?? "", card, text.status)
        }
        let opened = open(body: body, card: json, rootId: rootId, opener: opener)
        let object = opened.card.flatMap { (try? JSONSerialization.jsonObject(with: Data($0.utf8))) as? [String: Any] }
        return (opened.body, object, opened.status)
    }

    static func worse(_ a: SealStatus, _ b: SealStatus?) -> SealStatus {
        guard let b else { return a }
        return a.isReadable ? b : a
    }
}

extension SessionEntity {
    /// The display title: the opened `sealed_title`, else the clear `title`.
    public var openedTitle: String? {
        SealedOpener.shared.title(sealedTitle: sealedTitle, title: title, rootId: rootId)
    }

    /// The key a writer seals to, when this root row carries one.
    public var sealKey: TreeSealKey? {
        guard let sealPublicKey, let sealEpoch, sealEpoch > 0 else { return nil }
        return TreeSealKey(publicKey: sealPublicKey, epoch: UInt32(sealEpoch))
    }
}
