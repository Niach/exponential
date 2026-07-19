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

    public init(
        teamId: String,
        name: String,
        prefix: String,
        color: String? = nil,
        icon: String? = nil,
        repository: BoardRepositoryChoice? = nil
    ) {
        self.teamId = teamId
        self.name = name
        self.prefix = prefix
        self.color = color
        self.icon = icon
        self.repository = repository
    }

    enum CodingKeys: String, CodingKey {
        case teamId, name, prefix, color, icon, repository
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
    }
}

public struct SetBoardRepositoryInput: Encodable, Sendable {
    public let boardId: String
    public let repositoryId: String

    public init(boardId: String, repositoryId: String) {
        self.boardId = boardId
        self.repositoryId = repositoryId
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
    public func setRepository(accountId: String, boardId: String, repositoryId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "boards.setRepository",
            input: SetBoardRepositoryInput(boardId: boardId, repositoryId: repositoryId)
        )
    }
}
