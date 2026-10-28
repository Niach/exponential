import Foundation

// S-3 (owner 2026-10-28): THE settings nav ×4 = `settings-nav.json` `nav`
// (groups, injected `@lists`, `children`, `pages`, per-platform pages),
// a 1:1 port of the TS reference `settingsNavGroups` /
// `settingsNavItemVisible` / `settingsNavPage`, locked by `navCases`
// (`SettingsNavTests`). The `rows` half stays generated
// (`SettingsNav.generated.swift`).
public extension SettingsNav {
    enum Platform: String, Sendable, CaseIterable {
        case web
        case webMobile = "web-mobile"
        case desktop
        case ios
        case android
    }

    /// A page's label + icon + visibility (a `rows` row or a `pages` page).
    struct Page: Equatable, Sendable {
        public let label: String
        public let icon: String
        public let visible: Visibility
        /// nil = every platform.
        public let platforms: [Platform]?
    }

    struct Group: Equatable, Sendable, Identifiable {
        /// team | boards | agents | apps | device | personal.
        public let id: String
        public let label: String
        /// nil = every platform.
        public let platforms: [Platform]?
        /// Page ids and injected `@lists` in order.
        public let items: [String]
    }

    /// The injected lists (`@…` items): the client renders them in place.
    enum List {
        public static let boards = "@boards"
        public static let newBoard = "@new-board"
        /// The viewer's OWN Assistant (brand mark, opens the Assistant page).
        public static let assistant = "@assistant"
        /// The team agents (role agent).
        public static let agents = "@agents"
        public static let newAgent = "@new-agent"
        public static let apps = "@apps"
        public static let newApp = "@new-app"
    }

    static let groups: [Group] = [
        Group(id: "team", label: "Team", platforms: nil, items: [
            "general", "members", "issues", "repositories", "prompts", "widget", "storage", "billing", "import",
        ]),
        Group(id: "boards", label: "Boards", platforms: nil, items: [List.boards, List.newBoard, "archived-boards"]),
        Group(id: "agents", label: "Agents", platforms: nil, items: [
            List.assistant, List.agents, List.newAgent, "mcp-servers",
        ]),
        Group(id: "apps", label: "Apps", platforms: nil, items: [List.apps, List.newApp]),
        Group(id: "device", label: "This device", platforms: [.desktop], items: [
            "tools", "coding-clis", "worktrees", "sessions",
        ]),
        Group(id: "personal", label: "Personal", platforms: nil, items: ["account", "notifications", "security", "about"]),
    ]

    /// A page's sub-pages (Issues › Labels, Statuses).
    static let children: [String: [String]] = ["issues": ["labels", "statuses"]]

    static let pages: [String: Page] = [
        "storage": Page(label: "Storage", icon: "settings-storage", visible: .owner, platforms: nil),
        "import": Page(label: "Import", icon: "settings-import", visible: .owner, platforms: [.web]),
        "archived-boards": Page(label: "Archived boards", icon: "ui-archive", visible: .owner, platforms: nil),
        "tools": Page(label: "Tools", icon: "settings-tools", visible: .all, platforms: [.desktop]),
        "coding-clis": Page(label: "Coding CLIs", icon: "settings-agents", visible: .all, platforms: [.desktop]),
        "worktrees": Page(label: "Worktrees", icon: "settings-local-repos", visible: .all, platforms: [.desktop]),
        "sessions": Page(label: "Sessions", icon: "settings-sessions", visible: .all, platforms: [.desktop]),
        "account": Page(label: "Account", icon: "settings-account", visible: .all, platforms: nil),
        "notifications": Page(label: "Notifications", icon: "settings-notifications", visible: .all, platforms: nil),
        "security": Page(label: "Security", icon: "settings-api", visible: .all, platforms: nil),
        "about": Page(label: "About", icon: "settings-about", visible: .all, platforms: [.desktop, .ios, .android]),
    ]

    /// `rows` rows that exist on some platforms only.
    static let rowPlatforms: [String: [Platform]] = [
        "billing": [.web],
        "devices": [.ios, .android, .webMobile],
    ]

    /// The `@new-…` entries' visibility (an `@list` not named = everyone).
    static let newEntries: [String: Visibility] = [
        List.newBoard: .owner,
        List.newAgent: .owner,
        List.newApp: .owner,
    ]

    static func group(_ id: String) -> Group? { groups.first { $0.id == id } }

    /// The label + icon of a page id (a `rows` row or a `pages` page).
    static func page(_ id: String) -> Page? {
        if let row = rows.first(where: { $0.id == id }) {
            return Page(label: row.label, icon: row.icon, visible: row.visible, platforms: rowPlatforms[id])
        }
        return pages[id]
    }

    private static func visible(_ visibility: Visibility, isOwner: Bool, cloud: Bool) -> Bool {
        switch visibility {
        case .all: true
        case .owner: isOwner
        case .ownerCloud: isOwner && cloud
        }
    }

    /// One item (page id or `@list`) for a viewer on a platform.
    static func itemVisible(_ item: String, platform: Platform, isOwner: Bool, cloud: Bool = false) -> Bool {
        if item.hasPrefix("@") {
            guard let visibility = newEntries[item] else { return true }
            return visible(visibility, isOwner: isOwner, cloud: cloud)
        }
        guard let page = page(item) else { return false }
        if let platforms = page.platforms, !platforms.contains(platform) { return false }
        return visible(page.visible, isOwner: isOwner, cloud: cloud)
    }

    /// The nav's groups for a viewer on a platform, in order, each with its
    /// visible items (an empty group is dropped).
    static func visibleGroups(platform: Platform, isOwner: Bool, cloud: Bool = false) -> [Group] {
        groups.compactMap { group in
            if let platforms = group.platforms, !platforms.contains(platform) { return nil }
            let items = group.items.filter { itemVisible($0, platform: platform, isOwner: isOwner, cloud: cloud) }
            return items.isEmpty ? nil : Group(id: group.id, label: group.label, platforms: group.platforms, items: items)
        }
    }
}
