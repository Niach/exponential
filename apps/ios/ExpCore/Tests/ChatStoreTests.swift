@testable import ExpCore
import GRDB
import XCTest

// #EDE-114 (chat-store.json S3 = phones): the iOS wiring of the chat store:
// the wire shapes, the storage setting, the hub (feeds in, OPEN out, the old
// tails read in once, retention, sign-out vs expiry), the catch-up chunk
// routing and the walk's driver. Over fakes everywhere (`swift test`, no
// library); over the real store (`chat-store-ffi`) when ExpCore links it.

/// An in-memory store with the facade's semantics the wiring relies on.
final class FakeChatStore: ChatStoreBackend, @unchecked Sendable {
    let lock = NSLock()
    var events: [String: [UInt64: String]] = [:]
    var messages: [String: [String: Any]] = [:]
    var metas: [String: String] = [:]
    var retentions: [UInt32?] = []
    var answered: [(String, ChatStorePageAccess)] = []

    private func key(_ session: String, _ device: String) -> String { "\(session)|\(device)" }

    func putFeed(rootId: String, sessionId: String, deviceId: String, eventsJson: String, nowMs: Int64) throws -> UInt32 {
        let rows = try JSONSerialization.jsonObject(with: Data(eventsJson.utf8)) as? [[String: Any]] ?? []
        return lock.withLock {
            var added: UInt32 = 0
            for row in rows {
                guard let seq = (row["seq"] as? NSNumber)?.uint64Value, let event = row["event"],
                      let data = try? JSONSerialization.data(withJSONObject: event, options: [.sortedKeys]) else { continue }
                if events[key(sessionId, deviceId), default: [:]][seq] == nil {
                    events[key(sessionId, deviceId), default: [:]][seq] = String(decoding: data, as: UTF8.self)
                    added += 1
                }
            }
            return added
        }
    }

    func putMessages(messagesJson: String) throws {
        let rows = try JSONSerialization.jsonObject(with: Data(messagesJson.utf8)) as? [[String: Any]] ?? []
        lock.withLock { for row in rows { messages[row["id"] as? String ?? ""] = row } }
    }

    func lockedMessageIds() throws -> [String] {
        lock.withLock { messages.values.filter { $0["body"] is NSNull }.compactMap { $0["id"] as? String }.sorted() }
    }

    private func rows(_ session: String, _ device: String, below: UInt64?, limit: Int?) -> [(UInt64, String)] {
        var held = (events[key(session, device)] ?? [:]).sorted { $0.key < $1.key }.filter { below == nil || $0.key < below! }
        if let limit { held = Array(held.suffix(limit)) }
        return held.map { ($0.key, $0.value) }
    }

    func openTail(sessionId: String, deviceId: String, tail: UInt32) throws -> String {
        let held = lock.withLock { rows(sessionId, deviceId, below: nil, limit: Int(tail)) }
        let truncated = lock.withLock { (events[key(sessionId, deviceId)]?.count ?? 0) > Int(tail) }
        let list = held.map { "{\"seq\":\($0.0),\"event\":\($0.1)}" }.joined(separator: ",")
        return "{\"events\":[\(list)],\"truncated\":\(truncated)}"
    }

    func page(sessionId: String, deviceId: String, beforeSeq: UInt64, limit: UInt32) throws -> String {
        let held = lock.withLock { rows(sessionId, deviceId, below: beforeSeq, limit: Int(limit)) }
        return "{\"events\":[\(held.map(\.1).joined(separator: ","))],\"seqs\":[\(held.map { String($0.0) }.joined(separator: ","))]}"
    }

    func count(sessionId: String, deviceId: String) throws -> UInt64 {
        lock.withLock { UInt64(events[key(sessionId, deviceId)]?.count ?? 0) }
    }

    func searchChat(rootId: String, query: String, limit: UInt32) throws -> String { "{\"hits\":[],\"total\":0}" }
    func searchAll(query: String, limit: UInt32) throws -> String { "{\"chats\":[],\"total\":0}" }

    func applyRetention(nowMs: Int64, windowDays: UInt32?) throws -> UInt64 {
        lock.withLock { retentions.append(windowDays) }
        return 0
    }

    func meta(key: String) throws -> String? { lock.withLock { metas[key] } }
    func setMeta(key: String, value: String) throws { lock.withLock { metas[key] = value } }

    func answerPage(sessionId: String, pageJson: String, access: ChatStorePageAccess) throws -> String? {
        lock.withLock { answered.append((sessionId, access)) }
        let requestId = ChatStoreWire.typeAndRequest(pageJson).requestId ?? ""
        return "{\"t\":\"history_chunk\",\"requestId\":\"\(requestId)\",\"events\":[],\"seqs\":[],\"done\":true}"
    }
}

/// A walk of scripted asks (one per session) that records the answers.
final class FakeWalker: ChatCatchUpWalker, @unchecked Sendable {
    let lock = NSLock()
    var queue: [String] = []
    var asked: Set<String> = []
    var answers: [String: String?] = [:]
    var windowDays: UInt32?? = .none
    var installs: [ChatCatchUpInstall] = []
    var timeoutMs: UInt64 = 2_000

