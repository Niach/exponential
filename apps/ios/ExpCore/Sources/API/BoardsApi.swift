import Foundation

/// v4: a board IS a repository. Create/retarget always carry a repo — either
/// an existing team-registry repo (`repositoryId`) or a brand-new repo
/// connected inline by `fullName` (server validates the GitHub-App install and
/// upserts the registry row in the same transaction). Mirrors the server
/// `repositoryInputSchema` (apps/web/src/lib/trpc/boards.ts).
public enum BoardRepositoryChoice: Encodable, Sendable, Equatable {
    /// Target an existing registry repo (same-team, not archived).
    case repositoryId(String)
    /// Connect a new repo inline by `owner/name`; the extra fields seed the
    /// registry row (all optional server-side). The installation id is
    /// resolved server-side from GitHub — clients never send it.
    case fullName(String, defaultBranch: String? = nil, isPrivate: Bool? = nil)

    private enum IdKeys: String, CodingKey { case repositoryId }
    private enum NameKeys: String, CodingKey { case fullName, defaultBranch, `private` }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case let .repositoryId(id):
            var c = encoder.container(keyedBy: IdKeys.self)
            try c.encode(id, forKey: .repositoryId)
        case let .fullName(name, defaultBranch, isPrivate):
            var c = encoder.container(keyedBy: NameKeys.self)
            try c.encode(name, forKey: .fullName)
            try c.encodeIfPresent(defaultBranch, forKey: .defaultBranch)
            try c.encodeIfPresent(isPrivate, forKey: .private)
        }
    }
}

public struct CreateBoardInput: Encodable, Sendable {
    public let teamId: String
    public let name: String
    public let prefix: String
    public var color: String?
    // The curated glyph name.
    public let icon: String?
    // Optional on EVERY board now (the type collapse): coding/PR affordances
    // gate on repo presence, never on a required repo. The server no longer
    // rejects a repo-less board.
    public let repository: BoardRepositoryChoice?
    // EXP-712: the board's own branch — coding sessions start from it and its
    // PRs target it. Only meaningful alongside `repository` (the server drops
    // it otherwise); nil = follow the repo's default branch.
    public let defaultBranch: String?

    public init(
        teamId: String,
        name: String,
        prefix: String,
        color: String? = nil,
        icon: String? = nil,
        repository: BoardRepositoryChoice? = nil,
        defaultBranch: String? = nil
    ) {
        self.teamId = teamId
        self.name = name
        self.prefix = prefix
        self.color = color
        self.icon = icon
        self.repository = repository
        self.defaultBranch = defaultBranch
    }

    enum CodingKeys: String, CodingKey {
        case teamId, name, prefix, color, icon, repository, defaultBranch
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(teamId, forKey: .teamId)
        try c.encode(name, forKey: .name)
        try c.encode(prefix, forKey: .prefix)
        try c.encodeIfPresent(color, forKey: .color)
        try c.encodeIfPresent(icon, forKey: .icon)
        // Omit `repository` entirely when nil so the server's optional schema
        // sees an absent key (not JSON null, which the union would reject).
        try c.encodeIfPresent(repository, forKey: .repository)
        // Same reasoning: absent, never null — the server's `nullish()` would
        // take a null, but omitting keeps create's payload minimal.
        try c.encodeIfPresent(defaultBranch, forKey: .defaultBranch)
    }
}

public struct SetBoardRepositoryInput: Encodable, Sendable {
    public let boardId: String
    /// Null clears the board's repo (coding affordances gate on presence).
    public let repositoryId: String?
    /// EXP-712: a retarget RESETS the board's branch unless one is passed —
    /// a branch belongs to the repo it was picked in.
    public let defaultBranch: String?

    public init(boardId: String, repositoryId: String?, defaultBranch: String? = nil) {
        self.boardId = boardId
        self.repositoryId = repositoryId
        self.defaultBranch = defaultBranch
    }

    enum CodingKeys: String, CodingKey {
        case boardId, repositoryId, defaultBranch
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(boardId, forKey: .boardId)
        // Explicit null: the server reads it as "no repository", so this key
        // can never be omitted the way an optional input would be.
        try c.encode(repositoryId, forKey: .repositoryId)
        try c.encodeIfPresent(defaultBranch, forKey: .defaultBranch)
    }
}

/// `boards.update` — a partial patch. Only the keys a caller sets are sent,
/// except `defaultBranch`, whose explicit null MEANS "follow the repo again"
/// (EXP-712) and so is always written when the caller opted into it.
public struct UpdateBoardInput: Encodable, Sendable {
    public let boardId: String
    public let name: String?
    public let color: String?
    public let icon: String?
    private let branchPatch: Bool
    private let defaultBranch: String?

    public init(
        boardId: String,
        name: String? = nil,
        color: String? = nil,
        icon: String? = nil
    ) {
        self.boardId = boardId
        self.name = name
        self.color = color
        self.icon = icon
        branchPatch = false
        defaultBranch = nil
    }

    /// The branch patch: `defaultBranch: nil` sends JSON null (follow the repo).
    public init(boardId: String, defaultBranch: String?) {
        self.boardId = boardId
        name = nil
        color = nil
        icon = nil
        branchPatch = true
        self.defaultBranch = defaultBranch
    }

