import ExpPush
import SwiftUI
#if !LIVE_ACTIVITY_EXTENSION
import ExpUI
#endif

// #EXP-1639 (live-activity.json): the drawing of a session's Live Activity,
// compiled into the widget extension (where the OS hosts it) and the app
// (the `os-live-activity` capture specimen, `LiveActivitySpecimens`). Draws
// only the opened content (`LiveActivityDisplay`); a sealed title shows as
// the generic copy, never fetched. Forced-dark tokens like every screen.

/// The app's mark (the widget's own asset catalog, also bundled by the app).
struct SessionActivityMark: View {
    var size: CGFloat

    var body: some View {
        Image("AppMark")
            .resizable()
            .interpolation(.high)
            .frame(width: size, height: size)
            .clipShape(RoundedRectangle(cornerRadius: size * 0.225, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// The badge: the OS-counted timer from `startedAt`, "Needs you", or a text
/// (`PR #n`, an ended timer's "Done").
struct SessionActivityBadge: View {
    var display: LiveActivityDisplay
    /// The Dynamic Island's compact trailing slot: a dot for Needs you.
    var compact = false

    var body: some View {
        if display.badge == .timer, !display.isDone, let startedAt = display.startedAt {
            Text(timerInterval: startedAt...startedAt.addingTimeInterval(12 * 3600), countsDown: false)
                .monospacedDigit()
                .font(.system(size: compact ? 14 : 15, weight: .semibold))
                .foregroundStyle(DesignTokens.Palette.foreground)
                .multilineTextAlignment(.trailing)
                .frame(maxWidth: compact ? 48 : 72, alignment: .trailing)
        } else if display.needsYou {
            HStack(spacing: 6) {
                Circle()
                    .fill(DesignTokens.Semantic.yellow)
                    .frame(width: 8, height: 8)
                if !compact, let text = display.badgeText {
                    Text(text)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(DesignTokens.Semantic.yellow)
                }
            }
            .accessibilityLabel(display.badgeText ?? "")
        } else if let text = display.badgeText {
            Text(text)
                .font(.system(size: compact ? 12 : 13, weight: .semibold))
                .foregroundStyle(DesignTokens.Palette.foreground)
                .lineLimit(1)
                .padding(.horizontal, compact ? 0 : 8)
                .padding(.vertical, compact ? 0 : 3)
                .background {
                    if !compact { Capsule().fill(GlassTokens.fillCard) }
                }
        }
    }
}

/// Title and subtitle, the shared head of the Lock Screen and the expanded
/// island.
struct SessionActivityHeading: View {
    var display: LiveActivityDisplay

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(display.title)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(DesignTokens.Palette.foreground)
                .lineLimit(1)
            if !display.subtitle.isEmpty {
                Text(display.subtitle)
                    .font(.system(size: 14))
                    .foregroundStyle(DesignTokens.Palette.foreground.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
        }
    }
}

struct SessionActivityDetail: View {
    var detail: String

    var body: some View {
        Text(detail)
            .font(.system(size: 14))
            .foregroundStyle(DesignTokens.Palette.foreground.opacity(TextOpacity.primary))
            .lineLimit(2)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The Lock Screen (and banner) presentation: icon, title, subtitle, badge,
/// detail.
struct SessionActivityLockScreen: View {
    var display: LiveActivityDisplay

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .center, spacing: 12) {
                SessionActivityMark(size: 40)
                SessionActivityHeading(display: display)
                Spacer(minLength: 8)
                SessionActivityBadge(display: display)
            }
            if let detail = display.detail {
                SessionActivityDetail(detail: detail)
            }
        }
        .padding(16)
    }
}

/// The expanded Dynamic Island's centre: title, subtitle, detail.
struct SessionActivityExpanded: View {
    var display: LiveActivityDisplay

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            SessionActivityHeading(display: display)
            if let detail = display.detail {
                SessionActivityDetail(detail: detail)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
