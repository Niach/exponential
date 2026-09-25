import SwiftUI

// EXP-1029 contract — the action picker: the team's actions (and the two
// listed builtins) by curated icon (`ActionIconDisplay`) + name. The
// composer's action chip and the automation editor pick one.

public struct ActionPickerAction: Identifiable, Hashable {
    public let id: String
    public let name: String
    /// Contract `boardIcon` (the curated action set); nil = the default.
    public let icon: String?
    public let description: String?

    public init(id: String, name: String, icon: String? = nil, description: String? = nil) {
        self.id = id
        self.name = name
        self.icon = icon
        self.description = description
    }
}

public struct ActionPicker<Trigger: View>: View {
    public let actions: [ActionPickerAction]
    public let value: String?
    public let onChange: (String) -> Void
    /// The sheet headline; the default names the picker (Android's
    /// `ActionPicker.kt` carries the same parameter).
    public let title: String
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
        actions: [ActionPickerAction],
        value: String?,
        onChange: @escaping (String) -> Void,
        title: String = "Action",
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.actions = actions
        self.value = value
        self.onChange = onChange
        self.title = title
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ actions: [ActionPickerAction]) -> [PickerItem<String>] {
        actions.map { action in
            PickerItem(value: action.id, label: action.name, icon: action.icon, description: action.description)
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(actions),
            mode: .single,
            value: value.map { [$0] } ?? [],
            onChange: { picked in picked.first.map(onChange) },
            search: true,
            emptyText: "No actions",
            title: title,
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            trigger: trigger
        )
    }
}
