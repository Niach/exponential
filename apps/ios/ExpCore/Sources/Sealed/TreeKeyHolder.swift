import Foundation
import os

// EXP-1459: this install as a HOLDER of session-tree keys. It fetches the
// wraps addressed to it (`sessionKeys.mine`), unwraps them with its X25519
// secret (derived from the install identity, `PushKeypair::derive`) into the
// key ring, and answers `sessionKeys.pending` on start and every minute while
// online: readers still missing a wrap for a tree it holds get one,
// automatically (no approval, #EXP-1457 decision). As a WRITER it seals to a
// tree's key, creating the key when an existing root has none yet, or hands
// a new thread its inline `seal`.

private let logger = Logger(subsystem: "at.exponential", category: "TreeKeys")

/// `sessions.send`'s inline `seal` for a root without a key (a new thread,
/// agent conversation or app editor; contract addendum 2, 2026-10-30): the
/// server creates the root, the key and the sealed first message in one
/// transaction, under `sessionKeys.create`'s rules.
public struct NewThreadSeal: Encodable, Equatable, Sendable {
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
    private let installs: InstallsApi?
    private let crypto: SealedCrypto
    private let ring: SealedKeyRing
    private let identity: @Sendable () -> Data?
    private let installId: @Sendable (String) async -> String?
    private let now: @Sendable () -> Date
    private var requested: [String: Date] = [:]
    private var synced: Set<String> = []
    private var cachedKey: (identity: Data, secret: Data, publicKey: String)?

    public init(
        keys: SessionKeysRegistry,
        installs: InstallsApi?,
        crypto: SealedCrypto,
        ring: SealedKeyRing,
        identity: @escaping @Sendable () -> Data?,
        installId: @escaping @Sendable (String) async -> String?,
        now: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.keys = keys
        self.installs = installs
        self.crypto = crypto
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

    /// One holder pass for an account: every wrap this install holds (the
    /// first pass) and the pending wraps it owes others.
    /// False while this account has no registered install (nothing ran).
    @discardableResult
    public func sync(accountId: String) async -> Bool {
        guard crypto.isAvailable, let install = await installId(accountId) else { return false }
        if !synced.contains(accountId) {
            if await fetch(accountId: accountId, installId: install, rootIds: nil) { synced.insert(accountId) }
        }
        await answerPending(accountId: accountId, installId: install)
        return true
    }

    /// The identity rotated: every held secret was wrapped for the old key.
    public func forgetAll() {
        synced.removeAll()
        requested.removeAll()
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
            return true
        } catch {
            logger.error("sessionKeys.mine failed: \(error.localizedDescription)")
            return false
        }
    }

    func answerPending(accountId: String, installId: String) async {
        do {
            let pending = try await keys.pending(accountId: accountId, installId: installId)
            var wraps: [PendingTreeKeyWrap] = []
            for pair in pending {
                guard let secret = ring.keys(rootId: pair.rootId)[pair.epoch],
                      let wrapped = try? crypto.wrap(installPublic: pair.targetX25519Public, treeSecret: secret) else { continue }
                wraps.append(PendingTreeKeyWrap(rootId: pair.rootId, epoch: pair.epoch, targetInstallId: pair.targetInstallId, wrapped: wrapped))
            }
            guard !wraps.isEmpty else { return }
            try await keys.wrap(accountId: accountId, installId: installId, wraps: wraps)
            logger.info("Wrapped \(wraps.count) tree keys for other readers")
        } catch {
            logger.error("sessionKeys.pending/wrap failed: \(error.localizedDescription)")
        }
    }

    // MARK: writing

