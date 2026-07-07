import Foundation

public struct EnsureDefaultResult: Decodable, Sendable {
    public let workspace: WorkspaceResult

    public init(workspace: WorkspaceResult) {
        self.workspace = workspace
    }
}

public struct WorkspaceResult: Decodable, Sendable {
    public let id: String
    public let name: String
    public let slug: String

    public init(id: String, name: String, slug: String) {
        self.id = id
        self.name = name
        self.slug = slug
    }
}

public struct CreateWorkspaceInput: Encodable, Sendable {
    public let name: String
    public var iconUrl: String?

    public init(name: String, iconUrl: String? = nil) {
        self.name = name
        self.iconUrl = iconUrl
    }
}

public struct DeleteWorkspaceInput: Encodable, Sendable {
    public let workspaceId: String

    public init(workspaceId: String) {
        self.workspaceId = workspaceId
    }
}

public struct DeleteProjectInput: Encodable, Sendable {
    public let projectId: String

    public init(projectId: String) {
        self.projectId = projectId
    }
}

public struct GetWorkspaceBySlugInput: Encodable, Sendable {
    public let slug: String

    public init(slug: String) {
        self.slug = slug
    }
}

/// `workspaces.getBySlug` — the public-aware lookup the web join gate uses.
/// Membership-only sync means a public board never appears locally for a
/// non-member, so this is the only way to resolve it before joining.
/// `membership` is the caller's role ("owner"/"member") or nil when not a
/// member; private workspaces the caller can't read come back as NOT_FOUND.
public struct WorkspaceBySlugResult: Decodable, Sendable {
    public let id: String
    public let name: String
    public let slug: String
    public let isPublic: Bool
    public let membership: String?

    public init(id: String, name: String, slug: String, isPublic: Bool, membership: String?) {
        self.id = id
        self.name = name
        self.slug = slug
        self.isPublic = isPublic
        self.membership = membership
    }
}

private struct EmptyInput: Encodable {}

public final class WorkspacesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func ensureDefault(accountId: String) async throws -> WorkspaceResult {
        let result: EnsureDefaultResult = try await trpc.mutation(accountId: accountId, path: "workspaces.ensureDefault", input: EmptyInput())
        return result.workspace
    }

    public func create(accountId: String, name: String, iconUrl: String? = nil) async throws -> WorkspaceResult {
        let result: EnsureDefaultResult = try await trpc.mutation(accountId: accountId, path: "workspaces.create", input: CreateWorkspaceInput(name: name, iconUrl: iconUrl))
        return result.workspace
    }

    public func delete(accountId: String, workspaceId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaces.delete", input: DeleteWorkspaceInput(workspaceId: workspaceId))
    }

    public func deleteProject(accountId: String, projectId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "projects.delete", input: DeleteProjectInput(projectId: projectId))
    }

    public func getBySlug(accountId: String, slug: String) async throws -> WorkspaceBySlugResult {
        try await trpc.query(accountId: accountId, path: "workspaces.getBySlug", input: GetWorkspaceBySlugInput(slug: slug))
    }
}
