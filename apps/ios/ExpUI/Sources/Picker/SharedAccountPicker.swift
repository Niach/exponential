import ExpCore
import SwiftUI

// EXP-1029 contract — the account picker under the shared picker API.
//
// The EXP-991 picker (`AccountPicker.swift`: `AccountPickerMenu` +
// `AccountPickerTriggerLabel`, brand mark + login email per row, the EXP-992
// rate-limit preview `AccountLimitBars` under the picked login) MOVES onto
// `GlassPicker` in EXP-1021, keeping its options and its preview: the bars
// ride as each row's description. This file declares the typed view over
// the same `AccountOption`s; the body renders the trigger until then.
// (The file is not `AccountPicker.swift`: swiftc refuses two files of one
// name in a module, and that one still holds the menu.)

public struct AccountPicker<Trigger: View>: View {
    public let options: [AccountOption]
    /// The picked option's profile id, or nil while none is.
    public let value: String?
    public let onChange: (String) -> Void
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
        options: [AccountOption],
        value: String?,
        onChange: @escaping (String) -> Void,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.options = options
        self.value = value
        self.onChange = onChange
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ options: [AccountOption]) -> [PickerItem<String>] {
        options.map { option in
            PickerItem(value: option.id, label: option.email, keywords: [option.email, option.agent])
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(options),
            mode: .single,
            value: value.map { [$0] } ?? [],
            onChange: { picked in picked.first.map(onChange) },
            title: "Account",
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            trigger: trigger
        )
    }
}
