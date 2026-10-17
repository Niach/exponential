import ExpCore
import SwiftUI

// EXP-1345 contract (composer-menu.json, picker rule ×4) — the model picker:
// a single-select pick is THE Picker, never an anchored menu. Phones open a
// bottom sheet of the agent's models (`LaunchVocabulary.modelValues`), each
// by its label, the picked row wearing the TRAILING check. The launch
// composer's options line and the session composer's model pill pick here.

public struct ModelPicker<Trigger: View>: View {
    /// The agent whose models are on offer (`claude`, `codex`, …).
    public let agent: String
    public let value: String?
    public let onChange: (String) -> Void
    public let title: String
    public let disabled: Bool
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    public let onDismiss: (() -> Void)?
    private let trigger: () -> Trigger

    public init(
        agent: String,
        value: String?,
        onChange: @escaping (String) -> Void,
        title: String = "Model",
        disabled: Bool = false,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.agent = agent
        self.value = value
        self.onChange = onChange
        self.title = title
        self.disabled = disabled
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    /// One row per model the agent offers, in contract order, by its label.
    nonisolated public static func items(agent: String) -> [PickerItem<String>] {
        LaunchVocabulary.modelValues(for: agent).map { value in
            PickerItem(value: value, label: LaunchVocabulary.modelLabel(value))
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(agent: agent),
            mode: .single,
            value: value.map { [$0] } ?? [],
            onChange: { picked in picked.first.map(onChange) },
            emptyText: "No models",
            title: title,
            disabled: disabled,
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            trigger: trigger
        )
    }
}
