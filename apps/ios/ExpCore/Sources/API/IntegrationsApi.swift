import Foundation

// SLOP-7/SLOP-26: ONE GitHub flow. Mirrors `apps/web/src/lib/trpc/integrations.ts`.
// A member's GitHub connection is their own linked GitHub account; the
// installations and push-able repositories are listed LIVE off GitHub with
// that token. Nothing is claimed per team any more, so `installations[]`
// carries no re-auth / stale marks — a dead token is reported ONCE on the
// whole connection (`needsReconnect`).

/// The viewer's connection half every surface renders (`status` and `repos`
/// share it). `linked` falls back to `installed` and `needsReconnect` to
/// false on a server predating the fields.
public struct GithubStatusResult: Decodable, Sendable {
    public let configured: Bool
    /// The viewer has a GitHub account linked (a token exists, live or dead).
    public let linked: Bool
    /// Linked, but GitHub refuses the token — Reconnect is the one fix.
    public let needsReconnect: Bool
    /// The linked account's GitHub login; nil until the token answered.
    public let login: String?
    /// The App is installed on at least one account the token can see.
    public let installed: Bool
    /// GitHub's install page for the App (nil without `GITHUB_APP_SLUG`).
    public let installUrl: String?
    /// The guided web page (`/integrations/github?return=app`), which hands
    /// back through `exponential://github-connected`. The fallback connect
    /// hop when the server does not offer the GitHub link ticket.
    public let connectUrl: String?
    public let installations: [GithubInstallation]

    public init(
        configured: Bool,
        linked: Bool,
        needsReconnect: Bool = false,
        login: String? = nil,
        installed: Bool,
        installUrl: String?,
        connectUrl: String? = nil,
        installations: [GithubInstallation] = []
    ) {
        self.configured = configured
        self.linked = linked
        self.needsReconnect = needsReconnect
        self.login = login
        self.installed = installed
        self.installUrl = installUrl
        self.connectUrl = connectUrl
        self.installations = installations
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let installed = try container.decode(Bool.self, forKey: .installed)
        self.init(
            configured: try container.decode(Bool.self, forKey: .configured),
            linked: try container.decodeIfPresent(Bool.self, forKey: .linked) ?? installed,
            needsReconnect: try container.decodeIfPresent(Bool.self, forKey: .needsReconnect) ?? false,
            login: try container.decodeIfPresent(String.self, forKey: .login),
            installed: installed,
            installUrl: try container.decodeIfPresent(String.self, forKey: .installUrl),
            connectUrl: try container.decodeIfPresent(String.self, forKey: .connectUrl),
            installations: try container.decodeIfPresent([GithubInstallation].self, forKey: .installations) ?? []
        )
    }

    private enum CodingKeys: String, CodingKey {
        case configured, linked, needsReconnect, login, installed, installUrl, connectUrl, installations
    }

    /// The missing prerequisite, nil once repositories can be listed.
    public var prerequisite: GithubConnect.Prerequisite? {
        GithubConnect.prerequisite(
            configured: configured, linked: linked, needsReconnect: needsReconnect, installed: installed
        )
    }
}

/// One GitHub App installation the viewer's token sees (mirrors the web
/// `installationSummary`). `suspended` marks a GitHub-side App suspension
/// (REV2-29): the installation lists no repos and mints no tokens until it is
/// UNSUSPENDED on GitHub — a reconnect cannot fix it, so the UI never nudges
/// one. `hasMore` exists only on the `repos` endpoint (nil on `status`).
public struct GithubInstallation: Decodable, Sendable, Identifiable {
    public var id: Int { installationId }
    public let installationId: Int
    public let accountLogin: String?
    public let accountType: String?
    public let manageUrl: String
    public let suspended: Bool?
    public let hasMore: Bool?

    public init(
        installationId: Int,
        accountLogin: String?,
        accountType: String? = nil,
        manageUrl: String,
        suspended: Bool? = nil,
        hasMore: Bool? = nil
    ) {
        self.installationId = installationId
        self.accountLogin = accountLogin
        self.accountType = accountType
        self.manageUrl = manageUrl
        self.suspended = suspended
        self.hasMore = hasMore
    }

    /// Suspension with the servers-predating-the-field default applied.
    public var isSuspended: Bool { suspended ?? false }
}

