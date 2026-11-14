import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1832: the read-only folders' capture specimens (`-uiTesting
/// -uiTestingStyleguide sidebar-folders-picker|sidebar-folders-apps`): the
/// REAL board picker sheet and the REAL Apps rows over a fixed layout (one
/// folder open, one folded), as sidebar-folders.json `rows` lays them out.
struct SidebarFoldersSpecimen: View {
    static let kinds: Set<String> = ["sidebar-folders-picker", "sidebar-folders-apps"]
    private static let teamId = "specimen-team"

    let kind: String
    @State private var open = true

    init(kind: String) {
        self.kind = kind
        // The specimen always starts from its fixed folds.
        for id in ["f-apps", "f-mon", "f-ops", "f-personal"] {
            SidebarFolders.setOpen(!Self.folded.contains(id), teamId: Self.teamId, folderId: id)
        }
    }

    @State private var flipped: [String: Bool] = [:]

    private static func entry(_ id: String, _ kind: SidebarFolders.Kind, target: String? = nil, parent: String? = nil, name: String? = nil, _ order: Double) -> SidebarFolders.Entry {
        SidebarFolders.Entry(id: id, kind: kind, targetId: target, parentId: parent, name: name, sortOrder: order)
    }

    // MARK: Board picker

    private static let boards = [
        BoardPickerBoard(id: "exponential", name: "Exponential", icon: "rocket", colorHex: "#22c55e"),
        BoardPickerBoard(id: "feedback", name: "Feedback", icon: "mail", colorHex: "#f97316"),
        BoardPickerBoard(id: "exp-apps", name: "Exponential apps", icon: "sparkles", colorHex: "#a855f7"),
        BoardPickerBoard(id: "potential", name: "Potential Apps", icon: "pen-tool", colorHex: "#6366f1"),
        BoardPickerBoard(id: "errors", name: "Errors", icon: "bug", colorHex: "#ef4444"),
        BoardPickerBoard(id: "performance", name: "Performance", icon: "activity", colorHex: "#f59e0b"),
        BoardPickerBoard(id: "next", name: "Next", icon: "square-kanban", colorHex: "#6366f1"),
    ]

    private static let boardEntries = [
        entry("e-exp", .item, target: "exponential", 1),
        entry("e-feed", .item, target: "feedback", 2),
        entry("f-apps", .folder, name: "Apps", 3),
        entry("e-apps", .item, target: "exp-apps", parent: "f-apps", 1),
        entry("e-pot", .item, target: "potential", parent: "f-apps", 2),
        entry("f-mon", .folder, name: "Monitoring", 4),
        entry("e-err", .item, target: "errors", parent: "f-mon", 1),
        entry("e-perf", .item, target: "performance", parent: "f-mon", 2),
        entry("e-next", .item, target: "next", 5),
    ]

    // MARK: Apps

    private static func app(_ id: String, _ name: String, icon: String, color: String, version: Int) -> AppListEntry {
        AppListEntry(row: AppEntity(
            id: id, teamId: teamId, slug: id, name: name, icon: icon, color: color,
            headVersion: version, publishedVersion: version, createdBy: "specimen",
            createdAt: "", updatedAt: ""
        ))
    }

    private static let apps = [
        app("crm", "CRM", icon: "gem", color: "#f59e0b", version: 3),
        app("invoices", "Invoices", icon: "credit-card", color: "#22c55e", version: 2),
        app("notes", "Notes", icon: "file-text", color: "#a1a1aa", version: 1),
        app("standup", "Standup", icon: "users", color: "#06b6d4", version: 4),
        app("timesheet", "Timesheet", icon: "clock", color: "#6366f1", version: 1),
        app("todos", "Todos", icon: "flag", color: "#ef4444", version: 2),
    ]

    private static let appEntries = [
        entry("f-ops", .folder, name: "Ops tools", 1),
        entry("e-crm", .item, target: "crm", parent: "f-ops", 1),
        entry("e-standup", .item, target: "standup", parent: "f-ops", 2),
        entry("e-inv", .item, target: "invoices", 2),
        entry("f-personal", .folder, name: "Personal", 3),
        entry("e-notes", .item, target: "notes", parent: "f-personal", 1),
        entry("e-time", .item, target: "timesheet", parent: "f-personal", 2),
        entry("e-todos", .item, target: "todos", parent: "f-personal", 3),
    ]

    /// The specimen's folded folder (the rest stay open).
    private static let folded: Set<String> = ["f-mon", "f-personal"]

    var body: some View {
        ZStack {
            AppBackground()
            if kind == "sidebar-folders-apps" {
                appsList
            } else {
                BoardPicker(
                    boards: Self.boards,
                    value: "exponential",
                    onChange: { _ in },
                    title: "Move to board",
                    open: $open,
                    hideTrigger: true,
                    folders: BoardPickerFolders(teamId: Self.teamId, entries: Self.boardEntries),
                    trigger: { EmptyView() }
                )
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-cards-screen")
    }

    private func isOpen(_ id: String) -> Bool { flipped[id] ?? !Self.folded.contains(id) }

    private var appsList: some View {
        let byId = Dictionary(uniqueKeysWithValues: Self.apps.map { ($0.id, $0) })
        let rows = SidebarFolders.rows(
            SidebarFolders.tree(items: Self.apps.map(\.id), entries: Self.appEntries),
            isOpen: isOpen
        )
        return ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(Apps.Copy.title) {
                    GlassPill(Apps.Copy.newApp, icon: AppIcons.uiAdd, mode: .action {})
                }
                ForEach(rows) { row in
                    if row.group {
                        AppsFolderRow(folder: row) { flipped[row.id] = !isOpen(row.id) }
                    } else if let app = byId[row.id] {
                        AppsListRow(app: app, depth: row.depth, showsMenu: true, onRun: {}, onMenu: {})
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
        .scrollContentBackground(.hidden)
    }
}
