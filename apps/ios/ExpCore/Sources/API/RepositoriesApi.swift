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
    /// GitHub's own default (`githubDefaultBranch`): picking it clears the
    /// pin. Older servers omit it = the effective branch.
    public let githubDefaultBranch: String
    public let isPrivate: Bool
    public let boards: [RepoBoardSummary]
    public let sharedBy: RepoSharer?
    /// #EXP-1616: `github` | `ecode` (Exponential's own git); older servers
    /// omit it = GitHub.
    public let provider: String

    /// `ecode` = Exponential hosts the repo.
    public var isEcode: Bool { provider == "ecode" }

    /// The route's `$name`: the full name's last segment.
    public var name: String { fullName.split(separator: "/").last.map(String.init) ?? fullName }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        fullName = try c.decode(String.self, forKey: .fullName)
        defaultBranch = (try? c.decode(String.self, forKey: .defaultBranch)) ?? "main"
        githubDefaultBranch = (try? c.decode(String.self, forKey: .githubDefaultBranch)) ?? defaultBranch
        isPrivate = (try? c.decode(Bool.self, forKey: .isPrivate)) ?? false
        boards = (try? c.decode([RepoBoardSummary].self, forKey: .boards)) ?? []
        sharedBy = try c.decodeIfPresent(RepoSharer.self, forKey: .sharedBy)
        provider = (try? c.decodeIfPresent(String.self, forKey: .provider)) ?? "github"
    }

    enum CodingKeys: String, CodingKey {
        case id, fullName, defaultBranch, githubDefaultBranch, boards, sharedBy, provider
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

/// #EXP-1616 `repositories.settings`: an Exponential repo's Settings tab.
public struct EcodeRepoSettings: Decodable, Sendable, Equatable {
    public struct Backup: Decodable, Sendable, Equatable, Identifiable {
        public let id: String
        /// ISO-8601 wire timestamp.
        public let createdAt: String
        public let bytes: Int
        /// How many refs the bundle holds.
        public let refs: Int

        public init(id: String, createdAt: String, bytes: Int, refs: Int) {
            self.id = id
            self.createdAt = createdAt
            self.bytes = bytes
            self.refs = refs
        }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decode(String.self, forKey: .id)
            createdAt = (try? c.decode(String.self, forKey: .createdAt)) ?? ""
            bytes = (try? c.decode(Int.self, forKey: .bytes)) ?? 0
            refs = (try? c.decode(Int.self, forKey: .refs)) ?? 0
        }

        enum CodingKeys: String, CodingKey { case id, createdAt, bytes, refs }
    }

    public let id: String
    public let fullName: String
    /// The clone URL, `exp://<team>/<name>`.
    public let remoteUrl: String?
    public let defaultBranch: String
    public let mirrorUrl: String?
    public let mirrorEnabled: Bool
    /// Newest first.
    public let backups: [Backup]

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        fullName = (try? c.decode(String.self, forKey: .fullName)) ?? ""
        remoteUrl = try? c.decodeIfPresent(String.self, forKey: .remoteUrl)
        defaultBranch = (try? c.decode(String.self, forKey: .defaultBranch)) ?? "main"
        mirrorUrl = try? c.decodeIfPresent(String.self, forKey: .mirrorUrl)
        mirrorEnabled = (try? c.decode(Bool.self, forKey: .mirrorEnabled)) ?? false
        let rows = (try? c.decode([Backup].self, forKey: .backups)) ?? []
        backups = rows.sorted { $0.createdAt > $1.createdAt }
    }

    enum CodingKeys: String, CodingKey { case id, fullName, remoteUrl, defaultBranch, mirrorUrl, mirrorEnabled, backups }
}

/// #EXP-1616 `repositories.branches`: an Exponential repo's branch heads.
public struct EcodeBranch: Decodable, Sendable, Equatable {
    public let name: String
    public let sha: String

    public init(name: String, sha: String) {
        self.name = name
        self.sha = sha
    }
}

private struct EcodeBranches: Decodable { let branches: [EcodeBranch] }

