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

    func updateRole(memberId: String, role: String) async throws {
        try await trpc.mutationVoid(path: "workspaceMembers.updateRole", input: UpdateRoleInput(memberId: memberId, role: role))
    }

    func remove(memberId: String) async throws {
        try await trpc.mutationVoid(path: "workspaceMembers.remove", input: RemoveMemberInput(memberId: memberId))
    }
}
