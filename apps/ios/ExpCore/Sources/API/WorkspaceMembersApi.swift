import Foundation

public struct UpdateRoleInput: Encodable, Sendable {
    public let memberId: String
    public let role: String

    public init(memberId: String, role: String) {
        self.memberId = memberId
        self.role = role
    }
}

public struct RemoveMemberInput: Encodable, Sendable {
    public let memberId: String

    public init(memberId: String) {
        self.memberId = memberId
    }
}

public struct JoinWorkspaceInput: Encodable, Sendable {
    public let workspaceId: String

    public init(workspaceId: String) {
        self.workspaceId = workspaceId
    }
}

public final class WorkspaceMembersApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Self-service join of a PUBLIC workspace (the server rejects private
    /// ones — those require an invite). Idempotent server-side, so retries are
    /// safe. Membership is what makes a public board sync for a signed-in
    /// user, so after this succeeds the shape pipeline must re-request for the
    /// board's data to appear (SyncManager.restartPipeline).
    public func join(accountId: String, workspaceId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceMembers.join", input: JoinWorkspaceInput(workspaceId: workspaceId))
    }

    public func updateRole(accountId: String, memberId: String, role: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceMembers.updateRole", input: UpdateRoleInput(memberId: memberId, role: role))
    }

    public func remove(accountId: String, memberId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceMembers.remove", input: RemoveMemberInput(memberId: memberId))
    }
}
