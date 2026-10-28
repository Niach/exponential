import ExpCore
import ExpUI
import SwiftUI

/// What the Chats header's bubbles row reads from `ChatStoriesHost`: the
/// rings (`Stories.rings`, the active team's, ring order), a ring's avatar
/// and the viewer's opener (a ring key).
struct ChatStories {
    let rings: [Stories.StoryRing]
    let avatar: (Stories.StoryRing) -> StoryRingAvatar
    let open: (String) -> Void
}

/// S-3 STORIES (stories.json STORY RING): the Chats list's story rings and
/// THE viewer they open (full screen at that ring). The rings ride the agent
/// bubbles row (chats.json BUBBLES, `AgentBubbles rings`): a bubble wears its
/// agent's ring, the rest follow it; the host paints no row of its own,
/// `content` draws the bubbles from `ChatStories`.
struct ChatStoriesHost<Content: View>: View {
    let teamId: String?
    @ViewBuilder let content: (ChatStories) -> Content

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster

    @State private var model = StoriesModel()
    @State private var now = Date()
    @State private var opened: StoryViewerTarget?
    /// The session the header's tap opens once the viewer is gone.
    @State private var pendingSession: Stories.Story?

    var body: some View {
        let rings = model.rings(now: now, teamId: teamId)
        // A VStack, not a Group: its tasks must run while the row is empty.
        VStack(spacing: 0) {
            content(ChatStories(rings: rings, avatar: model.avatar) { key in
                opened = StoryViewerTarget(key: key)
            })
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .task(id: accountId) { model.bind(accountId: accountId, db: deps.db) }
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                now = Date()
            }
        }
        .fullScreenCover(item: $opened, onDismiss: openPendingSession) { target in
            StoryViewer(
                rings: rings,
                startKey: target.key,
                card: { model.card($0) },
                avatar: model.avatar,
                onSeen: markSeen,
                onAnswer: answer,
                onOpenSession: { story in
                    pendingSession = story
                    opened = nil
                },
                onClose: { opened = nil }
            )
            .environment(deps)
            .environment(\.accountId, accountId)
        }
    }

    private func openPendingSession() {
        guard let story = pendingSession else { return }
        pendingSession = nil
        pushRoute(.thread(
            accountId: accountId,
            threadId: story.rootId,
            child: story.sessionId == story.rootId ? nil : .session(story.sessionId)
        ))
    }

    private func markSeen(_ story: Stories.Story) {
        let api = deps.sessionsApi
        let accountId = accountId
        Task { try? await api.markStoriesSeen(accountId: accountId, sessionId: story.rootId, at: story.createdAt) }
    }

    private func answer(_ card: ThreadCardAnswer) async -> Bool {
        do {
            try await deps.sessionsApi.answer(
                accountId: accountId, messageId: card.messageId, answer: card.answer, decision: card.decision,
                fresh: card.fresh, feedback: card.feedback, answers: card.answers, surfaceData: card.surfaceData
            )
            return true
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "answer", thing: card.decision == nil ? "the question" : "the plan"),
                description: error.userFacingMessage
            )
            return false
        }
    }
}

private struct StoryViewerTarget: Identifiable {
    let key: String
    var id: String { key }
}

/// The rings side by side (THE ring button, the bubbles' metrics): the
/// story-ring specimen's row.
struct StoryRingStrip: View {
    let rings: [Stories.StoryRing]
    let avatar: (Stories.StoryRing) -> StoryRingAvatar
    let onOpen: (String) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: ChatListMetrics.bubbleSpacing) {
                ForEach(rings) { ring in
                    StoryRingButton(ring: ring, onOpen: onOpen) { StoryRingAvatarView(avatar: avatar(ring)) }
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 2)
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
