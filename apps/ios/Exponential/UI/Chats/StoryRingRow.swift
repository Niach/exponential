import ExpCore
import ExpUI
import SwiftUI

/// S-3 STORIES (stories.json): the Chats list's row of story rings — one
/// per agent and per root chat with stories (`Stories.rings`, the active
/// team's), the ring's avatar over its title; a tap opens THE viewer full
/// screen at that ring. Nothing (no row) while there is no ring. Interim:
/// it sits above the agent bubbles until the bubbles wear their own rings.
struct StoryRingRow: View {
    let teamId: String?

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
            if !rings.isEmpty {
                StoryRingStrip(rings: rings, avatar: model.avatar) { key in
                    opened = StoryViewerTarget(key: key)
                }
                .padding(.top, 4)
                .padding(.bottom, 10)
            }
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

/// The rings side by side (the bubbles' metrics): the ring around its
/// avatar, the title under it, waiting / unseen titles in full white.
struct StoryRingStrip: View {
    let rings: [Stories.StoryRing]
    let avatar: (Stories.StoryRing) -> StoryRingAvatar
    let onOpen: (String) -> Void

    var body: some View {
        let size = ChatListMetrics.bubble
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: ChatListMetrics.bubbleSpacing) {
                ForEach(rings) { ring in
                    Button { onOpen(ring.key) } label: {
                        VStack(spacing: 6) {
                            StoryRing(stories: ring.stories, size: size) { StoryRingAvatarView(avatar: avatar(ring)) }
                            Text(ring.title)
                                .font(.subheadline.weight(ring.unseen ? .medium : .regular))
                                .foregroundStyle(.white.opacity(ring.unseen ? 1 : TextOpacity.secondary))
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .frame(maxWidth: size + 12)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(ring.title)
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
