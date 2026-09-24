import SwiftUI

// EXP-1029 contract — the priority picker: contract `issuePriority` in its
// order, each by the priority glyph in its tone. The app hands in its
// `IssuePriority` option rows; the picker never owns the table.

public struct PriorityPickerOption: Identifiable, Hashable {
    public var id: String { value }
    /// Contract `issuePriority`.
    public let value: String
    public let label: String
    /// An `AppIcons` name.
    public let icon: String?
    public let colorHex: String?

    public init(value: String, label: String, icon: String? = nil, colorHex: String? = nil) {
        self.value = value
        self.label = label
        self.icon = icon
        self.colorHex = colorHex
    }
}

public struct PriorityPicker<Trigger: View>: View {
    public let options: [PriorityPickerOption]
    public let mode: PickerMode
    public let value: Set<String>
    public let onChange: (Set<String>) -> Void
    private let trigger: () -> Trigger

    public init(
        options: [PriorityPickerOption],
        mode: PickerMode = .single,
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.options = options
        self.mode = mode
        self.value = value
        self.onChange = onChange
        self.trigger = trigger
    }

    public static func items(_ options: [PriorityPickerOption]) -> [PickerItem<String>] {
        options.map { option in
            PickerItem(
                value: option.value,
                label: option.label,
                icon: option.icon,
                color: option.colorHex.flatMap { Color(hex: $0) }
            )
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(options),
            mode: mode,
            value: value,
            onChange: onChange,
            title: "Priority",
            trigger: trigger
        )
    }
}
