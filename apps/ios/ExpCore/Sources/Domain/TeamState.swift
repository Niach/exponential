import Foundation

@Observable
public final class TeamState {
    public var teams: [TeamEntity] = []
    public var boards: [BoardEntity] = []
    public var activeTeamId: String?
    /// EXP-1075: the caller's OWN working sessions per team
    /// (`SessionTree.liveByTeam`). The board switcher SHEET reads it for its
    /// per-team dots (EXP-1210: the pill that opens it wears none).
    public var liveRunsByTeam: [String: SessionTree.TeamLiveSessions] = [:]

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