    enum CodingKeys: String, CodingKey {
        case boardId, name, color, icon, defaultBranch
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(boardId, forKey: .boardId)
        try c.encodeIfPresent(name, forKey: .name)
        try c.encodeIfPresent(color, forKey: .color)
        try c.encodeIfPresent(icon, forKey: .icon)
        if branchPatch { try c.encode(defaultBranch, forKey: .defaultBranch) }
    }
}

public struct BoardResult: Decodable, Sendable {
    public let board: BoardResultData

    public init(board: BoardResultData) {
        self.board = board
    }
}

public struct BoardResultData: Decodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

public final class BoardsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Create a board. The server uppercases `prefix` and defaults `color`
    /// to `#6366f1` when omitted. Returns the new board id.
    public func create(accountId: String, _ input: CreateBoardInput) async throws -> String {
        let result: BoardResult = try await trpc.mutation(accountId: accountId, path: "boards.create", input: input)
        return result.board.id
    }

    /// Retarget a board's backing repo (owner/admin, server-enforced). The
    /// repo must already be in the team registry — connecting a brand-new
    /// repo happens via `create`'s inline path or `repositories.add`. Electric
    /// surfaces the updated `repository_id`.
    public func setRepository(
        accountId: String,
        boardId: String,
        repositoryId: String?,
        defaultBranch: String? = nil
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "boards.setRepository",
            input: SetBoardRepositoryInput(
                boardId: boardId,
                repositoryId: repositoryId,
                defaultBranch: defaultBranch
            )
        )
    }

    /// Patch a board (`boards.update`). EXP-712: `defaultBranch` nil means
    /// "follow the repo's default branch again" and is sent as JSON null.
    public func setDefaultBranch(accountId: String, boardId: String, defaultBranch: String?) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "boards.update",
            input: UpdateBoardInput(boardId: boardId, defaultBranch: defaultBranch)
        )
    }
}

/// #NEW-37: an archived or trashed board, as `boards.listArchived` /
/// `boards.listDeleted` return it (owner-only reads: both states are out of
/// every synced shape, EXP-500). Dates are ISO strings (no transformer).
public struct PutAwayBoard: Decodable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let color: String?
    public let icon: String?
    public let repositoryId: String?
    public let archivedAt: String?
    public let purgeAt: String?

    public init(
        id: String,
        name: String,
        color: String? = nil,
        icon: String? = nil,
        repositoryId: String? = nil,
        archivedAt: String? = nil,
        purgeAt: String? = nil
    ) {
        self.id = id
        self.name = name
        self.color = color
        self.icon = icon
        self.repositoryId = repositoryId
        self.archivedAt = archivedAt
        self.purgeAt = purgeAt
    }
}

private struct TeamIdInput: Encodable, Sendable {
    let teamId: String
}

private struct BoardIdInput: Encodable, Sendable {
    let boardId: String
}

public extension BoardsApi {
    /// `boards.listArchived` (owner-only).
    func listArchived(accountId: String, teamId: String) async throws -> [PutAwayBoard] {
        try await trpc.query(accountId: accountId, path: "boards.listArchived", input: TeamIdInput(teamId: teamId))
    }

    /// `boards.listDeleted` (owner-only): the 48h trash, each with its `purgeAt`.
    func listDeleted(accountId: String, teamId: String) async throws -> [PutAwayBoard] {
        try await trpc.query(accountId: accountId, path: "boards.listDeleted", input: TeamIdInput(teamId: teamId))
    }

    /// `boards.unarchive` (owner-only).
    func unarchive(accountId: String, boardId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "boards.unarchive", input: BoardIdInput(boardId: boardId))
    }

    /// `boards.restore` (owner-only): out of the trash before the purge.
    func restore(accountId: String, boardId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "boards.restore", input: BoardIdInput(boardId: boardId))
    }
}

/// #NEW-37 (settings-nav.json `boards`): the Archived / Trash rows' captions ×4
/// (web `formatArchivedOn` / `formatPurgeCountdown`, desktop `all_boards.rs`).
public enum PutAwayBoardCaption {
    /// "Archived Jul 1, 2026"; bare "Archived" without a timestamp.
    public static func archived(_ archivedAt: String?) -> String {
        guard let date = parse(archivedAt) else { return "Archived" }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "UTC")
        formatter.dateFormat = "MMM d, yyyy"
        return "Archived \(formatter.string(from: date))"
    }

    /// "Purges in ~Nh" (hours rounded up), "Purging soon" once due, "Purges
    /// soon" without a timestamp.
    public static func purge(_ purgeAt: String?, now: Date = Date()) -> String {
        guard let date = parse(purgeAt) else { return "Purges soon" }
        let ms = (date.timeIntervalSince(now) * 1000).rounded()
        if ms <= 0 { return "Purging soon" }
        let hours = Int((ms / 3_600_000).rounded(.up))
        return "Purges in ~\(hours)h"
    }

    private static func parse(_ raw: String?) -> Date? {
        guard let raw, !raw.isEmpty else { return nil }
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: raw) ?? ISO8601DateFormatter().date(from: raw)
    }
}