/// One repo the viewer can push to (mirrors web `InstallationRepo`). JSON
/// keys are camelCase so the plain decoder maps them directly; `private` is a
/// Swift keyword so it's backticked.
public struct GithubPickerRepo: Decodable, Sendable, Identifiable {
    public var id: String { fullName }
    public let fullName: String
    public let `private`: Bool
    public let defaultBranch: String
    public let installationId: Int
}

public struct GithubReposResult: Decodable, Sendable {
    public let configured: Bool
    public let linked: Bool
    public let needsReconnect: Bool
    public let login: String?
    public let installed: Bool
    public let installUrl: String?
    public let connectUrl: String?
    public let repos: [GithubPickerRepo]
    public let hasMore: Bool
    public let installations: [GithubInstallation]

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        configured = try container.decode(Bool.self, forKey: .configured)
        installed = try container.decode(Bool.self, forKey: .installed)
        linked = try container.decodeIfPresent(Bool.self, forKey: .linked) ?? installed
        needsReconnect = try container.decodeIfPresent(Bool.self, forKey: .needsReconnect) ?? false
        login = try container.decodeIfPresent(String.self, forKey: .login)
        installUrl = try container.decodeIfPresent(String.self, forKey: .installUrl)
        connectUrl = try container.decodeIfPresent(String.self, forKey: .connectUrl)
        repos = try container.decode([GithubPickerRepo].self, forKey: .repos)
        hasMore = try container.decode(Bool.self, forKey: .hasMore)
        installations = try container.decodeIfPresent([GithubInstallation].self, forKey: .installations) ?? []
    }

    private enum CodingKeys: String, CodingKey {
        case configured, linked, needsReconnect, login, installed, installUrl, connectUrl, repos, hasMore, installations
    }

    /// The missing prerequisite, nil once the live list renders.
    public var prerequisite: GithubConnect.Prerequisite? {
        GithubConnect.prerequisite(
            configured: configured, linked: linked, needsReconnect: needsReconnect, installed: installed
        )
    }
}

/// `{}` — the body of a mutation that takes no input.
private struct EmptyInput: Encodable {}

public final class IntegrationsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// The viewer's GitHub connection for a team context (member-gated):
    /// linked or not, the login, the installations their token sees, and the
    /// two hops. `mobile: true` marks `connectUrl` so the guided page hands
    /// back through `exponential://github-connected`.
    public func githubStatus(accountId: String, teamId: String, mobile: Bool = false) async throws -> GithubStatusResult {
        struct Input: Encodable {
            let teamId: String
            let platform: String?
        }
        return try await trpc.query(
            accountId: accountId,
            path: "integrations.github.status",
            input: Input(teamId: teamId, platform: mobile ? "mobile" : nil)
        )
    }

    /// The repositories the viewer may add: every push-able repo of every
    /// installation their token sees, deduped and sorted. Always mobile-marked.
    /// `refresh` bypasses the server's per-user discovery cache — pass it when
    /// re-querying right after a GitHub hop so new repos show immediately.
    public func githubRepos(accountId: String, teamId: String, refresh: Bool = false) async throws -> GithubReposResult {
        struct Input: Encodable {
            let platform: String
            let teamId: String
            let refresh: Bool?
        }
        return try await trpc.query(
            accountId: accountId,
            path: "integrations.github.repos",
            input: Input(platform: "mobile", teamId: teamId, refresh: refresh ? true : nil)
        )
    }

    /// FEED-30: the Add-repository picker's "Add by name" escape hatch
    /// (`integrations.github.lookupRepo`). Resolves an `owner/name` through
    /// EXACTLY the connect gate (the viewer's token, push access, the App
    /// installed), so a failure's message names the real reason and is shown
    /// verbatim. Read-only; the result is exactly a picker row.
    public func lookupRepo(accountId: String, teamId: String, fullName: String) async throws -> GithubPickerRepo {
        struct Input: Encodable {
            let teamId: String
            let fullName: String
        }
        return try await trpc.query(
            accountId: accountId,
            path: "integrations.github.lookupRepo",
            input: Input(teamId: teamId, fullName: fullName)
        )
    }

    /// Disconnect the viewer's own GitHub account (`integrations.github.
    /// disconnect`). Repositories already added keep working — their tokens
    /// mint off the App installation, not off this person's token.
    public func githubDisconnect(accountId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "integrations.github.disconnect",
            input: EmptyInput()
        )
    }
}
