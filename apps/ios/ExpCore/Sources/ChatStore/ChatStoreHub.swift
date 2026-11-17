import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "ChatStore")

/// The eNet side the chat store needs (the install node; a fake in the tests).
public protocol ChatStoreNode: AnyObject, Sendable {
    var steerInbox: SteerEnetInbox { get }
    func endpointId() async -> String?
    func sendChat(peer: String, body: Data) async throws
    func registry(accountId: String) async -> [String: [InstallEntry]]
}

extension InstallNodeCoordinator: ChatStoreNode {}

/// #EDE-114 (chat-store.json S3 = phones): every signed-in account's CHAT
/// STORE, `{dir}/accounts/<account>/chat-store.sqlite`, `dir` =
/// Application Support/ChatStore in the app container (Data Protection
/// complete until first unlock, excluded from backup, shared with no
/// extension). Fed by what the phone receives: the steer feeds a session
/// view applies (`recordFeed`), the synced messages opened with the tree key
/// (a sealed one stored without a body and opened again once its key
/// arrives), the catch-up pages. A session view OPENS from it (`openTail`),
/// older pages come from it first (`page`). Every write runs on ONE serial
/// utility queue; reads run on the caller's (never the main) thread.
public final class ChatStoreHub: @unchecked Sendable {
    public static let shared = ChatStoreHub(factory: .linked, directory: defaultDirectory)

