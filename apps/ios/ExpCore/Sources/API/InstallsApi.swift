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
    /// This client's `installs.kind`.
    public static let iosKind = "ios"

    public let installId: String
    public let userId: String?
    public let kind: String
    public let endpointId: String
    public let x25519Public: String
    /// The `devices` row a desktop/daemon install is bound to (EXP-1459:
    /// the writer of a new chat wraps its tree key for the target device).
    public let deviceId: String?
    /// Set on `installs.mine` rows this user revoked.
    public let revokedAt: String?
    /// #EDE-114: when the install was last seen (the catch-up's HOLDERS
    /// order, newest first).
    public let lastSeenAt: String?

    public init(
        installId: String,
        userId: String?,
        kind: String,
        endpointId: String,
        x25519Public: String,
        deviceId: String? = nil,
        revokedAt: String? = nil,
        lastSeenAt: String? = nil
    ) {
        self.installId = installId
        self.userId = userId
        self.kind = kind
        self.endpointId = endpointId
        self.x25519Public = x25519Public
        self.deviceId = deviceId
        self.revokedAt = revokedAt
        self.lastSeenAt = lastSeenAt
    }
}

/// The registry calls the install node makes (a fake in the tests).
public protocol InstallsRegistry: Sendable {
    func register(accountId: String, input: RegisterInstallInput) async throws -> RegisterInstallResult
    func list(accountId: String, teamId: String) async throws -> [InstallEntry]
    func revoke(accountId: String, installId: String) async throws
}

/// The install lists a tree key holder reads its readers from (a fake in tests).
public protocol TreeReaderDirectory: Sendable {
    func mine(accountId: String) async throws -> [InstallEntry]
    func list(accountId: String, teamId: String) async throws -> [InstallEntry]
}

/// #EXP-1950 (fixtures/install-trust.json): install approval. The server
/// stores and relays; this install judges every signature itself
/// (`InstallTrust.reconcile`). A fake (a lying server) in the tests.
public protocol InstallTrustApi: Sendable {
    func trust(accountId: String, installId: String) async throws -> TrustAnswer
    func requestApproval(accountId: String, installId: String) async throws
    func claimRoot(accountId: String, installId: String, signature: String) async throws
    /// `approve` an own install, confirm (`reset`) or turn down (`decline`) a team-mate's new root.
    func approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) async throws
    /// A New install row's Remove: its wraps go, its trees rotate.
    func revoke(accountId: String, installId: String) async throws
}

private struct InstallsTeamInput: Encodable { let teamId: String }
private struct InstallIdInput: Encodable { let installId: String }
private struct ClaimRootInput: Encodable { let installId: String; let signature: String }
private struct ApproveInstallInput: Encodable {
    let installId: String
    let subjectInstallId: String
    let kind: String
    let signature: String
}

public final class InstallsApi: InstallsRegistry, TreeReaderDirectory, InstallTrustApi {
    let trpc: TrpcClient

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

    public func trust(accountId: String, installId: String) async throws -> TrustAnswer {
        try await trpc.query(accountId: accountId, path: "installs.trust", input: InstallIdInput(installId: installId))
    }

    public func requestApproval(accountId: String, installId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "installs.requestApproval", input: InstallIdInput(installId: installId))
    }

    public func claimRoot(accountId: String, installId: String, signature: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "installs.claimRoot", input: ClaimRootInput(installId: installId, signature: signature))
    }

    public func approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "installs.approve", input: ApproveInstallInput(
            installId: installId, subjectInstallId: subjectInstallId, kind: kind.rawValue, signature: signature
        ))
    }
}
