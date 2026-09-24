import SwiftUI

// EXP-1029 contract — the status picker: the team's statuses (EXP-314) in
// display order, each by its glyph in its colour (`ResolvedStatus`, the
// status-icons rule). The issue header, the create-issue sheet and the bulk
// edit pick one.

public struct StatusPickerStatus: Identifiable, Hashable {
    public let id: String
    public let name: String
    /// Contract `issueStatusCategory`.
    public let category: String
    /// The resolved row colour (custom rows' hex, builtins' token).
    public let colorHex: String?
    /// The resolved glyph for the row (an `AppIcons` name).
    public let icon: String?
    /// EXP-1021: the resolved colour when it is a design TOKEN rather than a
    /// hex. A builtin row keeps the platform's status token — theme-aware,
    /// and no hex names it (EXP-314) — so it wins over `colorHex` when set.
    public let color: Color?

    public init(
        id: String,
        name: String,
        category: String,
        colorHex: String? = nil,
        icon: String? = nil,
        color: Color? = nil
    ) {
        self.id = id
        self.name = name
        self.category = category
        self.colorHex = colorHex
        self.icon = icon
        self.color = color
    }
}

public struct StatusPicker<Trigger: View>: View {
    public let statuses: [StatusPickerStatus]
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
        statuses: [StatusPickerStatus],
        mode: PickerMode = .single,
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.statuses = statuses
        self.mode = mode
        self.value = value
        self.onChange = onChange
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ statuses: [StatusPickerStatus]) -> [PickerItem<String>] {
        statuses.map { status in
            PickerItem(
                value: status.id,
                label: status.name,
                icon: status.icon,
                color: status.color ?? status.colorHex.flatMap { Color(hex: $0) },
                keywords: [status.name, status.category]
            )
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(statuses),
            mode: mode,
            value: value,
            onChange: onChange,
            emptyText: "No statuses",
            title: "Status",
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            trigger: trigger
        )
    }
}
