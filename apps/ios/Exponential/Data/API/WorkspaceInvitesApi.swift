import Foundation

struct CreateInviteInput: Encodable {
    let workspaceId: String
    let role: String
}

struct AcceptInviteInput: Encodable {
    let token: String
}

struct RevokeInviteInput: Encodable {
    let inviteId: String
}

struct InviteCreateResult: Decodable {
    let invite: InviteTokenResult
}

struct InviteTokenResult: Decodable {
    let id: String
    let token: String
}

struct InviteDetailResult: Decodable {
    let invite: InviteDetail?
}

struct InviteDetail: Decodable {
    let id: String
    let role: String
    let expiresAt: String
    let acceptedAt: String?
    let workspace: InviteWorkspace?
}

struct InviteWorkspace: Decodable {
    let name: String
}

final class WorkspaceInvitesApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func create(accountId: String, workspaceId: String, role: String) async throws -> InviteTokenResult {
        let result: InviteCreateResult = try await trpc.mutation(
            accountId: accountId,
            path: "workspaceInvites.create",
            input: CreateInviteInput(workspaceId: workspaceId, role: role)
        )
        return result.invite
    }

    func accept(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceInvites.accept", input: AcceptInviteInput(token: token))
    }

    func revoke(accountId: String, inviteId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceInvites.revoke", input: RevokeInviteInput(inviteId: inviteId))
    }
}
