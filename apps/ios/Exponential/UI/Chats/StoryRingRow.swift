import ExpCore
import ExpUI
import SwiftUI

/// The rings side by side (THE ring button, THE STORY ROW's geometry): the
/// story-ring specimen's row.
struct StoryRingStrip: View {
    let rings: [Stories.StoryRing]
    let avatar: (Stories.StoryRing) -> StoryRingAvatar
    let onOpen: (String) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: CGFloat(Stories.Row.gap)) {
                ForEach(rings) { ring in
                    StoryRingButton(ring: ring, onOpen: onOpen) { StoryRingAvatarView(avatar: avatar(ring)) }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, CGFloat(Stories.Row.padBottom))
        }
    }
}

/// A ring's avatar (stories.json AVATARS): an agent ring = the agent's icon
/// in its colour on the muted disc (the bubble's look); a chat ring = the
/// chat row's BARE brand mark on the muted disc (chats.json LIST lead: no
/// badge, no PR glyph).
enum StoryRingAvatar: Equatable, Sendable {
    /// The agent's icon name and colour (hex).
    case agent(icon: String, color: String?)
    /// The root's CLI (nil = claude, the chat row's rule).
    case chat(cli: String?)
}

/// THE ring avatar view.
struct StoryRingAvatarView: View {
    let avatar: StoryRingAvatar

    var body: some View {
        switch avatar {
        case let .agent(icon, color):
            StoryAvatarMark(icon: icon, color: color.flatMap { Color(hex: $0) })
        case let .chat(cli):
            GeometryReader { proxy in
                ZStack {
                    Circle().fill(DesignTokens.Palette.muted)
                    BeatingAgentMark(agent: cli, beating: false)
                        .frame(width: proxy.size.width * 0.45, height: proxy.size.width * 0.45)
                }
            }
            .accessibilityHidden(true)
        }
    }
}
