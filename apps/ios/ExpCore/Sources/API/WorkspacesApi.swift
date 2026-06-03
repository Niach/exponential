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

public struct UpdateWorkspaceInput: Encodable, Sendable {
    public let id: String
    public var name: String?
    public var isPublic: Bool?
    public var publicWritePolicy: String?
    public var iconUrl: String?

    public init(id: String, name: String? = nil, isPublic: Bool? = nil, publicWritePolicy: String? = nil, iconUrl: String? = nil) {
        self.id = id
        self.name = name
        self.isPublic = isPublic
        self.publicWritePolicy = publicWritePolicy
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

private struct EmptyInput: Encodable {}
private struct EmptyResult: Decodable {}

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

    public func update(accountId: String, _ input: UpdateWorkspaceInput) async throws {
        let _: EmptyResult = try await trpc.mutation(accountId: accountId, path: "workspaces.update", input: input)
    }

    public func delete(accountId: String, workspaceId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "workspaces.delete", input: DeleteWorkspaceInput(workspaceId: workspaceId))
    }

    public func deleteProject(accountId: String, projectId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "projects.delete", input: DeleteProjectInput(projectId: projectId))
    }
}
