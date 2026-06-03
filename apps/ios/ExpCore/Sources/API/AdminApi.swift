import Foundation

public struct AdminUser: Decodable, Identifiable, Sendable {
    public let id: String
    public let name: String?
    public let email: String
    public let isAdmin: Bool
    public let createdAt: String?
    public let workspaceCount: Int?
    public let providers: [String]?

    public init(
        id: String,
        name: String? = nil,
        email: String,
        isAdmin: Bool,
        createdAt: String? = nil,
        workspaceCount: Int? = nil,
        providers: [String]? = nil
    ) {
        self.id = id
        self.name = name
        self.email = email
        self.isAdmin = isAdmin
        self.createdAt = createdAt
        self.workspaceCount = workspaceCount
        self.providers = providers
    }
}

public struct AdminWorkspace: Decodable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let slug: String
    public let plan: String?
    public let memberCount: Int?
    public let projectCount: Int?
    public let owners: [AdminOwner]?

    public init(
        id: String,
        name: String,
        slug: String,
        plan: String? = nil,
        memberCount: Int? = nil,
        projectCount: Int? = nil,
        owners: [AdminOwner]? = nil
    ) {
        self.id = id
        self.name = name
        self.slug = slug
        self.plan = plan
        self.memberCount = memberCount
        self.projectCount = projectCount
        self.owners = owners
    }
}

public struct AdminOwner: Decodable, Sendable {
    public let name: String?
    public let email: String

    public init(name: String? = nil, email: String) {
        self.name = name
        self.email = email
    }
}

public struct SetAdminInput: Encodable, Sendable {
    public let userId: String
    public let isAdmin: Bool

    public init(userId: String, isAdmin: Bool) {
        self.userId = userId
        self.isAdmin = isAdmin
    }
}

public struct DeleteUserInput: Encodable, Sendable {
    public let userId: String

    public init(userId: String) {
        self.userId = userId
    }
}

public final class AdminApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func listUsers(accountId: String) async throws -> [AdminUser] {
        // Server defines admin.listUsers as a `.query` (GET) returning a bare array.
        try await trpc.query(accountId: accountId, path: "admin.listUsers")
    }

    public func setUserAdmin(accountId: String, userId: String, isAdmin: Bool) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "admin.setUserAdmin", input: SetAdminInput(userId: userId, isAdmin: isAdmin))
    }

    public func deleteUser(accountId: String, userId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "admin.deleteUser", input: DeleteUserInput(userId: userId))
    }

    public func listWorkspaces(accountId: String) async throws -> [AdminWorkspace] {
        // Server defines admin.listWorkspaces as a `.query` (GET) returning a bare array.
        try await trpc.query(accountId: accountId, path: "admin.listWorkspaces")
    }

    public func deleteWorkspace(accountId: String, workspaceId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "admin.deleteWorkspace", input: DeleteWorkspaceInput(workspaceId: workspaceId))
    }
}
