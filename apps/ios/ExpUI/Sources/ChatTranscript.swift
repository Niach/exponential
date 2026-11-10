import ExpCore
import SwiftUI

// Session 58cb32df (chats.json TRANSCRIPT + SURFACE CARD, ×4): the phone
// chat's transcript pieces — the centred divider pill a `/new` or
// `/compact` line paints, THE message bubble + its row frame (#EXP-1736), the live-arrival rise of
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
    /// THE bubble's radius (transcript-grouping.json `layout.radius`).
    public static let radius: CGFloat = TranscriptGrouping.Layout.radius
    /// A live bubble's rise (`standard` / `decelerate`).
    public static let arrivalRise: CGFloat = 6
}

/// #EXP-1736: which side a bubble sits on: the viewer's own RIGHT, anyone
/// else's LEFT under its sender's avatar.
public enum MessageBubbleSide: Equatable, Sendable {
    case own
    case other
}

/// #EXP-1736 (transcript-grouping.json): THE message bubble ×4, every chat
/// a group chat (web packages/ui `MessageBubble`). The glass card bubble
/// (16pt radius) at most `layout.maxWidth.phone` of its column, hugging its
/// text: `own` = the viewer's (right, the active glass fill inside the
/// strong hairline, neither avatar nor name); `other` = anyone else's
/// (left, THE card surface), its sender's NAME on the group's first bubble
/// (`author`, 12pt medium secondary). #EXP-1772: set `onPress` = a RELAY
/// (another session's message, a child's report): its name (ending in ` ›`)
/// sits ABOVE the bubble, outside it, and only that line opens the
/// sender's session; the bubble itself is never a link. The
/// footer line (the `HH:mm`, a queued row's caption) sits bottom-right
/// inside. The avatar column is `TranscriptRow`'s.
public struct MessageBubble<Content: View, Footer: View>: View {
    let side: MessageBubbleSide
    let author: String?
    let onPress: (() -> Void)?
    let hasFooter: Bool
    @ViewBuilder let content: () -> Content
    @ViewBuilder let footer: () -> Footer

    public init(
        side: MessageBubbleSide = .other,
        author: String? = nil,
        onPress: (() -> Void)? = nil,
        hasFooter: Bool = true,
        @ViewBuilder content: @escaping () -> Content,
        @ViewBuilder footer: @escaping () -> Footer
    ) {
        self.side = side
        self.author = author
        self.onPress = onPress
        self.hasFooter = hasFooter
        self.content = content
        self.footer = footer
    }

    private var own: Bool { side == .own }
    private var name: String? {
        guard !own, let author, !author.isEmpty else { return nil }
        return author
    }

    private var relay: Bool { onPress != nil && !own }

