import Foundation

/// GitHub App install state for the signed-in user (mirrors the web
/// `integrations.github.status` output). `installUrl` can be nil even when
/// configured (server without `GITHUB_APP_SLUG`).
public struct GithubStatusResult: Decodable, Sendable {
    public let configured: Bool
    public let installed: Bool
    public let installUrl: String?
    public let accounts: [String]

    public init(configured: Bool, installed: Bool, installUrl: String?, accounts: [String]) {
        self.configured = configured
        self.installed = installed
        self.installUrl = installUrl
        self.accounts = accounts
    }
}

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

    /// GitHub App install state, for the account integrations card.
    public func githubStatus(accountId: String) async throws -> GithubStatusResult {
        try await trpc.query(accountId: accountId, path: "integrations.github.status")
    }

    /// Repos the user's GitHub App is installed on, for the connect-repo picker.
    /// `platform: "mobile"` marks the caller so the server returns an install
    /// URL whose post-install page renders phone-sized and deep-links back into
    /// the app via `exp://github-connected` (instead of stranding the user in
    /// the browser). `refresh` bypasses the server's per-user repo cache — pass
    /// it when re-querying right after an install so new repos show immediately.
    public func githubRepos(accountId: String, refresh: Bool = false) async throws -> GithubReposResult {
        struct Input: Encodable {
            let platform: String
            let refresh: Bool?
        }
        return try await trpc.query(
            accountId: accountId,
            path: "integrations.github.repos",
            input: Input(platform: "mobile", refresh: refresh ? true : nil)
        )
    }
}
