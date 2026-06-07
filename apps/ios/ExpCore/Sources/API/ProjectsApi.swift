import Foundation

public struct CreateProjectInput: Encodable, Sendable {
    public let workspaceId: String
    public let name: String
    public let prefix: String
    public var color: String?

    public init(workspaceId: String, name: String, prefix: String, color: String? = nil) {
        self.workspaceId = workspaceId
        self.name = name
        self.prefix = prefix
        self.color = color
    }
}

public struct ProjectResult: Decodable, Sendable {
    public let project: ProjectResultData

    public init(project: ProjectResultData) {
        self.project = project
    }
}

public struct ProjectResultData: Decodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

public struct LinkRepoInput: Encodable, Sendable {
    public let projectId: String
    public let repo: String

    public init(projectId: String, repo: String) {
        self.projectId = projectId
        self.repo = repo
    }
}

public struct UnlinkRepoInput: Encodable, Sendable {
    public let projectId: String

    public init(projectId: String) {
        self.projectId = projectId
    }
}

public final class ProjectsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Create a project. The server uppercases `prefix` and defaults `color`
    /// to `#6366f1` when omitted. Returns the new project id.
    public func create(accountId: String, _ input: CreateProjectInput) async throws -> String {
        let result: ProjectResult = try await trpc.mutation(accountId: accountId, path: "projects.create", input: input)
        return result.project.id
    }

    /// Link a GitHub repo (`owner/name`) to a project. Owner-gated server-side
    /// (`assertWorkspaceOwner`). Electric surfaces the updated `githubRepo`.
    public func linkGithubRepo(accountId: String, projectId: String, repo: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "projects.linkGithubRepo",
            input: LinkRepoInput(projectId: projectId, repo: repo)
        )
    }

    /// Remove the GitHub repo link from a project. Owner-gated server-side.
    public func unlinkGithubRepo(accountId: String, projectId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "projects.unlinkGithubRepo",
            input: UnlinkRepoInput(projectId: projectId)
        )
    }
}
