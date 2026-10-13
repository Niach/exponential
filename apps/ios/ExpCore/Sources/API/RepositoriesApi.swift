import Foundation

// Mirrors apps/web/src/lib/trpc/repositories.ts. Repositories are server-only
// (NOT an Electric shape) — read on demand by the settings registry and the
// repo pickers.

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
/// the repo (EXP-557) — null for sharer-less rows; row management is
/// sharer-or-owner.
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
        sharedBy = try c.decodeIfPresent(RepoSharer.self, forKey: .sharedBy)
    }

    enum CodingKeys: String, CodingKey {
        case id, fullName, defaultBranch, boards, sharedBy
        case isPrivate = "private"
    }
}

/// `repositories.listBranches` — the repo's branch names, straight from
/// GitHub (EXP-462; EXP-712 reuses it for the board's branch field).
public struct RepoBranches: Decodable, Sendable {
    public let branches: [String]

    public init(branches: [String]) {
        self.branches = branches
    }
}

/// `repositories.add` returns the upserted registry row; only its id matters.
private struct AddRepoResult: Decodable {
    let repository: AddedRepo?

    struct AddedRepo: Decodable { let id: String }
}

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
    /// (`repositories.add`, web parity — repositories-section.tsx). Returns the
    /// upserted registry row's id so a caller can point a board straight at it
    /// (EXP-712); the settings list ignores it and re-fetches instead.
    @discardableResult
    public func add(
        accountId: String,
        teamId: String,
        fullName: String,
        defaultBranch: String,
        isPrivate: Bool
    ) async throws -> String? {
        let result: AddRepoResult = try await trpc.mutation(
            accountId: accountId,
            path: "repositories.add",
            input: AddRepoInput(
                teamId: teamId,
                fullName: fullName,
                defaultBranch: defaultBranch,
                private: isPrivate
            )
        )
        return result.repository?.id
    }

    /// Member-readable: the repo's branch names from GitHub (`.query`).
    /// Powers the board form's branch field and the repo default-branch pin.
    public func listBranches(accountId: String, repositoryId: String) async throws -> [String] {
        let result: RepoBranches = try await trpc.query(
            accountId: accountId,
            path: "repositories.listBranches",
            input: RepositoryIdInput(repositoryId: repositoryId)
        )
        return result.branches
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
}
