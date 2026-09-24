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
    private let trigger: () -> Trigger

    public init(
        boards: [BoardPickerBoard],
        value: String?,
        onChange: @escaping (String) -> Void,
        search: Bool = true,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.boards = boards
        self.value = value
        self.onChange = onChange
        self.search = search
        self.trigger = trigger
    }

    public static func items(_ boards: [BoardPickerBoard]) -> [PickerItem<String>] {
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
            trigger: trigger
        )
    }
}