/// `repositories.create {teamId, name, defaultBranch?, mirrorUrl?}`.
private struct CreateRepoInput: Encodable {
    let teamId: String
    let name: String
    let defaultBranch: String
    let mirrorUrl: String?

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(teamId, forKey: .teamId)
        try c.encode(name, forKey: .name)
        try c.encode(defaultBranch, forKey: .defaultBranch)
        try c.encode(mirrorUrl, forKey: .mirrorUrl)
    }

    enum CodingKeys: String, CodingKey { case teamId, name, defaultBranch, mirrorUrl }
}

/// `repositories.setMirror`: `mirrorUrl: nil` is sent as an explicit null.
private struct SetMirrorInput: Encodable {
    let repositoryId: String
    let mirrorUrl: String?
    let enabled: Bool

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(repositoryId, forKey: .repositoryId)
        try c.encode(mirrorUrl, forKey: .mirrorUrl)
        try c.encode(enabled, forKey: .enabled)
    }

    enum CodingKeys: String, CodingKey { case repositoryId, mirrorUrl, enabled }
}

private struct RestoreInput: Encodable {
    let repositoryId: String
    let backupId: String
}

/// `repositories.add` returns the upserted registry row; only its id matters.
private struct AddRepoResult: Decodable {
    let repository: AddedRepo?

    struct AddedRepo: Decodable { let id: String }
}

private struct RepositoryIdInput: Encodable { let repositoryId: String }
/// `branch: nil` is sent as an explicit null (follow GitHub's default again).
private struct SetDefaultBranchInput: Encodable {
    let repositoryId: String
    let branch: String?

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(repositoryId, forKey: .repositoryId)
        try c.encode(branch, forKey: .branch)
    }

    enum CodingKeys: String, CodingKey { case repositoryId, branch }
}
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

    /// Sharer-or-owner (EXP-462, web `DefaultBranchMenu`): pin the repo's
    /// default branch; nil clears the pin (GitHub's default again).
    public func setDefaultBranch(accountId: String, repositoryId: String, branch: String?) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "repositories.setDefaultBranch",
            input: SetDefaultBranchInput(repositoryId: repositoryId, branch: branch)
        )
    }

    // MARK: - Exponential repos (#EXP-1616, repo-page.json)

    /// Owners: create an EMPTY Exponential repo on the team bucket; its
    /// first `git push exp://<team>/<name>` fills it. Returns the new id.
    @discardableResult
    public func create(
        accountId: String,
        teamId: String,
        name: String,
        defaultBranch: String,
        mirrorUrl: String?
    ) async throws -> String? {
        let result: AddRepoResult = try await trpc.mutation(
            accountId: accountId,
            path: "repositories.create",
            input: CreateRepoInput(teamId: teamId, name: name, defaultBranch: defaultBranch, mirrorUrl: mirrorUrl)
        )
        return result.repository?.id
    }

    /// Member-readable: the repo page's Settings tab.
    public func settings(accountId: String, repositoryId: String) async throws -> EcodeRepoSettings {
        try await trpc.query(
            accountId: accountId,
            path: "repositories.settings",
            input: RepositoryIdInput(repositoryId: repositoryId)
        )
    }

    /// Member-readable: the repo's branch heads with their shas.
    public func branches(accountId: String, repositoryId: String) async throws -> [EcodeBranch] {
        let result: EcodeBranches = try await trpc.query(
            accountId: accountId,
            path: "repositories.branches",
            input: RepositoryIdInput(repositoryId: repositoryId)
        )
        return result.branches
    }

    /// Owners: the optional GitHub mirror, pushed after each landing.
    public func setMirror(accountId: String, repositoryId: String, mirrorUrl: String?, enabled: Bool) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "repositories.setMirror",
            input: SetMirrorInput(repositoryId: repositoryId, mirrorUrl: mirrorUrl, enabled: enabled)
        )
    }

    /// Owners: restore a bundle backup (a device job).
    public func restore(accountId: String, repositoryId: String, backupId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "repositories.restore",
            input: RestoreInput(repositoryId: repositoryId, backupId: backupId)
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
}
