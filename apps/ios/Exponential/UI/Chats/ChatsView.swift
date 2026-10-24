import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// Thread 58cb32df (chats.json, #EXP-1373): the phone's CHATS tab — the
/// large inline title "Chats" with the search and INBOX circles (a dot while
/// any of the active team's notifications is unread; the inbox is a pushed
/// page), the team agents as BUBBLES, the filter chips All · Needs you ·
/// Running · Unread, then ONE flat WhatsApp-like list of the ACTIVE team's
/// chats (`Chats.chatList` over `partitionThreads`: no bands, no nesting,
/// newest activity first). A row opens the chat; a long press opens THE
/// menu's sheet titled with the chat (Stop · Stop and delete, or Delete); a
/// bubble's hold its agent menu.
struct ChatsView: View {
    /// The Chats header's inbox dot.
    var inboxUnread = false

    @Environment(AppDependencies.self) private var deps
    @Environment(SessionListsModel.self) private var lists
    @Environment(TeamState.self) private var teamState
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster

    @State private var filter: Chats.ChatFilter = .all
    @State private var isOwner = false
    /// A row's or a bubble's menu sheet, and its pick (run on dismiss).
    @State private var menu: ChatMenuTarget?
    @State private var pendingPick: (target: ChatMenuTarget, entry: ChatMenuSheet.Entry)?
    @State private var deleteTarget: String?
    @State private var agentSettings: AgentSettingsTarget?
    /// The clock the time labels read (a minute tick keeps "Yesterday" true).
    @State private var now = Date()

    var body: some View {
        ZStack {
            AppBackground()
            if lists.loaded {
                content
            } else {
                // EXP-1275 (D8): the first load is skeleton rows.
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationTitle(Chats.Copy.title)
        .toolbarTitleDisplayMode(.inlineLarge)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .ownChromeToolbarItem(placement: .topBarTrailing) {
            HStack(spacing: 10) {
                NavigationLink(value: AppRoute.search) {
                    Self.headerCircle(AppIcons.navSearch, dot: false)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Search")
                .accessibilityIdentifier("chats-search")
                NavigationLink(value: AppRoute.myWork) {
                    Self.headerCircle(AppIcons.navInbox, dot: inboxUnread)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Chats.Copy.inbox)
                .accessibilityIdentifier(Chats.TestIds.inboxButton)
            }
        }
        .sheet(item: $menu, onDismiss: runPendingPick) { target in
            ChatMenuSheet(title: target.title, entries: target.entries) { entry in
                pendingPick = (target, entry)
                menu = nil
            }
        }
        .chatDeleteConfirm(target: $deleteTarget) { rootId in actions.delete(rootId: rootId) }
        .navigationDestination(item: $agentSettings) { target in
            AgentSettingsPage(teamId: target.teamId, agentId: target.agentId, isOwner: true)
                .environment(\.accountId, accountId)
        }
        .task(id: teamState.activeTeamId) { await resolveOwner() }
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                now = Date()
            }
        }
        .accessibilityIdentifier(SessionTree.ListCopy.listTestId)
    }

    private nonisolated(unsafe) static let iso = ISO8601DateFormatter()

    private var actions: ChatActions {
        ChatActions(deps: deps, accountId: accountId, toaster: toaster)
    }

    /// The header's two trailing circles (search, the inbox with its dot).
    static func headerCircle(_ icon: String, dot: Bool) -> some View {
        AppIcon(icon, size: AppIcon.Size.large)
            .foregroundStyle(.white)
            .frame(width: 40, height: 40)
            .background(GlassTokens.opaqueCardFill, in: Circle())
            .overlay(Circle().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline))
            .overlay(alignment: .topTrailing) {
                if dot {
                    Circle()
                        .fill(DesignTokens.Semantic.blue)
                        .frame(width: 9, height: 9)
                        .offset(x: -2, y: 2)
                        .accessibilityLabel("Unread")
                }
            }
            .contentShape(Circle())
    }

