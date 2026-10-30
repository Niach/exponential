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

/// The key calls a holder makes (a fake in the tests).
public protocol SessionKeysRegistry: Sendable {
    func create(accountId: String, input: CreateTreeKeyInput) async throws
    func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey]
    func pending(accountId: String, installId: String) async throws -> [PendingTreeKey]
    func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws
    func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey]
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

public final class SessionKeysApi: SessionKeysRegistry {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
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
}
