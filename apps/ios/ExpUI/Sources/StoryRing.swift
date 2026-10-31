import ExpCore
import SwiftUI

// S-3 STORIES ×4 (fixtures/stories.json `ring` + `progress`, styleguide
// `story-ring` / `story-viewer`; web, desktop and Android draw the same two
// primitives): THE story ring around an avatar and THE story progress bar on
// top of the viewer. Every number comes from `Stories.Ring` /
// `Stories.Progress`.

public extension Stories.Story {
    /// The story's arc colour: waiting = the ask dock's plan/question tint,
    /// unseen = the foreground, seen = the glass-stroke hairline.
    var arcColor: Color {
        if waiting { return (kind == .question ? Dock.Tint.question : Dock.Tint.plan).color }
        return seen ? GlassTokens.strokeCard : .white
    }
}

/// An avatar inside a ring split into one arc per story, `Stories.Ring.gap`
/// degrees apart (one story = a closed circle), `Stories.Ring.width` wide,
/// starting at twelve o'clock and running clockwise in story order.
public struct StoryRing<Avatar: View>: View {
    let colors: [Color]
    let size: CGFloat
    let avatar: Avatar

    public init(stories: [Stories.Story], size: CGFloat, @ViewBuilder avatar: () -> Avatar) {
        self.init(colors: stories.map(\.arcColor), size: size, avatar: avatar)
    }

    /// The arcs' colours in order (a specimen's static ring).
    public init(colors: [Color], size: CGFloat, @ViewBuilder avatar: () -> Avatar) {
        self.colors = colors
        self.size = size
        self.avatar = avatar()
    }

    /// The background gap between the ring and the avatar (the bubbles' 3pt).
    static var inset: CGFloat { CGFloat(Stories.Ring.width) + 3 }

    public var body: some View {
        let width = CGFloat(Stories.Ring.width)
        ZStack {
            ForEach(Array(colors.enumerated()), id: \.offset) { index, color in
                StoryArc(index: index, count: colors.count)
                    .stroke(color, style: StrokeStyle(lineWidth: width, lineCap: .butt))
            }
            .padding(width / 2)
            avatar
                .frame(width: size - 2 * Self.inset, height: size - 2 * Self.inset)
                .clipShape(Circle())
        }
        .frame(width: size, height: size)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(Stories.TestIds.ring)
    }
}

/// One ring as a button (THE STORY ROW's item, `StoryRowItem`): the ring
/// around its avatar, the title under it; a tap hands the ring's key up. The
/// Chats header's trailing rings (the root rings and an agent ring with no
/// bubble) and the story-ring specimen.
public struct StoryRingButton<Avatar: View>: View {
    let ring: Stories.StoryRing
    let onOpen: (String) -> Void
    let avatar: Avatar

    public init(ring: Stories.StoryRing, onOpen: @escaping (String) -> Void, @ViewBuilder avatar: () -> Avatar) {
        self.ring = ring
        self.onOpen = onOpen
        self.avatar = avatar()
    }

    public var body: some View {
        Button { onOpen(ring.key) } label: {
            StoryRowItem(title: ring.title, unseen: ring.unseen) {
                StoryRing(stories: ring.stories, size: ChatListMetrics.bubble) { avatar }
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(ring.title)
    }
}

/// One story's arc of the ring.
struct StoryArc: Shape {
    let index: Int
    let count: Int

    func path(in rect: CGRect) -> Path {
        var path = Path()
        guard count > 0 else { return path }
        let radius = min(rect.width, rect.height) / 2
        let center = CGPoint(x: rect.midX, y: rect.midY)
        if count == 1 {
            path.addEllipse(in: CGRect(x: center.x - radius, y: center.y - radius, width: 2 * radius, height: 2 * radius))
            return path
        }
        let gap = Stories.Ring.gap
        let sweep = (360 - gap * Double(count)) / Double(count)
        let start = -90 + gap / 2 + Double(index) * (sweep + gap)
        path.addArc(
            center: center, radius: radius,
            startAngle: .degrees(start), endAngle: .degrees(start + sweep), clockwise: false
        )
        return path
    }
}

/// An agent ring's avatar: the agent's icon in its colour on the muted disc
/// (the chats header's bubble); nil colour = the icon muted.
public struct StoryAvatarMark: View {
    let icon: String
    let color: Color?

    public init(icon: String, color: Color?) {
        self.icon = icon
        self.color = color
    }

    public var body: some View {
        GeometryReader { proxy in
            ZStack {
                Circle().fill(DesignTokens.Palette.muted)
                AppIcon(icon, size: proxy.size.width * 0.42, weight: .medium)
                    .foregroundStyle(color ?? .white.opacity(TextOpacity.secondary))
            }
        }
        .accessibilityHidden(true)
    }
}

/// THE story progress (#EXP-1493): ONE segment per story of the ring,
/// `Stories.Progress.gap` apart, `height` tall; done = foreground, the
/// current = its fill `fraction` (a gallery: (picture + its fraction) / n),
/// todo = the glass-stroke track.
public struct StoryProgress: View {
    let count: Int
    let index: Int
    let fraction: Double

    public init(count: Int, index: Int, fraction: Double) {
        self.count = count
        self.index = index
        self.fraction = fraction
    }

    public var body: some View {
        let height = CGFloat(Stories.Progress.height)
        HStack(spacing: 0) {
            ForEach(0..<max(count, 0), id: \.self) { segment in
                let fill = segment < index ? 1 : segment == index ? min(max(fraction, 0), 1) : 0
                if segment > 0 { Color.clear.frame(width: CGFloat(Stories.Progress.gap), height: height) }
                GeometryReader { proxy in
                    ZStack(alignment: .leading) {
                        Capsule().fill(GlassTokens.strokeCard)
                        Capsule().fill(Color.white).frame(width: proxy.size.width * fill)
                    }
                }
                .frame(height: height)
            }
        }
        .frame(height: height)
        .accessibilityElement(children: .ignore)
        .accessibilityValue("\(min(index + 1, count)) / \(count)")
        .accessibilityIdentifier(Stories.TestIds.progress)
    }
}
