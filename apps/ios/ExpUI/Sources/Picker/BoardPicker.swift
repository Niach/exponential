import ExpCore
import SwiftUI

// EXP-1029 contract — the board picker: every row draws the board's ICON
// (`BoardIconDisplay`) and COLOUR, everywhere a board is PICKED — the
// composer, the create-issue sheet, move-to-board, a trigger's board
// filter. Not the board SWITCHER: navigating to a board is a nav sheet by
// EXP-698 design, not a value being picked. EXP-1021 fills the rows over
// `GlassPicker`.

public struct BoardPickerBoard: Identifiable, Hashable {
    public let id: String
    public let name: String
    /// Contract `boardIcon`; nil = the default glyph.
    public let icon: String?
    /// The board's hex colour; nil = the foreground.
    public let colorHex: String?

    public init(id: String, name: String, icon: String? = nil, colorHex: String? = nil) {
        self.id = id
        self.name = name
        self.icon = icon
        self.colorHex = colorHex
    }
}

/// #EXP-1832 (sidebar-folders.json): the member's board folders, read-only
/// on phones: the team (the device-local open flags are keyed by it) and
/// the synced `sidebar_layout` entries of section `board`.
public struct BoardPickerFolders: Equatable {
    public let teamId: String
    public let entries: [SidebarFolders.Entry]

    public init(teamId: String, entries: [SidebarFolders.Entry]) {
        self.teamId = teamId
        self.entries = entries
    }
}

public struct BoardPicker<Trigger: View>: View {
    /// EXP-1030 — the row that clears the pick; it reports the empty string,
    /// the unset sentinel every board-valued field already stores.
    nonisolated public static var noneValue: String { "" }

    public let boards: [BoardPickerBoard]
    public let value: String?
    public let onChange: (String) -> Void
    public let search: Bool
    /// EXP-1030 — offer a leading "None" row, for an OPTIONAL board (an
    /// action's optional `board` input): without it a pick can never be
    /// taken back.
    public let allowsNone: Bool
    /// The sheet headline; the default names the picker. A flow that means
    /// something more than "pick a board" — "Move to board", "Escalate to
    /// issue" — says so here, exactly as on Android (`BoardPicker.kt`).
    public let title: String
    /// EXP-1021 — the surface controls every typed picker forwards verbatim
    /// (web's `PickerSurfaceProps`): a host that opens the picker from its own
    /// property row or `…` menu drives `open` and hides the trigger, and
    /// `onDismiss` fires once the sheet finished animating away (what a
    /// hand-off to a SECOND picker is promoted on).
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    public let onDismiss: (() -> Void)?
    /// #EXP-1832: the folders the rows fold into; nil = flat.
    public let folders: BoardPickerFolders?
    private let trigger: () -> Trigger
    /// #EXP-1832: folds flipped while the sheet is up (also written to
    /// UserDefaults, where the next open reads them).
    @State private var flipped: [String: Bool] = [:]

    public init(
        boards: [BoardPickerBoard],
        value: String?,
        onChange: @escaping (String) -> Void,
        search: Bool = true,
        allowsNone: Bool = false,
        title: String = "Board",
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        folders: BoardPickerFolders? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.folders = folders
        self.boards = boards
        self.value = value
        self.onChange = onChange
        self.search = search
        self.allowsNone = allowsNone
        self.title = title
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(
        _ boards: [BoardPickerBoard], allowsNone: Bool = false
    ) -> [PickerItem<String>] {
        let rows = boards.map { board in
            PickerItem(
                value: board.id,
                label: board.name,
                icon: board.icon,
                color: board.colorHex.flatMap { Color(hex: $0) }
            )
        }
        return allowsNone ? [PickerItem(value: noneValue, label: "None")] + rows : rows
    }

    /// #EXP-1832: the rows folded by the member's folders
    /// (`SidebarFolders.tree` + `rows`, every folder expanded so a search
    /// still finds a folded board; `PickerFolders.visible` folds them).
    nonisolated public static func items(
        _ boards: [BoardPickerBoard],
        allowsNone: Bool = false,
        folders: BoardPickerFolders?,
        isOpen: (String) -> Bool
    ) -> [PickerItem<String>] {
        guard let folders else { return items(boards, allowsNone: allowsNone) }
        let byId = Dictionary(boards.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let tree = SidebarFolders.tree(items: boards.map(\.id), entries: folders.entries)
        var parent: String?
        let rows = SidebarFolders.rows(tree) { _ in true }.compactMap { row -> PickerItem<String>? in
            if row.group {
                parent = row.id
                return PickerItem(
                    value: row.id,
                    label: row.name ?? SidebarFolders.defaultFolderName,
                    icon: AppIcons.uiFolder,
                    group: PickerGroup(count: row.count, open: isOpen(row.id))
                )
            }
            guard let board = byId[row.id] else { return nil }
            return PickerItem(
                value: board.id,
                label: board.name,
                icon: board.icon,
                color: board.colorHex.flatMap { Color(hex: $0) },
                depth: row.depth,
                parent: row.depth > 0 ? parent : nil
            )
        }
        return allowsNone ? [PickerItem(value: noneValue, label: "None")] + rows : rows
    }

    private func isOpen(_ folderId: String) -> Bool {
        guard let folders else { return true }
        return flipped[folderId] ?? SidebarFolders.isOpen(teamId: folders.teamId, folderId: folderId)
    }

    private func toggle(_ folderId: String) {
        guard let folders else { return }
        let open = !isOpen(folderId)
        flipped[folderId] = open
        SidebarFolders.setOpen(open, teamId: folders.teamId, folderId: folderId)
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(boards, allowsNone: allowsNone, folders: folders, isOpen: isOpen),
            mode: .single,
            value: value.map { [$0] } ?? [],
            onChange: { picked in picked.first.map(onChange) },
            search: search,
            emptyText: "No boards",
            title: title,
            // The placeholder keeps saying "boards" whatever the header says.
            searchPlaceholder: "Search boards",
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            onToggleGroup: toggle,
            trigger: trigger
        )
    }
}
