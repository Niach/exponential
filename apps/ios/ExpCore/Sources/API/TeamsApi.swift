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

public struct TeamIdInput: Encodable, Sendable {
    public let teamId: String

    public init(teamId: String) {
        self.teamId = teamId
    }
}

/// `teams.inviteCapacity` — seats left for new members, pending invites
/// already counted. `nil` = unlimited (self-hosted, or a plan with no seat
/// ceiling).
public struct InviteCapacityResult: Decodable, Sendable {
    public let remaining: Int?

    public init(remaining: Int?) {
        self.remaining = remaining
    }
}

/// `teams.update`: owner-only, every field optional (nil = unchanged).
public struct UpdateTeamInput: Encodable, Sendable {
    public let teamId: String
    public var name: String?
    /// EXP-1105: yolo mode.
    public var yoloMode: Bool?
    /// EXP-1025: the team prompt (raw markdown; "" = none).
    public var agentPrompt: String?
    /// VAPP-201: the team time zone. nil = unchanged, `.some(nil)` = clear
    /// it (sent as JSON null: each device's own zone).
    public var timeZone: String??
    /// #EXP-1645: the team image. nil = unchanged, `.some(nil)` = remove it
    /// (sent as JSON null); a new image goes through the upload route.
    public var iconUrl: String??

    public init(
        teamId: String,
        name: String? = nil,
        yoloMode: Bool? = nil,
        agentPrompt: String? = nil,
        timeZone: String?? = nil,
        iconUrl: String?? = nil
    ) {
        self.teamId = teamId
        self.name = name
        self.yoloMode = yoloMode
        self.agentPrompt = agentPrompt
        self.timeZone = timeZone
        self.iconUrl = iconUrl
    }

    private enum CodingKeys: String, CodingKey {
        case teamId, name, yoloMode, agentPrompt, timeZone, iconUrl
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(teamId, forKey: .teamId)
        try c.encodeIfPresent(name, forKey: .name)
        try c.encodeIfPresent(yoloMode, forKey: .yoloMode)
        try c.encodeIfPresent(agentPrompt, forKey: .agentPrompt)
        if let timeZone {
            if let timeZone { try c.encode(timeZone, forKey: .timeZone) } else { try c.encodeNil(forKey: .timeZone) }
        }
        if let iconUrl {
            if let iconUrl { try c.encode(iconUrl, forKey: .iconUrl) } else { try c.encodeNil(forKey: .iconUrl) }
        }
    }
}

/// `teams.getAgentPrompt`: the server-only team prompt (not on the shape).
public struct TeamAgentPromptResult: Decodable, Sendable {
    public let agentPrompt: String

    public init(agentPrompt: String) {
        self.agentPrompt = agentPrompt
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

    /// Seats left for new members (EXP-725), any member may ask. nil =
    /// unlimited. The invite control renders ONLY while this is non-zero — at
    /// the cap the app removes it entirely instead of naming a seat limit
    /// (App Store 3.1.1).
    public func inviteCapacity(accountId: String, teamId: String) async throws -> Int? {
        let result: InviteCapacityResult = try await trpc.query(
            accountId: accountId,
            path: "teams.inviteCapacity",
            input: TeamIdInput(teamId: teamId)
        )
        return result.remaining
    }

    public func create(accountId: String, name: String, iconUrl: String? = nil) async throws -> TeamResult {
        let result: CreateTeamResult = try await trpc.mutation(accountId: accountId, path: "teams.create", input: CreateTeamInput(name: name, iconUrl: iconUrl))
        return result.team
    }

    /// Owner-only `teams.update` (General: Name, Yolo mode, Team prompt).
    /// The synced team row echoes name + yolo mode back.
    public func update(accountId: String, _ input: UpdateTeamInput) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teams.update", input: input)
    }

    /// Member-readable `teams.getAgentPrompt` — the ONE read path of the
    /// team prompt.
    public func agentPrompt(accountId: String, teamId: String) async throws -> String {
        let result: TeamAgentPromptResult = try await trpc.query(
            accountId: accountId,
            path: "teams.getAgentPrompt",
            input: TeamIdInput(teamId: teamId)
        )
        return result.agentPrompt
    }

    public func delete(accountId: String, teamId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teams.delete", input: DeleteTeamInput(teamId: teamId))
    }

    public func deleteBoard(accountId: String, boardId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "boards.delete", input: DeleteBoardInput(boardId: boardId))
    }
}
