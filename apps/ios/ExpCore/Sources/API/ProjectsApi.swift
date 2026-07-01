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

// MARK: - Preview config mirror (projects.updatePreviewConfig)

/// The display-only run-target mirror written to `projects.preview_config`.
/// NEVER carries build/run commands (those live only in the repo file); this is
/// safe display metadata + the feedback routing target. Mirrors the web
/// `previewMirrorInputSchema`.
public struct PreviewMirrorTarget: Codable, Sendable, Equatable {
    public let id: String
    public let name: String
    public let platform: String

    public init(id: String, name: String, platform: String) {
        self.id = id
        self.name = name
        self.platform = platform
    }
}

public struct PreviewMirrorInput: Encodable, Sendable {
    public let targets: [PreviewMirrorTarget]
    public let feedbackProjectId: String?

    public init(targets: [PreviewMirrorTarget], feedbackProjectId: String?) {
        self.targets = targets
        self.feedbackProjectId = feedbackProjectId
    }

    enum CodingKeys: String, CodingKey { case targets, feedbackProjectId }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(targets, forKey: .targets)
        // feedbackProjectId is `.optional()` server-side — omit when nil.
        try c.encodeIfPresent(feedbackProjectId, forKey: .feedbackProjectId)
    }
}

public struct UpdatePreviewConfigInput: Encodable, Sendable {
    public let projectId: String
    // nil clears the mirror (encoded as JSON null); a value replaces it.
    public let previewConfig: PreviewMirrorInput?

    public init(projectId: String, previewConfig: PreviewMirrorInput?) {
        self.projectId = projectId
        self.previewConfig = previewConfig
    }

    enum CodingKeys: String, CodingKey { case projectId, previewConfig }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(projectId, forKey: .projectId)
        // previewConfig is `.nullable()` server-side — encode JSON null to clear.
        if let previewConfig {
            try c.encode(previewConfig, forKey: .previewConfig)
        } else {
            try c.encodeNil(forKey: .previewConfig)
        }
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

    /// Write the display-only preview mirror (`projects.preview_config`):
    /// the discovered run targets + the feedback routing target. Owner-gated
    /// server-side (`assertWorkspaceOwner`); the desktop populates `targets`
    /// after it clones + parses the repo file. Pass `previewConfig: nil` to clear.
    public func updatePreviewConfig(accountId: String, _ input: UpdatePreviewConfigInput) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "projects.updatePreviewConfig",
            input: input
        )
    }
}
