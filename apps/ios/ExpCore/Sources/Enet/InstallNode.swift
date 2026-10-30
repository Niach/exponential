import Foundation
import os

// EXP-1457 step 1 / VAPP-37 / VAPP-40: every install runs an eNet node and
// registers it with the install registry. This file is binding-free (the
// pure registration + allowlist rules and the service over two small
// protocols), so it compiles and is tested without the xcframework;
// `EnetBinding.swift` plugs the real node in when it is linked (`ENET_NODE`).
// No message content yet (EXP-1458 / EXP-1459): the node only admits its
// teams' installs.

private let logger = Logger(subsystem: "at.exponential", category: "InstallNode")

/// The eNet crypto an install needs (the binding's free functions).
public protocol InstallCrypto: Sendable {
    /// Lowercase hex of the identity's ed25519 public key (its endpoint id).
    func endpointId(identity: Data) throws -> String
    /// The X25519 public key derived from the identity (`PushKeypair::derive`).
    func x25519Public(identity: Data) throws -> Data
    /// The identity's 64-byte ed25519 signature over `message`.
    func sign(identity: Data, message: Data) throws -> Data
}

/// A running eNet node, as far as the install registry is concerned.
public protocol InstallNodeDriver: AnyObject, Sendable {
    var endpointId: String { get }
    func allowedPeers() -> [String]
    func allow(peer: String) throws
    func revoke(peer: String) throws
    func shutdown() async
    /// VAPP-33: one chat message, returned once the peer acked it.
    func sendChat(peer: String, body: Data) async throws
    /// The node's next event; nil once it stopped. ONE consumer drains it
    /// (`InstallNodeService`'s pump), dispatching by tag.
    func nextEvent() async -> InstallNodeEvent?
}

/// The node events the app consumes (everything else is `other`).
public enum InstallNodeEvent: Sendable, Equatable {
    case chat(from: String, body: Data)
    case peerRefused(peer: String)
    case other
}

struct InstallNodeNotRunning: Error {}

extension InstallNodeDriver {
    public func sendChat(peer: String, body: Data) async throws { throw InstallNodeNotRunning() }
    public func nextEvent() async -> InstallNodeEvent? { nil }
}

public enum InstallRegistration {
    public static let kind = "ios"

    /// The UTF-8 string the install identity signs (#VAPP-37 wire contract).
    public static func message(userId: String, endpointId: String, x25519Public: String, signedAt: String) -> String {
        "exp-install-v1\n\(userId)\n\(endpointId)\n\(x25519Public)\n\(signedAt)"
    }

    /// ISO 8601 UTC with milliseconds, the form `Date.toISOString` prints.
    public static func signedAt(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        formatter.timeZone = TimeZone(identifier: "UTC")
        return formatter.string(from: date)
    }

    /// The signed `installs.register` input for one account.
    public static func input(
        identity: Data,
        userId: String,
        crypto: InstallCrypto,
        label: String?,
        platform: String?,
        now: Date
    ) throws -> RegisterInstallInput {
        let endpointId = try crypto.endpointId(identity: identity)
        let x25519 = base64Url(try crypto.x25519Public(identity: identity))
        let signedAt = signedAt(now)
        let text = message(userId: userId, endpointId: endpointId, x25519Public: x25519, signedAt: signedAt)
        let signature = try crypto.sign(identity: identity, message: Data(text.utf8))
        return RegisterInstallInput(
            kind: kind,
            label: label.map { String($0.prefix(64)) },
            platform: platform.map { String($0.prefix(32)) },
            endpointId: endpointId,
            x25519Public: x25519,
            signedAt: signedAt,
            signature: base64Url(signature)
        )
    }

