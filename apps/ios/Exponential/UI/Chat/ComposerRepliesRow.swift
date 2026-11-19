import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1211: the agent's quick replies (`sessions_suggest` `replies`)
/// directly above THE session composer, chat and raw session view alike
/// (web `ComposerReplies`, fixtures/session-chat.json `repliesCases`): ONE
/// sideways-scrolling row of small action pills, a trailing dismiss icon
/// button. A tap sends that reply as the next message.
struct ComposerRepliesRow: View {
    let replies: ComposerReplies
    /// A call is out: every pill waits.
    var disabled: Bool = false
    let onSend: (ComposerReply) -> Void
    let onDismiss: () -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(replies.items) { item in
                    // Floats over the transcript: the opaque card under the glass.
                    GlassPill(item.text, mode: .action { onSend(item) }, isOpaque: true, enabled: !disabled)
                        .frame(maxWidth: 260)
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("\(SessionTree.RepliesCopy.testId)-\(item.id)")
                }
                Button(action: onDismiss) {
                    AppIcon(AppIcons.uiClose, size: 12)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(disabled)
                .accessibilityLabel(SessionTree.RepliesCopy.dismiss)
                .accessibilityIdentifier("\(SessionTree.RepliesCopy.testId)-dismiss")
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 4)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(SessionTree.RepliesCopy.label)
        .accessibilityIdentifier(SessionTree.RepliesCopy.testId)
    }
}

/// The live row for one session: observes its synced `messages`, shows
/// `composerReplies` (nothing when none is left), and runs
/// `sessions.sendReply` / `sessions.dismissReplies`; a refusal toasts.
/// #EXP-1958: a SEALED card's reply leaves sealed through the composer's
/// writer (`sessions.send` + `cardItem`), its dismiss names the open replies;
/// one this install cannot open sends nothing (`CardItemWire`).
struct SessionComposerReplies: View {
    let sessionId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.motion) private var motion
    @State private var messages: [TreeMessage] = []
    @State private var inFlight = false
    /// Sent / dismissed ahead of the Electric echo (keyed on the card).
    @State private var sentIds: Set<String> = []
    @State private var dismissedId: String?

    private var replies: ComposerReplies? {
        guard let replies = SessionTree.composerReplies(sessionId: sessionId, messages: messages),
              replies.messageId != dismissedId
        else { return nil }
        let items = replies.items.filter { !sentIds.contains("\(replies.messageId)/\($0.id)") }
        return items.isEmpty
            ? nil
            : ComposerReplies(
                messageId: replies.messageId, items: items,
                sessionId: replies.sessionId, cardSealStatus: replies.cardSealStatus
            )
    }

    var body: some View {
        // A real container, never a `Group`: an empty Group is no view, so
        // the observing `.task` below would never start.
        VStack(spacing: 0) {
            if let replies {
                ComposerRepliesRow(
                    replies: replies,
                    disabled: inFlight,
                    onSend: { send(replies, $0) },
                    onDismiss: { dismiss(replies) }
                )
                .transition(.opacity)
            }
        }
        .animation(motion.standard, value: replies)
        .task(id: sessionId) {
            guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
            let id = sessionId
            // Large chats: only the card rows (a `replies` card is one),
            // opened off main, never the whole conversation.
            let observation = ValueObservation.tracking { db in
                try SessionQueries.cardRows(db: db, sessionId: id).map(TreeMessage.init(messageRow:))
            }
            do {
                for try await rows in observation.values(in: pool) {
                    messages = rows
                }
            } catch {}
        }
    }

    private func send(_ replies: ComposerReplies, _ item: ComposerReply) {
        guard !inFlight else { return }
        inFlight = true
        let api = deps.sessionsApi
        let accountId = accountId
        let messageId = replies.messageId
        let target = replies.sessionId ?? sessionId
        Task {
            do {
                switch CardItemWire.wire(replies.cardSealStatus, action: .reply) {
                case .server, .stamp:
                    try await api.sendReply(accountId: accountId, messageId: messageId, itemId: item.id)
                case .sealed:
                    try await api.send(
                        accountId: accountId, CardItemWire.message(sessionId: target, text: item.text),
                        cardItem: CardItemRef(messageId: messageId, itemId: item.id)
                    )
                case .refused:
                    throw SealRefusal()
                }
                sentIds.insert("\(messageId)/\(item.id)")
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the reply"),
                    description: error.userFacingMessage
                )
            }
            inFlight = false
        }
    }

    private func dismiss(_ replies: ComposerReplies) {
        guard !inFlight else { return }
        inFlight = true
        let api = deps.sessionsApi
        let accountId = accountId
        let messageId = replies.messageId
        Task {
            do {
                switch CardItemWire.wire(replies.cardSealStatus, action: .dismissReplies) {
                case .server, .sealed:
                    try await api.dismissReplies(accountId: accountId, messageId: messageId)
                case .stamp:
                    try await api.dismissReplies(accountId: accountId, messageId: messageId, itemIds: replies.items.map(\.id))
                case .refused:
                    throw SealRefusal()
                }
                dismissedId = messageId
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "dismiss", thing: "the quick replies"),
                    description: error.userFacingMessage
                )
            }
            inFlight = false
        }
    }
}
