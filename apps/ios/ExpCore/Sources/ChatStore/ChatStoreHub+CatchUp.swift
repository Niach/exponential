import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "ChatCatchUp")

/// #EDE-114 THE PHONE WALK (chat-store.json `catchUp`, phone limits): for
/// every chat the account reads (every synced `sessions` row with a
/// `deviceId`), the events it lacks, from the device running the session and
/// then the account's other devices. A pass runs in the FOREGROUND only on an
/// unmetered network, every `passIntervalMs` and when the account's sessions
/// change; in the BACKGROUND only inside the OS's window (a `BGProcessingTask`
/// asking for the network and external power, the app's). RETENTION runs
/// after each pass. The phone answers device-naming pages off its store like
/// any reader (only while its node is up).
extension ChatStoreHub {
    /// After a change of the account's sessions, a pass waits this long (and
    /// runs at most once per `kickMinInterval`).
    static let kickDebounce: Duration = .seconds(15)
    static let kickMinInterval: TimeInterval = 60

    /// The install node (AppDependencies, once): its inbox hands the catch-up
    /// chunks and pages here before any viewer link sees them.
    public func attach(node: ChatStoreNode) {
        lock.withLock { self.node = node }
        node.steerInbox.setInterceptor { [weak self] from, sessionId, frame in
            self?.intercept(from: from, sessionId: sessionId, frame: frame) ?? false
        }
        let open = lock.withLock { Array(stores.keys) }
        for accountId in open { startCatchUp(accountId: accountId) }
    }

    /// The foreground pass's conditions: the app is active and the network
    /// is unmetered (not expensive, not constrained). Opening the gate runs a
    /// pass at once and then every `passIntervalMs`; closing it stops it.
    public func setForeground(active: Bool? = nil, unmetered: Bool? = nil) {
        let (open, wasOpen) = lock.withLock { () -> (Bool, Bool) in
            let was = foreground && self.unmetered
            if let active { foreground = active }
            if let unmetered { self.unmetered = unmetered }
            return (foreground && self.unmetered, was)
        }
        guard open != wasOpen else { return }
        if open { startScheduler() } else { stopCatchUp() }
    }

    var gateOpen: Bool { lock.withLock { foreground && unmetered } }

    func startScheduler() {
        lock.withLock {
            schedulerTask?.cancel()
            schedulerTask = Task.detached(priority: .utility) { [weak self] in
                while !Task.isCancelled {
                    guard let self, self.gateOpen else { return }
                    let interval = await self.runPasses()
                    try? await Task.sleep(for: .milliseconds(Int(interval)))
                }
            }
        }
    }

    /// Stops every pass where it is (the gate closed, the OS ended the
    /// background window): the watermarks keep the place.
    public func stopCatchUp() {
        let walks = lock.withLock { () -> [ChatCatchUpLoop] in
            schedulerTask?.cancel()
            schedulerTask = nil
            return Array(loops.values)
        }
        for loop in walks { loop.stop() }
    }

    /// One pass for every signed-in account (the background window's, or the
    /// scheduler's); returns the interval to the next.
    @discardableResult
    public func runPasses() async -> UInt64 {
        let accountIds = lock.withLock { Array(loops.keys).sorted() }
        var interval: UInt64 = 300_000
        for accountId in accountIds {
            guard let loop = lock.withLock({ loops[accountId] }) else { continue }
            interval = loop.walker.passIntervalMs
            await runPass(accountId: accountId, loop: loop)
        }
        return interval
    }

    /// Waits (at most `timeout`) for the node to be up and a store's walk to
    /// exist (a background window may launch the app).
    public func waitUntilReady(timeout: Duration) async {
        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline, !Task.isCancelled {
            let (node, walks) = lock.withLock { (self.node, !loops.isEmpty) }
            if walks, await node?.endpointId() != nil { return }
            try? await Task.sleep(for: .milliseconds(500))
        }
    }

    /// The account's loop, once its store is open and the node attached.
    func startCatchUp(accountId: String) {
        guard let factory else { return }
        let ready = lock.withLock { () -> (ChatStoreBackend, String, ChatStoreNode)? in
            guard loops[accountId] == nil, let store = stores[accountId], let userId = accounts[accountId], let node else { return nil }
            return (store, userId, node)
        }
        guard let ready else { return }
        let (store, userId, node) = ready
        guard let walker = factory.catchUp(store, "", userId, KeepToolSteps.windowDays(defaults)) else { return }
        let loop = ChatCatchUpLoop(walker: walker, now: now) { [weak node] peer, sessionId, frame in
            guard let node else { throw InstallNodeNotRunning() }
            try await node.sendChat(peer: peer, body: SteerEnet.encode(sessionId: sessionId, frames: [frame]))
        }
        let generation = generation(accountId: accountId)
        let kept = lock.withLock { () -> Bool in
            guard loops[accountId] == nil, (generations[accountId] ?? 0) == generation, accounts[accountId] != nil else { return false }
            loops[accountId] = loop
            return true
        }
        guard kept else { return }
        startSessions(accountId: accountId)
        if gateOpen { kick(accountId: accountId, after: .zero) }
    }

    /// Watches the account's `sessions` rows: a change kicks a pass.
    func startSessions(accountId: String) {
        guard let pool = try? lock.withLock({ db })?.pool(forAccountId: accountId) else { return }
        let observer = DatabaseRegionObservation(tracking: Table("sessions"))
            .start(in: pool, onError: { _ in }, onChange: { [weak self] _ in
                self?.kick(accountId: accountId, after: ChatStoreHub.kickDebounce)
            })
        lock.withLock { sessionObservers[accountId] = observer }
    }

