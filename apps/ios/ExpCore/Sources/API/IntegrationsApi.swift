import Foundation

public struct GoogleStatusResult: Decodable, Sendable {
    public let connected: Bool
    public let connectedAt: String?

    public init(connected: Bool, connectedAt: String?) {
        self.connected = connected
        self.connectedAt = connectedAt
    }
}

private struct EmptyIntegrationInput: Encodable {}

/// One repo the user's GitHub App can connect (mirrors web `InstallationRepo`).
/// JSON keys are camelCase so the plain decoder maps them directly; `private` is
/// a Swift keyword so it's backticked.
public struct GithubPickerRepo: Decodable, Sendable, Identifiable {
    public var id: String { fullName }
    public let fullName: String
    public let `private`: Bool
    public let defaultBranch: String
    public let installationId: Int
}

public struct GithubReposResult: Decodable, Sendable {
    public let configured: Bool
    public let installed: Bool
    public let installUrl: String?
    public let repos: [GithubPickerRepo]
    public let hasMore: Bool
}

public final class IntegrationsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func googleStatus(accountId: String) async throws -> GoogleStatusResult {
        // Server defines integrations.google.status as a `.query` (GET).
        try await trpc.query(accountId: accountId, path: "integrations.google.status")
    }

    /// Repos the user's GitHub App is installed on, for the connect-repo picker.
    public func githubRepos(accountId: String) async throws -> GithubReposResult {
        try await trpc.query(accountId: accountId, path: "integrations.github.repos")
    }

    public func googleDisconnect(accountId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "integrations.google.disconnect", input: EmptyIntegrationInput())
    }

    public func googleBackfill(accountId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "integrations.google.backfill", input: EmptyIntegrationInput())
    }
}
