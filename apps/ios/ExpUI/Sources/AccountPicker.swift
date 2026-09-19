import ExpCore
import SwiftUI

// EXP-872: THE account picker (×4: web `@exp/ui` `AccountPicker`, desktop
// `coding_selects::account_picker`, Android `AccountPickerPill`). It REPLACES
// the agent picker + the account picker on every launch surface: the list is
// every signed-in login the machine reports, across both agents, and picking
// one implies its agent (`AccountOptions.flatten` owns the rules, this file
// only draws them).
//
// The trigger and every row read the same way: the agent's brand mark + the
// login's EMAIL. Never the profile name, never the word "default" — the
// device default is simply FIRST. A dead credential rides as a muted badge
// beside the email (`AgentAccountHealth.badgeLabel`).
//
// EXP-992: on TOUCH the limits preview sits INLINE under the email — three
// tiny bars labelled `5h` / `week` / `<model lowercased>`. (Web hovers them
// out to the side; a phone has no hover, so the row carries them.)
//
// Marks are resolved by the CALLER (`AgentBrandMark` lives in the app target),
// so an id with no asset falls back exactly as it does everywhere else.

/// The bar labels, byte-identical ×4. The model bar is labelled by the
/// window's own name, lower-cased (`fable`).
public enum AccountLimitLabels {
    public static let fiveHour = "5h"
    public static let week = "week"

    /// The bars in order: 5h, week, then the model window when there is one.
    public static func bars(_ limits: AccountLimits) -> [(label: String, used: Double)] {
        var bars: [(label: String, used: Double)] = [
            (fiveHour, limits.fiveHour),
            (week, limits.week),
        ]
        if let model = limits.model {
            bars.append((model.label.lowercased(), model.used))
        }
        return bars
    }
}

/// The compact three-bar block: a 10pt label column and a 4pt meter per row.
/// Shared by the picker's rows and any form that shows a login's headroom.
public struct AccountLimitBars: View {
    let limits: AccountLimits

    public init(limits: AccountLimits) {
        self.limits = limits
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            ForEach(AccountLimitLabels.bars(limits), id: \.label) { bar in
                HStack(spacing: 6) {
                    Text(bar.label)
                        .font(.system(size: 10))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                        .frame(width: 32, alignment: .leading)
                    AgentUsageTrack(
                        percent: bar.used * 100,
                        severity: AgentUsagePresentation.severity(bar.used * 100),
                        height: 4
                    )
                }
            }
        }
        .accessibilityHidden(true)
    }
}

/// The picker's trigger content — also what a LONE login renders (no menu to
/// open), so the two read identically.
public struct AccountPickerTriggerLabel: View {
    let option: AccountOption?
    let mark: Image?
    var chevron: Bool = true

    public init(option: AccountOption?, mark: Image?, chevron: Bool = true) {
        self.option = option
        self.mark = mark
        self.chevron = chevron
    }

    public var body: some View {
        HStack(spacing: 5) {
            if let mark {
                mark
                    .resizable()
                    .scaledToFit()
                    .frame(width: 14, height: 14)
            }
            if let option {
                Text(option.email)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let badge = option.health.badgeLabel {
                    Text(badge)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }
            if chevron {
                AppIcon(AppIcons.uiChevronDown, size: 10)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 28)
        .background(GlassTokens.fillRow, in: Capsule())
        .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
        .contentShape(Capsule())
    }
}

/// One login's menu row: the brand mark, the email (+ its health badge), a
/// check on the current pick, and the inline limits under it.
struct AccountPickerRow: View {
    let option: AccountOption
    let mark: Image?
    let isSelected: Bool
    let action: () -> Void

    @Environment(\.glassMenuDismiss) private var dismissMenu

    var body: some View {
        Button {
            dismissMenu()
            action()
        } label: {
            HStack(spacing: 10) {
                if let mark {
                    mark
                        .resizable()
                        .scaledToFit()
                        .frame(width: 16, height: 16)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 6) {
                        Text(option.email)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(GlassMenuTokens.textOpacity))
                            .lineLimit(1)
                            .truncationMode(.middle)
                        if let badge = option.health.badgeLabel {
                            Text(badge)
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                        }
                    }
                    if let limits = option.limits {
                        AccountLimitBars(limits: limits)
                    }
                }
                Spacer(minLength: 0)
                if isSelected {
                    AppIcon(AppIcons.uiCheck, size: 14)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
            .padding(.horizontal, GlassMenuTokens.itemHPadding)
            .padding(.vertical, 8)
            .frame(
                maxWidth: .infinity,
                minHeight: GlassMenuTokens.itemMinHeight,
                alignment: .leading
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// THE account picker: a `GlassMenu` over every login the machine reports.
/// A single option is not a choice — it renders as the plain trigger label,
/// chevron-less, exactly like a lone agent used to.
public struct AccountPickerMenu: View {
    let options: [AccountOption]
    let selection: AccountOption?
    let mark: (String) -> Image?
    let onSelect: (AccountOption) -> Void

    public init(
        options: [AccountOption],
        selection: AccountOption?,
        mark: @escaping (String) -> Image?,
        onSelect: @escaping (AccountOption) -> Void
    ) {
        self.options = options
        self.selection = selection
        self.mark = mark
        self.onSelect = onSelect
    }

    /// What the trigger names: the caller's pick, else the first option (the
    /// device default) — the trigger always says something.
    private var current: AccountOption? {
        selection ?? options.first
    }

    public var body: some View {
        if options.count > 1 {
            GlassMenu {
                ForEach(options, id: \.key) { option in
                    AccountPickerRow(
                        option: option,
                        mark: mark(option.agent),
                        isSelected: option.key == current?.key
                    ) {
                        onSelect(option)
                    }
                }
            } label: {
                AccountPickerTriggerLabel(
                    option: current, mark: current.flatMap { mark($0.agent) }
                )
            }
            .accessibilityLabel(current?.email ?? "Account")
        } else {
            AccountPickerTriggerLabel(
                option: current,
                mark: current.flatMap { mark($0.agent) },
                chevron: false
            )
            .accessibilityLabel(current?.email ?? "Account")
        }
    }
}
