import ExpCore
import ExpUI
import SwiftUI

/// Thread 58cb32df (chats.json MENUS, ×4): a chat's menu on a phone — THE
/// Menu in its SHEET look titled with the chat's title (a row's or a
/// bubble's long press, the chat header's `…`). The rows are
/// `threadMenuEntries` (Stop · Stop and delete · Delete) or
/// `agentMenuEntries` (Compact conversation · New conversation · Stop ·
/// Agent settings). A pick hands the entry back; the host runs it once the
/// sheet has dismissed (a confirm or a push from under a dismissing sheet is
/// dropped).
struct ChatMenuSheet: View {
    enum Entry: Hashable {
        case thread(Chats.ThreadMenuEntry)
        case agent(Chats.AgentMenuEntry)
        /// EXP-1307: the Guide's Close PR (the thread header's `…` keeps it).
        case closePr
    }

    let title: String?
    let entries: [Entry]
    let onPick: (Entry) -> Void

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
                ForEach(Array(entries.enumerated()), id: \.element) { index, entry in
                    if index > 0, Self.separates(entries[index - 1], entry) {
                        GlassDivider().padding(.vertical, 4)
                    }
                    Button {
                        onPick(entry)
                    } label: {
                        GlassMenuItemLabel(Self.label(entry), icon: Self.icon(entry), destructive: Self.destructive(entry))
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("chat-menu-\(Self.testId(entry))")
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.top, title == nil ? GlassSheetTokens.headerTopPadding : 0)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat-menu-sheet")
        }
    }

    /// The agent menu's sends sit apart from Stop / Agent settings (the
    /// draft's separator); the Guide's Close PR from the chat's entries.
    static func separates(_ previous: Entry, _ next: Entry) -> Bool {
        switch (previous, next) {
        case (.agent(.newConversation), .agent): true
        case (.thread, .closePr), (.closePr, .thread): true
        default: false
        }
    }

    static func label(_ entry: Entry) -> String {
        switch entry {
        case let .thread(e): e.label
        case let .agent(e): e.label
        case .closePr: ClosePrCopy.menuItem
        }
    }

    static func icon(_ entry: Entry) -> String {
        switch entry {
        case let .thread(e): AppIcons.conceptOrName(e.icon) ?? e.icon
        case let .agent(e): AppIcons.conceptOrName(e.icon) ?? e.icon
        case .closePr: AppIcons.prClosed
        }
    }

    static func destructive(_ entry: Entry) -> Bool {
        switch entry {
        case let .thread(e): e.destructive
        case .agent: false
        case .closePr: true
        }
    }

    static func testId(_ entry: Entry) -> String {
        switch entry {
        case let .thread(e): e.rawValue
        case let .agent(e): e.rawValue
        case .closePr: "close-pr"
        }
    }
}

/// The chats' actions ×4 (chats.json): Stop = `sessions.end` on the ROOT
/// (the whole tree), Stop and delete / Delete = `sessions.delete`, the agent
/// menu's Compact / New conversation SEND the builtin lines, its Stop =
/// `sessions.stop` (the turn interrupt; an undelivered stop toasts). A
/// failure = a toast (`copy.toasts`).
@MainActor
struct ChatActions {
    let deps: AppDependencies
    let accountId: String
    let toaster: Toaster

    func stop(rootId: String) {
        let api = deps.sessionsApi
        Task {
            do {
                try await api.end(accountId: accountId, sessionId: rootId)
            } catch {
                toaster.error(Chats.Copy.Toasts.stopFailed, description: error.userFacingMessage)
            }
        }
    }

    /// Resolves true once the server deleted the chat.
    func delete(rootId: String, done: @escaping @MainActor (Bool) -> Void = { _ in }) {
        let api = deps.sessionsApi
        Task {
            do {
                try await api.delete(accountId: accountId, sessionId: rootId)
                done(true)
            } catch {
                toaster.error(Chats.Copy.Toasts.deleteFailed, description: error.userFacingMessage)
                done(false)
            }
        }
    }

