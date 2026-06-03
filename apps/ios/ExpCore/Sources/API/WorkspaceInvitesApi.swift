import Foundation

public struct CreateInviteInput: Encodable, Sendable {
    public let workspaceId: String
    public let role: String

    public init(workspaceId: String, role: String) {
        self.workspaceId = workspaceId
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
    public let workspace: InviteWorkspace?

    public init(id: String, role: String, expiresAt: String, acceptedAt: String?, workspace: InviteWorkspace?) {
        self.id = id
        self.role = role
        self.expiresAt = expiresAt
        self.acceptedAt = acceptedAt
        self.workspace = workspace
    }
}

public struct InviteWorkspace: Decodable, Sendable {
    public let name: String

    public init(name: String) {
        self.name = name
    }
}

public final class WorkspaceInvitesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func create(accountId: String, workspaceId: String, role: String) async throws -> InviteTokenResult {
        let result: InviteCreateResult = try await trpc.mutation(
            accountId: accountId,
            path: "workspaceInvites.create",
            input: CreateInviteInput(workspaceId: workspaceId, role: role)
        )
        return result.invite
    }

    public func accept(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceInvites.accept", input: AcceptInviteInput(token: token))
    }

    public func revoke(accountId: String, inviteId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceInvites.revoke", input: RevokeInviteInput(inviteId: inviteId))
    }
}
