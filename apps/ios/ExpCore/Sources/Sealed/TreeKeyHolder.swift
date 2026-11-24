import Foundation
import os

// EXP-1459: this install as a HOLDER of session-tree keys. It fetches the
// wraps addressed to it (`sessionKeys.mine`), unwraps them with its X25519
// secret (derived from the install identity, `PushKeypair::derive`) into the
// key ring, and answers `sessionKeys.pending` on start and every minute while
// online: readers still missing a wrap for a tree it holds get one,
// automatically (no approval, #EXP-1457 decision). Before that it ROTATES
// the roots that are due (#EXP-1951, sealed-content.json `rotation`: a
// reader was dropped). As a WRITER it seals to a
// tree's key, creating the key when an existing root has none yet, or hands
// a new chat its inline `seal`.
// #EXP-1950: every wrap it makes is signed by this install's identity
// (`WrapSigner`; none = unsigned). A new root's inline seal cannot be (its id
// is unknown before the send): its wraps are made and signed again once the
// root exists, as are the keys held from before (decision (c),
// `sessionKeys.resign`, a bounded batch of roots per pass).

private let logger = Logger(subsystem: "at.exponential", category: "TreeKeys")

/// `sessions.send`'s inline `seal` for a root without a key (a new chat,
/// agent conversation or app editor; contract addendum 2, 2026-10-30): the
/// server creates the root, the key and the sealed first message in one
/// transaction, under `sessionKeys.create`'s rules.
public struct NewChatSeal: Encodable, Equatable, Sendable {
    public let installId: String
    public let publicKey: String
    public let epoch: UInt32
    public let wraps: [TreeKeyWrap]

    public init(installId: String, publicKey: String, epoch: UInt32, wraps: [TreeKeyWrap]) {
        self.installId = installId
        self.publicKey = publicKey
        self.epoch = epoch
        self.wraps = wraps
    }
}

/// A key the writer seals to.
public struct TreeSealKey: Equatable, Sendable {
    public let publicKey: String
    public let epoch: UInt32

    public init(publicKey: String, epoch: UInt32) {
        self.publicKey = publicKey
        self.epoch = epoch
    }
}

/// #EXP-1960: one `sessionKeys.create` for a keyless root.
public enum TreeKeyCreate: Equatable, Sendable {
    case keyed(TreeSealKey)
    case failed(CreateFailure)
    /// No eNet, no install or no own wrap: nothing was asked.
    case unavailable
}

public enum TreeKeyError: Error, Equatable {
    /// This install is not registered (yet) or the build has no eNet.
    case noInstall
    case unavailable
}

/// The readers a writer resolves on its own (`installs.mine` + the target
/// device's install + every team install when shared); holders fill the rest.
public enum TreeReaders {
    public static func resolve(
        own: [InstallEntry],
        team: [InstallEntry],
        selfInstallId: String,
        deviceIds: [String],
        shared: Bool
    ) -> [InstallEntry] {
        var out: [String: InstallEntry] = [:]
        for entry in own where entry.revokedAt == nil { out[entry.installId] = entry }
        for entry in team {
            if shared || entry.deviceId.map(deviceIds.contains) == true { out[entry.installId] = entry }
        }
        return out.values.sorted { lhs, rhs in
            // The caller's own wrap first (the server requires it).
            if (lhs.installId == selfInstallId) != (rhs.installId == selfInstallId) { return lhs.installId == selfInstallId }
            return lhs.installId < rhs.installId
        }
    }
}

