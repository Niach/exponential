import Foundation

@Observable
public final class TeamState {
    public var teams: [TeamEntity] = []
    public var boards: [BoardEntity] = []
    public var activeTeamId: String?
    /// EXP-1075: the caller's OWN live runs per team, recomputed beside the
    /// Agents tab dots (`AppNavigator.recomputeAgentDots`). The board
    /// switcher reads it for the "another team has your live runs" dot — the
    /// team-scoped lists otherwise leave those runs unannounced anywhere.
    public var liveRunsByTeam: [String: CodingSessionOwnership.TeamLiveRuns] = [:]

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
}
