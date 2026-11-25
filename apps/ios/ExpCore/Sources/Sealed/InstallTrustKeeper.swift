import Foundation
import os

// #EXP-1950 (fixtures/install-trust.json; web `lib/enet/install-trust.ts`):
// this install's side of install approval and the four rules, per account,
// around the pure `InstallTrust`. It follows `installs.trust` into the pin
// store (the Keychain, `TrustStorePersistence`), claims its account's root
// when there is no chain (with the approvals of the installs that existed
// with it, decision e), asks ONCE to be approved when it waits, and
// answers for the tree key holder whether a wrap is kept (R1), a key sealed
// to (R2, R3) and a target wrapped for (R4). Release 1 is RECORD-ONLY
// (`TrustMode.build`) for keys; the one thing it refuses that a released
// build did is a wrap for an install that ASKED (or claims a root) and is
// not approved (and a waiting install wraps for nobody but itself).
// It APPROVES a new install only when a person presses Approve; by itself
// only the installs that existed with it once (`approveExisting`).
// An older server without `installs.trust` (NOT_FOUND) = `unavailable`:
// every target is wrapped as before and recorded `legacy`.

private let logger = Logger(subsystem: "at.exponential", category: "InstallTrust")

/// The persistent side of the pin store: one item per account (the app's
/// own Keychain group, `AfterFirstUnlockThisDeviceOnly`; no extension reads it).
public struct TrustStorePersistence: Sendable {
    /// nil = no store for the account (decision e: an existing install);
    /// throws when it cannot be read, never taken for none.
    public let load: @Sendable (_ accountId: String) throws -> Data?
    public let save: @Sendable (_ accountId: String, _ data: Data) -> Void
    public let removeAll: @Sendable () -> Void

    public init(
        load: @escaping @Sendable (String) throws -> Data?,
        save: @escaping @Sendable (String, Data) -> Void,
        removeAll: @escaping @Sendable () -> Void
    ) {
        self.load = load
        self.save = save
        self.removeAll = removeAll
    }

    /// In memory (tests, previews).
    public static func memory() -> TrustStorePersistence {
        let box = MemoryBox()
        return TrustStorePersistence(
            load: { box.get($0) },
            save: { box.set($0, $1) },
            removeAll: { box.clear() }
        )
    }

    private final class MemoryBox: @unchecked Sendable {
        private let lock = NSLock()
        private var items: [String: Data] = [:]
        func get(_ key: String) -> Data? { lock.withLock { items[key] } }
        func set(_ key: String, _ data: Data) { lock.withLock { items[key] = data } }
        func clear() { lock.withLock { items.removeAll() } }
    }
}

/// `trusted` / `waiting` / `claimRoot` (`TrustStatus`), `unknown` before the
/// first answer, `unavailable` on a server without `installs.trust`.
public enum InstallTrustState: String, Equatable, Sendable { case unknown, unavailable, trusted, waiting, claimRoot }

/// What the UI draws: the rows (Needs you, the phone's Devices) and this
/// install's own waiting state with its code.
public struct InstallTrustSnapshot: Equatable, Sendable {
    public var state: InstallTrustState
    public var rows: [TrustRow]
    /// This install's own code (shown while it waits).
    public var code: String?

    public init(state: InstallTrustState, rows: [TrustRow], code: String?) {
        self.state = state
        self.rows = rows
        self.code = code
    }

    public static let none = InstallTrustSnapshot(state: .unknown, rows: [], code: nil)

    /// The rows Needs you lists: none while this install itself waits
    /// (nav-badges.json `installs`).
    public var waitingRows: Int { waiting ? 0 : rows.count }

    /// Waiting for approval: the waiting EmptyState instead of the rows.
    public var waiting: Bool { state == .waiting && code != nil }
}

extension Notification.Name {
    /// Posted (any thread) when an account's trust state or rows changed;
    /// userInfo `accountId`.
    public static let installTrustChanged = Notification.Name("at.exponential.installTrustChanged")
}