    func setInstalls(selfEndpoint: String, installs: [ChatCatchUpInstall]) { lock.withLock { self.installs = installs } }
    func setWindowDays(_ windowDays: UInt32?) { lock.withLock { self.windowDays = .some(windowDays) } }
    func beginPass(sessions: [ChatCatchUpSession], nowMs: Int64) -> UInt32 {
        lock.withLock {
            queue = sessions.map(\.sessionId)
            return UInt32(queue.count)
        }
    }
    func nextWalk(nowMs: Int64) -> String? { lock.withLock { queue.isEmpty ? nil : queue.removeFirst() } }
    func nextAsk(sessionId: String, nowMs: Int64) -> ChatCatchUpAsk? {
        lock.withLock {
            guard asked.insert(sessionId).inserted else { return nil }
            return ChatCatchUpAsk(
                requestId: "cu-\(sessionId)", sessionId: sessionId, peer: "peer-\(sessionId)",
                frame: "{\"t\":\"history_page\",\"requestId\":\"cu-\(sessionId)\",\"deviceId\":\"dev\",\"limit\":200,\"have\":[]}",
                waitMs: 0
            )
        }
    }
    func takeAnswer(requestId: String, chunkJson: String?, nowMs: Int64) -> ChatCatchUpTaken? {
        lock.withLock { answers[requestId] = .some(chunkJson) }
        return ChatCatchUpTaken(sessionId: String(requestId.dropFirst(3)), rootId: "r", added: chunkJson == nil ? 0 : 1)
    }
    func stop() { lock.withLock { queue = [] } }
    var concurrency: Int { 2 }
    var askTimeoutMs: UInt64 { timeoutMs }
    var passIntervalMs: UInt64 { 300_000 }
}

/// The node: an inbox, an endpoint, a registry, and what was sent.
final class FakeChatStoreNode: ChatStoreNode, @unchecked Sendable {
    let steerInbox = SteerEnetInbox()
    let lock = NSLock()
    var sent: [(peer: String, body: Data)] = []
    var lists: [String: [String: [InstallEntry]]] = [:]
    var onSend: (@Sendable (String, Data) -> Void)?

    func endpointId() async -> String? { "self-endpoint" }
    func sendChat(peer: String, body: Data) async throws {
        let hook = lock.withLock { () -> (@Sendable (String, Data) -> Void)? in
            sent.append((peer, body))
            return onSend
        }
        hook?(peer, body)
    }
    func registry(accountId: String) async -> [String: [InstallEntry]] { lock.withLock { lists[accountId] ?? [:] } }
}

final class ChatStoreTests: XCTestCase {
    private var dirs: [URL] = []
    private var accounts: [(DatabaseManager, String)] = []

    override func tearDown() {
        for dir in dirs { try? FileManager.default.removeItem(at: dir) }
        for (manager, id) in accounts {
            manager.closePool(forAccountId: id)
            DatabaseManager.deleteFiles(forAccountId: id)
        }
        super.tearDown()
    }

