import SwiftUI

// The ONE pill (EXP-698 round 2). Before this file iOS had three pill types
// (`GlassPillLabel`, `GlassPillButton`, `GlassChip`) plus nineteen raw
// `.glassButton()` capsules, each picking its own font, padding and vertical
// rhythm — a filter pill, a property chip and a repo chip were three different
// heights sitting in the same row. `GlassPill` is the whole vocabulary:
//
//   size    .sm (24pt) — the DEFAULT, and what almost every pill is: a
//                         metadata chip, a filter, a property, a role badge,
//                         an inline action. A short pill beside a taller
//                         control is correct — the members list's role badge
//                         sits 12pt from the name and 12pt from the row's
//                         32pt circle button, and reads as a label, not as a
//                         second button.
//           .md (32pt) — reserved for a pill that is one of SEVERAL PEER
//                        ACTIONS in a bar of its own (the steer screen's
//                        Merge / Fix conflicts beside the Latest-changes
//                        chip, an empty state's primary action), where a
//                        24pt capsule reads as a caption instead of a
//                        button.
//   mode    .action    — a plain tap
//           .select    — a tap that also carries a selected state
//           .readonly  — a label capsule; not a Button, not hit-testable
//
// Chrome is the same in every combination: a `Capsule` wearing `fillCard` +
// the `strokeCard` hairline (`.glassButton()`), label white at
// `TextOpacity.secondary`. Only `.select` when SELECTED changes it, to
// `fillActive` + `strokeActive` with a full-opacity label — the same "selected"
// treatment a `GlassRow` and a segmented segment wear.

/// The pinned geometry of a glass pill, per size rung. Numbers live here
/// rather than inline so `GlassPillTokenTests` can hold them still: a pill
/// that quietly grows two points stops lining up with the circle buttons and
/// chips it shares a row with.
public enum GlassPillTokens {
    /// `controlSm` — the chip/filter rung.
    public static let heightSm: CGFloat = DesignTokens.Size.controlSm
    /// `controlMd` — the rung that matches `CircleIconButton`.
    public static let heightMd: CGFloat = DesignTokens.Size.controlMd

    public static let horizontalPaddingSm: CGFloat = 8
    public static let horizontalPaddingMd: CGFloat = 12

    /// Gap between the leading mark, the label and the trailing slot.
    public static let spacingSm: CGFloat = 4
    public static let spacingMd: CGFloat = 6

    /// The leading registry glyph.
    public static let glyphSm: CGFloat = 12
    public static let glyphMd: CGFloat = 16

    /// The status/label dot — a plain filled disc, never a ring.
    public static let dotSize: CGFloat = 6
}

/// The two rungs. A pill is either chip-sized or control-sized; there is no
/// third height and no free `verticalPadding` knob (the one EXP-678 added is
/// gone — the tall "Latest changes" pills are `.md`).
public enum GlassPillSize {
    case sm
    case md

    var height: CGFloat {
        self == .sm ? GlassPillTokens.heightSm : GlassPillTokens.heightMd
    }

    var horizontalPadding: CGFloat {
        self == .sm ? GlassPillTokens.horizontalPaddingSm : GlassPillTokens.horizontalPaddingMd
    }

    var spacing: CGFloat {
        self == .sm ? GlassPillTokens.spacingSm : GlassPillTokens.spacingMd
    }

    /// The leading glyph size — public so a call site building its own leading
    /// view (a brand image, a board icon, a spinner) matches the registry
    /// glyphs the `icon:` init draws.
    public var glyphSize: CGFloat {
        self == .sm ? GlassPillTokens.glyphSm : GlassPillTokens.glyphMd
    }

    var font: Font {
        self == .sm ? .caption.weight(.medium) : .subheadline.weight(.medium)
    }
}

public enum GlassPillMode {
    /// A plain tap.
    case action(() -> Void)
    /// A tap that also carries a selected state (a label toggle, a tab).
    case select(isSelected: Bool, action: () -> Void)
    /// A label capsule — resting chrome, no Button, no hit testing, so it never
    /// swallows a tap meant for the row behind it.
    case readonly
}