/// One recorded decision (the newest last, bounded).
public struct TrustDecision: Equatable, Sendable {
    public enum Rule: String, Sendable { case r1 = "R1", r3 = "R3", r4 = "R4", call }
    public let rule: Rule
    public let subject: String
    public let verdict: String
}

public actor InstallTrustKeeper {
    static let decisionLog = 200

    private struct Account {
        var installId: String?
        var store: TrustStore
        var state: InstallTrustState = .unknown
        var rows: [TrustRow] = []
        var served: [String: ServedInstall] = [:]
        var roots: Set<String> = []
        var asked = false
        /// The kept store could not be read: never cached, never synced
        /// (a write would replace it).
        var unreadable = false
    }

    private let api: InstallTrustApi
    private let crypto: TrustCrypto
    private let persistence: TrustStorePersistence
    private let identity: @Sendable () -> Data?
    public let mode: TrustMode
    private var accounts: [String: Account] = [:]
    private var cachedEndpoint: (identity: Data, endpointId: String)?
    public private(set) var decisions: [TrustDecision] = []

    public init(
        api: InstallTrustApi,
        crypto: TrustCrypto,
        persistence: TrustStorePersistence,
        identity: @escaping @Sendable () -> Data?,
        mode: TrustMode = .build
    ) {
        self.api = api
        self.crypto = crypto
        self.persistence = persistence
        self.identity = identity
        self.mode = mode
    }

    /// This install's endpoint id (its identity's), nil without one.
    public func endpointId() -> String? {
        guard let seed = identity() else { return nil }
        if let cachedEndpoint, cachedEndpoint.identity == seed { return cachedEndpoint.endpointId }
        guard let id = try? crypto.endpointId(identity: seed) else { return nil }
        if cachedEndpoint != nil { accounts.removeAll() } // a rotated identity: every store is another install's
        cachedEndpoint = (seed, id)
        return id
    }

    /// The account's pin store as kept (its own endpoint's; another is
    /// ignored). None at all = decision (e): this install EXISTED (a build
    /// that mints the identity or signs a new account in writes the store
    /// first, `markNew`), so it may adopt at its first reconcile. A store
    /// kept for another endpoint (a rotated identity) or that does not
    /// decode is a new install's; one that cannot be read is never none.
    private func account(_ accountId: String) -> Account? {
        guard let endpoint = endpointId() else { return nil }
        if let found = accounts[accountId], found.store.selfEndpointId == endpoint { return found }
        let data: Data?
        do {
            data = try persistence.load(accountId)
        } catch {
            logger.error("The pin store could not be read: \(error.localizedDescription)")
            var unread = Account(store: TrustStore(selfEndpointId: endpoint, userId: ""))
            unread.unreadable = true
            return unread
        }
        var store = TrustStore(selfEndpointId: endpoint, userId: "", existing: data == nil)
        if let data, let kept = try? JSONDecoder().decode(TrustStore.self, from: data), kept.selfEndpointId == endpoint {
            store = kept
        }
        let fresh = Account(store: store)
        accounts[accountId] = fresh
        return fresh
    }

    /// Decision (e): writes the account's empty store (`existing` false)
    /// BEFORE this install registers for it: an account signed in under this
    /// build, or every signed-in account of an identity this build minted. A
    /// store this endpoint already keeps stays; one that cannot be read is
    /// left alone.
    public static func markNew(accountId: String, identity: Data, crypto: TrustCrypto, persistence: TrustStorePersistence) {
        guard let endpoint = try? crypto.endpointId(identity: identity) else { return }
        let kept: Data?
        do {
            kept = try persistence.load(accountId)
        } catch {
            return
        }
        if let kept, let store = try? JSONDecoder().decode(TrustStore.self, from: kept), store.selfEndpointId == endpoint { return }
        guard let empty = try? JSONEncoder().encode(TrustStore(selfEndpointId: endpoint, userId: "")) else { return }
        persistence.save(accountId, empty)
    }

    private func persist(_ accountId: String) {
        guard let store = accounts[accountId]?.store, let data = try? JSONEncoder().encode(store) else { return }
        persistence.save(accountId, data)
    }

    public func snapshot(accountId: String) -> InstallTrustSnapshot {
        guard let found = account(accountId) else { return .none }
        return InstallTrustSnapshot(state: found.state, rows: found.rows, code: InstallCode.of(endpointId: found.store.selfEndpointId))
    }

    /// The pin store (tests, the holder's records).
    public func store(accountId: String) -> TrustStore? { account(accountId)?.store }

    private func set(_ accountId: String, state: InstallTrustState, rows: [TrustRow]? = nil) {
        guard var found = accounts[accountId] else { return }
        let changed = found.state != state || (rows != nil && rows != found.rows)
        found.state = state
        if let rows { found.rows = rows }
        accounts[accountId] = found
        if changed {
            NotificationCenter.default.post(name: .installTrustChanged, object: nil, userInfo: ["accountId": accountId])
        }
    }

    // MARK: following the server

    /// Follows `installs.trust` once: claims the root when the account has
    /// no chain (with the existing installs' approvals), approves the
    /// existing ones its chain does not reach yet, asks once while it waits.
    public func sync(accountId: String, installId: String) async {
        guard let seed = identity(), var current = account(accountId), !current.unreadable else { return }
        current.installId = installId
        accounts[accountId] = current
        for _ in 0..<3 {
            let answer: TrustAnswer
            do {
                answer = try await api.trust(accountId: accountId, installId: installId)
            } catch {
                if error.trpcErrorCode == "NOT_FOUND" { set(accountId, state: .unavailable, rows: []) }
                return
            }
            guard var found = accounts[accountId] else { return }
            // The account's user, from this install's own row.
            if let me = answer.installs.first(where: { $0.installId == installId }), me.userId != found.store.userId {
                if found.store.userId.isEmpty {
                    found.store.userId = me.userId
                } else {
                    found.store = TrustStore(selfEndpointId: found.store.selfEndpointId, userId: me.userId)
                }
            }
            guard !found.store.userId.isEmpty else { return }
            let view = InstallTrust.reconcile(found.store, served: answer, crypto: crypto)
            found.store = view.store
            found.served = Dictionary(answer.installs.map { ($0.endpointId, $0) }, uniquingKeysWith: { first, _ in first })
            found.roots = Set(answer.approvals.filter { $0.kind == .root && $0.approverEndpointId == $0.subjectEndpointId }.map(\.subjectEndpointId))
            accounts[accountId] = found
            persist(accountId)
            set(accountId, state: InstallTrustState(rawValue: view.status.rawValue) ?? .unknown, rows: view.rows)
            let me = found.served[found.store.selfEndpointId]
            switch view.status {
            case .claimRoot:
                guard let me, let signature = try? InstallApproval.sign(.root, subject: me, identity: seed, crypto: crypto) else { return }
                let approvals = existingApprovals(view.approveExisting, of: found, identity: seed)
                if (try? await api.claimRoot(accountId: accountId, installId: installId, signature: signature, approvals: approvals)) != nil {
                    logger.info("Claimed the root of this account's install chain")
                }
                // A CONFLICT (another install started the chain meanwhile) is read next round.
                continue
            case .trusted:
                for approval in existingApprovals(view.approveExisting, of: found, identity: seed) {
                    try? await api.approve(
                        accountId: accountId, installId: installId, subjectInstallId: approval.subjectInstallId, kind: .approve, signature: approval.signature
                    )
                }
            case .waiting:
                if accounts[accountId]?.asked == false, me?.approvalRequested != true {
                    let asked = (try? await api.requestApproval(accountId: accountId, installId: installId)) != nil
                    accounts[accountId]?.asked = asked
                }
            }
            return
        }
    }

    /// The `approve`s of the installs that existed with this one (decision
    /// e), over the key it pinned them with.
    private func existingApprovals(_ endpointIds: [String], of account: Account, identity seed: Data) -> [RootClaimApproval] {
        endpointIds.compactMap { endpointId in
            guard let row = account.served[endpointId], let pin = account.store.pins[endpointId],
                  let message = InstallApproval.message(.approve, userId: pin.userId, endpointId: endpointId, x25519Public: pin.x25519Public),
                  let signature = try? crypto.sign(identity: seed, message: message) else { return nil }
            return RootClaimApproval(subjectInstallId: row.installId, signature: signature)
        }
    }

    /// A person pressed Approve on a row.
    public func approve(accountId: String, row: TrustRow) async throws {
        guard let found = account(accountId), found.state == .trusted, let installId = found.installId else { throw InstallTrustError.notTrusted }
        guard let seed = identity(), let subject = found.served[row.endpointId], subject.installId == row.installId else { throw InstallTrustError.installNotFound }
        let kind: ApprovalKind = row.kind == .reset ? .reset : .approve
        let signature = try InstallApproval.sign(kind, subject: subject, identity: seed, crypto: crypto)
        try await api.approve(accountId: accountId, installId: installId, subjectInstallId: row.installId, kind: kind, signature: signature)
        await sync(accountId: accountId, installId: installId)
    }

    /// A person pressed Remove: an own install is revoked (its wraps go, its
    /// trees rotate); a team-mate's reset is declined.
    public func remove(accountId: String, row: TrustRow) async throws {
        guard let found = account(accountId), found.state == .trusted, let installId = found.installId else { throw InstallTrustError.notTrusted }
        switch row.kind {
        case .newInstall:
            try await api.revoke(accountId: accountId, installId: row.installId)
        case .reset:
            guard let seed = identity(), let subject = found.served[row.endpointId], subject.installId == row.installId else { throw InstallTrustError.installNotFound }
            let signature = try InstallApproval.sign(.decline, subject: subject, identity: seed, crypto: crypto)
            try await api.approve(accountId: accountId, installId: installId, subjectInstallId: row.installId, kind: .decline, signature: signature)
        }
        await sync(accountId: accountId, installId: installId)
    }

    // MARK: the rules, for the key holder

    private func log(_ rule: TrustDecision.Rule, _ subject: String, _ verdict: String) {
        decisions.append(TrustDecision(rule: rule, subject: subject, verdict: verdict))
        if decisions.count > Self.decisionLog { decisions.removeFirst(decisions.count - Self.decisionLog) }
    }

    /// #EXP-2123 (install-trust.json `keyMaker`): only a JOINED install makes
    /// a chat key (an older server without `installs.trust` counts as joined).
    /// A store not reconciled yet is reconciled first.
    public func makesKeys(accountId: String, installId: String) async -> Bool {
        if (account(accountId)?.state ?? .unknown) == .unknown { await sync(accountId: accountId, installId: installId) }
        guard let found = account(accountId) else { return false }
        return found.state == .unavailable || found.store.joined
    }

    /// R1: whether a served wrap is kept, recorded in the store.
    public func keep(accountId: String, wrap: HeldTreeKey, installSecret: Data) -> InstallTrust.KeepVerdict {
        guard let seed = identity(), let found = account(accountId) else { return .refuse(.malformed) }
        let verdict = InstallTrust.keepWrap(wrap, identity: seed, installSecret: installSecret, store: found.store, mode: mode, crypto: crypto)
        switch verdict {
        case let .keep(kept, _, _):
            log(.r1, "\(wrap.rootId)#\(wrap.epoch)", kept.rawValue)
            record(accountId: accountId, rootId: wrap.rootId, epoch: wrap.epoch, record: kept)
        case let .refuse(reason):
            log(.r1, "\(wrap.rootId)#\(wrap.epoch)", "refused:\(reason.rawValue)")
        }
        return verdict
    }

    /// What this install recorded about a key it holds (`legacy` = held before, decision c).
    public func recordOf(accountId: String, rootId: String, epoch: UInt32) -> KeyRecord {
        account(accountId)?.store.record(rootId: rootId, epoch: epoch) ?? .legacy
    }

    /// Records a key (its own new keys are `verified`); persisted.
    public func record(accountId: String, rootId: String, epoch: UInt32, record: KeyRecord) {
        guard var found = account(accountId), !found.unreadable else { return }
        if found.store.roots[rootId.lowercased()]?.epochs[String(epoch)] == record { return }
        found.store.recordKey(rootId: rootId, epoch: epoch, record: record)
        accounts[accountId] = found
        persist(accountId)
    }

    /// Decision (c): every key held before the store knew it stays, `legacy`.
    public func recordLegacy(accountId: String, held: [(rootId: String, epoch: UInt32)]) {
        guard var found = account(accountId), !found.unreadable else { return }
        var changed = false
        for key in held where found.store.roots[key.rootId.lowercased()]?.epochs[String(key.epoch)] == nil {
            found.store.recordKey(rootId: key.rootId, epoch: key.epoch, record: .legacy)
            changed = true
        }
        guard changed else { return }
        accounts[accountId] = found
        persist(accountId)
    }

    /// R2 + R3: whether this install seals to (root, epoch, key).
    public func seal(accountId: String, rootId: String, key: TreeSealKey, held: Data?) -> InstallTrust.SealVerdict {
        guard let found = account(accountId) else { return .seal(record: .legacy) }
        let verdict = InstallTrust.sealTo(
            rootId: rootId.lowercased(), epoch: key.epoch, publicKey: key.publicKey,
            held: held.map { (secret: $0, record: found.store.record(rootId: rootId, epoch: key.epoch)) },
            vouch: key.vouch, store: found.store, mode: mode, crypto: crypto
        )
        switch verdict {
        case let .seal(record): log(.r3, "\(rootId)#\(key.epoch)", record.rawValue)
        case let .refuse(reason): log(.r3, "\(rootId)#\(key.epoch)", "refused:\(reason.rawValue)")
        }
        return verdict
    }

    /// R4: whether this install wraps a key for a target the server names
    /// (`pending`'s pair, a reader of a new key, a rotation, a resign).
    public func target(accountId: String, installId: String, x25519Public: String, endpointId: String?, userId: String?) -> InstallTrust.TargetVerdict {
        guard let found = account(accountId) else { return .refuse(.notTrusted) }
        if found.state == .unavailable { return .wrap(record: .legacy) }
        if installId == found.installId { return .wrap(record: .verified) }
        let known = endpointId.flatMap { found.served[$0] } ?? found.served.values.first { $0.installId == installId }
        let served = ServedInstall(
            installId: installId,
            userId: userId ?? known?.userId ?? "",
            endpointId: endpointId ?? known?.endpointId ?? "",
            x25519Public: x25519Public,
            registrationSignature: known?.registrationSignature,
            registrationSignedAt: known?.registrationSignedAt,
            approvalRequested: known?.approvalRequested ?? false
        )
        let verdict: InstallTrust.TargetVerdict
        // The registration binds the key `installs.trust` lists; a target
        // served elsewhere with another key is checked against it.
        if let known, known.x25519Public != x25519Public, found.store.pins[served.endpointId] == nil {
            verdict = .refuse(.keyMismatch)
        } else {
            verdict = InstallTrust.wrapTarget(served, store: found.store, mode: mode, roots: found.roots, crypto: crypto)
        }
        switch verdict {
        case let .wrap(record): log(.r4, installId, record.rawValue)
        case let .refuse(reason): log(.r4, installId, "refused:\(reason.rawValue)")
        }
        return verdict
    }

    /// A call's key: the keyring's secret of (root, epoch), by the rule; nil = `no_key`.
    public func callKey(accountId: String, rootId: String, epoch: UInt32, held: Data?) -> Data? {
        guard let found = account(accountId) else { return held }
        let verdict = InstallTrust.callKey(
            rootId: rootId, epoch: epoch, held: held.map { (secret: $0, record: found.store.record(rootId: rootId, epoch: epoch)) },
            store: found.store, mode: mode
        )
        switch verdict {
        case let .key(secret, record):
            log(.call, "\(rootId)#\(epoch)", record.rawValue)
            return secret
        case .noKey:
            log(.call, "\(rootId)#\(epoch)", "noKey")
            return nil
        case .rollback:
            log(.call, "\(rootId)#\(epoch)", "rollback")
            return nil
        }
    }

    /// The identity rotated or the last account signed out.
    public func forgetAll() {
        accounts.removeAll()
        cachedEndpoint = nil
        persistence.removeAll()
    }
}
