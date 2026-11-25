import Foundation

/// #EXP-2109: the phones' walk of the sidebar tree (one level per page), the
/// Swift mirror of packages/domain-contract/src/sidebar-tree.ts (fixture
/// `fixtures/sidebar-tree.json`, its `$comment` is the spec): `teams`,
/// `teamItems` and `phone`. A team's ONE mixed list = its boards, agents,
/// apps and the root sessions the member placed, over the member's folders
/// (`SidebarFolders`, section `team`).
public enum SidebarTree {
    /// The synced `sidebar_layout` section the tree reads.
    public static let section = "team"

    /// Fixture `copy`, the parts the phones draw.
    public enum Copy {
        public static let teams = "Teams"
        public static let newTeam = "New team"
        public static let joinTeam = "Join team"
    }

    /// The Teams header's + menu (`teamsMenu`).
    public enum TeamsMenuItem: String, Sendable, CaseIterable {
        case newTeam = "new-team"
        case joinTeam = "join-team"
    }

    public static let teamsMenu: [TeamsMenuItem] = TeamsMenuItem.allCases

    public enum ItemKind: String, Sendable { case board, agent, app, session }

    public struct TeamItem: Equatable, Sendable {
        public let id: String
        public let kind: ItemKind

        public init(id: String, kind: ItemKind) {
            self.id = id
            self.kind = kind
        }
    }

    public struct Agent: Equatable, Sendable {
        public let id: String
        public let sortOrder: Double
        public let role: String

        public init(id: String, sortOrder: Double, role: String) {
            self.id = id
            self.sortOrder = sortOrder
            self.role = role
        }
    }

    public struct Session: Equatable, Sendable {
        public let id: String
        public let teamId: String
        public let parentId: String?

        public init(id: String, teamId: String, parentId: String?) {
            self.id = id
            self.teamId = teamId
            self.parentId = parentId
        }
    }

    /// By name (case-insensitive), ties by id.
    private static func byName(_ a: (id: String, name: String), _ b: (id: String, name: String)) -> Bool {
        let an = a.name.lowercased(), bn = b.name.lowercased()
        return an != bn ? an < bn : a.id < b.id
    }

    /// `teams` (TS `sidebarTeams`): the member teams by name; the personal
    /// team never shows.
    public static func teams(_ teams: [(id: String, name: String, kind: String)]) -> [String] {
        teams.filter { $0.kind != "personal" }.map { (id: $0.id, name: $0.name) }.sorted(by: byName).map(\.id)
    }

    /// `teamItems` (TS `sidebarTeamItems`): boards by sortOrder, agents (role
    /// agent) by sortOrder, apps by name, then the placed root sessions in
    /// entry order.
    public static func teamItems(
        teamId: String,
        boards: [(id: String, sortOrder: Double)],
        agents: [Agent],
        apps: [(id: String, name: String)],
        sessions: [Session],
        entries: [SidebarFolders.Entry]
    ) -> [TeamItem] {
        let byId = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var seen = Set<String>()
        let placed = entries
            .filter { $0.kind == .item && $0.targetId != nil }
            .sorted { $0.sortOrder != $1.sortOrder ? $0.sortOrder < $1.sortOrder : $0.id < $1.id }
            .compactMap { $0.targetId.flatMap { byId[$0] } }
            .filter { $0.teamId == teamId && $0.parentId == nil && seen.insert($0.id).inserted }
        return boards
            .sorted { $0.sortOrder != $1.sortOrder ? $0.sortOrder < $1.sortOrder : $0.id < $1.id }
            .map { TeamItem(id: $0.id, kind: .board) }
            + agents.filter { $0.role == "agent" }
            .sorted { $0.sortOrder != $1.sortOrder ? $0.sortOrder < $1.sortOrder : $0.id < $1.id }
            .map { TeamItem(id: $0.id, kind: .agent) }
            + apps.sorted(by: byName).map { TeamItem(id: $0.id, kind: .app) }
            + placed.map { TeamItem(id: $0.id, kind: .session) }
    }

    // MARK: - Open keys

    /// The device-local expanded flag of a team (`openKeys.team`); no flag = OPEN.
    public static func teamOpenKey(_ teamId: String) -> String { "sidebar-team-open:\(teamId)" }

    // MARK: - `phone`

    public enum RowKind: Equatable, Hashable, Sendable {
        case team, folder
        case item(ItemKind)

        public var rawValue: String {
            switch self {
            case .team: "team"
            case .folder: "folder"
            case let .item(kind): kind.rawValue
            }
        }
    }

    /// One row of the Teams tab (TS `PhoneTreeRow`).
    public struct PhoneRow: Equatable, Hashable, Sendable, Identifiable {
        public let kind: RowKind
        public let id: String
        public let teamId: String
        public let depth: Int
        /// A folder's name; nil on the rest.
        public let name: String?
        public let open: Bool
        public let count: Int

        public init(kind: RowKind, id: String, teamId: String, depth: Int, name: String?, open: Bool, count: Int) {
            self.kind = kind
            self.id = id
            self.teamId = teamId
            self.depth = depth
            self.name = name
            self.open = open
            self.count = count
        }
    }

    public struct PhoneTeam: Sendable {
        public let id: String
        public let items: [TeamItem]
        public let entries: [SidebarFolders.Entry]
        public let open: Bool
        public let closedFolders: Set<String>

        public init(id: String, items: [TeamItem], entries: [SidebarFolders.Entry], open: Bool, closedFolders: Set<String>) {
            self.id = id
            self.items = items
            self.entries = entries
            self.open = open
            self.closedFolders = closedFolders
        }
    }

    /// THE Teams tab (TS `phoneTreeRows`): the teams in order, each its row
    /// and, while open, its nodes one level deeper.
    public static func phoneTreeRows(_ teams: [PhoneTeam]) -> [PhoneRow] {
        teams.flatMap { team -> [PhoneRow] in
            let kinds = Dictionary(team.items.map { ($0.id, $0.kind) }, uniquingKeysWith: { _, b in b })
            let rows = SidebarFolders.rows(
                SidebarFolders.tree(items: team.items.map(\.id), entries: team.entries)
            ) { !team.closedFolders.contains($0) }
            let head = PhoneRow(
                kind: .team, id: team.id, teamId: team.id, depth: 0, name: nil, open: team.open,
                count: rows.filter { $0.depth == 0 }.count
            )
            guard team.open else { return [head] }
            return [head] + rows.compactMap { row in
                let kind: RowKind
                if row.kind == .folder {
                    kind = .folder
                } else if let item = kinds[row.id] {
                    kind = .item(item)
                } else {
                    return nil
                }
                return PhoneRow(
                    kind: kind, id: row.id, teamId: team.id, depth: row.depth + 1, name: row.name, open: row.open,
                    count: row.count
                )
            }
        }
    }

    /// The team whose empty state the Teams tab shows (TS `phoneTeamsEmpty`):
    /// the viewer's only team, without a node.
    public static func phoneTeamsEmpty(_ teams: [(id: String, count: Int)]) -> String? {
        teams.count == 1 && teams[0].count == 0 ? teams[0].id : nil
    }
}
