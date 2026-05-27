import Foundation

struct AdminUser: Decodable, Identifiable {
    let id: String
    let name: String?
    let email: String
    let isAdmin: Bool
    let createdAt: String?
    let workspaceCount: Int?
    let providers: [String]?
}

struct AdminWorkspace: Decodable, Identifiable {
    let id: String
    let name: String
    let slug: String
    let plan: String?
    let memberCount: Int?
    let projectCount: Int?
    let owners: [AdminOwner]?
}

struct AdminOwner: Decodable {
    let name: String?
    let email: String
}

struct AdminUsersResult: Decodable {
    let users: [AdminUser]
}

struct AdminWorkspacesResult: Decodable {
    let workspaces: [AdminWorkspace]
}

struct SetAdminInput: Encodable {
    let userId: String
    let isAdmin: Bool
}

struct DeleteUserInput: Encodable {
    let userId: String
}

struct DeleteWorkspaceInput: Encodable {
    let workspaceId: String
}

private struct EmptyAdminInput: Encodable {}

final class AdminApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func listUsers(accountId: String) async throws -> [AdminUser] {
        let result: AdminUsersResult = try await trpc.mutation(accountId: accountId, path: "admin.listUsers", input: EmptyAdminInput())
        return result.users
    }

    func setUserAdmin(accountId: String, userId: String, isAdmin: Bool) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "admin.setUserAdmin", input: SetAdminInput(userId: userId, isAdmin: isAdmin))
    }

    func deleteUser(accountId: String, userId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "admin.deleteUser", input: DeleteUserInput(userId: userId))
    }

    func listWorkspaces(accountId: String) async throws -> [AdminWorkspace] {
        let result: AdminWorkspacesResult = try await trpc.mutation(accountId: accountId, path: "admin.listWorkspaces", input: EmptyAdminInput())
        return result.workspaces
    }

    func deleteWorkspace(accountId: String, workspaceId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "admin.deleteWorkspace", input: DeleteWorkspaceInput(workspaceId: workspaceId))
    }
}
