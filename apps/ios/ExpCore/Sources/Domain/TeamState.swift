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
            .filter { $0.teamId == wsId && $0.archivedAt == nil }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }
}