    // MARK: - List

    private var list: Chats.ChatList {
        Chats.chatList(
            groups: lists.teamGroups(teamState.activeTeamId),
            messages: lists.chatMessages,
            reads: lists.reads,
            prStates: lists.prStates,
            words: lists.words,
            filter: filter,
            now: Self.iso.string(from: now),
            utcOffsetMinutes: TimeZone.current.secondsFromGMT(for: now) / 60
        )
    }

    private var bubbles: [AgentBubbleItem] {
        let agents = lists.agents.filter { $0.teamId == teamState.activeTeamId }
        return Chats.agentBubbles(
            agents: agents,
            sessions: lists.sessions,
            messages: lists.chatMessages,
            reads: lists.reads,
            me: deps.auth.userId
        ).map { bubble in
            AgentBubbleItem(
                id: bubble.agent.agentId,
                name: bubble.agent.name,
                icon: BoardTypeDisplay.iconName(for: bubble.agent.icon) ?? SessionTree.ListCopy.agentDefaultIcon,
                color: Color(hex: bubble.agent.color)
                    ?? Color(hex: SessionTree.ListCopy.agentDefaultColor) ?? .white,
                ring: bubble.ring == .needsYou ? .needsYou : bubble.ring == .unread ? .unread : .none
            )
        }
    }

    @ViewBuilder
    private var content: some View {
        let list = list
        let bubbles = bubbles
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if !bubbles.isEmpty {
                    AgentBubbles(
                        items: bubbles,
                        onTap: { agentId in pushRoute(.agentChannel(accountId: accountId, agentId: agentId)) },
                        onHold: openAgentMenu
                    )
                    .padding(.horizontal, -16)
                    .padding(.top, 4)
                    .padding(.bottom, 14)
                }
                filterChips(list.counts)
                    .padding(.bottom, 4)
                if list.rows.isEmpty {
                    EmptyState(.threads)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 48)
                } else {
                    ForEach(list.rows) { row in
                        chatRow(row)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .scrollContentBackground(.hidden)
        .tabBarBottomInset()
    }

    private func filterChips(_ counts: [Chats.ChatFilter: Int]) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(Chats.filters, id: \.self) { option in
                    GlassPill(
                        Chats.chatFilterLabel(option, counts: counts),
                        size: .md,
                        mode: .select(isSelected: filter == option) { filter = option }
                    )
                    .accessibilityIdentifier("\(Chats.TestIds.chatFilter)-\(option.rawValue)")
                }
            }
            .padding(.horizontal, 16)
        }
        .padding(.horizontal, -16)
    }

    private func chatRow(_ row: Chats.ChatRow) -> some View {
        let label: String? = switch row.preview.kind {
        case .question: Chats.Copy.Preview.question.components(separatedBy: "{text}").first
        case .plan: Chats.Copy.Preview.plan.components(separatedBy: "{text}").first?
            .trimmingCharacters(in: .whitespaces)
        default: nil
        }
        return ChatRowView(
            title: row.title,
            time: row.time,
            preview: row.preview.text,
            previewLabel: label?.trimmingCharacters(in: .whitespaces),
            previewItalic: row.preview.kind == .working,
            needsYou: row.needsYou,
            unreadCount: row.unreadCount,
            word: row.word?.text,
            wordIcon: row.word == .stack ? AppIcons.prStack : nil
        ) {
            ChatLeadMark(lead: row.lead, cli: lists.clis[row.sessionId])
        }
        .glassLongPress(
            enabled: canSteer(row.sessionId),
            onTap: { pushRoute(.thread(accountId: accountId, threadId: row.sessionId)) },
            onHold: { openThreadMenu(row) }
        )
        .accessibilityIdentifier(Chats.TestIds.chatRow)
    }

    // MARK: - Menus

    /// The starter or a team owner steers a chat (its menu); anyone else
    /// gets none.
    private func canSteer(_ rootId: String) -> Bool {
        isOwner || (lists.rootStarter[rootId] != nil && lists.rootStarter[rootId] == deps.auth.userId)
    }

    private func openThreadMenu(_ row: Chats.ChatRow) {
        let entries = Chats.threadMenuEntries(
            rootStatus: lists.rootStatus[row.sessionId] ?? "", canSteer: canSteer(row.sessionId)
        )
        guard !entries.isEmpty else { return }
        menu = ChatMenuTarget(id: row.sessionId, title: row.title, entries: entries.map { .thread($0) })
    }

    private func openAgentMenu(_ agentId: String) {
        guard let agent = lists.agents.first(where: { $0.agentId == agentId }) else { return }
        let entries = Chats.agentMenuEntries(busy: agent.busy, isOwner: isOwner)
        menu = ChatMenuTarget(id: agentId, title: agent.name, entries: entries.map { .agent($0) }, agent: true)
    }

    private func runPendingPick() {
        guard let pick = pendingPick else { return }
        pendingPick = nil
        switch pick.entry {
        case .thread(.stop):
            actions.stop(rootId: pick.target.id)
        case .thread(.stopAndDelete), .thread(.delete):
            deleteTarget = pick.target.id
        case let .agent(entry):
            let agentId = pick.target.id
            switch entry {
            case .compact, .newConversation:
                actions.sendToAgent(agentId: agentId, entry: entry)
            case .stop:
                if let conversation = lists.agentConversations[agentId] {
                    actions.interrupt(sessionId: conversation)
                }
            case .settings:
                if let teamId = teamState.activeTeamId {
                    agentSettings = AgentSettingsTarget(teamId: teamId, agentId: agentId)
                }
            }
        case .closePr:
            break
        }
    }

    /// Owners own the destructive surface (`resolveTeamAccess`).
    private func resolveOwner() async {
        guard let teamId = teamState.activeTeamId,
              let pool = try? deps.db.pool(forAccountId: accountId) else {
            isOwner = false
            return
        }
        let userId = deps.auth.userId
        isOwner = (try? await pool.read { db in
            guard let userId else { return false }
            return try TeamMemberEntity
                .filter(Column("team_id") == teamId && Column("user_id") == userId)
                .fetchOne(db)?.role == "owner"
        }) ?? false
    }
}

