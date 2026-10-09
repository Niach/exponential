import SwiftUI

// THE switch row (web `GlassToggleRow`, desktop `glass_toggle_row`, Android
// `SwitchRow`): a label, an optional muted second line, the ONE glass switch
// (`GlassToggleStyle`) trailing. Every on/off control in the app renders
// through it, so a stock `Toggle(` never reaches a screen with its own
// geometry, tint or label rhythm.
//
// Three shapes, one control:
//   - `.row` (default): a form / list row — label left, switch right. The
//     host owns the row padding (a `Form` row, a sheet's 12pt rhythm).
//   - `.pill`: the composer options row's switch, where the capsule IS the
//     toggle (EXP-859: the 51×31 switch is too tall for a 28pt row, and
//     scaling it left SwiftUI laying out the unscaled size).
//   - no title: the BARE switch, for a row that already names what it
//     switches (a trigger row's enable). It takes only its own width and must
//     carry an accessibility label.

public enum GlassToggleRowStyle: Sendable {
    case row
    case pill
}

public enum GlassToggleRowTokens {
    /// The pill switch: the glass switch at row scale (34×20, 16pt thumb, the
    /// same 2pt inset as the 51×31 one).
    public static let pillTrackWidth: CGFloat = 34
    public static let pillTrackHeight: CGFloat = 20
    public static let pillThumb: CGFloat = 16
    public static let pillHeight: CGFloat = 28
}

public struct GlassToggleRow<Leading: View>: View {
    let title: String?
    let description: String?
    @Binding var isOn: Bool
    let style: GlassToggleRowStyle
    let leading: Leading

    public init(
        _ title: String?,
        description: String? = nil,
        isOn: Binding<Bool>,
        style: GlassToggleRowStyle = .row,
        @ViewBuilder leading: () -> Leading
    ) {
        self.title = title
        self.description = description
        self._isOn = isOn
        self.style = style
        self.leading = leading()
    }

    public var body: some View {
        switch style {
        case .row:
            if let title {
                Toggle(isOn: $isOn) {
                    HStack(spacing: 12) {
                        leading
                        VStack(alignment: .leading, spacing: 2) {
                            Text(title)
                                .foregroundStyle(.white.opacity(TextOpacity.primary))
                                .lineLimit(1)
                            if let description, !description.isEmpty {
                                Text(description)
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                        }
                    }
                }
                .toggleStyle(.glass)
            } else {
                Toggle("", isOn: $isOn)
                    .labelsHidden()
                    .toggleStyle(.glass)
                    .fixedSize()
            }
        case .pill:
            Toggle(isOn: $isOn) {
                Text(title ?? "")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            .toggleStyle(GlassPillToggleStyle())
        }
    }
}

extension GlassToggleRow where Leading == EmptyView {
    public init(
        _ title: String?,
        description: String? = nil,
        isOn: Binding<Bool>,
        style: GlassToggleRowStyle = .row
    ) {
        self.init(title, description: description, isOn: isOn, style: style) { EmptyView() }
    }
}

/// The `.pill` arm's capsule: caption + a row-scale track, drawn at the row's
/// own size so nothing is transformed (EXP-859).
struct GlassPillToggleStyle: ToggleStyle {
    @Environment(\.motion) private var motion
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        Button {
            withAnimation(motion.fast) { configuration.isOn.toggle() }
        } label: {
            HStack(spacing: 6) {
                configuration.label
                track(isOn: configuration.isOn)
            }
            .padding(.leading, 10)
            .padding(.trailing, 5)
            .frame(height: GlassToggleRowTokens.pillHeight)
            .background(GlassTokens.fillRow, in: Capsule())
            .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .opacity(isEnabled ? 1 : 0.4)
        .accessibilityAddTraits(.isToggle)
        .accessibilityValue(configuration.isOn ? "On" : "Off")
    }

    private func track(isOn: Bool) -> some View {
        Capsule()
            .fill(isOn ? DesignTokens.Palette.primary : GlassTokens.fillCard)
            .overlay(
                Capsule().stroke(
                    isOn ? Color.clear : GlassTokens.strokeCard,
                    lineWidth: GlassTokens.hairline
                )
            )
            .overlay(alignment: isOn ? .trailing : .leading) {
                Circle()
                    .fill(
                        isOn
                            ? DesignTokens.Palette.primaryForeground
                            : DesignTokens.Palette.mutedForeground
                    )
                    .frame(width: GlassToggleRowTokens.pillThumb, height: GlassToggleRowTokens.pillThumb)
                    .padding(2)
            }
            .frame(width: GlassToggleRowTokens.pillTrackWidth, height: GlassToggleRowTokens.pillTrackHeight)
    }
}
