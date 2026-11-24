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
        public static let new = "New"
        public static let newBoard = "Board"
        public static let newAgent = "Agent"
        public static let newApp = "App"
        public static let settings = "Settings"
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

    // MARK: - `phone`

    public enum MenuItem: String, Sendable { case new, settings }

    public struct MenuEntry: Equatable, Sendable {
        public let id: MenuItem
        public let children: [ItemKind]
    }

    /// A phone team page's `…` menu (TS `phoneTeamMenu`).
    public static func phoneTeamMenu(owner: Bool) -> [MenuEntry] {
        (owner ? [MenuEntry(id: .new, children: [.board, .agent, .app])] : [])
            + [MenuEntry(id: .settings, children: [])]
    }

    public enum RowKind: Equatable, Sendable {
        case folder
        case item(ItemKind)

        public var rawValue: String {
            switch self {
            case .folder: "folder"
            case let .item(kind): kind.rawValue
            }
        }
    }

    public struct PhoneRow: Equatable, Sendable, Identifiable {
        public let kind: RowKind
        public let id: String
        /// A folder's name; nil on an item.
        public let name: String?
    }

    /// One page of the Teams tab (TS `phoneTeamPage`): a team's top level
    /// (`folderId` nil) or a folder's children; nil = the folder is gone.
    public static func phoneTeamPage(
        items: [TeamItem], entries: [SidebarFolders.Entry], folderId: String?
    ) -> [PhoneRow]? {
        let kinds = Dictionary(items.map { ($0.id, $0.kind) }, uniquingKeysWith: { _, b in b })
        func row(_ id: String) -> PhoneRow? {
            kinds[id].map { PhoneRow(kind: .item($0), id: id, name: nil) }
        }
        let tree = SidebarFolders.tree(items: items.map(\.id), entries: entries)
        guard let folderId else {
            return tree.compactMap { node in
                switch node {
                case let .folder(id, name, _): PhoneRow(kind: .folder, id: id, name: name)
                case let .item(id): row(id)
                }
            }
        }
        for node in tree {
            if case let .folder(id, _, children) = node, id == folderId { return children.compactMap(row) }
        }
        return nil
    }
}
