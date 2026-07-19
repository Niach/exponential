import Foundation

public struct CreateInviteInput: Encodable, Sendable {
    public let teamId: String
    public let role: String
    // Optional recipient address (EXP-188): the server persists it for the
    // pending list and mails the invite link. nil is simply omitted from the
    // JSON (synthesized Codable uses encodeIfPresent) — the server's zod
    // schema wants the key absent, not null.
    public let email: String?

    public init(teamId: String, role: String, email: String? = nil) {
        self.teamId = teamId
        self.role = role
        self.email = email
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
    // Email-delivery outcome (EXP-188): nil = no email requested, true =
    // delivered to a transport, false = requested but not delivered (no
    // transport configured / send error) — the owner should fall back to
    // sharing the link by hand.
    public let emailDelivered: Bool?

    public init(invite: InviteTokenResult, emailDelivered: Bool? = nil) {
        self.invite = invite
        self.emailDelivered = emailDelivered
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

    public func create(
        accountId: String, teamId: String, role: String, email: String? = nil
    ) async throws -> InviteCreateResult {
        try await trpc.mutation(
            accountId: accountId,
            path: "teamInvites.create",
            input: CreateInviteInput(teamId: teamId, role: role, email: email)
        )
    }

    public func accept(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamInvites.accept", input: AcceptInviteInput(token: token))
    }

    public func revoke(accountId: String, inviteId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamInvites.revoke", input: RevokeInviteInput(inviteId: inviteId))
    }
}
