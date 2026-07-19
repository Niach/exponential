import Foundation

public struct GetDefaultResult: Decodable, Sendable {
    // Nullable: teams.getDefault never creates (EXP-188) — a fresh signup
    // (or an owner who deleted their last team) resolves to nil.
    public let team: TeamResult?

    public init(team: TeamResult?) {
        self.team = team
    }
}

public struct CreateTeamResult: Decodable, Sendable {
    public let team: TeamResult

    public init(team: TeamResult) {
        self.team = team
    }
}

public struct TeamResult: Decodable, Sendable {
    public let id: String
    public let name: String
    public let slug: String

    public init(id: String, name: String, slug: String) {
        self.id = id
        self.name = name
        self.slug = slug
    }
}

public struct CreateTeamInput: Encodable, Sendable {
    public let name: String
    public var iconUrl: String?

    public init(name: String, iconUrl: String? = nil) {
        self.name = name
        self.iconUrl = iconUrl
    }
}

public struct DeleteTeamInput: Encodable, Sendable {
    public let teamId: String

    public init(teamId: String) {
        self.teamId = teamId
    }
}

public struct DeleteBoardInput: Encodable, Sendable {
    public let boardId: String

    public init(boardId: String) {
        self.boardId = boardId
    }
}

public final class TeamsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// The NON-CREATING default-team resolver (EXP-188, replaced
    /// `teams.ensureDefault`): the user's oldest non-feedback membership team,
    /// or nil when they have none. An input-less tRPC query, so it goes over
    /// GET like the other reads.
    public func getDefault(accountId: String) async throws -> TeamResult? {
        let result: GetDefaultResult = try await trpc.query(accountId: accountId, path: "teams.getDefault")
        return result.team
    }

    public func create(accountId: String, name: String, iconUrl: String? = nil) async throws -> TeamResult {
        let result: CreateTeamResult = try await trpc.mutation(accountId: accountId, path: "teams.create", input: CreateTeamInput(name: name, iconUrl: iconUrl))
        return result.team
    }

    public func delete(accountId: String, teamId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teams.delete", input: DeleteTeamInput(teamId: teamId))
    }

    public func deleteBoard(accountId: String, boardId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "boards.delete", input: DeleteBoardInput(boardId: boardId))
    }
}