    /// #EXP-1388: the selection bar's actions (Delete runs after its
    /// confirm through `deleteAll`). Mark read / unread and (un)archive batch
    /// in one call; Stop ends each chat. A failure = ONE toast.
    func run(_ item: Chats.ChatSelectionItem) {
        let api = deps.sessionsApi
        let accountId = accountId
        let toaster = toaster
        let ids = item.sessionIds
        Task {
            var failed: Error?
            switch item.action {
            case .markRead, .markUnread:
                do { try await api.setRead(accountId: accountId, sessionIds: ids, read: item.action == .markRead) } catch { failed = error }
            case .archive, .unarchive:
                do { try await api.setArchived(accountId: accountId, sessionIds: ids, archived: item.action == .archive) } catch { failed = error }
            case .stop:
                for id in ids {
                    do { try await api.end(accountId: accountId, sessionId: id) } catch { failed = error }
                }
            case .delete:
                for id in ids {
                    do { try await api.delete(accountId: accountId, sessionId: id) } catch { failed = error }
                }
            }
            if let failed {
                toaster.error(Chats.Copy.Toasts.selectionFailed, description: failed.userFacingMessage)
            }
        }
    }

    /// #EXP-1388: the confirmed multi-delete (`sessions.delete` per chat).
    func deleteAll(rootIds: [String]) {
        run(Chats.ChatSelectionItem(action: .delete, sessionIds: rootIds))
    }

    /// The TURN interrupt (composer stop, the agent menu's Stop).
    func interrupt(sessionId: String) {
        let api = deps.sessionsApi
        Task {
            do {
                let result = try await api.stop(accountId: accountId, sessionId: sessionId)
                if result.delivered == false {
                    toaster.warning(Chats.Copy.Toasts.stopNotDelivered)
                }
            } catch {
                toaster.error(Chats.Copy.Toasts.stopFailed, description: error.userFacingMessage)
            }
        }
    }

    /// Compact / New conversation into the caller's conversation with the
    /// agent — the same path a typed `/compact` or `/new` takes.
    func sendToAgent(agentId: String, entry: Chats.AgentMenuEntry) {
        guard let text = Chats.agentMenuSends[entry] else { return }
        let api = deps.sessionsApi
        Task {
            do {
                _ = try await api.send(accountId: accountId, .agent(agentId: agentId, text: text))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the message"),
                    description: error.userFacingMessage
                )
            }
        }
    }
}

extension View {
    /// `copy.confirmDelete` ×4: the destructive confirm before a delete.
    func chatDeleteConfirm(target: Binding<String?>, onConfirm: @escaping (String) -> Void) -> some View {
        chatDeleteConfirm(
            targets: Binding(
                get: { target.wrappedValue.map { [$0] } },
                set: { target.wrappedValue = $0?.first }
            ),
            onConfirm: { ids in if let id = ids.first { onConfirm(id) } }
        )
    }

    /// #EXP-1388: the same confirm for the selection's Delete, titled
    /// `chatDeleteTitle(n)`.
    func chatDeleteConfirm(targets: Binding<[String]?>, onConfirm: @escaping ([String]) -> Void) -> some View {
        background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: Binding(
                    get: { targets.wrappedValue.map(ChatDeleteTarget.init) },
                    set: { targets.wrappedValue = $0?.ids }
                )) { item in
                    GlassAlert(
                        title: Chats.chatDeleteTitle(item.ids.count),
                        message: Chats.Copy.ConfirmDelete.body,
                        actions: [
                            GlassAlertAction(PullRequestStack.cancelLabel, role: .outline, isCancel: true, id: "cancel") {},
                            GlassAlertAction(Chats.Copy.Menu.delete, role: .destructive, id: "delete") {
                                onConfirm(item.ids)
                            },
                        ]
                    )
                }
        }
    }
}

private struct ChatDeleteTarget: Identifiable {
    let ids: [String]
    var id: String { ids.joined(separator: ",") }
}
