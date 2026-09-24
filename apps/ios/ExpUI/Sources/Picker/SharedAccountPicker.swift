import ExpCore
import SwiftUI

// EXP-1029 contract — the account picker under the shared picker API.
//
// The EXP-991 picker (`AccountPicker.swift`: `AccountPickerMenu` +
// `AccountPickerTriggerLabel`, brand mark + login email per row, the EXP-992
// rate-limit preview `AccountLimitBars` under the picked login) MOVED onto
// `GlassPicker` here (EXP-1021/EXP-1030), keeping its options and its
// preview: the agent's brand mark is the row's leading MARK, a dead
// credential's badge its description, and the limit bars its DETAIL — a
// drawing, so they ride `renderDetail` rather than a string. `GlassPicker`
// owns the selection language, so the row's check is gone with the menu.
// (The file is not `AccountPicker.swift`: swiftc refuses two files of one
// name in a module, and that one still holds the trigger label and the
// `AccountPickerMenu` shim every launch surface names.)

public struct AccountPicker<Trigger: View>: View {
    public let options: [AccountOption]
    /// The picked option's `AccountOption.key` (`<agent>:<profileId>`), or
    /// nil while none is. EXP-1030: the KEY, never the profile id alone — a
    /// machine that reports an ambient `system` login for two agents has the
    /// same profile id twice, and a picker cannot key two rows the same.
    public let value: String?
    /// Reports the picked option's `key`.
    public let onChange: (String) -> Void
    /// The agent's brand mark, resolved by the CALLER (`AgentBrandMark` lives
    /// in the app target) so an id with no asset falls back exactly as it
    /// does everywhere else.
    public let mark: ((String) -> Image?)?
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
        mark: ((String) -> Image?)? = nil,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.options = options
        self.value = value
        self.onChange = onChange
        self.mark = mark
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ options: [AccountOption]) -> [PickerItem<String>] {
        options.map { option in
            PickerItem(
                value: option.key,
                label: option.email,
                // EXP-849: a login the agent REFUSED stays on offer wearing
                // its badge — muted under the email, as the menu drew it.
                description: option.health.badgeLabel,
                keywords: [option.email, option.agent]
            )
        }
    }

    private func option(for key: String) -> AccountOption? {
        options.first { $0.key == key }
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
            // The brand mark is what makes a login row a LOGIN row, and an
            // `Image` is not a `PickerItem` glyph (a picker icon is a
            // registry name), so it draws over the primitive's mark.
            renderMark: { item in
                guard let mark, let option = option(for: item.value),
                      let image = mark(option.agent)
                else { return nil }
                return AnyView(
                    image
                        .resizable()
                        .scaledToFit()
                        .frame(width: 16, height: 16)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                )
            },
            // EXP-992: the inline limits preview stays with the row.
            renderDetail: { item in
                guard let limits = option(for: item.value)?.limits else { return nil }
                return AnyView(AccountLimitBars(limits: limits))
            },
            trigger: trigger
        )
    }
}