    /// The key a NEW thread starts with: a fresh keypair, held here, with
    /// wraps for the readers this install can resolve. Store the secret under
    /// the new root's id once the server answered (`adopt`).
    public func newThreadSeal(
        accountId: String,
        teamId: String?,
        deviceIds: [String],
        shared: Bool
    ) async throws -> (seal: NewThreadSeal, key: GeneratedTreeKey) {
        guard crypto.isAvailable else { throw TreeKeyError.unavailable }
        guard let install = await installId(accountId) else { throw TreeKeyError.noInstall }
        let key = crypto.generateTreeKey()
        let wraps = try await readerWraps(accountId: accountId, installId: install, teamId: teamId, deviceIds: deviceIds, shared: shared, secret: key.secret)
        return (NewThreadSeal(installId: install, publicKey: key.publicKey, epoch: 1, wraps: wraps), key)
    }

    /// The server created `rootId` with the key from `newThreadSeal`.
    public func adopt(rootId: String, key: GeneratedTreeKey, epoch: UInt32 = 1) {
        ring.add([(rootId, epoch, key.secret)])
    }

    /// The key to seal to on an EXISTING root: its synced key when this
    /// install holds it, else the server's (fetching our wrap), else a new
    /// one (`sessionKeys.create`, epoch 1). nil = no key can be had (no eNet,
    /// not registered): the caller sends plaintext.
    public func sealKey(
        accountId: String,
        rootId: String,
        known: TreeSealKey?,
        teamId: String?,
        deviceIds: [String],
        shared: Bool
    ) async -> TreeSealKey? {
        guard crypto.isAvailable, let install = await installId(accountId) else { return nil }
        if let known { return known }
        if let remote = try? await keys.publicKeys(accountId: accountId, rootIds: [rootId]).first {
            return TreeSealKey(publicKey: remote.publicKey, epoch: remote.epoch)
        }
        let key = crypto.generateTreeKey()
        do {
            let wraps = try await readerWraps(accountId: accountId, installId: install, teamId: teamId, deviceIds: deviceIds, shared: shared, secret: key.secret)
            try await createKey(accountId: accountId, rootId: rootId, installId: install, key: key, wraps: wraps)
            ring.add([(rootId, 1, key.secret)])
            return TreeSealKey(publicKey: key.publicKey, epoch: 1)
        } catch {
            // Another writer keyed the root meanwhile: seal to theirs.
            logger.error("sessionKeys.create failed: \(error.localizedDescription)")
            _ = await fetch(accountId: accountId, installId: install, rootIds: [rootId])
            if let remote = try? await keys.publicKeys(accountId: accountId, rootIds: [rootId]).first {
                return TreeSealKey(publicKey: remote.publicKey, epoch: remote.epoch)
            }
            return nil
        }
    }

    private func createKey(accountId: String, rootId: String, installId: String, key: GeneratedTreeKey, wraps: [TreeKeyWrap]) async throws {
        do {
            try await keys.create(accountId: accountId, input: CreateTreeKeyInput(rootId: rootId, installId: installId, epoch: 1, publicKey: key.publicKey, wraps: wraps))
        } catch TrpcError.httpError(400, _) where wraps.count > 1 {
            // A resolved reader the server does not count: our own wrap only,
            // holders fill the rest.
            let own = wraps.filter { $0.installId == installId }
            try await keys.create(accountId: accountId, input: CreateTreeKeyInput(rootId: rootId, installId: installId, epoch: 1, publicKey: key.publicKey, wraps: own))
        }
    }

    private func readerWraps(
        accountId: String,
        installId: String,
        teamId: String?,
        deviceIds: [String],
        shared: Bool,
        secret: Data
    ) async throws -> [TreeKeyWrap] {
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
            readers.insert(InstallEntry(installId: installId, userId: nil, kind: InstallRegistration.kind, endpointId: "", x25519Public: key.publicKey), at: 0)
        }
        readers = readers.filter { !$0.x25519Public.isEmpty }
        return try readers.map { TreeKeyWrap(installId: $0.installId, wrapped: try crypto.wrap(installPublic: $0.x25519Public, treeSecret: secret)) }
    }
}
