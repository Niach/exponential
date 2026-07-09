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
    /// Per-installation grant state (grant model); `[]` on servers that predate
    /// the field.
    public let installations: [GithubInstallation]

    public init(configured: Bool, installed: Bool, installUrl: String?, connectUrl: String? = nil, accounts: [String], installations: [GithubInstallation] = []) {
        self.configured = configured
        self.installed = installed
        self.installUrl = installUrl
        self.connectUrl = connectUrl
        self.accounts = accounts
        self.installations = installations
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            configured: try container.decode(Bool.self, forKey: .configured),
            installed: try container.decode(Bool.self, forKey: .installed),
            installUrl: try container.decodeIfPresent(String.self, forKey: .installUrl),
            connectUrl: try container.decodeIfPresent(String.self, forKey: .connectUrl),
            accounts: try container.decode([String].self, forKey: .accounts),
            installations: try container.decodeIfPresent([GithubInstallation].self, forKey: .installations) ?? []
        )
    }

    private enum CodingKeys: String, CodingKey {
        case configured, installed, installUrl, connectUrl, accounts, installations
    }
}

/// One GitHub App installation linked to the workspace (mirrors the web
/// `installationSummary` + grant flags). `needsReauth` marks an installation
/// whose per-user repo grants were never captured (linked before the grant
/// model existed) — it yields zero repos until a member re-runs the OAuth
/// connect hop (`connectUrl`; the install page does NOT re-capture grants).
/// `hasMore` exists only on the `repos` endpoint (nil on `status`).
public struct GithubInstallation: Decodable, Sendable, Identifiable {
    public var id: Int { installationId }
    public let installationId: Int
    public let accountLogin: String?
    public let accountType: String?
    public let manageUrl: String
    public let needsReauth: Bool
    public let hasMore: Bool?
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
    /// Per-installation grant state (grant model); `[]` on servers that predate
    /// the field.
    public let installations: [GithubInstallation]

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        configured = try container.decode(Bool.self, forKey: .configured)
        installed = try container.decode(Bool.self, forKey: .installed)
        installUrl = try container.decodeIfPresent(String.self, forKey: .installUrl)
        connectUrl = try container.decodeIfPresent(String.self, forKey: .connectUrl)
        repos = try container.decode([GithubPickerRepo].self, forKey: .repos)
        hasMore = try container.decode(Bool.self, forKey: .hasMore)
        installations = try container.decodeIfPresent([GithubInstallation].self, forKey: .installations) ?? []
    }

    private enum CodingKeys: String, CodingKey {
        case configured, installed, installUrl, connectUrl, repos, hasMore, installations
    }
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
