import Foundation

// Mirrors apps/web/src/lib/trpc/repositories.ts. Repositories are server-only
// (NOT an Electric shape) — read on demand for the native "Start coding"
// launcher: resolve the issue's repo, then mint a short-lived push token.

/// The repo backing an issue's project (primary link, else the sole link).
public struct RepoForIssue: Decodable, Sendable {
    public let repositoryId: String
    public let fullName: String
    public let defaultBranch: String

    public init(repositoryId: String, fullName: String, defaultBranch: String) {
        self.repositoryId = repositoryId
        self.fullName = fullName
        self.defaultBranch = defaultBranch
    }
}

/// A short-lived GitHub App installation token for a repo (never persisted).
public struct InstallationToken: Decodable, Sendable {
    public let token: String
    public let fullName: String
    public let defaultBranch: String
    /// Server-reported expiry; not consumed client-side (the token is used
    /// immediately and re-minted per session). Tolerates string or numeric JSON.
    public let expiresAt: String?

    enum CodingKeys: String, CodingKey {
        case token, fullName, defaultBranch, expiresAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        token = try c.decode(String.self, forKey: .token)
        fullName = try c.decode(String.self, forKey: .fullName)
        defaultBranch = try c.decode(String.self, forKey: .defaultBranch)
        if let s = try? c.decode(String.self, forKey: .expiresAt) {
            expiresAt = s
        } else if let n = try? c.decode(Double.self, forKey: .expiresAt) {
            expiresAt = String(n)
        } else {
            expiresAt = nil
        }
    }
}

private struct IssueIdInput: Encodable { let issueId: String }
private struct RepositoryIdInput: Encodable { let repositoryId: String }

public final class RepositoriesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Resolve the repo backing an issue's project. `repositories.forIssue` is a
    /// `.query`, so this uses the input-bearing GET helper. `nil` ⇒ no linked repo
    /// (the launcher shows a "Link a repository" state).
    public func forIssue(accountId: String, issueId: String) async throws -> RepoForIssue? {
        try await trpc.query(
            accountId: accountId,
            path: "repositories.forIssue",
            input: IssueIdInput(issueId: issueId)
        )
    }

    /// Mint a short-lived, repo-scoped GitHub App installation token (session-
    /// gated mutation). Used to build the worktree's token-embedded push remote.
    public func installationToken(accountId: String, repositoryId: String) async throws -> InstallationToken {
        try await trpc.mutation(
            accountId: accountId,
            path: "repositories.installationToken",
            input: RepositoryIdInput(repositoryId: repositoryId)
        )
    }
}
