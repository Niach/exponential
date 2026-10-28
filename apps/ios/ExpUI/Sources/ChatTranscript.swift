import ExpCore
import SwiftUI

// Thread 58cb32df (chats.json TRANSCRIPT + SURFACE CARD, ×4): the phone
// chat's transcript pieces — the centred divider pill a `/new` or
// `/compact` line paints, the agent's left bubble, the live-arrival rise of
// a new bubble, and the inline surface card's frame with its ENTER
// animation (`surfaceCard.enter`, motion tokens only; reduce motion = a
// plain opacity fade).

/// `copy.dividers.<kind> · HH:mm` in a centred glass pill, never a bubble.
public struct TranscriptDividerPill: View {
    let icon: String
    let text: String

    public init(icon: String, text: String) {
        self.icon = icon
        self.text = text
    }

    public var body: some View {
        HStack(spacing: 8) {
            AppIcon(icon, size: 14)
            Text(text)
                .font(.subheadline)
                .lineLimit(1)
        }
        .foregroundStyle(.white.opacity(TextOpacity.secondary))
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .background(GlassTokens.opaqueCardFill, in: Capsule())
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(Chats.TestIds.divider)
    }
}

public enum ChatBubbleMetrics {
    /// The agent bubble's share of the column (the person's = 80%).
    public static let agentMaxFraction: CGFloat = 0.94
    /// The free trailing gap that caps the agent bubble at ~94% of a phone
    /// column (a layout-stable spacer rather than a measured fraction).
    public static let agentTrailingGap: CGFloat = 24
    /// The free leading gap that caps the person's bubble at ~80% of a
    /// phone column (chats.json CHAT MODE: `UserMessageBubble` max 80%).
    public static let personLeadingGap: CGFloat = 72
    public static let radius: CGFloat = 18
    /// The top-left corner the bubble hangs from.
    public static let tailRadius: CGFloat = 6
    /// A live bubble's rise (`standard` / `decelerate`).
    public static let arrivalRise: CGFloat = 6
}

/// The agent's left bubble: its content on the card fill, the time at the
/// bottom-right.
public struct AgentChatBubble<Content: View>: View {
    let time: String?
    @ViewBuilder let content: () -> Content

    public init(time: String?, @ViewBuilder content: @escaping () -> Content) {
        self.time = time
        self.content = content
    }

    public var body: some View {
        let shape = UnevenRoundedRectangle(
            topLeadingRadius: ChatBubbleMetrics.tailRadius,
            bottomLeadingRadius: ChatBubbleMetrics.radius,
            bottomTrailingRadius: ChatBubbleMetrics.radius,
            topTrailingRadius: ChatBubbleMetrics.radius
        )
        HStack(spacing: 0) {
            VStack(alignment: .trailing, spacing: 4) {
                content()
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let time, !time.isEmpty {
                    Text(time)
                        .font(.caption2)
                        .monospacedDigit()
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(DesignTokens.Palette.card, in: shape)
            .overlay(shape.stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
            // The column's last 6% stays free (max 94%).
            Spacer(minLength: ChatBubbleMetrics.agentTrailingGap)
        }
    }
}

public extension View {
    /// A NEW bubble (live arrival only) rises in: opacity + 6pt at
    /// `standard` / `decelerate`; `animate` false (history, a reopen) = none.
    func chatBubbleArrival(_ animate: Bool) -> some View {
        modifier(ChatBubbleArrival(animate: animate))
    }

}

private struct ChatBubbleArrival: ViewModifier {
    let animate: Bool
    @Environment(\.motion) private var motion
    @State private var shown = false

    func body(content: Content) -> some View {
        let hidden = animate && !shown
        content
            .opacity(hidden ? 0 : 1)
            .offset(y: hidden && !motion.reduceMotion ? ChatBubbleMetrics.arrivalRise : 0)
            .onAppear {
                guard animate, !shown else { return }
                withAnimation(motion.reduceMotion
                    ? motion.standard
                    : motion.decelerate(DesignTokens.Motion.Duration.standard)
                ) { shown = true }
            }
    }
}

private func motionDuration(_ name: String) -> TimeInterval {
    switch name {
    case "fast": DesignTokens.Motion.Duration.fast
    case "slow": DesignTokens.Motion.Duration.slow
    default: DesignTokens.Motion.Duration.standard
    }
}

/// THE inline surface card's frame: its own rounded glass frame, max width
/// the column. `animate` = the card ARRIVED while the chat was open (its id
/// was not in the first painted transcript): the frame fades in from
/// `fromOpacity`, rises `fromTranslateY` and scales from `fromScale` over
/// `slow` / `decelerate`, its hairline settling from the primary colour at
/// `glow.fromOpacity`. Reduce motion = the frame's opacity fade alone.
public struct SurfaceCardFrame<Content: View>: View {
    let title: String?
    let animate: Bool
    @ViewBuilder let content: () -> Content

    @Environment(\.motion) private var motion
    @State private var entered = false
    @State private var settled = false

    public init(title: String?, animate: Bool, @ViewBuilder content: @escaping () -> Content) {
        self.title = title
        self.animate = animate
        self.content = content
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
        let before = animate && !entered
        let glowing = animate && !settled && !motion.reduceMotion
        let enter = Chats.SurfaceEnter.Frame.self
        VStack(alignment: .leading, spacing: 10) {
            if let title, !title.isEmpty {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
            }
            content()
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GlassTokens.opaqueCardFill, in: shape)
        .overlay(
            shape.stroke(
                glowing
                    ? DesignTokens.Palette.primary.opacity(Chats.SurfaceEnter.Glow.fromOpacity)
                    : GlassTokens.strokeCard,
                lineWidth: glowing ? 1 : GlassTokens.hairline
            )
        )
        .opacity(before ? enter.fromOpacity : 1)
        .offset(y: before && !motion.reduceMotion ? enter.fromTranslateY : 0)
        .scaleEffect(before && !motion.reduceMotion ? enter.fromScale : 1)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Chats.TestIds.surfaceCard)
        .onAppear {
            guard animate, !entered else { return }
            let duration = motionDuration(enter.duration)
            withAnimation(motion.reduceMotion
                ? .linear(duration: duration)
                : motion.decelerate(duration)
            ) { entered = true }
            withAnimation(motion.decelerate(motionDuration(Chats.SurfaceEnter.Glow.duration))) {
                settled = true
            }
        }
    }
}
