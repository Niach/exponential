import Foundation

/// GitHub App install state for the signed-in user (mirrors the web
/// `integrations.github.status` output). `installUrl` can be nil even when
/// configured (server without `GITHUB_APP_SLUG`). `connectUrl` is the
/// mobile-friendly OAuth authorize URL that claims a GitHub account for a
/// workspace (single consent screen); nil when unavailable — callers prefer
/// `connectUrl ?? installUrl` for the connect hop.
public struct GithubStatusResult: Decodable, Sendable {
    public let configured: Bool
    public let installed: Bool
    public let installUrl: String?
    public let connectUrl: String?
    public let accounts: [String]

    public init(configured: Bool, installed: Bool, installUrl: String?, connectUrl: String? = nil, accounts: [String]) {
        self.configured = configured
        self.installed = installed
        self.installUrl = installUrl
        self.connectUrl = connectUrl
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
    /// Mobile-friendly OAuth authorize URL that claims a GitHub account for the
    /// workspace (single consent screen, no configure page); nil when
    /// unavailable. Prefer `connectUrl ?? installUrl` for the connect hop.
    public let connectUrl: String?
    public let repos: [GithubPickerRepo]
    public let hasMore: Bool
}

public final class IntegrationsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// GitHub App install state, for the account integrations card. Pass
    /// `workspaceId` to scope the result to that workspace's linked GitHub
    /// accounts; omit it to fall back to the server's deprecated
    /// union-across-memberships shim.
    public func githubStatus(accountId: String, workspaceId: String) async throws -> GithubStatusResult {
        struct Input: Encodable {
            let workspaceId: String
        }
        return try await trpc.query(
            accountId: accountId,
            path: "integrations.github.status",
            input: Input(workspaceId: workspaceId)
        )
    }

    /// Repos the user's GitHub App is installed on, for the connect-repo picker.
    /// `platform: "mobile"` marks the caller so the server returns an install
    /// URL whose post-install page renders phone-sized and deep-links back into
    /// the app via `exp://github-connected` (instead of stranding the user in
    /// the browser). Pass `workspaceId` to scope the result to that workspace's
    /// linked GitHub accounts; omit it to fall back to the server's deprecated
    /// union-across-memberships shim. `refresh` bypasses the server's per-user
    /// repo cache — pass it when re-querying right after an install so new repos
    /// show immediately.
    public func githubRepos(accountId: String, workspaceId: String, refresh: Bool = false) async throws -> GithubReposResult {
        struct Input: Encodable {
            let platform: String
            let workspaceId: String
            let refresh: Bool?
        }
        return try await trpc.query(
            accountId: accountId,
            path: "integrations.github.repos",
            input: Input(platform: "mobile", workspaceId: workspaceId, refresh: refresh ? true : nil)
        )
    }
}
