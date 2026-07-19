import Foundation

public struct CreateInviteInput: Encodable, Sendable {
    public let teamId: String
    public let role: String

    public init(teamId: String, role: String) {
        self.teamId = teamId
        self.role = role
    }
}

public struct AcceptInviteInput: Encodable, Sendable {
    public let token: String

    public init(token: String) {
        self.token = token
    }
}

public struct RevokeInviteInput: Encodable, Sendable {
    public let inviteId: String

    public init(inviteId: String) {
        self.inviteId = inviteId
    }
}

public struct InviteCreateResult: Decodable, Sendable {
    public let invite: InviteTokenResult

    public init(invite: InviteTokenResult) {
        self.invite = invite
    }
}

public struct InviteTokenResult: Decodable, Sendable {
    public let id: String
    public let token: String

    public init(id: String, token: String) {
        self.id = id
        self.token = token
    }
}

public struct InviteDetailResult: Decodable, Sendable {
    public let invite: InviteDetail?

    public init(invite: InviteDetail?) {
        self.invite = invite
    }
}

public struct InviteDetail: Decodable, Sendable {
    public let id: String
    public let role: String
    public let expiresAt: String
    public let acceptedAt: String?
    public let team: InviteTeam?

    public init(id: String, role: String, expiresAt: String, acceptedAt: String?, team: InviteTeam?) {
        self.id = id
        self.role = role
        self.expiresAt = expiresAt
        self.acceptedAt = acceptedAt
        self.team = team
    }
}

public struct InviteTeam: Decodable, Sendable {
    public let name: String

    public init(name: String) {
        self.name = name
    }
}

public final class TeamInvitesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func create(accountId: String, teamId: String, role: String) async throws -> InviteTokenResult {
        let result: InviteCreateResult = try await trpc.mutation(
            accountId: accountId,
            path: "teamInvites.create",
            input: CreateInviteInput(teamId: teamId, role: role)
        )
        return result.invite
    }

    public func accept(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamInvites.accept", input: AcceptInviteInput(token: token))
    }

    public func revoke(accountId: String, inviteId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamInvites.revoke", input: RevokeInviteInput(inviteId: inviteId))
    }
}