    public static var defaultDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return base.appendingPathComponent("ChatStore", isDirectory: true)
    }

    /// How long feed events wait to be written together.
    static let feedDebounce: DispatchTimeInterval = .milliseconds(500)
    /// How long the synced messages may sit before the store takes them in.
    static let messagesDebounce: DispatchTimeInterval = .seconds(3)
    /// The still-sealed messages are tried again no more often (and on
    /// every new key).
    static let lockedRetry: TimeInterval = 300
    static let lockedChunk = 500
    static let messagesChunk = 1000
    static let messagesMark = "messages.createdAt"
    /// How long a sign-out waits for the store's holders to let go, and how
    /// often it tries the delete.
    var closeWait: TimeInterval = 10
    static let wipeTries = 5

    public let factory: ChatStoreFactory?
    public let directory: URL
    let defaults: UserDefaults
    let tails: SteerStoredTailStore
    let now: @Sendable () -> Int64
    /// The ONE writer.
    let queue = DispatchQueue(label: "at.exponential.chat-store", qos: .utility)
    let lock = NSLock()
    /// Opened stores (opening runs under `openLock`, never twice).
    var stores: [String: ChatStoreBackend] = [:]
    let openLock = NSLock()
    /// Bumped per account by a sign-out or a close: a write already on its
    /// way for that account lands nowhere.
    var generations: [String: Int] = [:]
    /// The accounts the store serves (signed in), with their user id.
    var accounts: [String: String] = [:]
    /// Signed out deliberately, still listed until their token goes: never
    /// reopened meanwhile.
    var signingOut: Set<String> = []
    var pendingFeeds: [FeedKey: [String]] = [:]
    var feedFlushScheduled = false
    var messageObservers: [String: AnyDatabaseCancellable] = [:]
    var messagesScheduled: Set<String> = []
    var lockedTriedAt: [String: Date] = [:]
    var keysObserver: NSObjectProtocol?
    var db: DatabaseManager?
    var opener: SealedOpener = .shared
    // The catch-up (ChatStoreHub+CatchUp.swift).
    var node: ChatStoreNode?
    var loops: [String: ChatCatchUpLoop] = [:]
    var sessionObservers: [String: AnyDatabaseCancellable] = [:]
    var foreground = false
    var unmetered = false
    var schedulerTask: Task<Void, Never>?
    var lastKickAt: [String: Date] = [:]
    /// Called on the main actor when a catch-up stored events of a chat
    /// (rootId): an open view of it may refresh.
    public var onStored: (@Sendable (_ accountId: String, _ rootId: String) -> Void)?

    struct FeedKey: Hashable {
        let accountId: String
        let rootId: String
        let sessionId: String
        let deviceId: String
    }

    public init(
        factory: ChatStoreFactory?,
        directory: URL,
        defaults: UserDefaults = .standard,
        tails: SteerStoredTailStore = .shared,
        now: @escaping @Sendable () -> Int64 = ChatCatchUpLoop.nowMs
    ) {
        self.factory = factory
        self.directory = directory
        self.defaults = defaults
        self.tails = tails
        self.now = now
    }

    /// Whether this build has a store at all.
    public var isAvailable: Bool { factory != nil }

    /// The synced databases and the opener (AppDependencies, at launch).
    public func configure(db: DatabaseManager, opener: SealedOpener = .shared) {
        lock.withLock {
            self.db = db
            self.opener = opener
        }
        guard keysObserver == nil else { return }
        keysObserver = NotificationCenter.default.addObserver(forName: .sealedKeysChanged, object: nil, queue: nil) { [weak self] _ in
            self?.keysChanged()
        }
    }

    // MARK: - Accounts

    /// The signed-in accounts (accountId → userId). A new one's store opens
    /// off the main thread (retention, the old tails read in); one no longer
    /// signed in is CLOSED and its files stay (an expiry, a switch: SIGN-OUT
    /// `expired`). A deliberate sign-out calls `signOut` first.
    public func sync(accounts next: [String: String]) {
        guard factory != nil else { return }
        let (added, removed) = lock.withLock { () -> ([String], [String]) in
            signingOut.formIntersection(next.keys)
            let next = next.filter { !signingOut.contains($0.key) }
            let added = next.keys.filter { accounts[$0] == nil }
            let removed = accounts.keys.filter { next[$0] == nil }
            accounts = next
            return (added, removed)
        }
        for accountId in removed { close(accountId: accountId) }
        for accountId in added {
            queue.async { [self] in
                guard store(accountId: accountId) != nil else { return }
                migrateTails(accountId: accountId)
                startMessages(accountId: accountId)
                scheduleMessages(accountId: accountId, after: .seconds(0))
                startCatchUp(accountId: accountId)
            }
        }
    }

    /// The account's store, opened on first use (blocking: off main). nil =
    /// no store (no library, not signed in, the file failed to open).
    func store(accountId: String) -> ChatStoreBackend? {
        if let open = lock.withLock({ stores[accountId] }) { return open }
        guard let factory, lock.withLock({ accounts[accountId] != nil }) else { return nil }
        return openLock.withLock {
            if let open = lock.withLock({ stores[accountId] }) { return open }
            do {
                try prepare(accountId: accountId)
                let store = try factory.open(directory, accountId)
                _ = try? store.applyRetention(nowMs: now(), windowDays: KeepToolSteps.windowDays(defaults))
                let kept = lock.withLock { () -> Bool in
                    guard accounts[accountId] != nil else { return false }
                    stores[accountId] = store
                    return true
                }
                return kept ? store : nil
            } catch {
                logger.error("chat store open failed: \(String(describing: error), privacy: .public)")
                return nil
            }
        }
    }

    /// The protected directory: Data Protection complete until first unlock
    /// (the database and its WAL/SHM inherit it), excluded from backup.
    func prepare(accountId: String) throws {
        let fm = FileManager.default
        let folder = directory.appendingPathComponent("accounts", isDirectory: true)
            .appendingPathComponent(accountId, isDirectory: true)
        for dir in [directory, directory.appendingPathComponent("accounts", isDirectory: true), folder] {
            if !fm.fileExists(atPath: dir.path) {
                try fm.createDirectory(at: dir, withIntermediateDirectories: true, attributes: Self.protection)
            }
            try? fm.setAttributes(Self.protection, ofItemAtPath: dir.path)
        }
        var root = directory
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? root.setResourceValues(values)
    }

    static var protection: [FileAttributeKey: Any] {
        #if os(iOS)
        [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication]
        #else
        [:]
        #endif
    }

    /// The generation a writer captures (a sign-out since drops its writes).
    public func generation(accountId: String) -> Int {
        lock.withLock { generations[accountId] ?? 0 }
    }

    /// An expiry or an account switch: nothing more is written; the files
    /// stay for the next sign-in.
    public func close(accountId: String) {
        let (loop, messages, sessions) = lock.withLock { () -> (ChatCatchUpLoop?, AnyDatabaseCancellable?, AnyDatabaseCancellable?) in
            generations[accountId, default: 0] += 1
            accounts.removeValue(forKey: accountId)
            stores.removeValue(forKey: accountId)
            pendingFeeds = pendingFeeds.filter { $0.key.accountId != accountId }
            lockedTriedAt.removeValue(forKey: accountId)
            return (loops.removeValue(forKey: accountId), messageObservers.removeValue(forKey: accountId),
                    sessionObservers.removeValue(forKey: accountId))
        }
        loop?.stop()
        messages?.cancel()
        sessions?.cancel()
    }

    /// #EDE-120 SIGN-OUT (`signOut`, a DELIBERATE sign-out only): stops the
    /// walk and the writer, closes the store, then deletes the account's
    /// store files (never another file; tried again a few times, never
    /// awaited by the sign-out). Its old stored tails go too.
    public func signOut(accountId: String) {
        let held = lock.withLock { () -> WeakStore? in
            signingOut.insert(accountId)
            return stores[accountId].map { WeakStore($0) }
        }
        close(accountId: accountId)
        tails.removeAccounts([accountId])
        guard let factory else { return }
        let directory = directory
        let closeWait = closeWait
        queue.async {
            let deadline = Date().addingTimeInterval(closeWait)
            while held?.store != nil, Date() < deadline { Thread.sleep(forTimeInterval: 0.05) }
            for attempt in 1...Self.wipeTries {
                do {
                    try factory.deleteAccount(directory, accountId)
                    return
                } catch where attempt < Self.wipeTries {
                    Thread.sleep(forTimeInterval: 0.5 * Double(attempt))
                } catch {
                    logger.error("deleting the signed-out chat store failed: \(String(describing: error), privacy: .public)")
                }
            }
        }
    }

    /// Runs `body` after every write queued before it (tests, the sign-out).
    public func drain() {
        queue.sync {}
    }

    // MARK: - Feeds

    /// What a session view received from `sessionId`'s feed (`activity`
    /// frames, `history_chunk` pages), as the device the session is on
    /// (`deviceId`) sent them. Batched and written off the main thread.
    public func recordFeed(
        accountId: String, rootId: String, sessionId: String, deviceId: String, frames: [String], generation: Int
    ) {
        guard factory != nil, !frames.isEmpty, !deviceId.isEmpty else { return }
        let schedule = lock.withLock { () -> Bool in
            guard (generations[accountId] ?? 0) == generation, accounts[accountId] != nil else { return false }
            pendingFeeds[FeedKey(accountId: accountId, rootId: rootId, sessionId: sessionId, deviceId: deviceId), default: []]
                .append(contentsOf: frames)
            defer { feedFlushScheduled = true }
            return !feedFlushScheduled
        }
        guard schedule else { return }
        queue.asyncAfter(deadline: .now() + Self.feedDebounce) { [self] in flushFeeds() }
    }

    /// Writes every batched feed now (on the writer queue).
    func flushFeeds() {
        let batch = lock.withLock { () -> [FeedKey: [String]] in
            defer {
                pendingFeeds = [:]
                feedFlushScheduled = false
            }
            return pendingFeeds
        }
        for (key, frames) in batch {
            guard let json = ChatStoreWire.feedJson(frames), let store = store(accountId: key.accountId) else { continue }
            do {
                _ = try store.putFeed(rootId: key.rootId, sessionId: key.sessionId, deviceId: key.deviceId, eventsJson: json, nowMs: now())
            } catch {
                logger.error("storing a feed failed: \(String(describing: error), privacy: .public)")
            }
        }
    }

    // MARK: - Reads

    /// OPEN (blocking, off main): the session's stored events on `deviceId`
    /// through the REPLAY TAIL, as `activity` frame texts; an old stored
    /// tail of the session is read in first (then deleted).
    public func openTail(accountId: String, sessionId: String, deviceId: String) -> (frames: [String], truncated: Bool)? {
        guard !deviceId.isEmpty, let store = store(accountId: accountId) else { return nil }
        migrateTail(accountId: accountId, sessionId: sessionId, store: store)
        guard let json = try? store.openTail(sessionId: sessionId, deviceId: deviceId, tail: UInt32(AgentFeed.replayTail)) else {
            return nil
        }
        return ChatStoreWire.tailFrames(json)
    }

    /// An older page off the store (blocking, off main): the newest `limit`
    /// events below `beforeSeq` as one `history_chunk` text; nil = none.
    public func page(accountId: String, sessionId: String, deviceId: String, beforeSeq: Int, limit: Int) -> String? {
        guard beforeSeq > 0, !deviceId.isEmpty, let store = store(accountId: accountId),
              let json = try? store.page(sessionId: sessionId, deviceId: deviceId, beforeSeq: UInt64(beforeSeq), limit: UInt32(limit))
        else { return nil }
        return ChatStoreWire.pageChunk(json)
    }

    /// FIND IN CHAT off the store (blocking, off main): `{hits, total}`.
    public func searchChat(accountId: String, rootId: String, query: String, limit: Int) -> String? {
        try? store(accountId: accountId)?.searchChat(rootId: rootId, query: query, limit: UInt32(limit))
    }

    // MARK: - The storage setting

    /// `Keep tool steps` moved: stored, the walks take it, and every open
    /// store applies it at once.
    public func setWindowDays(_ days: UInt32?) {
        KeepToolSteps.setWindowDays(days, defaults)
        let (open, walks) = lock.withLock { (Array(stores.values), Array(loops.values)) }
        for loop in walks { loop.walker.setWindowDays(days) }
        let now = now()
        queue.async {
            for store in open { _ = try? store.applyRetention(nowMs: now, windowDays: days) }
        }
    }

    // MARK: - The old stored tails (#PERF-9)

    /// Reads in every old tail of the account once (then deletes it).
    func migrateTails(accountId: String) {
        guard let store = store(accountId: accountId) else { return }
        for sessionId in tails.sessionIds(accountId: accountId) {
            migrateTail(accountId: accountId, sessionId: sessionId, store: store)
        }
    }

    /// One session's old tail into the store: opened as the tail always was
    /// (a keyed tree's is sealed), stored as the feed of the device the
    /// session is on, then deleted. One that does not open YET (its key is
    /// not here) waits for the next open; a malformed one, or one of a
    /// session that is gone, is deleted.
    func migrateTail(accountId: String, sessionId: String, store: ChatStoreBackend) {
        guard let body = tails.read(accountId: accountId, sessionId: sessionId) else { return }
        let (db, opener) = lock.withLock { (self.db, self.opener) }
        guard let pool = try? db?.pool(forAccountId: accountId) else { return }
        guard let row = (try? pool.read({ try SessionEntity.fetchOne($0, key: sessionId) })) ?? nil else {
            tails.remove(accountId: accountId, sessionId: sessionId)
            return
        }
        let rootId = row.rootId.isEmpty ? row.id : row.rootId
        let root = rootId == row.id ? row : (try? pool.read { try SessionEntity.fetchOne($0, key: rootId) }) ?? nil
        let key = root?.sealKey
        let keyed = root == nil || key != nil
        let keys = opener.ring.keys(rootId: rootId)
        guard let frames = SteerStoredTail.open(body, keyed: keyed, keys: keys, crypto: opener.crypto) else {
            if keyed, SealedEnvelope.isSealedBody(body), keys.isEmpty { return }
            tails.remove(accountId: accountId, sessionId: sessionId)
            return
        }
        if let json = ChatStoreWire.feedJson(frames), !row.deviceId.isEmpty {
            do {
                _ = try store.putFeed(rootId: rootId, sessionId: sessionId, deviceId: row.deviceId, eventsJson: json, nowMs: now())
            } catch {
                return
            }
        }
        tails.remove(accountId: accountId, sessionId: sessionId)
    }
}

/// A store a sign-out waits on (its holders let go, then the files go).
final class WeakStore: @unchecked Sendable {
    weak var store: AnyObject?
    init(_ store: ChatStoreBackend) { self.store = store }
}