    func kick(accountId: String, after delay: Duration) {
        let due = lock.withLock { () -> Bool in
            guard foreground && unmetered else { return false }
            if delay != .zero, let last = lastKickAt[accountId], Date().timeIntervalSince(last) < Self.kickMinInterval { return false }
            lastKickAt[accountId] = Date()
            return true
        }
        guard due else { return }
        Task.detached(priority: .utility) { [weak self] in
            if delay != .zero { try? await Task.sleep(for: delay) }
            guard let self, self.gateOpen, let loop = self.lock.withLock({ self.loops[accountId] }) else { return }
            await self.runPass(accountId: accountId, loop: loop)
        }
    }

    /// One pass of one account: the chats it reads, the registry's installs,
    /// then RETENTION.
    func runPass(accountId: String, loop: ChatCatchUpLoop) async {
        guard !loop.isRunning, let node = lock.withLock({ self.node }), let endpoint = await node.endpointId() else { return }
        guard let pool = try? lock.withLock({ db })?.pool(forAccountId: accountId) else { return }
        let sessions = ((try? await pool.read { db in
            try SessionEntity.filter(Column("device_id") != "").fetchAll(db)
        }) ?? []).map(Self.catchUpSession)
        loop.walker.setInstalls(selfEndpoint: endpoint, installs: Self.catchUpInstalls(await node.registry(accountId: accountId)))
        let added = await loop.runPass(sessions) ?? 0
        if added > 0 { logger.info("catch-up stored \(added) events") }
        let windowDays = KeepToolSteps.windowDays(defaults)
        let now = now()
        queue.async { [self] in
            _ = try? store(accountId: accountId)?.applyRetention(nowMs: now, windowDays: windowDays)
        }
    }

    public static func catchUpSession(_ row: SessionEntity) -> ChatCatchUpSession {
        ChatCatchUpSession(
            sessionId: row.id, rootId: row.rootId.isEmpty ? row.id : row.rootId,
            deviceId: row.deviceId, updatedAt: row.updatedAt, busy: row.busy
        )
    }

    /// The registry's lists as the walk's installs (one per endpoint; a
    /// revoked or user-less one drops out).
    public static func catchUpInstalls(_ lists: [String: [InstallEntry]]) -> [ChatCatchUpInstall] {
        var seen: Set<String> = []
        var out: [ChatCatchUpInstall] = []
        for teamId in lists.keys.sorted() {
            for entry in lists[teamId] ?? [] where entry.revokedAt == nil {
                guard let userId = entry.userId, seen.insert(entry.endpointId).inserted else { continue }
                out.append(ChatCatchUpInstall(endpointId: entry.endpointId, userId: userId, deviceId: entry.deviceId, lastSeenAt: entry.lastSeenAt))
            }
        }
        return out
    }

    // MARK: - The node's inbox

    /// Takes a catch-up frame off the node ahead of the viewer links: a
    /// `history_chunk` answering one of this phone's pending asks, or a
    /// `history_page` naming a device (answered off the store).
    public func intercept(from: String, sessionId: String, frame: String) -> Bool {
        guard frame.contains("\"history_"), let factory else { return false }
        let (t, requestId) = ChatStoreWire.typeAndRequest(frame)
        switch t {
        case "history_chunk":
            guard let requestId else { return false }
            let walks = lock.withLock { Array(loops.values) }
            return walks.contains { $0.isPending(requestId) && $0.deliver(requestId: requestId, chunk: frame) }
        case "history_page":
            guard factory.isCatchUpPage(frame) else { return false }
            Task.detached(priority: .utility) { [weak self] in
                await self?.answer(from: from, sessionId: sessionId, frame: frame)
            }
            return true
        default:
            return false
        }
    }

    /// THE HOLDER: a device-naming page answered off the store of the
    /// account that holds the session's row, under the join's access rule
    /// (the asker's user from the session team's registry list,
    /// `viewerAllowed`); no row in any account = `no_such_session`.
    func answer(from peer: String, sessionId: String, frame: String) async {
        guard let node = lock.withLock({ self.node }) else { return }
        let (db, open) = lock.withLock { (self.db, Array(stores)) }
        var target: (store: ChatStoreBackend, access: ChatStorePageAccess)?
        for (accountId, store) in open.sorted(by: { $0.key < $1.key }) {
            guard let pool = db?.poolIfOpen(forAccountId: accountId),
                  let row = (try? await pool.read({ try SessionEntity.fetchOne($0, key: sessionId) })) ?? nil else { continue }
            let entries = await node.registry(accountId: accountId)[row.teamId] ?? []
            target = (store, Self.access(row: row, peer: peer, teamInstalls: entries))
            break
        }
        guard let found = target ?? open.first.map({ ($0.value, ChatStorePageAccess.noSuchSession) }) else { return }
        let (store, access) = found
        guard let chunk = try? store.answerPage(sessionId: sessionId, pageJson: frame, access: access) else { return }
        do {
            try await node.sendChat(peer: peer, body: SteerEnet.encode(sessionId: sessionId, frames: [chunk]))
        } catch {
            logger.info("answering a catch-up page failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// The join's access rule over the holder's synced row: the asker's
    /// user from the session team's registry list (unknown = refused), the
    /// starter, or anyone listed when the session is shared.
    public static func access(row: SessionEntity, peer: String, teamInstalls: [InstallEntry]) -> ChatStorePageAccess {
        let user = teamInstalls.first { $0.endpointId.lowercased() == peer.lowercased() && $0.revokedAt == nil }?.userId
        return SteerEnet.viewerAllowed(viewerUserId: user, starterUserId: row.userId, shared: row.shared) ? .allowed : .unauthorized
    }
}
