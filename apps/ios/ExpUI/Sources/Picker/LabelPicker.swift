import SwiftUI

// EXP-1029 contract — the label picker: ALWAYS multi, searchable, each row
// its colour dot + name; the sheet stays open across toggles.

public struct LabelPickerLabel: Identifiable, Hashable {
    public let id: String
    public let name: String
    /// The label's hex.
    public let colorHex: String?

    public init(id: String, name: String, colorHex: String? = nil) {
        self.id = id
        self.name = name
        self.colorHex = colorHex
    }
}

public struct LabelPicker<Trigger: View>: View {
    public let labels: [LabelPickerLabel]
    public let value: Set<String>
    public let onChange: (Set<String>) -> Void
    private let trigger: () -> Trigger

    public init(
        labels: [LabelPickerLabel],
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.labels = labels
        self.value = value
        self.onChange = onChange
        self.trigger = trigger
    }

    nonisolated public static func items(_ labels: [LabelPickerLabel]) -> [PickerItem<String>] {
        labels.map { label in
            PickerItem(value: label.id, label: label.name, color: label.colorHex.flatMap { Color(hex: $0) })
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(labels),
            mode: .multi,
            value: value,
            onChange: onChange,
            search: true,
            emptyText: "No labels",
            title: "Labels",
            trigger: trigger
        )
    }
}