    private func nameLine(_ name: String) -> some View {
        Text(name)
            .font(.system(size: TranscriptGrouping.Layout.nameSize, weight: .medium))
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .lineLimit(1)
            .truncationMode(.tail)
            .accessibilityIdentifier("message-bubble-author")
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: ChatBubbleMetrics.radius)
        let bubble = BubbleColumn(spacing: 4, trailingLast: hasFooter) {
            if let name, !relay {
                nameLine(name)
            }
            content()
            if hasFooter {
                footer()
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(own ? GlassTokens.fillActive : GlassTokens.opaqueCardFill, in: shape)
        .overlay(shape.stroke(own ? GlassTokens.strokeStrong : GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
        BubbleWidthCap(fraction: TranscriptGrouping.Layout.maxWidthPhone, trailing: own) {
            if let onPress, !own {
                VStack(alignment: .leading, spacing: 4) {
                    if let name {
                        nameLine(name)
                            .contentShape(Rectangle())
                            .onTapGesture(perform: onPress)
                            .accessibilityAddTraits(.isButton)
                            .accessibilityIdentifier("message-bubble-link")
                    }
                    bubble
                }
            } else {
                bubble
            }
        }
        .accessibilityIdentifier(own ? "user-message-bubble" : "chat-bubble")
    }
}

public extension MessageBubble where Footer == BubbleTime {
    /// The plain bubble: its `HH:mm` (nil = none) bottom-right inside.
    init(
        side: MessageBubbleSide = .other,
        author: String? = nil,
        time: String?,
        onPress: (() -> Void)? = nil,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.init(
            side: side,
            author: author,
            onPress: onPress,
            hasFooter: !(time ?? "").isEmpty,
            content: content,
            footer: { BubbleTime(text: time ?? "") }
        )
    }
}

/// A bubble's muted `HH:mm`.
public struct BubbleTime: View {
    let text: String

    public init(text: String) {
        self.text = text
    }

    public var body: some View {
        Text(text)
            .font(.caption2)
            .monospacedDigit()
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }
}

/// #EXP-1736: a sender's avatar, THE Avatar at its default 32: a member's
/// initials in their hue (or their picture), an agent's mark, a session's
/// glyph on the muted disc.
public enum SenderAvatarModel: Equatable, Sendable {
    case member(user: UserEntity?, id: String)
    case agent(UserAvatarAgent)
    case session
}

public struct SenderAvatar: View {
    let model: SenderAvatarModel

    public init(_ model: SenderAvatarModel) {
        self.model = model
    }

    public var body: some View {
        let size = TranscriptGrouping.Layout.avatar
        Group {
            switch model {
            case let .member(user, id):
                UserAvatar(user: user, id: id, size: size)
            case let .agent(agent):
                UserAvatar(agent: agent, size: size)
            case .session:
                ZStack {
                    Circle().fill(DesignTokens.Palette.muted)
                    AppIcon(AppIcons.codingAssistant, size: size / 2, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .frame(width: size, height: size)
            }
        }
        .accessibilityHidden(true)
        .accessibilityIdentifier("sender-avatar")
    }
}

/// #EXP-1736: ONE transcript row's frame (web `TranscriptRow`): the gap
/// above it (`layout.gap`), then `own` / `full` across the column, `other`
/// in the bubble column under its sender's avatar column (`avatar` set =
/// the group's first row, top-aligned; later rows keep the same width
/// free). Nil layout = a full row with no gap.
public struct TranscriptRow<Content: View>: View {
    let side: TranscriptGrouping.Side
    let gap: CGFloat
    let avatar: SenderAvatarModel?
    @ViewBuilder let content: () -> Content

    public init(
        side: TranscriptGrouping.Side,
        gap: CGFloat,
        avatar: SenderAvatarModel?,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.side = side
        self.gap = gap
        self.avatar = avatar
        self.content = content
    }

    /// The row as `transcriptLayout` placed it; `avatar` resolves the
    /// sender's avatar (asked only on a row that shows one).
    public init(
        layout: TranscriptGrouping.RowLayout?,
        avatar: (TranscriptGrouping.Sender) -> SenderAvatarModel,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.side = layout?.side ?? .full
        self.gap = layout?.gap ?? 0
        if let layout, layout.avatar, let sender = layout.sender {
            self.avatar = avatar(sender)
        } else {
            self.avatar = nil
        }
        self.content = content
    }

    public var body: some View {
        Group {
            if side == .other {
                HStack(alignment: .top, spacing: TranscriptGrouping.Layout.avatarGap) {
                    if let avatar {
                        SenderAvatar(avatar)
                    } else {
                        Color.clear.frame(width: TranscriptGrouping.Layout.avatar, height: 0)
                    }
                    content()
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                content()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.top, gap)
    }
}

/// The bubble's column: its subviews stacked leading, the LAST one (the
/// footer line) trailing when `trailingLast`, the column as wide as the
/// widest (the bubble hugs its text; no full-width stretch).
struct BubbleColumn: Layout {
    var spacing: CGFloat
    var trailingLast: Bool

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let sizes = subviews.map { $0.sizeThatFits(ProposedViewSize(width: proposal.width, height: nil)) }
        let width = sizes.map(\.width).max() ?? 0
        let height = sizes.map(\.height).reduce(0, +) + spacing * CGFloat(max(0, sizes.count - 1))
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for (index, subview) in subviews.enumerated() {
            let size = subview.sizeThatFits(ProposedViewSize(width: bounds.width, height: nil))
            let trailing = trailingLast && subviews.count > 1 && index == subviews.count - 1
            let x = trailing ? bounds.maxX - size.width : bounds.minX
            subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: size.width, height: size.height))
            y += size.height + spacing
        }
    }
}

/// Caps its one subview at `fraction` of the offered width, placed leading
/// (or trailing); the row itself takes the whole width.
struct BubbleWidthCap: Layout {
    var fraction: CGFloat
    var trailing: Bool

    private func cap(_ width: CGFloat?) -> CGFloat? {
        guard let width, width.isFinite else { return width }
        return width * fraction
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let child = subviews.first else { return .zero }
        let size = child.sizeThatFits(ProposedViewSize(width: cap(proposal.width), height: nil))
        let width = proposal.width.flatMap { $0.isFinite ? $0 : nil } ?? size.width
        return CGSize(width: width, height: size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let child = subviews.first else { return }
        let size = child.sizeThatFits(ProposedViewSize(width: cap(bounds.width), height: nil))
        let x = trailing ? bounds.maxX - size.width : bounds.minX
        child.place(at: CGPoint(x: x, y: bounds.minY), proposal: ProposedViewSize(width: size.width, height: size.height))
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
/// the column, the title (if any) as the plan card's surface caption above it. `animate` = the card ARRIVED while the chat was open (its id
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
        let shape = RoundedRectangle(cornerRadius: PlanCard.Card.surfaceRadius)
        let before = animate && !entered
        let glowing = animate && !settled && !motion.reduceMotion
        let enter = Chats.SurfaceEnter.Frame.self
        // The title = the plan card's surface caption (plan-card.json
        // `card`: 2xs medium muted, `surfaceLabelGap` ABOVE the frame) ×4;
        // the frame = its `surfaceRadius` + `surfacePadding` ×4.
        VStack(alignment: .leading, spacing: PlanCard.Card.surfaceLabelGap) {
            if let title, !title.isEmpty {
                Text(title)
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            content()
                .padding(PlanCard.Card.surfacePadding)
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
        }
        .frame(maxWidth: .infinity, alignment: .leading)
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
