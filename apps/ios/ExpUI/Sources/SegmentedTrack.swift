import ExpCore
import SwiftUI

// EXP-1051: the STACKED meter — `AgentUsageTrack`'s rail, filled by several
// layers instead of one. It draws the context-window bar: the layers the
// device attributed, then the conversation, over the free track.
//
// A sibling of `UsageTrack.swift` rather than a mode of it: the usage rail
// takes ONE percentage and a severity tone, this one takes a list of
// contract-toned slices and never colours itself by how full it is. Both wear
// the same capsule and the same track colour, so the two read as one family.
//
// The ×4 twins: web's stacked `Meter`, desktop `ui::context_layout`, Android
// `SegmentedTrack`. The slices come from
// `ContextLayoutPresentation.contextWindowView`, already ordered and already
// percentages OF THE WHOLE WINDOW — which is why nothing here normalises
// them: an overshooting layout CLIPS (the rule the fold is locked on), it does
// not rescale.

public struct SegmentedTrack: View {
    let slices: [ContextBarSlice]
    /// Percent marks drawn over the fill — the compaction floor and the two
    /// usage thresholds.
    var ticks: [Int]
    var height: CGFloat

    public init(slices: [ContextBarSlice], ticks: [Int] = [], height: CGFloat = 8) {
        self.slices = slices
        self.ticks = ticks
        self.height = height
    }

    public var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                // The track IS the `free` row: it is never a slice.
                Capsule()
                    .fill(GlassTokens.strokeStrong)
                HStack(spacing: 0) {
                    ForEach(slices) { slice in
                        Rectangle()
                            .fill(Self.tone(slice.tone))
                            .frame(width: width(slice.percent, in: geo.size.width))
                    }
                    Spacer(minLength: 0)
                }
                ForEach(ticks, id: \.self) { tick in
                    Rectangle()
                        .fill(Color.black.opacity(0.45))
                        .frame(width: 1)
                        .offset(x: width(Double(tick), in: geo.size.width))
                }
            }
            .clipShape(Capsule())
        }
        .frame(height: height)
        .accessibilityHidden(true)
    }

    private func width(_ percent: Double, in total: CGFloat) -> CGFloat {
        total * CGFloat(min(max(percent, 0), 100)) / 100
    }

    /// A contract tone (`neutral`, `track`, or one of the avatar hues) as a
    /// colour. The hues are the SAME palette the avatars and board icons use,
    /// so a layer's colour means nothing else on screen; `neutral` is the
    /// unopinionated white the agent's own base prompt gets, and `track` is
    /// the rail itself (the `free` legend row's swatch).
    public static func tone(_ tone: String) -> Color {
        switch tone {
        case "neutral": .white.opacity(0.3)
        case "track": GlassTokens.strokeStrong
        case "red": DesignTokens.Avatar.red
        case "orange": DesignTokens.Avatar.orange
        case "yellow": DesignTokens.Avatar.yellow
        case "green": DesignTokens.Avatar.green
        case "teal": DesignTokens.Avatar.teal
        case "blue": DesignTokens.Avatar.blue
        case "violet": DesignTokens.Avatar.violet
        case "pink": DesignTokens.Avatar.pink
        // A tone a newer contract added: the neutral fill, never nothing.
        default: .white.opacity(0.3)
        }
    }
}
