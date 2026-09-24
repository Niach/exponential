import ExpCore
import SwiftUI

// EXP-1029 contract, EXP-1021 implementation — the account picker under the
// shared picker API.
//
// The EXP-991 picker (`AccountPicker.swift`: `AccountPickerMenu` +
// `AccountPickerTriggerLabel`, brand mark + login email per row, the EXP-992
// rate-limit preview `AccountLimitBars` under each login) rides `GlassPicker`
// here, keeping its options AND its preview: the brand mark is the row's
// MARK, and the email + badge + bars are the row's BODY (`renderItem`, the
// one bespoke row body in the package — web's account picker does exactly the
// same on the same seam). A row is keyed by `AccountOption.key`, never the
// bare profile id: `system` repeats across agents.
// (The file is not `AccountPicker.swift`: swiftc refuses two files of one
// name in a module, and that one still holds the menu the other launch
// surfaces open.)

public struct AccountPicker<Trigger: View>: View {
    public let options: [AccountOption]
    /// The picked option's `key` (`<agent>:<profileId>`), or nil while none
    /// is — the same value `onChange` reports back.
    public let value: String?
    public let onChange: (String) -> Void
    /// The agent's brand mark, resolved by the CALLER (the assets live in the
    /// app target), so an id with no asset falls back exactly as it does
    /// everywhere else.
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

    /// One row per login: the email, its health badge as the muted second
    /// line (the bars replace it when a row draws its own body), and both the
    /// email and the agent as search keywords.
    nonisolated public static func items(_ options: [AccountOption]) -> [PickerItem<String>] {
        options.map { option in
            PickerItem(
                value: option.key,
                label: option.email,
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
            // A lone login is not a choice: the trigger still says which one
            // it is, and there is no sheet to open (web collapses its inline
            // word for the same reason).
            disabled: options.count <= 1,
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            renderMark: { item in
                guard let image = option(for: item.value).flatMap({ mark?($0.agent) })
                else { return nil }
                return AnyView(
                    image
                        .resizable()
                        .scaledToFit()
                        .frame(width: 16, height: 16)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                )
            },
            // EXP-992: the limits preview is why the account rows draw their
            // own body — a phone has no hover, so the three bars sit inline
            // under the email.
            renderItem: { item in
                guard let option = option(for: item.value) else { return nil }
                // The bars make this the one row taller than the 44pt tap
                // target, so it brings the breathing room with it.
                return AnyView(AccountOptionBody(option: option).padding(.vertical, 8))
            },
            trigger: trigger
        )
    }
}
