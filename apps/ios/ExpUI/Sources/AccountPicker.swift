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
// last used login is simply FIRST. A dead credential rides as a muted badge
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
    /// `capsule` = the composer's chip (chevron-down); `rowValue` = a FORM
    /// row's plain trailing value + chevron-right, like every picker row
    /// beside it (polish pin: picker rows end in chevron-right ×4).
    public enum Style: Sendable {
        case capsule
        case rowValue
    }

    let option: AccountOption?
    let mark: Image?
    var chevron: Bool = true
    var style: Style = .capsule

    public init(option: AccountOption?, mark: Image?, chevron: Bool = true, style: Style = .capsule) {
        self.option = option
        self.mark = mark
        self.chevron = chevron
        self.style = style
    }

    public var body: some View {
        switch style {
        case .capsule:
            content
                .padding(.horizontal, 10)
                .frame(height: 28)
                .background(GlassTokens.fillRow, in: Capsule())
                .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
                .contentShape(Capsule())
        case .rowValue:
            content
                .contentShape(Rectangle())
        }
    }

    private var content: some View {
        HStack(spacing: style == .rowValue ? 6 : 5) {
            if let mark {
                mark
                    .resizable()
                    .scaledToFit()
                    .frame(width: 14, height: 14)
            }
            if let option {
                Text(option.email)
                    .font(style == .rowValue ? .body : .caption.weight(.medium))
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
                switch style {
                case .capsule:
                    AppIcon(AppIcons.uiChevronDown, size: 10)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                case .rowValue:
                    AppIcon(AppIcons.uiChevronRight, size: 14)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
        }
    }
}

/// What a login READS as, brand mark aside: the email, its health badge, and
/// the EXP-992 bars under both — the shared picker's own row body
/// (`AccountPicker`, EXP-1021).
struct AccountOptionBody: View {
    let option: AccountOption

    var body: some View {
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
    }
}
