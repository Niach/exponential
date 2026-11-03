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

public struct AddPlaceholderInput: Encodable, Sendable {
    public let teamId: String
    public let email: String
    public var name: String?

    public init(teamId: String, email: String, name: String? = nil) {
        self.teamId = teamId
        self.email = email
        self.name = name
    }
}

public struct AddPlaceholderResult: Decodable, Sendable {
    public struct Member: Decodable, Sendable {
        public let name: String
    }

    public let member: Member
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

    /// FEED-108: "Add without inviting" — the placeholder member with no
    /// link (no mail, no seat). Returns the member's display name.
    public func addPlaceholder(accountId: String, teamId: String, email: String, name: String?) async throws -> String {
        let result: AddPlaceholderResult = try await trpc.mutation(
            accountId: accountId,
            path: "teamMembers.addPlaceholder",
            input: AddPlaceholderInput(teamId: teamId, email: email, name: name)
        )
        return result.member.name
    }
}
