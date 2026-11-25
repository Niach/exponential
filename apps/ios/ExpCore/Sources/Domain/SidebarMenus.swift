import Foundation

/// #EXP-2124 / #EXP-2109: the sidebar rows' menus, the Swift mirror of
/// packages/domain-contract/src/sidebar-menus.ts (fixture
/// `fixtures/sidebar-menus.json`, its `$comment` is the spec): the phone's
/// long-press menus. `SidebarMenusTests` replays the table.
public enum SidebarMenus {
    public enum Action: String, Sendable, CaseIterable {
        case open = "open"
        case openNewTab = "open-new-tab"
        case newChat = "new-chat"
        case new = "new"
        case newFolder = "new-folder"
        case newIssue = "new-issue"
        case newTeam = "new-team"
        case joinTeam = "join-team"
        case addDevice = "add-device"
        case compact = "compact"
        case newConversation = "new-conversation"
        case edit = "edit"
        case rename = "rename"
        case copyLink = "copy-link"
        case move = "move"
        case ungroup = "ungroup"
        case addToSidebar = "add-to-sidebar"
        case removeFromSidebar = "remove-from-sidebar"
        case settings = "settings"
        case archive = "archive"
        case delete = "delete"
        case separator
    }

    public enum Kind: String, Sendable, CaseIterable {
        case assistant, home
        case needsYou = "needs-you"
        case chats, drafts, devices, device, teams, team, folder, board, app, agent, session
    }

    public enum NewKind: String, Sendable, CaseIterable { case session, agent, app, board }

    public static let copy: [Action: String] = [
        .open: "Open",
        .openNewTab: "Open in new tab",
        .newChat: "New chat",
        .new: "New",
        .newFolder: "New folder",
        .newIssue: "New issue",
        .newTeam: "New team",
        .joinTeam: "Join team",
        .addDevice: "Add device",
        .compact: "Compact conversation",
        .newConversation: "New conversation",
        .edit: "Edit",
        .rename: "Rename",
        .copyLink: "Copy link",
        .move: "Move to folder",
        .ungroup: "Ungroup",
        .addToSidebar: "Add to sidebar",
        .removeFromSidebar: "Remove from sidebar",
        .settings: "Settings",
        .archive: "Archive",
        .delete: "Delete",
    ]
    public static let topLevel = "Top level"
    public static let linkCopied = "Link copied"

    /// The icon CONCEPT of a verb.
    public static let icons: [Action: String] = [
        .open: "ui-arrow-right",
        .openNewTab: "ui-external-link",
        .newChat: "action-chat",
        .new: "ui-add",
        .newFolder: "ui-folder",
        .newIssue: "nav-create-issue",
        .newTeam: "ui-add",
        .joinTeam: "ui-invite",
        .addDevice: "ui-add",
        .compact: "coding-compact",
        .newConversation: "action-chat",
        .edit: "ui-edit",
        .rename: "ui-edit",
        .copyLink: "ui-link",
        .move: "ui-folder",
        .ungroup: "ui-minus",
        .addToSidebar: "ui-add",
        .removeFromSidebar: "ui-minus",
        .settings: "nav-settings",
        .archive: "ui-archive",
        .delete: "ui-delete",
    ]
    public static let newCopy: [NewKind: String] = [
        .session: "Chat",
        .agent: "Agent",
        .app: "App",
        .board: "Board",
    ]
    public static let newIcons: [NewKind: String] = [
        .session: "action-chat",
        .agent: "settings-agents",
        .app: "nav-apps",
        .board: "nav-boards",
    ]
    private static let newOwnerOnly: Set<NewKind> = [.agent, .app, .board]
    /// `phone.drops`: a phone has no tabs.
    private static let phoneDrops: Set<Action> = [.openNewTab]

    /// What decides a kind's conditional rows (TS `SidebarMenuInput`).
    public struct Input: Sendable, Equatable {
        public var owner: Bool
        public var mine: Bool

        public init(owner: Bool = false, mine: Bool = false) {
            self.owner = owner
            self.mine = mine
        }
    }

    private struct Entry {
        let action: Action
        var owner = false
        var mine = false

        func shows(_ input: Input) -> Bool { (!owner || input.owner) && (!mine || input.mine) }
    }

