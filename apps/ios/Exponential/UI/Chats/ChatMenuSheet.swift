import ExpCore
import ExpUI
import SwiftUI

/// Session 58cb32df (chats.json MENUS, ×4): a chat's menu on a phone — THE
/// Menu in its SHEET look titled with the chat's title (a row's or a
/// bubble's long press, the session screen header's `…`). #FEED-239: the
/// header's and an agent row's rows are THE session screen menu
/// (`SessionScreen.menu`, `screen` entries: Publish · Compact conversation ·
/// New conversation · Settings · Export · Delete, a divider between groups),
/// the chat PR's entries after them. A pick hands the entry back; the host
/// runs it once the sheet has dismissed (a confirm or a push from under a
/// dismissing sheet is dropped).
struct ChatMenuSheet: View {
    enum Entry: Hashable {
        case chat(Chats.ChatMenuEntry)
        case agent(Chats.AgentMenuEntry)
        /// #FEED-239: THE session screen menu's entry for `kind`
        /// (`SessionScreen.menu`; never the separator).
        case screen(SessionScreen.MenuEntry, kind: String)
        /// #EXP-1611: the Guide PR's Checks submenu (pr-checks.json CHECKS
        /// LIST), a page pushed in this sheet.
        case checks
        /// #EXP-1710 (merge-train.json MENU): an owner's Merge now.
        case mergeNow
        /// #EXP-1710: a member's Hold auto-merge / Resume auto-merge.
        case hold
        case resume
        /// EXP-1307: the Guide's Close PR (the chat header's `…` keeps it).
        case closePr

        /// The chat PR's entries (`mergeMenuEntries`), apart from the chat's.
        var isPullRequest: Bool {
            switch self {
            case .chat, .agent, .screen: false
            case .checks, .mergeNow, .hold, .resume, .closePr: true
            }
        }
    }

    let title: String?
    let entries: [Entry]
    /// #EXP-1611: the `checks` entry's summary and its page's rows.
    var checksSummary: PrChecks.Summary?
    var checkRows: [PrChecks.ListRow] = []
    let onPick: (Entry) -> Void

    /// The pushed Checks page.
    @State private var checksOpen: Bool
    /// #EXP-1710: opened by the status pill: the check rows ARE the menu
    /// (no back row to the `…` entries).
    private let checksOnly: Bool
    @Environment(\.motion) private var motion

    /// `startOnChecks` (#EXP-1710): the status pill's tap opens the sheet as
    /// the check rows alone.
    init(
        title: String?,
        entries: [Entry],
        checksSummary: PrChecks.Summary? = nil,
        checkRows: [PrChecks.ListRow] = [],
        startOnChecks: Bool = false,
        onPick: @escaping (Entry) -> Void
    ) {
        self.title = title
        self.entries = entries
        self.checksSummary = checksSummary
        self.checkRows = checkRows
        self.onPick = onPick
        _checksOpen = State(initialValue: startOnChecks)
        checksOnly = startOnChecks
    }

    var body: some View {
        Group {
            if checksOpen {
                GlassSheetChrome(title: nil) {
                    VStack(spacing: 0) {
                        if !checksOnly {
                            PrChecksMenuBackRow { checksOpen = false }
                        }
                        PrChecksMenuRows(rows: checkRows)
                    }
                    .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
                    .padding(.top, GlassSheetTokens.headerTopPadding)
                    .padding(.bottom, 16)
                }
                .transition(.move(edge: .trailing))
            } else {
                rows
                    .transition(.move(edge: .leading))
            }
        }
        .animation(motion.standard, value: checksOpen)
    }

    private var rows: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
                ForEach(Array(entries.enumerated()), id: \.element) { index, entry in
                    if index > 0, Self.separates(entries[index - 1], entry) {
                        GlassDivider().padding(.vertical, 4)
                    }
                    Button {
                        if entry == .checks {
                            checksOpen = true
                        } else {
                            onPick(entry)
                        }
                    } label: {
                        if entry == .checks, let checksSummary {
                            PrChecksMenuEntryLabel(summary: checksSummary)
                        } else {
                            GlassMenuItemLabel(Self.label(entry), icon: Self.icon(entry), destructive: Self.destructive(entry))
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(Self.accessibilityId(entry))
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.top, title == nil ? GlassSheetTokens.headerTopPadding : 0)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat-menu-sheet")
        }
    }

    /// The agent menu's sends sit apart from Agent settings (the
    /// draft's separator); the Guide's Close PR from the chat's entries.
    static func separates(_ previous: Entry, _ next: Entry) -> Bool {
        switch (previous, next) {
        case (.agent(.newConversation), .agent): true
        // #FEED-239: `SessionScreen.menu`'s separators sit between its groups.
        case let (.screen(a, _), .screen(b, _)): screenGroup(a) != screenGroup(b)
        // #EXP-1611/#EXP-1710: the chat PR's entries (Checks, Merge now,
        // Hold | Resume, Close PR) sit apart from the chat's own.
        default: previous.isPullRequest != next.isPullRequest
        }
    }

