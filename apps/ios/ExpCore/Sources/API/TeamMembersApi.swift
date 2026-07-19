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

public final class TeamMembersApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    // The self-service `teamMembers.join` procedure was removed:
    // membership is invite-only.

    public func updateRole(accountId: String, memberId: String, role: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamMembers.updateRole", input: UpdateRoleInput(memberId: memberId, role: role))
    }

    public func remove(accountId: String, memberId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamMembers.remove", input: RemoveMemberInput(memberId: memberId))
    }
}