public struct GlassPill<Leading: View, Trailing: View>: View {

    /// Spelled out at call sites as `.sm` / `.md`; the type itself lives
    /// OUTSIDE the generic struct so every specialization shares one `Size`.
    public typealias Size = GlassPillSize
    public typealias Mode = GlassPillMode

    let label: String
    var size: GlassPillSize = .sm
    var mode: GlassPillMode = .readonly
    var dot: Color? = nil
    var isOpaque: Bool = false
    var enabled: Bool = true
    let leading: Leading
    let trailing: Trailing

    public init(
        _ label: String,
        size: GlassPillSize = .sm,
        mode: GlassPillMode = .readonly,
        dot: Color? = nil,
        isOpaque: Bool = false,
        enabled: Bool = true,
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.label = label
        self.size = size
        self.mode = mode
        self.dot = dot
        self.isOpaque = isOpaque
        self.enabled = enabled
        self.leading = leading()
        self.trailing = trailing()
    }

    private var isSelected: Bool {
        if case .select(let selected, _) = mode { return selected }
        return false
    }

    private var labelOpacity: Double {
        guard enabled else { return TextOpacity.quaternary }
        return isSelected ? TextOpacity.primary : TextOpacity.secondary
    }

    @ViewBuilder
    public var body: some View {
        switch mode {
        case .readonly:
            content.allowsHitTesting(false)
        case .action(let action), .select(_, let action):
            Button(action: action) {
                content.contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .disabled(!enabled)
        }
    }

    private var content: some View {
        HStack(spacing: size.spacing) {
            if let dot {
                Circle()
                    .fill(dot)
                    .frame(width: GlassPillTokens.dotSize, height: GlassPillTokens.dotSize)
            }
            leading
            if !label.isEmpty {
                Text(label)
                    .font(size.font)
                    .lineLimit(1)
            }
            trailing
        }
        .foregroundStyle(.white.opacity(labelOpacity))
        .padding(.horizontal, size.horizontalPadding)
        .frame(height: size.height)
        .glassButton(isActive: isSelected, isOpaque: isOpaque)
    }
}

// MARK: - Convenience inits

extension GlassPill where Trailing == EmptyView {
    public init(
        _ label: String,
        size: GlassPillSize = .sm,
        mode: GlassPillMode = .readonly,
        dot: Color? = nil,
        isOpaque: Bool = false,
        enabled: Bool = true,
        @ViewBuilder leading: () -> Leading
    ) {
        self.init(
            label,
            size: size,
            mode: mode,
            dot: dot,
            isOpaque: isOpaque,
            enabled: enabled,
            leading: leading
        ) { EmptyView() }
    }
}

extension GlassPill where Leading == EmptyView, Trailing == EmptyView {
    public init(
        _ label: String,
        size: GlassPillSize = .sm,
        mode: GlassPillMode = .readonly,
        dot: Color? = nil,
        isOpaque: Bool = false,
        enabled: Bool = true
    ) {
        self.init(
            label,
            size: size,
            mode: mode,
            dot: dot,
            isOpaque: isOpaque,
            enabled: enabled
        ) { EmptyView() } trailing: { EmptyView() }
    }
}

extension GlassPill where Leading == AppIcon, Trailing == EmptyView {
    /// The common case: a registry glyph leading the label, sized to the rung.
    public init(
        _ label: String,
        icon: String,
        size: GlassPillSize = .sm,
        mode: GlassPillMode = .readonly,
        dot: Color? = nil,
        isOpaque: Bool = false,
        enabled: Bool = true
    ) {
        self.init(
            label,
            size: size,
            mode: mode,
            dot: dot,
            isOpaque: isOpaque,
            enabled: enabled
        ) {
            AppIcon(icon, size: size.glyphSize)
        } trailing: { EmptyView() }
    }
}