    /// THE session screen menu as sheet entries: its separators become the
    /// dividers `separates` draws between groups.
    static func screenEntries(_ entries: [SessionScreen.MenuEntry], kind: SessionScreen.Kind) -> [Entry] {
        entries.filter { $0 != .separator }.map { .screen($0, kind: kind.rawValue) }
    }

    /// `SessionScreen.menu`'s groups: link an issue · publish · compact +
    /// new conversation · settings · export + delete.
    private static func screenGroup(_ entry: SessionScreen.MenuEntry) -> Int {
        switch entry {
        case .linkIssue: -1
        case .publish: 0
        case .compact, .newConversation: 1
        case .settings: 2
        case .export, .delete, .separator: 3
        }
    }

    static func label(_ entry: Entry) -> String {
        switch entry {
        case let .chat(e): e.label
        case let .agent(e): e.label
        case let .screen(e, kind): SessionScreen.menuLabel(kind: kind, entry: e)
        case .checks: PrChecks.Copy.checks
        case .mergeNow: MergeTrain.Copy.Menu.mergeNow
        case .hold: MergeTrain.Copy.Menu.hold
        case .resume: MergeTrain.Copy.Menu.resume
        case .closePr: ClosePrCopy.menuItem
        }
    }

    static func icon(_ entry: Entry) -> String {
        switch entry {
        case let .chat(e): AppIcons.conceptOrName(e.icon) ?? e.icon
        case let .agent(e): AppIcons.conceptOrName(e.icon) ?? e.icon
        case let .screen(e, _): e.iconConcept.flatMap { AppIcons.conceptOrName($0) } ?? AppIcons.uiMore
        case .checks: AppIcons.ciPassed
        case .mergeNow: AppIcons.prMerged
        case .hold: AppIcons.prHold
        case .resume: AppIcons.prResume
        case .closePr: AppIcons.prClosed
        }
    }

    static func destructive(_ entry: Entry) -> Bool {
        switch entry {
        case let .chat(e): e.destructive
        case let .screen(e, _): e.destructive
        case .agent, .checks, .mergeNow, .hold, .resume: false
        case .closePr: true
        }
    }

    static func testId(_ entry: Entry) -> String {
        switch entry {
        case let .chat(e): e.rawValue
        case let .agent(e): e.rawValue
        case let .screen(e, _): e.rawValue
        case .checks: "checks"
        case .mergeNow: "merge-now"
        case .hold: "hold"
        case .resume: "resume"
        case .closePr: "close-pr"
        }
    }

    /// The fixtures' test ids (pr-checks.json, merge-train.json), else
    /// `chat-menu-<entry>`.
    static func accessibilityId(_ entry: Entry) -> String {
        switch entry {
        case .checks: PrChecks.TestIds.checksMenu
        case .mergeNow: MergeTrain.TestIds.mergeNow
        case .hold: MergeTrain.TestIds.hold
        case .resume: MergeTrain.TestIds.resume
        // An app's entries keep the app menu's ids.
        case .screen(.publish, _): Apps.TestIds.publish
        case .screen(.export, _): AppExport.TestIds.menuItem
        case let .screen(e, kind) where kind == SessionScreen.Kind.app.rawValue: "app-menu-\(e.rawValue)"
        default: "chat-menu-\(testId(entry))"
        }
    }
}

/// The chats' actions ×4 (chats.json): Delete = `sessions.delete` (S-3: no
/// Stop), the agent menu's Compact / New conversation SEND the builtin
/// lines. A failure = a toast (`copy.toasts`).
@MainActor
struct ChatActions {
    let deps: AppDependencies
    let accountId: String
    let toaster: Toaster

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
    /// (200 ids a call). A failure = ONE
    /// toast, `chatSelectionFailedTitle` (`{n} of {m}` when only some failed).
    func run(_ item: Chats.ChatSelectionItem) {
        let api = deps.sessionsApi
        let accountId = accountId
        let toaster = toaster
        let ids = item.sessionIds
        Task {
            var failed: Error?
            var failures = 0
            switch item.action {
            case .markRead, .markUnread:
                do { try await api.setRead(accountId: accountId, sessionIds: ids, read: item.action == .markRead) } catch { failed = error }
            case .archive, .unarchive:
                do { try await api.setArchived(accountId: accountId, sessionIds: ids, archived: item.action == .archive) } catch { failed = error }
            case .delete:
                for id in ids {
                    do { try await api.delete(accountId: accountId, sessionId: id) } catch { failed = failed ?? error; failures += 1 }
                }
            }
            if let failed {
                toaster.error(
                    Chats.chatSelectionFailedTitle(failed: failures, total: ids.count),
                    description: failed.userFacingMessage
                )
            }
        }
    }

    /// #EXP-1388: the confirmed multi-delete (`sessions.delete` per chat).
    func deleteAll(rootIds: [String]) {
        run(Chats.ChatSelectionItem(action: .delete, sessionIds: rootIds))
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
