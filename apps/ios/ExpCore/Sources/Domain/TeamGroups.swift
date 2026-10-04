import Foundation

/// EXP-1186: the cross-team lists (Reviews, the Agent page's Running and
/// Recent runs, Actions) span EVERY member team like the Inbox, grouped by
/// team — but only when the caller is in more than one; a single team draws
/// exactly what it drew before. Teams order by name (case-insensitive, id as
/// the tie-break), the board switcher's team order; an item whose team is not
/// synced (not a member) is dropped, and items keep their input order inside
/// a group. Mirrored ×3 (web, Android).
public enum TeamGroups {
    public struct Group<Item>: Identifiable {
        public let team: TeamEntity
        public let items: [Item]
        public var id: String { team.id }

        public init(team: TeamEntity, items: [Item]) {
            self.team = team
            self.items = items
        }
    }

    /// Whether the lists group at all: more than one member team.
    public static func isMultiTeam(_ teams: [TeamEntity]) -> Bool {
        teams.count > 1
    }

    /// The team order every grouped list follows.
    public static func ordered(_ teams: [TeamEntity]) -> [TeamEntity] {
        teams.sorted { a, b in
            let an = a.name.lowercased()
            let bn = b.name.lowercased()
            if an != bn { return an < bn }
            return a.id < b.id
        }
    }

    /// `items` split per team in `ordered(teams)` order; empty teams are left
    /// out.
    public static func group<Item>(
        _ items: [Item],
        teams: [TeamEntity],
        teamId: (Item) -> String
    ) -> [Group<Item>] {
        let byTeam = Dictionary(grouping: items, by: teamId)
        return ordered(teams).compactMap { team in
            guard let teamItems = byTeam[team.id], !teamItems.isEmpty else { return nil }
            return Group(team: team, items: teamItems)
        }
    }
}
