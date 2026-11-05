#if !SHARE_EXTENSION
import ExpCore
#endif
import SwiftUI

// The agent bubble on its own: the share extension compiles it (EXP-1635).

/// THE agent bubble's circle (chats.json `shell.bubble`), shared by the Chats
/// header's bubbles (66pt) and S-3's composer agent row (composer-home.json
/// `agentRow`, 40pt): the glyph on a bg-muted disc inside a `ringGap`
/// background gap, then the ring.
public struct AgentBubbleCircle: View {
    public enum Glyph: Equatable {
        /// A registry glyph in its colour.
        case icon(String, Color)
        /// S-3: the Assistant's avatar = the Exponential brand mark in the
        /// foreground colour.
        case brand
        /// S-3: the composer row's Temporary session = the concept glyph
        /// (`ui-icon-placeholder`) in the muted foreground.
        case temporary(String)
    }

    let glyph: Glyph
    let size: CGFloat
    let ringGap: CGFloat
    let ringColor: Color
    let ringWidth: CGFloat
    /// The ring's slot outside the gap (the widest ring the circle wears),
    /// so a ring change never resizes the disc.
    let ringSlot: CGFloat
    /// The glyph's size; nil = the Chats bubble's share of the disc.
    let glyphOverride: CGFloat?

    public init(
        glyph: Glyph,
        size: CGFloat,
        ringGap: CGFloat,
        ringColor: Color,
        ringWidth: CGFloat,
        ringSlot: CGFloat = 2,
        glyphSize: CGFloat? = nil
    ) {
        self.glyph = glyph
        self.size = size
        self.ringGap = ringGap
        self.ringColor = ringColor
        self.ringWidth = ringWidth
        self.ringSlot = ringSlot
        self.glyphOverride = glyphSize
    }

    private var inner: CGFloat { size - 2 * ringGap - 2 * ringSlot }
    /// 26pt in the 66pt bubble, the same share at any size.
    private var glyphSize: CGFloat { glyphOverride ?? (inner * 26 / 56).rounded() }

    public var body: some View {
        ZStack {
            Circle().stroke(ringColor, lineWidth: ringWidth)
            Circle()
                .fill(DesignTokens.Palette.muted)
                .frame(width: inner, height: inner)
            AgentBubbleGlyph(glyph: glyph, size: glyphSize)
        }
        .frame(width: size, height: size)
    }
}

/// The bubble's glyph alone (`AgentBubbleCircle.Glyph` at `size`): the
/// circle's centre.
struct AgentBubbleGlyph: View {
    let glyph: AgentBubbleCircle.Glyph
    let size: CGFloat

    var body: some View {
        switch glyph {
        case let .icon(name, color):
            AppIcon(name, size: size, weight: .medium)
                .foregroundStyle(color)
        case .brand:
            // The static logo (FeedbackMark's still frame) without the
            // feedback mark's test id.
            MarkCanvas(color: .white, spans: FeedbackStates.Mark.staticSpans)
                .frame(width: size, height: size)
        case let .temporary(name):
            AppIcon(name, size: size, weight: .medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }
}
