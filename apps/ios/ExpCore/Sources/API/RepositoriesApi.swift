import Foundation

// Mirrors apps/web/src/lib/trpc/repositories.ts. Repositories are server-only
// (NOT an Electric shape) — read on demand for the native "Start coding"
// launcher: resolve the issue's repo, then mint a short-lived push token.

/// The repo backing an issue's board (v4: the board's `repositoryId`).
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

/// A board backed by a repo (v4 `repositories.list().boards[]`). Powers the
/// settings "used by" chips and mobile repo pickers.
public struct RepoBoardSummary: Decodable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let slug: String

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = (try? c.decode(String.self, forKey: .name)) ?? ""
        slug = (try? c.decode(String.self, forKey: .slug)) ?? ""
    }

    enum CodingKeys: String, CodingKey { case id, name, slug }
}

/// The member who shared a repo with the team (EXP-557 per-user sharing,
/// `repositories.list` row's `sharedBy`). `name`/`email` are display-only and
/// may be null server-side.
public struct RepoSharer: Decodable, Sendable, Equatable {
    public let id: String
    public let name: String?
    public let email: String?
}

/// One connected repo in the team registry (`repositories.list` row).
/// Decoded defensively — the server spreads the full DB row; we read only the
/// fields the settings surface needs. `private` is a Swift keyword, mapped to
/// `isPrivate`. `boards` are the boards this repo backs (v4 — no more
/// per-board links / primary star). `sharedBy` is the member who connected
/// the repo (EXP-557) — nil on old servers and for legacy sharer-less rows;
/// row management is sharer-or-owner.
public struct TeamRepo: Decodable, Sendable, Identifiable, Equatable {
    public let id: String
    public let fullName: String
    public let defaultBranch: String
    public let isPrivate: Bool
    public let boards: [RepoBoardSummary]
    public let sharedBy: RepoSharer?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        fullName = try c.decode(String.self, forKey: .fullName)
        defaultBranch = (try? c.decode(String.self, forKey: .defaultBranch)) ?? "main"
        isPrivate = (try? c.decode(Bool.self, forKey: .isPrivate)) ?? false
        boards = (try? c.decode([RepoBoardSummary].self, forKey: .boards)) ?? []
        sharedBy = (try? c.decodeIfPresent(RepoSharer.self, forKey: .sharedBy)) ?? nil
    }

    enum CodingKeys: String, CodingKey {
        case id, fullName, defaultBranch, boards, sharedBy
        case isPrivate = "private"
    }
}

private struct IssueIdInput: Encodable { let issueId: String }
private struct RepositoryIdInput: Encodable { let repositoryId: String }
private struct RepoTeamIdInput: Encodable { let teamId: String }
private struct AddRepoInput: Encodable {
    let teamId: String
    let fullName: String
    let defaultBranch: String
    let `private`: Bool
}

public final class RepositoriesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Resolve the repo backing an issue's board (v4: the board's
    /// `repositoryId`). `repositories.forIssue` is a `.query`, so this uses the
    /// input-bearing GET helper. `nil` only for dangling data (a valid issue
    /// always has a backing repo).
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

    // MARK: - Team registry (settings surface, masterplan §6)

    /// Member-readable: the team's repos, each with the boards it backs
    /// (v4 `boards[]` computed from `boards.repositoryId`).
    public func list(accountId: String, teamId: String) async throws -> [TeamRepo] {
        try await trpc.query(
            accountId: accountId,
            path: "repositories.list",
            input: RepoTeamIdInput(teamId: teamId)
        )
    }

    /// Member-level (EXP-557 — connecting SHARES the repo with the team):
    /// register a repo reachable through the caller's own GitHub connection
    /// (`repositories.add`, web parity — repositories-section.tsx). The
    /// `{repository}` response is discarded; callers re-fetch the registry list.
    public func add(
        accountId: String,
        teamId: String,
        fullName: String,
        defaultBranch: String,
        isPrivate: Bool
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "repositories.add",
            input: AddRepoInput(
                teamId: teamId,
                fullName: fullName,
                defaultBranch: defaultBranch,
                private: isPrivate
            )
        )
    }

    /// Sharer-or-owner (server-enforced, EXP-557): remove a repo. Blocked
    /// (CONFLICT) while any board still points at it — the caller surfaces the
    /// server message.
    public func remove(accountId: String, repositoryId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "repositories.remove",
            input: RepositoryIdInput(repositoryId: repositoryId)
        )
    }

    /// Middle tier of remote Changes visibility (§4.8, L18): the issue's
    /// `exp/<IDENTIFIER>` branch compared against the repo default branch,
    /// returned in the shared `prFiles` shape so the diff renderer is reused.
    /// `nil` ⇒ the branch was never pushed (or no repo/PR yet). `.query`, so
    /// this uses the GET-with-input helper.
    public func branchDiff(accountId: String, issueId: String) async throws -> PrFilesResult? {
        try await trpc.query(
            accountId: accountId,
            path: "repositories.branchDiff",
            input: IssueIdInput(issueId: issueId)
        )
    }
}
