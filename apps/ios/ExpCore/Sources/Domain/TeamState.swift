import Foundation

/// The account's synced teams and boards. #EXP-2109: there is NO active
/// team; a screen's team is derived from what it shows.
@Observable
public final class TeamState {
    public var teams: [TeamEntity] = []
    public var boards: [BoardEntity] = []

    public init() {}

    /// The member teams in order (sidebar-tree.json `teams`; a personal team
    /// never syncs).
    public var orderedTeams: [TeamEntity] { TeamGroups.ordered(teams) }

    public func team(_ id: String?) -> TeamEntity? {
        id.flatMap { id in teams.first { $0.id == id } }
    }

    /// A team's boards in sidebar order.
    public func boards(of teamId: String) -> [BoardEntity] {
        BoardOrder.sorted(boards.filter { $0.teamId == teamId })
    }

    /// The first board of the first team that has one.
    public var firstBoard: BoardEntity? {
        orderedTeams.lazy.compactMap { self.boards(of: $0.id).first }.first
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
