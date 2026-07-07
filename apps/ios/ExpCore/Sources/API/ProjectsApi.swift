import Foundation

/// v4: a project IS a repository. Create/retarget always carry a repo — either
/// an existing workspace-registry repo (`repositoryId`) or a brand-new repo
/// connected inline by `fullName` (server validates the GitHub-App install and
/// upserts the registry row in the same transaction). Mirrors the server
/// `repositoryInputSchema` (apps/web/src/lib/trpc/projects.ts).
public enum ProjectRepositoryChoice: Encodable, Sendable, Equatable {
    /// Target an existing registry repo (same-workspace, not archived).
    case repositoryId(String)
    /// Connect a new repo inline by `owner/name`; the extra fields seed the
    /// registry row (all optional server-side). The installation id is
    /// resolved server-side from GitHub — clients never send it.
    case fullName(String, defaultBranch: String? = nil, isPrivate: Bool? = nil)

    private enum IdKeys: String, CodingKey { case repositoryId }
    private enum NameKeys: String, CodingKey { case fullName, defaultBranch, `private` }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case let .repositoryId(id):
            var c = encoder.container(keyedBy: IdKeys.self)
            try c.encode(id, forKey: .repositoryId)
        case let .fullName(name, defaultBranch, isPrivate):
            var c = encoder.container(keyedBy: NameKeys.self)
            try c.encode(name, forKey: .fullName)
            try c.encodeIfPresent(defaultBranch, forKey: .defaultBranch)
            try c.encodeIfPresent(isPrivate, forKey: .private)
        }
    }
}

public struct CreateProjectInput: Encodable, Sendable {
    public let workspaceId: String
    public let name: String
    public let prefix: String
    public var color: String?
    // Required in v4 — a repo-less project can no longer exist.
    public let repository: ProjectRepositoryChoice

    public init(
        workspaceId: String,
        name: String,
        prefix: String,
        color: String? = nil,
        repository: ProjectRepositoryChoice
    ) {
        self.workspaceId = workspaceId
        self.name = name
        self.prefix = prefix
        self.color = color
        self.repository = repository
    }
}

public struct SetProjectRepositoryInput: Encodable, Sendable {
    public let projectId: String
    public let repositoryId: String

    public init(projectId: String, repositoryId: String) {
        self.projectId = projectId
        self.repositoryId = repositoryId
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

    /// Retarget a project's backing repo (owner/admin, server-enforced). The
    /// repo must already be in the workspace registry — connecting a brand-new
    /// repo happens via `create`'s inline path or `repositories.add`. Electric
    /// surfaces the updated `repository_id`.
    public func setRepository(accountId: String, projectId: String, repositoryId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "projects.setRepository",
            input: SetProjectRepositoryInput(projectId: projectId, repositoryId: repositoryId)
        )
    }
}
