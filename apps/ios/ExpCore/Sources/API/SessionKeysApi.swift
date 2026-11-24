import Foundation

// EXP-1458 / EXP-1459: the key API of sealed session content (tRPC
// `sessionKeys.*`; packages/domain-contract/fixtures/sealed-content.json is
// the spec). Every call names the caller's own install (`installs.register`).

public struct TreeKeyWrap: Codable, Equatable, Sendable {
    public let installId: String
    public let wrapped: String

    public init(installId: String, wrapped: String) {
        self.installId = installId
        self.wrapped = wrapped
    }
}

/// One wrap this install holds (`sessionKeys.mine`).
public struct HeldTreeKey: Decodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    public let wrapped: String

    public init(rootId: String, epoch: UInt32, wrapped: String) {
        self.rootId = rootId
        self.epoch = epoch
        self.wrapped = wrapped
    }
}

/// A reader still missing the current epoch's wrap (`sessionKeys.pending`).
public struct PendingTreeKey: Decodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    public let publicKey: String
    public let targetInstallId: String
    public let targetX25519Public: String

    public init(rootId: String, epoch: UInt32, publicKey: String, targetInstallId: String, targetX25519Public: String) {
        self.rootId = rootId
        self.epoch = epoch
        self.publicKey = publicKey
        self.targetInstallId = targetInstallId
        self.targetX25519Public = targetX25519Public
    }
}

public struct PendingTreeKeyWrap: Encodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    public let targetInstallId: String
    public let wrapped: String

    public init(rootId: String, epoch: UInt32, targetInstallId: String, wrapped: String) {
        self.rootId = rootId
        self.epoch = epoch
        self.targetInstallId = targetInstallId
        self.wrapped = wrapped
    }
}

public struct CreateTreeKeyInput: Encodable, Equatable, Sendable {
    public let rootId: String
    public let installId: String
    public let epoch: UInt32
    public let publicKey: String
    public let wraps: [TreeKeyWrap]

    public init(rootId: String, installId: String, epoch: UInt32, publicKey: String, wraps: [TreeKeyWrap]) {
        self.rootId = rootId
        self.installId = installId
        self.epoch = epoch
        self.publicKey = publicKey
        self.wraps = wraps
    }
}

/// A keyed root's current public key (`sessionKeys.publicKeys`).
public struct TreePublicKey: Decodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    public let publicKey: String

    public init(rootId: String, epoch: UInt32, publicKey: String) {
        self.rootId = rootId
        self.epoch = epoch
        self.publicKey = publicKey
    }
}

/// #FEED-250 (sealed-content.json `staleTree`): the server's word on the
/// caller's conversation with an agent / app (`sessions.sendTarget`): the
/// synced store is a cache. `rootId` nil = none yet (the send keys it
/// inline); `epoch` / `publicKey` nil = a keyless root.
public struct ServerSendTarget: Decodable, Equatable, Sendable {
    public let rootId: String?
    public let epoch: UInt32?
    public let publicKey: String?
    /// #EDE-97: the root is an outside MCP client's tree (in clear for
    /// life); absent on an older server = not.
    public let clearTree: Bool?
    /// #EXP-1941: the root's rotation is due (a call refuses to start on it).
    public let rotationDue: Bool?

    public init(rootId: String?, epoch: UInt32?, publicKey: String?, clearTree: Bool? = nil, rotationDue: Bool? = nil) {
        self.rootId = rootId
        self.epoch = epoch
        self.publicKey = publicKey
        self.clearTree = clearTree
        self.rotationDue = rotationDue
    }

    /// The root's current key, when the server names one.
    public var key: TreeSealKey? {
        guard let epoch, epoch > 0, let publicKey, !publicKey.isEmpty else { return nil }
        return TreeSealKey(publicKey: publicKey, epoch: epoch)
    }
}

/// #EXP-1960: `sessionKeys.requestKey`'s answer: `keyed` carries the key,
/// `requested` = the tree's devices were asked to key it, `noHost` = none hosts it.
public struct RequestKeyResult: Decodable, Equatable, Sendable {
    public let state: String
    public let epoch: UInt32?
    public let publicKey: String?

    public init(state: String, epoch: UInt32? = nil, publicKey: String? = nil) {
        self.state = state
        self.epoch = epoch
        self.publicKey = publicKey
    }
}

/// #EXP-1951: a root this install rotates (`sessionKeys.due`); `epoch` =
/// the one to supersede.
public struct DueTreeKey: Decodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32

    public init(rootId: String, epoch: UInt32) {
        self.rootId = rootId
        self.epoch = epoch
    }
}

/// The key calls a holder makes (a fake in the tests).
public protocol SessionKeysRegistry: Sendable {
    /// `sessions.sendTarget {agentId | appId}` (#FEED-250).
    func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget
    func create(accountId: String, input: CreateTreeKeyInput) async throws
    func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey]
    func pending(accountId: String, installId: String) async throws -> [PendingTreeKey]
    func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws
    func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey]
    /// `sessionKeys.requestKey {rootId}` (#EXP-1960).
    func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult
    /// `sessionKeys.due {installId}` (#EXP-1951, sealed-content.json `rotation`).
    func due(accountId: String, installId: String) async throws -> [DueTreeKey]
}

public extension SessionKeysRegistry {
    func due(accountId: String, installId: String) async throws -> [DueTreeKey] { [] }
}

private struct MineInput: Encodable {
    let installId: String
    let rootIds: [String]?
}
private struct PendingInput: Encodable { let installId: String }
private struct WrapInput: Encodable {
    let installId: String
    let wraps: [PendingTreeKeyWrap]
}
private struct PublicKeysInput: Encodable { let rootIds: [String] }
private struct RequestKeyInput: Encodable { let rootId: String }
private struct SendTargetInput: Encodable {
    let agentId: String?
    let appId: String?
}

public final class SessionKeysApi: SessionKeysRegistry {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
        try await trpc.query(accountId: accountId, path: "sessions.sendTarget", input: SendTargetInput(agentId: agentId, appId: appId))
    }

    public func create(accountId: String, input: CreateTreeKeyInput) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "sessionKeys.create", input: input)
    }

    public func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] {
        try await trpc.query(accountId: accountId, path: "sessionKeys.mine", input: MineInput(installId: installId, rootIds: rootIds))
    }

    public func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] {
        try await trpc.query(accountId: accountId, path: "sessionKeys.pending", input: PendingInput(installId: installId))
    }

    public func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "sessionKeys.wrap", input: WrapInput(installId: installId, wraps: wraps))
    }

    public func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
        try await trpc.query(accountId: accountId, path: "sessionKeys.publicKeys", input: PublicKeysInput(rootIds: rootIds))
    }

    public func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult {
        try await trpc.mutation(accountId: accountId, path: "sessionKeys.requestKey", input: RequestKeyInput(rootId: rootId))
    }

    public func due(accountId: String, installId: String) async throws -> [DueTreeKey] {
        try await trpc.query(accountId: accountId, path: "sessionKeys.due", input: PendingInput(installId: installId))
    }
}