public actor TreeKeyHolder {
    public static let pendingInterval: Duration = .seconds(60)
    /// A root asked for once is not asked again for this long.
    static let requestCooldown: TimeInterval = 30

    private let keys: SessionKeysRegistry
    private let installs: TreeReaderDirectory?
    private let crypto: SealedCrypto
    private let signer: WrapSigner?
    private let ring: SealedKeyRing
    private let identity: @Sendable () -> Data?
    private let installId: @Sendable (String) async -> String?
    /// #EDE-97: nudges this install's registration on an account it has
    /// none for yet (a writer is waiting for it).
    private let register: @Sendable (String) -> Void
    private let now: @Sendable () -> Date
    private var requested: [String: Date] = [:]
    private var synced: Set<String> = []
    private var cachedKey: (identity: Data, secret: Data, publicKey: String)?
    /// #EXP-1951: rotations the server refused this install, `root\nepoch`.
    private var left: Set<String> = []
    /// #EXP-1950: this identity's endpoint id (the own wrap's recipient).
    private var cachedEndpoint: (identity: Data, endpointId: String)?
    /// Roots to sign again: the epoch held and the readers known for it
    /// (a new root's inline seal: the ones it wrapped for; else none known).
    private var resignQueue: [String: (epoch: UInt32, readers: [InstallEntry])] = [:]
    /// The readers of inline seals not adopted yet, by their tree public key.
    private var unsignedSeals: [String: [InstallEntry]] = [:]
    /// `root\nepoch` signed again (or refused): never asked twice.
    private var resigned: Set<String> = []
    /// Roots signed again per pass at most.
    static let resignBatch = 20

    public init(
        keys: SessionKeysRegistry,
        installs: TreeReaderDirectory?,
        crypto: SealedCrypto,
        signer: WrapSigner? = nil,
        ring: SealedKeyRing,
        identity: @escaping @Sendable () -> Data?,
        installId: @escaping @Sendable (String) async -> String?,
        register: @escaping @Sendable (String) -> Void = { _ in },
        now: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.register = register
        self.keys = keys
        self.installs = installs
        self.crypto = crypto
        self.signer = signer
        self.ring = ring
        self.identity = identity
        self.installId = installId
        self.now = now
    }

    private func installKey() -> (secret: Data, publicKey: String)? {
        guard let seed = identity() else { return nil }
        if let cachedKey, cachedKey.identity == seed { return (cachedKey.secret, cachedKey.publicKey) }
        // A rotated identity: every wrap is fetched again for the new key.
        if cachedKey != nil { synced.removeAll() }
        guard let key = try? crypto.installKey(identity: seed) else { return nil }
        cachedKey = (seed, key.secret, key.publicKey)
        return key
    }

    /// #EXP-1950: this install's signature over a wrap it just made; nil =
    /// unsigned (no full binding, no identity, the recipient's key unknown).
    private func signature(rootId: String, epoch: UInt32, treePublicKey: String, recipientEndpointId: String?, wrapped: String) -> String? {
        guard let signer, let seed = identity(), let recipient = recipientEndpointId, !recipient.isEmpty else { return nil }
        do {
            return try signer.sign(identity: seed, statement: WrapStatement(
                rootId: rootId.lowercased(), epoch: epoch, treePublicKey: treePublicKey, recipientEndpointId: recipient, wrapped: wrapped
            ))
        } catch {
            logger.error("Signing a tree key wrap failed (root \(rootId, privacy: .public)): it goes unsigned")
            return nil
        }
    }

    /// This identity's endpoint id (a signature's recipient for our own wrap).
    private func ownEndpointId() -> String? {
        guard let signer, let seed = identity() else { return nil }
        if let cachedEndpoint, cachedEndpoint.identity == seed { return cachedEndpoint.endpointId }
        guard let id = try? signer.endpointId(identity: seed) else { return nil }
        cachedEndpoint = (seed, id)
        return id
    }

    /// One holder pass for an account: every wrap this install holds (the
    /// first pass) and the pending wraps it owes others.
    /// False while this account has no registered install (nothing ran).
    @discardableResult
    public func sync(accountId: String) async -> Bool {
        guard crypto.isAvailable, let install = await installId(accountId) else { return false }
        if !synced.contains(accountId) {
            if await fetch(accountId: accountId, installId: install, rootIds: nil) { synced.insert(accountId) }
        }
        await rotateDue(accountId: accountId, installId: install)
        await answerPending(accountId: accountId, installId: install)
        await resignHeld(accountId: accountId, installId: install)
        return true
    }

    /// The identity rotated: every held secret was wrapped for the old key.
    public func forgetAll() {
        synced.removeAll()
        requested.removeAll()
        left.removeAll()
        resignQueue.removeAll()
        unsignedSeals.removeAll()
        resigned.removeAll()
        cachedKey = nil
        ring.removeAll()
    }

    /// A row of `rootId` could not be opened: ask for its wraps (throttled).
    public func request(accountId: String, rootId: String) async {
        let root = rootId.lowercased()
        if let at = requested[root], now().timeIntervalSince(at) < Self.requestCooldown { return }
        requested[root] = now()
        guard crypto.isAvailable, let install = await installId(accountId) else { return }
        _ = await fetch(accountId: accountId, installId: install, rootIds: [root])
    }

    /// #EXP-1941: a call waits on the key of `rootId`: its wraps, asked
    /// every time (never throttled; the pull nudges the devices that hold it).
    public func pull(accountId: String, rootId: String) async {
        guard crypto.isAvailable, let install = await installId(accountId) else { return }
        _ = await fetch(accountId: accountId, installId: install, rootIds: [rootId.lowercased()])
    }

    /// The secrets this install holds for `rootId`, by epoch.
    public nonisolated func held(rootId: String) -> [UInt32: Data] {
        ring.keys(rootId: rootId)
    }

    @discardableResult
    func fetch(accountId: String, installId: String, rootIds: [String]?) async -> Bool {
        guard let secret = installKey()?.secret else { return false }
        do {
            let held = try await keys.mine(accountId: accountId, installId: installId, rootIds: rootIds)
            var entries: [(rootId: String, epoch: UInt32, secret: Data)] = []
            for wrap in held where !ring.holds(rootId: wrap.rootId, epoch: wrap.epoch) {
                do {
                    entries.append((wrap.rootId, wrap.epoch, try crypto.unwrap(installSecret: secret, wrapped: wrap.wrapped)))
                } catch {
                    logger.error("A tree key wrap did not open for this install (root \(wrap.rootId, privacy: .public))")
                }
            }
            ring.add(entries)
            // #EXP-1950 (c): a root whose newest wrap here is unsigned (an
            // older build made it) is signed again, own wrap at least.
            var newest: [String: HeldTreeKey] = [:]
            for wrap in held where (newest[wrap.rootId]?.epoch ?? 0) < wrap.epoch { newest[wrap.rootId] = wrap }
            for (rootId, wrap) in newest where wrap.signature == nil {
                let root = rootId.lowercased()
                guard resignQueue[root] == nil, !resigned.contains("\(root)\n\(wrap.epoch)") else { continue }
                resignQueue[root] = (wrap.epoch, [])
            }
            return true
        } catch {
            logger.error("sessionKeys.mine failed: \(error.localizedDescription)")
            return false
        }
    }

    /// #EXP-1951 (sealed-content.json `rotation`): a fresh key at the next
    /// epoch for every root the server names as due whose current secret this
    /// install holds, wrapped for THIS install only: the current readers get
    /// theirs from `pending`, which lists readers and nobody else.
    @discardableResult
    func rotateDue(accountId: String, installId: String) async -> Int {
        guard let own = installKey(), let due = try? await keys.due(accountId: accountId, installId: installId), !due.isEmpty else { return 0 }
        // The server holds a wrap of each for this install: unwrap what is missing.
        let missing = due.filter { !ring.holds(rootId: $0.rootId, epoch: $0.epoch) }.map(\.rootId)
        if !missing.isEmpty { _ = await fetch(accountId: accountId, installId: installId, rootIds: missing) }
        var rotated = 0
        for root in due {
            let held = ring.keys(rootId: root.rootId)
            let mark = "\(root.rootId)\n\(root.epoch)"
            guard let epoch = rotationEpoch(
                flagged: root.epoch, current: max(root.epoch, held.keys.max() ?? 0),
                held: held[root.epoch] != nil, left: left.contains(mark)
            ) else { continue }
            let key = crypto.generateTreeKey()
            guard let wrapped = try? crypto.wrap(installPublic: own.publicKey, treeSecret: key.secret) else { continue }
            let signed = signature(rootId: root.rootId, epoch: epoch, treePublicKey: key.publicKey, recipientEndpointId: ownEndpointId(), wrapped: wrapped)
            var failure: CreateFailure?
            do {
                try await keys.create(accountId: accountId, input: CreateTreeKeyInput(
                    rootId: root.rootId, installId: installId, epoch: epoch, publicKey: key.publicKey,
                    wraps: [TreeKeyWrap(installId: installId, wrapped: wrapped, signature: signed)]
                ))
            } catch {
                let body: TrpcErrorBody? = if case let .httpError(_, raw) = error as? TrpcError { TrpcErrorBody.parse(raw) } else { nil }
                failure = createFailure(code: body?.code, message: body?.message)
            }
            switch rotationAfter(failure) {
            case .adopt:
                ring.add([(root.rootId, epoch, key.secret)])
                rotated += 1
                logger.info("Rotated the tree key of \(root.rootId, privacy: .public) to epoch \(epoch)")
            case .reread:
                _ = await fetch(accountId: accountId, installId: installId, rootIds: [root.rootId])
            case .leave:
                left.insert(mark)
            case .retry:
                break
            }
        }
        return rotated
    }

    func answerPending(accountId: String, installId: String) async {
        do {
            let pending = try await keys.pending(accountId: accountId, installId: installId)
            var wraps: [PendingTreeKeyWrap] = []
            for pair in pending {
                guard let secret = ring.keys(rootId: pair.rootId)[pair.epoch],
                      let wrapped = try? crypto.wrap(installPublic: pair.targetX25519Public, treeSecret: secret) else { continue }
                // Signed over the key of the secret held, never the server's
                // word for it: a pair naming another key goes unsigned.
                let held = TreePublicKeys.of(secret: secret)
                let signed = held == pair.publicKey
                    ? signature(rootId: pair.rootId, epoch: pair.epoch, treePublicKey: pair.publicKey, recipientEndpointId: pair.targetEndpointId, wrapped: wrapped)
                    : nil
                wraps.append(PendingTreeKeyWrap(rootId: pair.rootId, epoch: pair.epoch, targetInstallId: pair.targetInstallId, wrapped: wrapped, signature: signed))
            }
            guard !wraps.isEmpty else { return }
            try await keys.wrap(accountId: accountId, installId: installId, wraps: wraps)
            logger.info("Wrapped \(wraps.count) tree keys for other readers")
        } catch {
            logger.error("sessionKeys.pending/wrap failed: \(error.localizedDescription)")
        }
    }

    // MARK: writing

    /// #EDE-97 (sealed-content.json `clearTree`, holder `late`): this
    /// install's id on `accountId`, waiting up to `wait` for a holder still
    /// coming up (its registration nudged once); nil = none within the wait
    /// (signed out, no eNet, registration refused).
    public func readyInstall(accountId: String, wait: Duration) async -> String? {
        guard crypto.isAvailable else { return nil }
        if let install = await installId(accountId) { return install }
        register(accountId)
        let clock = ContinuousClock()
        let deadline = clock.now + wait
        while clock.now < deadline {
            try? await Task.sleep(for: min(Self.readyPoll, deadline - clock.now))
            if Task.isCancelled { return nil }
            if let install = await installId(accountId) { return install }
        }
        return nil
    }

    static let readyPoll: Duration = .seconds(1)

    /// The key a NEW chat starts with: a fresh keypair, held here, with
    /// wraps for the readers this install can resolve. Store the secret under
    /// the new root's id once the server answered (`adopt`).
    public func newChatSeal(
        accountId: String,
        teamId: String?,
        deviceIds: [String],
        shared: Bool
    ) async throws -> (seal: NewChatSeal, key: GeneratedTreeKey) {
        guard crypto.isAvailable else { throw TreeKeyError.unavailable }
        guard let install = await installId(accountId) else { throw TreeKeyError.noInstall }
        let key = crypto.generateTreeKey()
        // Its root id is unknown until the server answers: unsigned now,
        // signed again for the same readers once adopted.
        let readers = try await readers(accountId: accountId, installId: install, teamId: teamId, deviceIds: deviceIds, shared: shared)
        let wraps = try readers.map { TreeKeyWrap(installId: $0.installId, wrapped: try crypto.wrap(installPublic: $0.x25519Public, treeSecret: key.secret)) }
        unsignedSeals[key.publicKey] = readers
        return (NewChatSeal(installId: install, publicKey: key.publicKey, epoch: 1, wraps: wraps), key)
    }

    /// The server created `rootId` with the key from `newChatSeal`.
    public func adopt(rootId: String, key: GeneratedTreeKey, epoch: UInt32 = 1) {
        ring.add([(rootId, epoch, key.secret)])
        if let readers = unsignedSeals.removeValue(forKey: key.publicKey) {
            resignQueue[rootId.lowercased()] = (epoch, readers)
        }
    }

    /// #EXP-1950 decision (c): fresh wraps of the current epoch, made and
    /// signed by this install, replace the unsigned ones (`sessionKeys.resign`;
    /// the server never touches a signed row and skips a stale epoch): for a
    /// new root's inline seal its readers, for a key held from before this
    /// install's own wrap and its account's other installs. Once per root, at
    /// most `resignBatch` roots per pass; an older server (NOT_FOUND) = not
    /// yet, tried again on a later pass.
    @discardableResult
    func resignHeld(accountId: String, installId: String) async -> Int {
        guard signer != nil, !resignQueue.isEmpty, let own = installKey(), let endpoint = ownEndpointId() else { return 0 }
        let batch = resignQueue.keys.sorted().prefix(Self.resignBatch)
        var accountInstalls: [InstallEntry]?
        var replaced = 0
        for root in batch {
            guard let entry = resignQueue[root] else { continue }
            let mark = "\(root)\n\(entry.epoch)"
            guard let secret = ring.keys(rootId: root)[entry.epoch], let treePublic = TreePublicKeys.of(secret: secret) else {
                resignQueue[root] = nil
                continue
            }
            var readers = entry.readers
            if readers.isEmpty {
                if accountInstalls == nil, let installs { accountInstalls = (try? await installs.mine(accountId: accountId)) ?? [] }
                readers = (accountInstalls ?? []).filter { $0.revokedAt == nil && $0.installId != installId }
            }
            readers.removeAll { $0.installId == installId }
            readers.insert(InstallEntry(installId: installId, userId: nil, kind: InstallEntry.iosKind, endpointId: endpoint, x25519Public: own.publicKey), at: 0)
            let wraps = resignWraps(rootId: root, epoch: entry.epoch, treePublic: treePublic, secret: secret, readers: readers)
            do {
                replaced += try await keys.resign(accountId: accountId, installId: installId, wraps: wraps).replaced
            } catch {
                if error.trpcErrorCode == "NOT_FOUND" || (error as? TreeKeyError) == .unavailable { return replaced }
                guard error.trpcErrorCode == "BAD_REQUEST" || error.trpcErrorCode == "FORBIDDEN" else { continue }
                // A target that is no reader refuses the call: our own wrap alone.
                if wraps.count > 1 {
                    let ownOnly = Array(wraps.prefix(1))
                    replaced += (try? await keys.resign(accountId: accountId, installId: installId, wraps: ownOnly).replaced) ?? 0
                }
            }
            resignQueue[root] = nil
            resigned.insert(mark)
        }
        if replaced > 0 { logger.info("Signed \(replaced) tree key wraps again") }
        return replaced
    }

    private func resignWraps(rootId: String, epoch: UInt32, treePublic: String, secret: Data, readers: [InstallEntry]) -> [ResignedTreeKeyWrap] {
        readers.compactMap { reader in
            guard !reader.x25519Public.isEmpty,
                  let wrapped = try? crypto.wrap(installPublic: reader.x25519Public, treeSecret: secret),
                  let signed = signature(rootId: rootId, epoch: epoch, treePublicKey: treePublic, recipientEndpointId: reader.endpointId, wrapped: wrapped)
            else { return nil }
            return ResignedTreeKeyWrap(rootId: rootId, epoch: epoch, installId: reader.installId, wrapped: wrapped, signature: signed)
        }
    }

    /// The key to seal to on an EXISTING root: its synced key when this
    /// install holds it, else the server's (fetching our wrap), else a new
    /// one (`sessionKeys.create`, epoch 1). nil = no key can be had (no eNet,
    /// not registered): the caller sends plaintext.
    /// `lookup` false = the caller already knows the root is keyless (#FEED-250,
    /// sealed-content.json `staleTree`): create first, the server's key only
    /// when the create is refused.
    public func sealKey(
        accountId: String,
        rootId: String,
        known: TreeSealKey?,
        teamId: String?,
        deviceIds: [String],
        shared: Bool,
        lookup: Bool = true
    ) async -> TreeSealKey? {
        guard crypto.isAvailable, let install = await installId(accountId) else { return nil }
        if let known { return known }
        if lookup, let remote = try? await keys.publicKeys(accountId: accountId, rootIds: [rootId]).first {
            return TreeSealKey(publicKey: remote.publicKey, epoch: remote.epoch)
        }
        var outcome = await createKey(accountId: accountId, rootId: rootId, teamId: teamId, deviceIds: deviceIds, shared: shared)
        if case .failed(.wrapRefused) = outcome {
            outcome = await createKey(accountId: accountId, rootId: rootId, teamId: teamId, deviceIds: deviceIds, shared: shared, ownOnly: true)
        }
        if case let .keyed(key) = outcome { return key }
        // Another writer keyed the root meanwhile: seal to theirs.
        _ = await fetch(accountId: accountId, installId: install, rootIds: [rootId])
        return await serverKey(accountId: accountId, rootId: rootId)
    }

    /// #EXP-1960: one `sessionKeys.create` (epoch 1) for a keyless root, its
    /// failure read for the writer's `keylessStep`; `ownOnly` = wrapped for
    /// this account's own installs only (`retryOwn`).
    public func createKey(
        accountId: String,
        rootId: String,
        teamId: String?,
        deviceIds: [String],
        shared: Bool,
        ownOnly: Bool = false
    ) async -> TreeKeyCreate {
        guard crypto.isAvailable, let install = await installId(accountId) else { return .unavailable }
        let key = crypto.generateTreeKey()
        let wraps: [TreeKeyWrap]
        do {
            let readers = try await readers(
                accountId: accountId, installId: install, teamId: ownOnly ? nil : teamId,
                deviceIds: ownOnly ? [] : deviceIds, shared: ownOnly ? false : shared
            )
            wraps = try readers.map { reader in
                let wrapped = try crypto.wrap(installPublic: reader.x25519Public, treeSecret: key.secret)
                return TreeKeyWrap(installId: reader.installId, wrapped: wrapped, signature: signature(
                    rootId: rootId, epoch: 1, treePublicKey: key.publicKey, recipientEndpointId: reader.endpointId, wrapped: wrapped
                ))
            }
        } catch {
            return .unavailable
        }
        do {
            try await keys.create(accountId: accountId, input: CreateTreeKeyInput(rootId: rootId, installId: install, epoch: 1, publicKey: key.publicKey, wraps: wraps))
        } catch {
            logger.error("sessionKeys.create failed: \(error.localizedDescription)")
            let body: TrpcErrorBody? = if case let .httpError(_, raw) = error as? TrpcError { TrpcErrorBody.parse(raw) } else { nil }
            return .failed(createFailure(code: body?.code, message: body?.message))
        }
        ring.add([(rootId, 1, key.secret)])
        return .keyed(TreeSealKey(publicKey: key.publicKey, epoch: 1))
    }

    /// #EXP-1960: asks the devices of a keyless tree this install may not key
    /// to key it (`sessionKeys.requestKey`); `key` = the tree's when `keyed`.
    public func requestKey(accountId: String, rootId: String) async -> (answer: RequestKeyAnswer, key: TreeSealKey?) {
        do {
            let result = try await keys.requestKey(accountId: accountId, rootId: rootId)
            guard let answer = RequestKeyAnswer(rawValue: result.state), answer != .notFound, answer != .error else { return (.error, nil) }
            let key = result.epoch.flatMap { epoch in result.publicKey.map { TreeSealKey(publicKey: $0, epoch: epoch) } }
            return (answer, key)
        } catch {
            logger.error("sessionKeys.requestKey failed: \(error.localizedDescription)")
            return (error.trpcErrorCode == "NOT_FOUND" ? .notFound : .error, nil)
        }
    }

    /// #FEED-250: the server's conversation root with an agent / app (the
    /// synced store is a cache); nil = it could not be asked.
    public func sendTarget(accountId: String, agentId: String?, appId: String?) async -> ServerSendTarget? {
        do {
            return try await keys.sendTarget(accountId: accountId, agentId: agentId, appId: appId)
        } catch {
            logger.error("sessions.sendTarget failed: \(error.localizedDescription)")
            return nil
        }
    }

    /// The server's current key of `rootId` (`sessionKeys.publicKeys`), if keyed.
    public func serverKey(accountId: String, rootId: String) async -> TreeSealKey? {
        guard let remote = try? await keys.publicKeys(accountId: accountId, rootIds: [rootId]).first else { return nil }
        return TreeSealKey(publicKey: remote.publicKey, epoch: remote.epoch)
    }

    private func readers(
        accountId: String,
        installId: String,
        teamId: String?,
        deviceIds: [String],
        shared: Bool
    ) async throws -> [InstallEntry] {
        var own: [InstallEntry] = []
        var team: [InstallEntry] = []
        if let installs {
            own = (try? await installs.mine(accountId: accountId)) ?? []
            if let teamId, shared || !deviceIds.isEmpty {
                team = (try? await installs.list(accountId: accountId, teamId: teamId)) ?? []
            }
        }
        var readers = TreeReaders.resolve(own: own, team: team, selfInstallId: installId, deviceIds: deviceIds, shared: shared)
        // installs.mine failed or lags the registration: the server needs our
        // own wrap, from our own derived key.
        if !readers.contains(where: { $0.installId == installId }) {
            guard let key = installKey() else { throw TreeKeyError.noInstall }
            readers.insert(InstallEntry(installId: installId, userId: nil, kind: InstallEntry.iosKind, endpointId: ownEndpointId() ?? "", x25519Public: key.publicKey), at: 0)
        }
        return readers.filter { !$0.x25519Public.isEmpty }
    }
}
