import SwiftUI

// EXP-1029 contract — the board picker: every row draws the board's ICON
// (`BoardIconDisplay`) and COLOUR, everywhere a board is picked (the
// composer, the create-issue sheet, move-to-board, the board switcher).
// EXP-1021 fills the rows over `GlassPicker`.

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

public struct BoardPicker<Trigger: View>: View {
    public let boards: [BoardPickerBoard]
    public let value: String?
    public let onChange: (String) -> Void
    public let search: Bool
    /// EXP-1021 — the surface controls every typed picker forwards verbatim
    /// (web's `PickerSurfaceProps`): a host that opens the picker from its own
    /// property row or `…` menu drives `open` and hides the trigger, and
    /// `onDismiss` fires once the sheet finished animating away (what a
    /// hand-off to a SECOND picker is promoted on).
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    public let onDismiss: (() -> Void)?
    private let trigger: () -> Trigger

    public init(
        boards: [BoardPickerBoard],
        value: String?,
        onChange: @escaping (String) -> Void,
        search: Bool = true,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.boards = boards
        self.value = value
        self.onChange = onChange
        self.search = search
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ boards: [BoardPickerBoard]) -> [PickerItem<String>] {
        boards.map { board in
            PickerItem(
                value: board.id,
                label: board.name,
                icon: board.icon,
                color: board.colorHex.flatMap { Color(hex: $0) }
            )
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(boards),
            mode: .single,
            value: value.map { [$0] } ?? [],
            onChange: { picked in picked.first.map(onChange) },
            search: search,
            emptyText: "No boards",
            title: "Board",
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            trigger: trigger
        )
    }
}
