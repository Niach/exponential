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
    /// EXP-1021: the priority TONE as a token — the priority palette is a
    /// design token, not a synced hex, so it wins over `colorHex` when set.
    public let color: Color?

    public init(
        value: String,
        label: String,
        icon: String? = nil,
        colorHex: String? = nil,
        color: Color? = nil
    ) {
        self.value = value
        self.label = label
        self.icon = icon
        self.colorHex = colorHex
        self.color = color
    }
}

public struct PriorityPicker<Trigger: View>: View {
    public let options: [PriorityPickerOption]
    public let mode: PickerMode
    public let value: Set<String>
    public let onChange: (Set<String>) -> Void
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
        options: [PriorityPickerOption],
        mode: PickerMode = .single,
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.options = options
        self.mode = mode
        self.value = value
        self.onChange = onChange
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ options: [PriorityPickerOption]) -> [PickerItem<String>] {
        options.map { option in
            PickerItem(
                value: option.value,
                label: option.label,
                icon: option.icon,
                color: option.color ?? option.colorHex.flatMap { Color(hex: $0) }
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
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            trigger: trigger
        )
    }
}
