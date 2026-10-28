import SwiftUI

/// S-3 (voice-call.json, styleguide entry `call-controls`, eUI `CallButton`):
/// THE round icon-only call button. A 56 pt circle: neutral = the muted
/// surface fill, toggled (or pressed) = the inverted neutral fill (the
/// foreground disc, the background ink), destructive = the destructive
/// token (the red End call). The label is the accessibility label; a
/// `caption` (the ringing call's Cancel) shows UNDER the circle.
public struct CallButton: View {
    public enum Tone: Sendable { case neutral, destructive }

    /// The circle's edge (web `size-14`, desktop / Android 56).
    public static let size: CGFloat = 56
    /// The glyph's edge inside it.
    public static let glyph: CGFloat = 24

    let icon: String
    let label: String
    var tone: Tone
    var toggled: Bool
    var caption: String?
    var action: () -> Void

    public init(
        icon: String, label: String, tone: Tone = .neutral, toggled: Bool = false,
        caption: String? = nil, action: @escaping () -> Void
    ) {
        self.icon = icon
        self.label = label
        self.tone = tone
        self.toggled = toggled
        self.caption = caption
        self.action = action
    }

    public var body: some View {
        VStack(spacing: 8) {
            Button(action: action) {
                AppIcon(icon, size: Self.glyph, weight: .medium)
            }
            .buttonStyle(CallButtonStyle(tone: tone, toggled: toggled))
            .accessibilityLabel(label)
            .accessibilityAddTraits(toggled ? .isSelected : [])
            if let caption {
                Text(caption)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Palette.mutedForeground)
                    .accessibilityHidden(true)
            }
        }
    }
}

private struct CallButtonStyle: ButtonStyle {
    let tone: CallButton.Tone
    let toggled: Bool

    func makeBody(configuration: Configuration) -> some View {
        let inverted = tone == .neutral && (toggled || configuration.isPressed)
        configuration.label
            .foregroundStyle(inverted ? DesignTokens.Palette.background : DesignTokens.Palette.foreground)
            .frame(width: CallButton.size, height: CallButton.size)
            .background(Circle().fill(fill(inverted: inverted)))
            .opacity(tone == .destructive && configuration.isPressed ? 0.85 : 1)
            .contentShape(Circle())
    }

    private func fill(inverted: Bool) -> Color {
        switch tone {
        case .destructive: DesignTokens.Palette.destructive
        case .neutral: inverted ? DesignTokens.Palette.foreground : DesignTokens.Palette.muted
        }
    }
}

/// THE row of round call buttons, evenly spaced, in the caller's order
/// (voice-call.json `callView(…).buttons`).
public struct CallControls<Content: View>: View {
    /// The gap between two circles (web `gap-8`).
    public static var spacing: CGFloat { 32 }

    private let content: Content

    public init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    public var body: some View {
        HStack(alignment: .top, spacing: Self.spacing) { content }
    }
}
