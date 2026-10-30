import Foundation

// EXP-1457 / VAPP-37: the eNet install registry (tRPC `installs.*`). Every
// install registers its endpoint id + X25519 key on start, and loads its
// teams' installs as its node's access allowlist.

/// `installs.register`'s input; `signature` = base64url ed25519 by the
/// install identity over `InstallRegistration.message(...)`.
public struct RegisterInstallInput: Encodable, Equatable, Sendable {
    public let kind: String
    public let label: String?
    public let platform: String?
    public let endpointId: String
    public let x25519Public: String
    public let signedAt: String
    public let signature: String

    public init(kind: String, label: String?, platform: String?, endpointId: String, x25519Public: String, signedAt: String, signature: String) {
        self.kind = kind
        self.label = label
        self.platform = platform
        self.endpointId = endpointId
        self.x25519Public = x25519Public
        self.signedAt = signedAt
        self.signature = signature
    }
}

public struct RegisterInstallResult: Decodable, Equatable, Sendable {
    public let installId: String
}

/// One row of `installs.list` / `installs.mine` (the fields a node needs).
public struct InstallEntry: Decodable, Equatable, Sendable {
    public let installId: String
    public let userId: String?
    public let kind: String
    public let endpointId: String
    public let x25519Public: String

    public init(installId: String, userId: String?, kind: String, endpointId: String, x25519Public: String) {
        self.installId = installId
        self.userId = userId
        self.kind = kind
        self.endpointId = endpointId
        self.x25519Public = x25519Public
    }
}

/// The registry calls the install node makes (a fake in the tests).
public protocol InstallsRegistry: Sendable {
    func register(accountId: String, input: RegisterInstallInput) async throws -> RegisterInstallResult
    func list(accountId: String, teamId: String) async throws -> [InstallEntry]
    func revoke(accountId: String, installId: String) async throws
}

private struct InstallsTeamInput: Encodable { let teamId: String }
private struct InstallIdInput: Encodable { let installId: String }

public final class InstallsApi: InstallsRegistry {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func register(accountId: String, input: RegisterInstallInput) async throws -> RegisterInstallResult {
        try await trpc.mutation(accountId: accountId, path: "installs.register", input: input)
    }

    public func list(accountId: String, teamId: String) async throws -> [InstallEntry] {
        try await trpc.query(accountId: accountId, path: "installs.list", input: InstallsTeamInput(teamId: teamId))
    }

    public func mine(accountId: String) async throws -> [InstallEntry] {
        try await trpc.query(accountId: accountId, path: "installs.mine")
    }

    public func revoke(accountId: String, installId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "installs.revoke", input: InstallIdInput(installId: installId))
    }
}
