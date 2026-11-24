import Foundation

// EXP-1458 / EXP-1459: the key API of sealed session content (tRPC
// `sessionKeys.*`; packages/domain-contract/fixtures/sealed-content.json is
// the spec). Every call names the caller's own install (`installs.register`).

public struct TreeKeyWrap: Codable, Equatable, Sendable {
    public let installId: String
    public let wrapped: String
    /// #EXP-1950 (wrap-signature.json): this install's signature over the
    /// wrap; nil = unsigned (no full eNet, or the recipient's key unknown).
    public var signature: String?

    public init(installId: String, wrapped: String, signature: String? = nil) {
        self.installId = installId
        self.wrapped = wrapped
        self.signature = signature
    }
}

/// One wrap this install holds (`sessionKeys.mine`).
public struct HeldTreeKey: Decodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    public let wrapped: String
    /// #EXP-1950: who signed it; both nil = unsigned (an older build made it).
    public let signature: String?
    public let signerEndpointId: String?

    public init(rootId: String, epoch: UInt32, wrapped: String, signature: String? = nil, signerEndpointId: String? = nil) {
        self.rootId = rootId
        self.epoch = epoch
        self.wrapped = wrapped
        self.signature = signature
        self.signerEndpointId = signerEndpointId
    }
}

/// A reader still missing the current epoch's wrap (`sessionKeys.pending`).
public struct PendingTreeKey: Decodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    public let publicKey: String
    public let targetInstallId: String
    public let targetX25519Public: String
    /// #EXP-1950: the target's ed25519 key, the recipient a signature names
    /// (nil from an older server: the wrap goes unsigned).
    public let targetEndpointId: String?

    public init(rootId: String, epoch: UInt32, publicKey: String, targetInstallId: String, targetX25519Public: String, targetEndpointId: String? = nil) {
        self.rootId = rootId
        self.epoch = epoch
        self.publicKey = publicKey
        self.targetInstallId = targetInstallId
        self.targetX25519Public = targetX25519Public
        self.targetEndpointId = targetEndpointId
    }
}

public struct PendingTreeKeyWrap: Encodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    public let targetInstallId: String
    public let wrapped: String
    public var signature: String?

    public init(rootId: String, epoch: UInt32, targetInstallId: String, wrapped: String, signature: String? = nil) {
        self.rootId = rootId
        self.epoch = epoch
        self.targetInstallId = targetInstallId
        self.wrapped = wrapped
        self.signature = signature
    }
}

/// #EXP-1950 decision (c): one wrap of a root's CURRENT epoch made again and
/// signed by this install (`sessionKeys.resign`): the server replaces an
/// unsigned row of that pair with it and never touches a signed one.
public struct ResignedTreeKeyWrap: Encodable, Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    /// The recipient install.
    public let installId: String
    public let wrapped: String
    public let signature: String

    public init(rootId: String, epoch: UInt32, installId: String, wrapped: String, signature: String) {
        self.rootId = rootId
        self.epoch = epoch
        self.installId = installId
        self.wrapped = wrapped
        self.signature = signature
    }
}

public struct ResignResult: Decodable, Equatable, Sendable {
    public let replaced: Int
    public let skipped: Int

    public init(replaced: Int, skipped: Int) {
        self.replaced = replaced
        self.skipped = skipped
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
    /// #EXP-1950: a signed wrap of that epoch vouching for the key (R3); none on an older server.
    public let vouch: InstallTrust.Vouch?

    public init(rootId: String, epoch: UInt32, publicKey: String, vouch: InstallTrust.Vouch? = nil) {
        self.rootId = rootId
        self.epoch = epoch
        self.publicKey = publicKey
        self.vouch = vouch
    }

    public var key: TreeSealKey { TreeSealKey(publicKey: publicKey, epoch: epoch, vouch: vouch) }
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
    /// #EXP-1950: a signed wrap of that epoch vouching for `publicKey` (R3).
    public let vouch: InstallTrust.Vouch?

    public init(rootId: String?, epoch: UInt32?, publicKey: String?, clearTree: Bool? = nil, rotationDue: Bool? = nil, vouch: InstallTrust.Vouch? = nil) {
        self.rootId = rootId
        self.epoch = epoch
        self.publicKey = publicKey
        self.clearTree = clearTree
        self.rotationDue = rotationDue
        self.vouch = vouch
    }

    /// The root's current key, when the server names one.
    public var key: TreeSealKey? {
        guard let epoch, epoch > 0, let publicKey, !publicKey.isEmpty else { return nil }
        return TreeSealKey(publicKey: publicKey, epoch: epoch, vouch: vouch)
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
    /// `sessionKeys.resign {installId, wraps}` (#EXP-1950 decision (c)); an
    /// older server answers NOT_FOUND for the procedure.
    func resign(accountId: String, installId: String, wraps: [ResignedTreeKeyWrap]) async throws -> ResignResult
}

public extension SessionKeysRegistry {
    func due(accountId: String, installId: String) async throws -> [DueTreeKey] { [] }
    func resign(accountId: String, installId: String, wraps: [ResignedTreeKeyWrap]) async throws -> ResignResult {
        throw TreeKeyError.unavailable
    }
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
private struct ResignInput: Encodable {
    let installId: String
    let wraps: [ResignedTreeKeyWrap]
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

    public func resign(accountId: String, installId: String, wraps: [ResignedTreeKeyWrap]) async throws -> ResignResult {
        try await trpc.mutation(accountId: accountId, path: "sessionKeys.resign", input: ResignInput(installId: installId, wraps: wraps))
    }
}
