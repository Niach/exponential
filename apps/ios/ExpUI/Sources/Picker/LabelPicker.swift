import SwiftUI

// EXP-1029 contract — the label picker: ALWAYS multi, searchable, each row
// its colour dot + name; the sheet stays open across toggles.

public struct LabelPickerLabel: Identifiable, Hashable {
    public let id: String
    public let name: String
    /// The label's hex.
    public let colorHex: String?
    /// A BULK edit's third state: this label sits on all of the selected
    /// issues, on only some of them, or on none. Absent = plain membership in
    /// the picker's `value` (one issue's labels).
    public let checked: PickerChecked?

    public init(
        id: String, name: String, colorHex: String? = nil, checked: PickerChecked? = nil
    ) {
        self.id = id
        self.name = name
        self.colorHex = colorHex
        self.checked = checked
    }
}

public struct LabelPicker<Trigger: View>: View {
    public let labels: [LabelPickerLabel]
    public let value: Set<String>
    public let onChange: (Set<String>) -> Void
    /// EXP-1021 — controlled filter text and a row under the list: what the
    /// labels sheet's "Create new label …" row needs, since only the host
    /// knows whether the typed name already exists. With `query` the host
    /// filters and the primitive renders `labels` verbatim.
    public let query: Binding<String>?
    public let footer: (() -> AnyView)?
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
        labels: [LabelPickerLabel],
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        query: Binding<String>? = nil,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        footer: (() -> AnyView)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.labels = labels
        self.value = value
        self.onChange = onChange
        self.query = query
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.footer = footer
        self.trigger = trigger
    }

    nonisolated public static func items(_ labels: [LabelPickerLabel]) -> [PickerItem<String>] {
        labels.map { label in
            PickerItem(
                value: label.id,
                label: label.name,
                color: label.colorHex.flatMap { Color(hex: $0) },
                checked: label.checked
            )
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
            query: query,
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            footer: footer,
            trigger: trigger
        )
    }
}