struct ChatMenuTarget: Identifiable {
    let id: String
    let title: String
    let entries: [ChatMenuSheet.Entry]
    var agent = false
}

private struct AgentSettingsTarget: Hashable, Identifiable {
    let teamId: String
    let agentId: String
    var id: String { agentId }
}

/// A chat row's lead in its 52pt disc (`lead`): needs you = the brand mark
/// with the amber badge, working = the working mark, else the root's PR
/// glyph (open emerald, merged violet), else the bare mark.
struct ChatLeadMark: View {
    let lead: Chats.ChatLead
    var cli: String?

    var body: some View {
        switch lead {
        case .needsYou:
            BeatingAgentMark(agent: cli, beating: false)
                .frame(width: 26, height: 26)
                .overlay(alignment: .topTrailing) {
                    Circle()
                        .fill(DesignTokens.Semantic.yellow)
                        .frame(width: 10, height: 10)
                        .offset(x: 4, y: -4)
                }
        case .working:
            AgentWorkingMark(agent: cli)
                .frame(width: 22, height: 22)
        case .open:
            AppIcon(AppIcons.prOpen, size: 24)
                .foregroundStyle(DesignTokens.Semantic.green)
        case .merged:
            AppIcon(AppIcons.prMerged, size: 24)
                .foregroundStyle(DesignTokens.Avatar.violet)
        case .waiting:
            BeatingAgentMark(agent: cli, beating: false)
                .frame(width: 24, height: 24)
        }
    }
}