    private func tempDir() -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("chat-store-\(UUID().uuidString)")
        dirs.append(dir)
        return dir
    }

    private func defaults() -> UserDefaults {
        let name = "chat-store-tests-\(UUID().uuidString)"
        return UserDefaults(suiteName: name)!
    }

    private func activity(_ seq: Int, _ text: String) -> String {
        "{\"t\":\"activity\",\"seq\":\(seq),\"event\":{\"kind\":\"narration\",\"text\":\"\(text)\"}}"
    }

    /// A hub over fakes: the opened stores and the deleted accounts.
    private final class FakeFactory: @unchecked Sendable {
        let lock = NSLock()
        var stores: [String: FakeChatStore] = [:]
        var deleted: [String] = []
        var walkers: [FakeWalker] = []

        var factory: ChatStoreFactory {
            ChatStoreFactory(
                open: { _, accountId in
                    self.lock.withLock {
                        if let store = self.stores[accountId] { return store }
                        let store = FakeChatStore()
                        self.stores[accountId] = store
                        return store
                    }
                },
                catchUp: { _, _, _, _ in
                    self.lock.withLock {
                        let walker = FakeWalker()
                        self.walkers.append(walker)
                        return walker
                    }
                },
                deleteAccount: { _, accountId in
                    self.lock.withLock {
                        self.deleted.append(accountId)
                        self.stores.removeValue(forKey: accountId)
                    }
                }
            )
        }
    }

    private func hub(_ fake: FakeFactory, db: DatabaseManager? = nil, tails: SteerStoredTailStore? = nil, defaults: UserDefaults? = nil) -> ChatStoreHub {
        let hub = ChatStoreHub(
            factory: fake.factory, directory: tempDir(), defaults: defaults ?? self.defaults(),
            tails: tails ?? SteerStoredTailStore(directory: tempDir())
        )
        if let db { hub.configure(db: db, opener: SealedOpener(crypto: TailFakeCrypto(), ring: SealedKeyRing(persistence: .memory))) }
        return hub
    }

    // MARK: - Wire

    func testFeedEventsComeFromActivityFramesAndPages() throws {
        let chunk = "{\"t\":\"history_chunk\",\"requestId\":\"p1\",\"events\":[{\"kind\":\"narration\",\"text\":\"a\"},{\"kind\":\"narration\",\"text\":\"b\"}],\"seqs\":[3,4],\"done\":true}"
        let events = ChatStoreWire.feedEvents([activity(7, "x"), chunk, "{\"t\":\"keepalive\"}", "{\"t\":\"activity\",\"event\":{\"kind\":\"turn\"}}"])
        XCTAssertEqual(events.compactMap { ($0["seq"] as? NSNumber)?.intValue }, [7, 3, 4])
        XCTAssertNil(ChatStoreWire.feedJson(["{\"t\":\"keepalive\"}"]))
    }

    func testOpenTailAndPagesReadAsFeedFrames() throws {
        let (frames, truncated) = ChatStoreWire.tailFrames(
            "{\"events\":[{\"seq\":1,\"event\":{\"kind\":\"narration\",\"text\":\"hi\"}}],\"truncated\":true}"
        )
        XCTAssertTrue(truncated)
        let frame = try XCTUnwrap(SteerWireFrame.parse(try XCTUnwrap(frames.first)))
        XCTAssertEqual(frame.t, "activity")
        XCTAssertEqual(frame.seq, 1)
        XCTAssertNotNil(frame.activity.event)
        let page = try XCTUnwrap(ChatStoreWire.pageChunk("{\"events\":[{\"kind\":\"narration\",\"text\":\"old\"}],\"seqs\":[0]}"))
        let parsed = try XCTUnwrap(SteerWireFrame.parse(page))
        XCTAssertEqual(parsed.t, "history_chunk")
        XCTAssertEqual(parsed.pageSeqs, [0])
        XCTAssertEqual(parsed.page.count, 1)
        XCTAssertNil(ChatStoreWire.pageChunk("{\"events\":[],\"seqs\":[]}"))
    }

    func testACatchUpPageNamesADevice() {
        XCTAssertTrue(ChatStoreWire.isCatchUpPage("{\"t\":\"history_page\",\"requestId\":\"cu-1\",\"deviceId\":\"d\",\"limit\":200,\"have\":[]}"))
        XCTAssertFalse(ChatStoreWire.isCatchUpPage("{\"t\":\"history_page\",\"requestId\":\"p1\",\"beforeSeq\":9,\"limit\":200}"))
    }

    // MARK: - The storage setting

    func testKeepToolStepsMapsTheOptions() {
        let defaults = defaults()
        XCTAssertEqual(KeepToolSteps.label, "Keep tool steps")
        XCTAssertEqual(KeepToolSteps.options.map(KeepToolSteps.optionLabel), ["7 days", "30 days", "90 days", "1 year", "Forever"])
        XCTAssertEqual(KeepToolSteps.windowDays(defaults), 30, "30 days by default")
        KeepToolSteps.setWindowDays(7, defaults)
        XCTAssertEqual(KeepToolSteps.windowDays(defaults), 7)
        KeepToolSteps.setWindowDays(nil, defaults)
        XCTAssertNil(KeepToolSteps.windowDays(defaults), "Forever")
        defaults.set(12, forKey: KeepToolSteps.defaultsKey)
        XCTAssertEqual(KeepToolSteps.windowDays(defaults), 30, "not an option = the default")
        #if ENET_NODE
        XCTAssertEqual(KeepToolSteps.defaultDays, phoneRetentionDays())
        #endif
    }

    func testAShorterWindowAppliesAtOnce() throws {
        let fake = FakeFactory()
        let defaults = defaults()
        let hub = hub(fake, defaults: defaults)
        hub.sync(accounts: ["a1": "u1"])
        hub.drain()
        let store = try XCTUnwrap(fake.stores["a1"])
        XCTAssertEqual(store.retentions, [30], "retention runs at store open")
        hub.setWindowDays(7)
        hub.drain()
        XCTAssertEqual(store.retentions, [30, 7])
        XCTAssertEqual(KeepToolSteps.windowDays(defaults), 7)
        hub.setWindowDays(nil)
        hub.drain()
        XCTAssertEqual(store.retentions.last, .some(nil))
    }

    // MARK: - Feeds in, OPEN out

    func testASessionOpensFromWhatItsFeedRecorded() throws {
        let fake = FakeFactory()
        let hub = hub(fake)
        hub.sync(accounts: ["a1": "u1"])
        hub.drain()
        let generation = hub.generation(accountId: "a1")
        hub.recordFeed(accountId: "a1", rootId: "r1", sessionId: "s1", deviceId: "dev", frames: [activity(1, "one"), activity(2, "two")], generation: generation)
        hub.recordFeed(accountId: "a1", rootId: "r1", sessionId: "s1", deviceId: "dev", frames: [activity(2, "dup"), activity(3, "three")], generation: generation)
        hub.queue.sync { hub.flushFeeds() }
        let open = try XCTUnwrap(hub.openTail(accountId: "a1", sessionId: "s1", deviceId: "dev"))
        XCTAssertEqual(open.frames.compactMap { SteerWireFrame.parse($0)?.seq }, [1, 2, 3])
        XCTAssertTrue(open.frames[1].contains("two"), "dedup keeps the first")
        XCTAssertEqual(hub.openTail(accountId: "a1", sessionId: "s1", deviceId: "other")?.frames, [], "keyed by the device")
        let page = try XCTUnwrap(hub.page(accountId: "a1", sessionId: "s1", deviceId: "dev", beforeSeq: 3, limit: 1))
        XCTAssertEqual(SteerWireFrame.parse(page)?.pageSeqs, [2])
        // A stale generation (a sign-out since) writes nothing.
        hub.recordFeed(accountId: "a1", rootId: "r1", sessionId: "s1", deviceId: "dev", frames: [activity(9, "late")], generation: generation - 1)
        hub.queue.sync { hub.flushFeeds() }
        XCTAssertEqual(try fake.stores["a1"]?.count(sessionId: "s1", deviceId: "dev"), 3)
    }

    // MARK: - The old stored tails

    func testAnOldTailIsReadInOnceThenDeleted() throws {
        let manager = DatabaseManager()
        let accountId = "chat-store-tail-\(UUID().uuidString.prefix(8))"
        accounts.append((manager, accountId))
        let pool = try manager.pool(forAccountId: accountId)
        let at = "2026-11-17T09:00:00Z"
        try pool.write { db in
            try SessionEntity(id: "s-plain", teamId: "t1", userId: "u1", deviceId: "dev-1", createdAt: at, updatedAt: at).save(db)
            try SessionEntity(
                id: "s-keyed", teamId: "t1", userId: "u1", deviceId: "dev-2", createdAt: at, updatedAt: at,
                sealPublicKey: "pub", sealEpoch: 3
            ).save(db)
            try SessionEntity(
                id: "s-locked", teamId: "t1", userId: "u1", deviceId: "dev-2", createdAt: at, updatedAt: at,
                sealPublicKey: "pub", sealEpoch: 4
            ).save(db)
        }
        let tails = SteerStoredTailStore(directory: tempDir())
        let crypto = TailFakeCrypto()
        try tails.writeBody(accountId: accountId, sessionId: "s-plain", body: SteerStoredTail.encode([activity(1, "plain")]))
        let key = TreeSealKey(publicKey: "pub", epoch: 3)
        try tails.writeBody(accountId: accountId, sessionId: "s-keyed", body: try XCTUnwrap(SteerStoredTail.body([activity(5, "sealed")], key: key, crypto: crypto)))
        try tails.writeBody(accountId: accountId, sessionId: "s-locked", body: try XCTUnwrap(SteerStoredTail.body([activity(6, "later")], key: TreeSealKey(publicKey: "pub", epoch: 4), crypto: crypto)))
        try tails.writeBody(accountId: accountId, sessionId: "s-gone", body: SteerStoredTail.encode([activity(1, "x")]))
        let ring = SealedKeyRing(persistence: .memory)
        ring.add([(rootId: "s-keyed", epoch: 3, secret: Data(count: 32))])
        let fake = FakeFactory()
        let hub = ChatStoreHub(factory: fake.factory, directory: tempDir(), defaults: defaults(), tails: tails)
        hub.configure(db: manager, opener: SealedOpener(crypto: crypto, ring: ring))
        hub.sync(accounts: [accountId: "u1"])
        hub.drain()
        let store = try XCTUnwrap(fake.stores[accountId])
        XCTAssertEqual(try store.count(sessionId: "s-plain", deviceId: "dev-1"), 1)
        XCTAssertEqual(try store.count(sessionId: "s-keyed", deviceId: "dev-2"), 1, "a sealed tail opens with the tree key")
        XCTAssertEqual(tails.sessionIds(accountId: accountId), ["s-locked"], "read in once, then deleted; one without its key yet waits")
        XCTAssertEqual(try store.count(sessionId: "s-locked", deviceId: "dev-2"), 0)
        // Its key arrives: the next open reads it in.
        ring.add([(rootId: "s-locked", epoch: 4, secret: Data(count: 32))])
        let open = try XCTUnwrap(hub.openTail(accountId: accountId, sessionId: "s-locked", deviceId: "dev-2"))
        XCTAssertEqual(open.frames.compactMap { SteerWireFrame.parse($0)?.seq }, [6])
        XCTAssertEqual(tails.sessionIds(accountId: accountId), [])
    }

    // MARK: - Messages

    func testSyncedMessagesAreStoredOpenedOrWithoutABody() throws {
        let ring = SealedKeyRing(persistence: .memory)
        let opener = SealedOpener(crypto: TailFakeCrypto(), ring: ring)
        let at = "2026-11-17T09:00:00.000Z"
        let plain = MessageEntity(id: "m1", conversationId: "c", rootId: "r1", teamId: "t", authorKind: "user", body: "hi", createdAt: at, updatedAt: at, sessionId: "s1")
        let sealed = MessageEntity(id: "m2", conversationId: "c", rootId: "r1", teamId: "t", authorKind: "agent", body: "exp-sealed:v1:3:secret", createdAt: at, updatedAt: at, sessionId: "s1")
        let chatless = MessageEntity(id: "m3", conversationId: "c", rootId: "r1", teamId: "t", authorKind: "user", body: "x", createdAt: at, updatedAt: at)
        XCTAssertEqual(ChatStoreHub.storedMessage(plain, opener: opener)?["body"] as? String, "hi")
        XCTAssertEqual(ChatStoreHub.storedMessage(plain, opener: opener)?["createdAt"] as? Int64, ChatStoreWire.isoMs(at))
        XCTAssertTrue(ChatStoreHub.storedMessage(sealed, opener: opener)?["body"] is NSNull, "no key yet: no body")
        XCTAssertNil(ChatStoreHub.storedMessage(chatless, opener: opener))
        ring.add([(rootId: "r1", epoch: 3, secret: Data(count: 32))])
        XCTAssertEqual(ChatStoreHub.storedMessage(sealed, opener: opener)?["body"] as? String, "secret")
    }

    // MARK: - Sign-out

    func testADeliberateSignOutDeletesTheStoreAnExpiryKeepsIt() throws {
        let fake = FakeFactory()
        let tails = SteerStoredTailStore(directory: tempDir())
        try tails.writeBody(accountId: "a2", sessionId: "s", body: "x")
        let hub = hub(fake, tails: tails)
        hub.closeWait = 0.2
        hub.sync(accounts: ["a1": "u1", "a2": "u2"])
        hub.drain()
        XCTAssertNotNil(fake.stores["a1"])
        // Expiry: the token went, no sign-out: closed, files kept.
        hub.sync(accounts: ["a2": "u2"])
        hub.drain()
        XCTAssertEqual(fake.deleted, [])
        XCTAssertNil(hub.lock.withLock { hub.stores["a1"] })
        // A deliberate sign-out: stopped, deleted, and never reopened while
        // the account is still listed.
        let generation = hub.generation(accountId: "a2")
        hub.signOut(accountId: "a2")
        hub.drain()
        XCTAssertEqual(fake.deleted, ["a2"])
        XCTAssertEqual(tails.sessionIds(accountId: "a2"), [])
        XCTAssertGreaterThan(hub.generation(accountId: "a2"), generation)
        hub.sync(accounts: ["a2": "u2"])
        hub.drain()
        XCTAssertNil(fake.stores["a2"])
        hub.sync(accounts: [:])
        hub.sync(accounts: ["a2": "u2"])
        hub.drain()
        XCTAssertNotNil(fake.stores["a2"], "a later sign-in opens a fresh store")
    }

    // MARK: - The catch-up

    func testCatchUpChunksReachTheLoopBeforeTheLinks() async throws {
        let fake = FakeFactory()
        let hub = hub(fake, db: DatabaseManager())
        let node = FakeChatStoreNode()
        hub.attach(node: node)
        let walker = FakeWalker()
        let loop = ChatCatchUpLoop(walker: walker) { _, _, _ in }
        hub.lock.withLock { hub.loops["a1"] = loop }
        let link = SteerEnetLink(sessionId: "s1", peer: "peer", inbox: node.steerInbox) { _ in }
        node.steerInbox.register(link)
        // The loop waits for `cu-s1`.
        let pass = Task { await loop.runPass([ChatCatchUpSession(sessionId: "s1", rootId: "r1", deviceId: "dev", updatedAt: nil, busy: false)]) }
        for _ in 0..<100 where !loop.isPending("cu-s1") { try await Task.sleep(for: .milliseconds(10)) }
        XCTAssertTrue(loop.isPending("cu-s1"))
        let ours = "{\"t\":\"history_chunk\",\"requestId\":\"cu-s1\",\"deviceId\":\"dev\",\"events\":[],\"seqs\":[],\"done\":true}"
        let viewers = "{\"t\":\"history_chunk\",\"requestId\":\"p1\",\"events\":[],\"seqs\":[],\"done\":true}"
        XCTAssertTrue(node.steerInbox.deliver(from: "peer", body: SteerEnet.encode(sessionId: "s1", frames: [ours, viewers])))
        _ = await pass.value
        let answer = try XCTUnwrap(walker.answers["cu-s1"] ?? nil)
        XCTAssertEqual(ChatStoreWire.typeAndRequest(answer).requestId, "cu-s1")
        let next = await link.next()
        let received = try XCTUnwrap(next)
        XCTAssertEqual(received.map { ChatStoreWire.typeAndRequest($0).requestId }, ["p1"], "only the viewer's own page reaches its link")
        _ = viewers
        link.close()
    }

    func testAnAskWithoutAnAnswerTimesOut() async throws {
        let walker = FakeWalker()
        walker.timeoutMs = 50
        let loop = ChatCatchUpLoop(walker: walker) { _, _, _ in }
        let added = await loop.runPass([ChatCatchUpSession(sessionId: "s1", rootId: "r", deviceId: "d", updatedAt: nil, busy: false)])
        XCTAssertEqual(added, 0)
        XCTAssertEqual(walker.answers["cu-s1"], .some(nil))
        // An unreachable peer is an answer of nil at once.
        let failing = FakeWalker()
        let failed = ChatCatchUpLoop(walker: failing) { _, _, _ in throw URLError(.cannotConnectToHost) }
        _ = await failed.runPass([ChatCatchUpSession(sessionId: "s2", rootId: "r", deviceId: "d", updatedAt: nil, busy: false)])
        XCTAssertEqual(failing.answers["cu-s2"], .some(nil))
    }

    func testTheLoopCarriesEachAskInASteerEnvelope() async throws {
        let walker = FakeWalker()
        let node = FakeChatStoreNode()
        let loopBox = LoopBox()
        node.onSend = { peer, body in
            guard let envelope = SteerEnet.parse(body), let requestId = ChatStoreWire.typeAndRequest(envelope.frames[0]).requestId else { return }
            // The holder answers at once.
            loopBox.loop?.deliver(requestId: requestId, chunk: "{\"t\":\"history_chunk\",\"requestId\":\"\(requestId)\",\"events\":[],\"seqs\":[],\"done\":true}")
        }
        let loop = ChatCatchUpLoop(walker: walker) { peer, sessionId, frame in
            try await node.sendChat(peer: peer, body: SteerEnet.encode(sessionId: sessionId, frames: [frame]))
        }
        loopBox.loop = loop
        let added = await loop.runPass([
            ChatCatchUpSession(sessionId: "s1", rootId: "r", deviceId: "d", updatedAt: nil, busy: false),
            ChatCatchUpSession(sessionId: "s2", rootId: "r", deviceId: "d", updatedAt: nil, busy: true),
        ])
        XCTAssertEqual(added, 2)
        XCTAssertEqual(Set(node.sent.map(\.peer)), ["peer-s1", "peer-s2"])
        XCTAssertEqual(Set(node.sent.compactMap { SteerEnet.parse($0.body)?.sessionId }), ["s1", "s2"])
    }

    private final class LoopBox: @unchecked Sendable { var loop: ChatCatchUpLoop? }

    func testAPhoneAnswersADeviceNamingPageUnderTheJoinsRule() async throws {
        let manager = DatabaseManager()
        let accountId = "chat-store-holder-\(UUID().uuidString.prefix(8))"
        accounts.append((manager, accountId))
        let pool = try manager.pool(forAccountId: accountId)
        let at = "2026-11-17T09:00:00Z"
        try await pool.write { db in
            try SessionEntity(id: "s1", teamId: "t1", userId: "starter", deviceId: "dev-1", createdAt: at, updatedAt: at).save(db)
        }
        let fake = FakeFactory()
        let hub = hub(fake, db: manager)
        let node = FakeChatStoreNode()
        node.lists[accountId] = ["t1": [
            InstallEntry(installId: "i1", userId: "starter", kind: "desktop", endpointId: "own-desktop", x25519Public: ""),
            InstallEntry(installId: "i2", userId: "teammate", kind: "ios", endpointId: "teammate-phone", x25519Public: ""),
        ]]
        hub.attach(node: node)
        hub.sync(accounts: [accountId: "starter"])
        hub.drain()
        let page = "{\"t\":\"history_page\",\"requestId\":\"cu-9\",\"deviceId\":\"dev-1\",\"limit\":200,\"have\":[]}"
        XCTAssertTrue(node.steerInbox.deliver(from: "own-desktop", body: SteerEnet.encode(sessionId: "s1", frames: [page])))
        XCTAssertTrue(node.steerInbox.deliver(from: "teammate-phone", body: SteerEnet.encode(sessionId: "s1", frames: [page])))
        XCTAssertTrue(node.steerInbox.deliver(from: "own-desktop", body: SteerEnet.encode(sessionId: "nope", frames: [page])))
        for _ in 0..<200 where node.lock.withLock({ node.sent.count }) < 3 { try await Task.sleep(for: .milliseconds(10)) }
        let store = try XCTUnwrap(fake.stores[accountId])
        let verdicts = Dictionary(store.answered.map { ("\($0.0)", $0.1) }, uniquingKeysWith: { a, b in a == .allowed ? b : a })
        XCTAssertEqual(store.answered.count, 3)
        XCTAssertEqual(verdicts["nope"], .noSuchSession)
        XCTAssertTrue(store.answered.contains { $0.0 == "s1" && $0.1 == .allowed })
        XCTAssertTrue(store.answered.contains { $0.0 == "s1" && $0.1 == .unauthorized }, "a teammate's device, the session not shared")
        XCTAssertEqual(Set(node.sent.map(\.peer)), ["own-desktop", "teammate-phone"], "answered to the asker only")
    }

    func testTheRegistryListsBecomeTheWalksInstalls() {
        let lists: [String: [InstallEntry]] = [
            "t1": [
                InstallEntry(installId: "1", userId: "u1", kind: "desktop", endpointId: "e1", x25519Public: "", deviceId: "dev", lastSeenAt: "2026-11-17T09:00:00.000Z"),
                InstallEntry(installId: "2", userId: nil, kind: "ios", endpointId: "e2", x25519Public: ""),
                InstallEntry(installId: "3", userId: "u1", kind: "ios", endpointId: "e3", x25519Public: "", revokedAt: "x"),
            ],
            "t2": [InstallEntry(installId: "1", userId: "u1", kind: "desktop", endpointId: "e1", x25519Public: "", deviceId: "dev")],
        ]
        XCTAssertEqual(ChatStoreHub.catchUpInstalls(lists), [
            ChatCatchUpInstall(endpointId: "e1", userId: "u1", deviceId: "dev", lastSeenAt: "2026-11-17T09:00:00.000Z"),
        ])
    }

    // MARK: - The real store (chat-store-ffi)

    #if ENET_NODE
    func testTheRealStoreOpensSearchesAndSignsOut() throws {
        let dir = tempDir()
        let defaults = defaults()
        let hub = ChatStoreHub(factory: ChatStoreFactory.linked, directory: dir, defaults: defaults, tails: SteerStoredTailStore(directory: tempDir()))
        hub.sync(accounts: ["u1": "user-1", "u10": "user-10"])
        hub.drain()
        let generation = hub.generation(accountId: "u1")
        // A tool step of now (an older one would go at open: RETENTION).
        let tool = "{\"t\":\"activity\",\"seq\":2,\"event\":{\"kind\":\"tool\",\"id\":\"c1\",\"name\":\"Bash\",\"detail\":\"cargo test\",\"at\":\(ChatCatchUpLoop.nowMs())}}"
        hub.recordFeed(accountId: "u1", rootId: "r1", sessionId: "s1", deviceId: "dev", frames: [
            "{\"t\":\"activity\",\"seq\":1,\"event\":{\"kind\":\"user_message\",\"text\":\"Fix the café\"}}", tool,
        ], generation: generation)
        hub.recordFeed(accountId: "u10", rootId: "r1", sessionId: "s1", deviceId: "dev", frames: [tool], generation: hub.generation(accountId: "u10"))
        hub.queue.sync { hub.flushFeeds() }
        let open = try XCTUnwrap(hub.openTail(accountId: "u1", sessionId: "s1", deviceId: "dev"))
        XCTAssertEqual(open.frames.compactMap { SteerWireFrame.parse($0)?.seq }, [1, 2])
        // FTS5 trigram in the system SQLite: case- and diacritics-insensitive.
        let found = try XCTUnwrap(hub.searchChat(accountId: "u1", rootId: "r1", query: "CAFE", limit: 10))
        let result = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(found.utf8)) as? [String: Any])
        XCTAssertEqual(result["total"] as? Int, 1, found)
        let file = dir.appendingPathComponent("accounts/u1/chat-store.sqlite")
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path))
        XCTAssertEqual(try dir.resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup, true)
        #if os(iOS) && !targetEnvironment(simulator)
        let protection = try FileManager.default.attributesOfItem(atPath: dir.appendingPathComponent("accounts/u1").path)[.protectionKey] as? FileProtectionType
        XCTAssertEqual(protection, .completeUntilFirstUserAuthentication)
        #endif
        // Expiry keeps, sign-out deletes (never another account's).
        hub.sync(accounts: ["u10": "user-10"])
        hub.drain()
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path))
        hub.sync(accounts: ["u1": "user-1", "u10": "user-10"])
        hub.drain()
        XCTAssertEqual(hub.openTail(accountId: "u1", sessionId: "s1", deviceId: "dev")?.frames.count, 2, "kept for the next sign-in")
        hub.signOut(accountId: "u1")
        hub.drain()
        for name in ["chat-store.sqlite", "chat-store.sqlite-wal", "chat-store.sqlite-shm"] {
            XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("accounts/u1/\(name)").path), name)
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: dir.appendingPathComponent("accounts/u10/chat-store.sqlite").path))
    }

    func testTheRealStoreAnswersAPageAndTheWalkTakesIt() throws {
        let holder = try ChatStore.inMemory()
        _ = try holder.putFeed(rootId: "r", sessionId: "s1", deviceId: "dev", eventsJson: "[{\"seq\":1,\"event\":{\"kind\":\"narration\",\"text\":\"a\"}},{\"seq\":2,\"event\":{\"kind\":\"narration\",\"text\":\"b\"}}]", nowMs: 1)
        let asker = try ChatStore.inMemory()
        let walker = try XCTUnwrap(ChatStoreFactory.linked?.catchUp(asker, "", "u1", 30))
        walker.setInstalls(selfEndpoint: "me", installs: [ChatCatchUpInstall(endpointId: "runner", userId: "u1", deviceId: "dev", lastSeenAt: nil)])
        let now = ChatCatchUpLoop.nowMs()
        XCTAssertEqual(walker.beginPass(sessions: [ChatCatchUpSession(sessionId: "s1", rootId: "r", deviceId: "dev", updatedAt: "x", busy: false)], nowMs: now), 1)
        let sessionId = try XCTUnwrap(walker.nextWalk(nowMs: now))
        let ask = try XCTUnwrap(walker.nextAsk(sessionId: sessionId, nowMs: now))
        XCTAssertEqual(ask.peer, "runner")
        XCTAssertTrue(ChatStoreFactory.linked?.isCatchUpPage(ask.frame) == true)
        let chunk = try XCTUnwrap(try holder.answerPage(sessionId: "s1", pageJson: ask.frame, access: ChatStorePageAccess.allowed))
        let taken = try XCTUnwrap(walker.takeAnswer(requestId: ask.requestId, chunkJson: chunk, nowMs: now))
        XCTAssertEqual(taken.added, 2)
        XCTAssertEqual(try asker.count(sessionId: "s1", deviceId: "dev"), 2)
        let refused = try XCTUnwrap(try holder.answerPage(sessionId: "s1", pageJson: ask.frame, access: ChatStorePageAccess.unauthorized))
        XCTAssertTrue(refused.contains("unauthorized"))
    }

    /// #EDE-114 MEASUREMENT: OPEN on a big chat: 17,000 events (tool +
    /// tool_update with ~900 bytes of output, narration), then the store's
    /// open + `openTail` + the frames parsed and decoded (the model's
    /// off-main path). Prints the median of 5 runs.
    func testOpenTimeOfABigChat() throws {
        let dir = tempDir()
        let seed = try ChatStore.open(dataDir: dir.path, accountId: "big")
        let output = String(repeating: "x", count: 880)
        let now = ChatCatchUpLoop.nowMs()
        var seq = 0
        var batch: [[String: Any]] = []
        while seq < 17_000 {
            let call = "c\(seq)"
            let rows: [[String: Any]] = [
                ["kind": "tool", "id": call, "name": "Bash", "detail": "cargo test -p chat-store \(seq)", "at": now],
                ["kind": "tool_update", "id": call, "status": "completed", "output": "\(output) \(seq)", "at": now],
                ["kind": "narration", "text": "Step \(seq): the tests pass, moving on to the next part of the change.", "at": now],
            ]
            for event in rows where seq < 17_000 {
                batch.append(["seq": seq, "event": event])
                seq += 1
            }
            if batch.count >= 1_000 {
                let data = try JSONSerialization.data(withJSONObject: batch)
                _ = try seed.putFeed(rootId: "r", sessionId: "s", deviceId: "dev", eventsJson: String(decoding: data, as: UTF8.self), nowMs: now)
                batch = []
            }
        }
        if !batch.isEmpty {
            let data = try JSONSerialization.data(withJSONObject: batch)
            _ = try seed.putFeed(rootId: "r", sessionId: "s", deviceId: "dev", eventsJson: String(decoding: data, as: UTF8.self), nowMs: now)
        }
        XCTAssertEqual(try seed.count(sessionId: "s", deviceId: "dev"), 17_000)
        var opens: [Double] = [], tails: [Double] = [], decodes: [Double] = [], totals: [Double] = []
        var rows = 0
        for _ in 0..<5 {
            let t0 = Date()
            let store = try ChatStore.open(dataDir: dir.path, accountId: "big")
            let t1 = Date()
            let json = try store.openTail(sessionId: "s", deviceId: "dev", tail: UInt32(AgentFeed.replayTail))
            let t2 = Date()
            let frames = ChatStoreWire.tailFrames(json).frames.compactMap(SteerWireFrame.parse)
            let t3 = Date()
            rows = frames.filter { $0.activity.event != nil }.count
            opens.append(t1.timeIntervalSince(t0) * 1000)
            tails.append(t2.timeIntervalSince(t1) * 1000)
            decodes.append(t3.timeIntervalSince(t2) * 1000)
            totals.append(t3.timeIntervalSince(t0) * 1000)
        }
        func median(_ values: [Double]) -> String { String(format: "%.1f", values.sorted()[values.count / 2]) }
        print("EDE-114 OPEN TIME (17000 events, median of 5): open \(median(opens)) ms, openTail \(median(tails)) ms, parse+decode \(median(decodes)) ms (\(rows) events), total \(median(totals)) ms")
        XCTAssertGreaterThanOrEqual(rows, AgentFeed.replayTail)
    }
    #endif
}
