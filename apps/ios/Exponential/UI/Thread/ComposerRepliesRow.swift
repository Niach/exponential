import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1211: the agent's quick replies (`sessions_suggest` `replies`)
/// directly above THE session composer, thread and raw session view alike
/// (web `ComposerReplies`, fixtures/session-thread.json `repliesCases`): ONE
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
                    GlassPill(item.text, mode: .action { onSend(item) }, enabled: !disabled)
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
        return items.isEmpty ? nil : ComposerReplies(messageId: replies.messageId, items: items)
    }

    var body: some View {
        // A real container, never a `Group`: an empty Group is no view, so
        // the observing `.task` below would never start.
        VStack(spacing: 0) {
            if let replies {
                ComposerRepliesRow(
                    replies: replies,
                    disabled: inFlight,
                    onSend: { send(replies.messageId, $0) },
                    onDismiss: { dismiss(replies.messageId) }
                )
                .transition(.opacity)
            }
        }
        .animation(motion.standard, value: replies)
        .task(id: sessionId) {
            guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
            let id = sessionId
            let observation = ValueObservation.tracking { db in
                try SessionQueries.messages(db: db, sessionId: id)
            }
            do {
                for try await rows in observation.values(in: pool) {
                    messages = rows.map { TreeMessage(messageRow: $0) }
                }
            } catch {}
        }
    }

    private func send(_ messageId: String, _ item: ComposerReply) {
        guard !inFlight else { return }
        inFlight = true
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await api.sendReply(accountId: accountId, messageId: messageId, itemId: item.id)
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

    private func dismiss(_ messageId: String) {
        guard !inFlight else { return }
        inFlight = true
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await api.dismissReplies(accountId: accountId, messageId: messageId)
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
