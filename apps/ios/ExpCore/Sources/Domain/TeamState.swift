import Foundation

@Observable
public final class TeamState {
    public var teams: [TeamEntity] = []
    public var boards: [BoardEntity] = []
    public var activeTeamId: String?

    public init() {}

    public var activeTeam: TeamEntity? {
        teams.first { $0.id == activeTeamId } ?? teams.first
    }

    public var filteredBoards: [BoardEntity] {
        guard let wsId = activeTeam?.id else { return [] }
        return boards
            .filter { $0.teamId == wsId }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }

    /// #EXP-2079 (agent-settings.json `assistant.chatTeams`): the teams the
    /// lists of a chat read, off the synced teams in the cross-team order
    /// (`TeamGroups.ordered`). `ownTeamId` = the session's / agent's team,
    /// `pickedTeamIds` = the conversation root's Teams pick.
    public func chatTeamIds(ownTeamId: String?, pickedTeamIds: [String]?) -> [String] {
        AgentSettings.chatTeams(
            ownTeamId: ownTeamId,
            syncedTeamIds: TeamGroups.ordered(teams).map(\.id),
            pickedTeamIds: pickedTeamIds
        )
    }
}