    /// RFC 4648 §5 base64url without padding.
    public static func base64Url(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

public enum InstallAllowlist {
    /// The endpoint ids a node admits: every listed install but its own.
    public static func desired(_ lists: [[InstallEntry]], own: String) -> Set<String> {
        Set(lists.joined().map(\.endpointId)).subtracting([own])
    }

    /// What to allow and what to revoke to go from `current` to `desired`.
    public static func diff(current: Set<String>, desired: Set<String>) -> (allow: [String], revoke: [String]) {
        (desired.subtracting(current).sorted(), current.subtracting(desired).sorted())
    }
}

/// A signed-in account as the install node sees it.
public struct InstallAccount: Equatable, Sendable {
    public let accountId: String
    public let userId: String
    public let teamIds: [String]

    public init(accountId: String, userId: String, teamIds: [String]) {
        self.accountId = accountId
        self.userId = userId
        self.teamIds = teamIds
    }
}

/// The install's identity (the keychain in the app).
public struct InstallIdentitySource: Sendable {
    public let loadOrCreate: @Sendable () -> Data?
    /// Forget it: the registry revoked its endpoint id.
    public let rotate: @Sendable () -> Void

    public init(loadOrCreate: @escaping @Sendable () -> Data?, rotate: @escaping @Sendable () -> Void) {
        self.loadOrCreate = loadOrCreate
        self.rotate = rotate
    }
}

/// Owns the install's ONE eNet node: started lazily on the first `sync`
/// (never on the main thread: an actor), registered with every signed-in
/// account's server, its access allowlist = the installs of every team those
/// accounts are in. Stopped once no account is left.
public actor InstallNodeService {
    private let registry: InstallsRegistry
    private let crypto: InstallCrypto
    private let identity: InstallIdentitySource
    private let startNode: @Sendable (Data) async throws -> InstallNodeDriver
    private let label: String?
    private let platform: String?
    private let now: @Sendable () -> Date
    /// Called after the identity rotated (the derived push key changed).
    private let onIdentityRotated: @Sendable () -> Void

    private var node: InstallNodeDriver?
    /// VAPP-33: the ONE task draining `node.nextEvent()`.
    private var pump: Task<Void, Never>?
    /// Incoming steer envelopes, routed to the open viewer links.
    public nonisolated let steerInbox = SteerEnetInbox()
    /// Every event the pump does not route itself (no consumer yet besides
    /// the tests; bounded so an unread tap never grows).
    public nonisolated let otherEvents: AsyncStream<InstallNodeEvent>
    private nonisolated let otherEventsSink: AsyncStream<InstallNodeEvent>.Continuation
    private var accounts: [InstallAccount] = []
    /// accountId → installId, for accounts registered with this node's identity.
    private var installIds: [String: String] = [:]

    public init(
        registry: InstallsRegistry,
        crypto: InstallCrypto,
        identity: InstallIdentitySource,
        label: String?,
        platform: String?,
        now: @escaping @Sendable () -> Date = { Date() },
        onIdentityRotated: @escaping @Sendable () -> Void = {},
        startNode: @escaping @Sendable (Data) async throws -> InstallNodeDriver
    ) {
        self.registry = registry
        self.crypto = crypto
        self.identity = identity
        self.label = label
        self.platform = platform
        self.now = now
        self.onIdentityRotated = onIdentityRotated
        self.startNode = startNode
        (otherEvents, otherEventsSink) = AsyncStream.makeStream(bufferingPolicy: .bufferingNewest(64))
    }

    public var endpointId: String? { node?.endpointId }

    public func installId(accountId: String) -> String? { installIds[accountId] }

    /// The signed-in accounts and their teams changed (or the periodic
    /// refresh): starts the node if needed, registers every account not yet
    /// registered by this node, and reloads the allowlist. An empty list
    /// stops the node.
    public func sync(_ accounts: [InstallAccount]) async {
        self.accounts = accounts
        installIds = installIds.filter { id, _ in accounts.contains { $0.accountId == id } }
        guard !accounts.isEmpty else {
            await stop()
            return
        }
        // At most one rotation per pass: a second FORBIDDEN means something
        // else is wrong, and looping would mint identities forever.
        for attempt in 0..<2 {
            guard let seed = identity.loadOrCreate() else {
                logger.error("No install identity (keychain refused it): eNet stays off")
                return
            }
            guard await ensureNode(seed) != nil else { return }
            if await registerPending(seed: seed) || attempt == 1 { break }
            logger.info("The registry revoked this install: rotating its identity")
            identity.rotate()
            installIds.removeAll()
            await stop()
            onIdentityRotated()
        }
        await refreshAllowlist()
    }

    /// Reloads the allowlist from `installs.list` for every team of every
    /// account. A failed list keeps that team's current entries out of the
    /// revoke set (one flaky call must not cut every peer off).
    public func refreshAllowlist() async {
        guard let node else { return }
        var lists: [[InstallEntry]] = []
        var failed = false
        for account in accounts {
            for teamId in account.teamIds {
                do {
                    lists.append(try await registry.list(accountId: account.accountId, teamId: teamId))
                } catch {
                    failed = true
                    logger.error("installs.list failed: \(error.localizedDescription)")
                }
            }
        }
        let current = Set(node.allowedPeers())
        let desired = InstallAllowlist.desired(lists, own: node.endpointId)
        let (allow, revoke) = InstallAllowlist.diff(current: current, desired: desired)
        for peer in allow { try? node.allow(peer: peer) }
        if !failed {
            for peer in revoke { try? node.revoke(peer: peer) }
        }
    }

    /// One account signs out: on an explicit sign-out of THIS install the
    /// registry revokes it for that account (awaited before the credentials
    /// go), then the node drops that account's teams; the last account stops
    /// the node and forgets the identity (its endpoint id is revoked for
    /// good, so the next sign-in makes a new one).
    public func signOut(accountId: String, revoke: Bool) async {
        if revoke, let installId = installIds[accountId] {
            do {
                try await registry.revoke(accountId: accountId, installId: installId)
            } catch {
                logger.error("installs.revoke failed: \(error.localizedDescription)")
            }
        }
        installIds.removeValue(forKey: accountId)
        let remaining = accounts.filter { $0.accountId != accountId }
        if remaining.isEmpty, revoke {
            await stop()
            identity.rotate()
            onIdentityRotated()
            accounts = []
            return
        }
        await sync(remaining)
    }

    public func stop() async {
        guard let node else { return }
        self.node = nil
        pump?.cancel()
        pump = nil
        await node.shutdown()
    }

    /// VAPP-33: one chat message over the running node.
    public func sendChat(peer: String, body: Data) async throws {
        guard let node else { throw InstallNodeNotRunning() }
        try await node.sendChat(peer: peer, body: body)
    }

    /// Admit `peer` now (a steer device dials the viewer back) when the
    /// allowlist has not caught up yet; the next refresh keeps it only if
    /// the registry lists it.
    public func admit(peer: String) {
        guard let node, !node.allowedPeers().contains(peer) else { return }
        try? node.allow(peer: peer)
    }

    private func startPump(_ node: InstallNodeDriver) {
        pump?.cancel()
        let inbox = steerInbox
        let others = otherEventsSink
        pump = Task.detached(priority: .utility) {
            while !Task.isCancelled, let event = await node.nextEvent() {
                switch event {
                case let .chat(from, body):
                    if !inbox.deliver(from: from, body: body) { others.yield(event) }
                case let .peerRefused(peer):
                    logger.info("eNet refused \(peer.prefix(12), privacy: .public)")
                    others.yield(event)
                case .other:
                    others.yield(event)
                }
            }
        }
    }

    private func ensureNode(_ seed: Data) async -> InstallNodeDriver? {
        if let node { return node }
        do {
            let started = try await startNode(seed)
            node = started
            startPump(started)
            logger.info("eNet node started: \(started.endpointId, privacy: .public)")
            return started
        } catch {
            logger.error("eNet node failed to start: \(String(describing: error), privacy: .public)")
            return nil
        }
    }

    /// Registers every account this node has not registered yet. False when
    /// a server answered FORBIDDEN (this endpoint id was revoked there).
    private func registerPending(seed: Data) async -> Bool {
        for account in accounts where installIds[account.accountId] == nil {
            do {
                let input = try InstallRegistration.input(
                    identity: seed, userId: account.userId, crypto: crypto,
                    label: label, platform: platform, now: now()
                )
                let result = try await registry.register(accountId: account.accountId, input: input)
                installIds[account.accountId] = result.installId
            } catch TrpcError.httpError(403, _) {
                return false
            } catch {
                // CONFLICT (another account on this server owns the endpoint),
                // offline, an old server: retried on the next pass.
                logger.error("installs.register failed: \(error.localizedDescription)")
            }
        }
        return true
    }
}