    /// The groups in order: open, kind, arrange, settings, danger.
    private static let table: [Kind: [[Entry]]] = [
        .assistant: [
            [.init(action: .open), .init(action: .openNewTab)],
            [.init(action: .compact), .init(action: .newConversation)],
            [],
            [.init(action: .settings)],
            [],
        ],
        .home: [
            [.init(action: .open), .init(action: .openNewTab)],
            [.init(action: .newChat)],
            [],
            [],
            [],
        ],
        .needsYou: [
            [.init(action: .open), .init(action: .openNewTab)],
            [],
            [],
            [],
            [],
        ],
        .chats: [
            [.init(action: .open), .init(action: .openNewTab)],
            [.init(action: .newChat)],
            [],
            [],
            [],
        ],
        .drafts: [
            [.init(action: .open), .init(action: .openNewTab)],
            [],
            [],
            [],
            [],
        ],
        .devices: [
            [],
            [.init(action: .addDevice)],
            [],
            [],
            [],
        ],
        .device: [
            [.init(action: .open), .init(action: .openNewTab)],
            [],
            [.init(action: .rename, mine: true), .init(action: .copyLink)],
            [.init(action: .settings)],
            [],
        ],
        .teams: [
            [],
            [.init(action: .newTeam), .init(action: .joinTeam)],
            [],
            [],
            [],
        ],
        .team: [
            [],
            [.init(action: .new), .init(action: .newFolder)],
            [],
            [.init(action: .settings)],
            [],
        ],
        .folder: [
            [],
            [],
            [.init(action: .rename), .init(action: .ungroup)],
            [],
            [],
        ],
        .board: [
            [.init(action: .open), .init(action: .openNewTab)],
            [.init(action: .newIssue)],
            [.init(action: .copyLink), .init(action: .move)],
            [.init(action: .settings, owner: true)],
            [.init(action: .archive, owner: true)],
        ],
        .app: [
            [.init(action: .open), .init(action: .openNewTab)],
            [.init(action: .edit)],
            [.init(action: .copyLink), .init(action: .move)],
            [.init(action: .settings)],
            [],
        ],
        .agent: [
            [.init(action: .open), .init(action: .openNewTab)],
            [.init(action: .compact), .init(action: .newConversation)],
            [.init(action: .move)],
            [.init(action: .settings, owner: true)],
            [],
        ],
        .session: [
            [.init(action: .open), .init(action: .openNewTab)],
            [],
            [.init(action: .copyLink), .init(action: .move), .init(action: .removeFromSidebar)],
            [],
            [],
        ],
    ]

    public static func destructive(_ kind: Kind, _ action: Action) -> Bool {
        table[kind]?.last?.contains { $0.action == action } ?? false
    }

    /// TS `sidebarItemMenu`.
    public static func menu(_ kind: Kind, _ input: Input = Input()) -> [Action] {
        groups(kind, input, drops: [])
    }

    /// A row's long-press menu (TS `sidebarPhoneMenu`); Open alone = none.
    /// `drops` = the verbs the row has no destination for.
    public static func phoneMenu(_ kind: Kind, _ input: Input = Input(), drops: Set<Action> = []) -> [Action] {
        let menu = groups(kind, input, drops: phoneDrops.union(drops))
        return menu == [.open] ? [] : menu
    }

    /// A device's row in the phones' Devices list: there is no device page to
    /// open; a teammate's has only its link.
    public static func deviceMenu(mine: Bool) -> [Action] {
        phoneMenu(.device, Input(mine: mine), drops: mine ? [.open] : [.open, .settings])
    }

    private static func groups(_ kind: Kind, _ input: Input, drops: Set<Action>) -> [Action] {
        var out: [Action] = []
        for group in table[kind] ?? [] {
            let rows = group.filter { $0.shows(input) && !drops.contains($0.action) }.map(\.action)
            guard !rows.isEmpty else { continue }
            if !out.isEmpty { out.append(.separator) }
            out += rows
        }
        return out
    }

    /// THE New list (TS `sidebarNewKinds`).
    public static func newKinds(owner: Bool) -> [NewKind] {
        NewKind.allCases.filter { owner || !newOwnerOnly.contains($0) }
    }

    public struct MoveTargets: Equatable, Sendable {
        public let folders: [Folder]
        /// Inside a folder: that folder (Top level = after it).
        public let topLevel: String?

        public struct Folder: Equatable, Sendable {
            public let id: String
            public let name: String?
        }
    }

    /// Move to folder › (TS `sidebarMoveTargets`) over a team's rows.
    public static func moveTargets(_ rows: [SidebarFolders.Row], id: String) -> MoveTargets {
        var topLevel: String?
        var holder: String?
        for row in rows {
            if row.kind == .folder { holder = row.id } else if row.depth == 0 { holder = nil }
            if row.id == id { topLevel = row.kind == .folder ? nil : holder }
        }
        return MoveTargets(
            folders: rows.filter { $0.kind == .folder && $0.id != topLevel }.map { .init(id: $0.id, name: $0.name) },
            topLevel: topLevel
        )
    }
}
