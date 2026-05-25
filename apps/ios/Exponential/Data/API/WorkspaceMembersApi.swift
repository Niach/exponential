import Foundation

struct UpdateRoleInput: Encodable {
    let memberId: String
    let role: String
}

struct RemoveMemberInput: Encodable {
    let memberId: String
}

final class WorkspaceMembersApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func updateRole(accountId: String, memberId: String, role: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceMembers.updateRole", input: UpdateRoleInput(memberId: memberId, role: role))
    }

    func remove(accountId: String, memberId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaceMembers.remove", input: RemoveMemberInput(memberId: memberId))
    }
}
