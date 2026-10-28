import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// Thread 58cb32df (chats.json, #EXP-1373): the phone's CHATS tab — the
/// page header (#EXP-1384 `shell.header`: the large title "Chats", the
/// native search and INBOX circles), the team agents as BUBBLES, the filter chips All · Needs you ·
/// Running · Unread, then ONE flat WhatsApp-like list of the ACTIVE team's
/// chats (`Chats.chatList` over `partitionThreads`: no bands, no nesting,
/// newest activity first). A row opens the chat; a bubble's hold its agent
/// menu. #EXP-1388 SELECTION + ARCHIVE: a row's long press ENTERS the
/// multi-selection with that row (the row's touch menu is gone); while
/// selecting nothing shifts, a selected row is a full-width tint with the
/// check badge on its disc, a tap
/// toggles and THE BAR (the issue list's bulk bar, in the tab bar's slot)
/// acts on the selection (`chatSelectionActions`). The Archived entry row
/// pushes the same list in `view: .archived` (the Archived page).
struct ChatsView: View {
    /// #EXP-1388: `.archived` = the pushed Archived page.
    var view: Chats.ChatListView = .list

    @Environment(AppDependencies.self) private var deps
    @Environment(\.motion) private var motion
    @Environment(TabBarChrome.self) private var tabBarChrome: TabBarChrome?
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
    /// #EXP-1388: the multi-selection (selection order kept for the bar).
    @State private var selectionActive = false
    @State private var selectedIds: [String] = []
    /// The bar's Delete confirm (`chatDeleteTitle(n)`).
    @State private var deleteSelection: [String]?

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
        .pageHeader(view == .archived ? Chats.Copy.Archived.title : Chats.Copy.Headers.chats, pushed: view == .archived)
        // #EXP-1388 THE BAR: the issue list's bulk bar in the tab bar's slot
        // (the tab bar stands down while it is up).
        .overlay(alignment: .bottom) {
            VStack(spacing: 8) {
                if selectionActive {
                    ChatSelectionBar(
                        count: selectedIds.count,
                        items: selectionItems,
                        onClose: exitSelection,
                        onAction: runSelection
                    )
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 4)
        }
        .onChange(of: selectionActive) { _, active in
            tabBarChrome?.suppressed = active
        }
        .onAppear { tabBarChrome?.suppressed = selectionActive }
        .onDisappear { tabBarChrome?.suppressed = false }
        .sensoryFeedback(.impact(weight: .medium), trigger: selectionActive) { _, entered in entered }
        // The selection keeps only rows still in the list.
        .onChange(of: list.rows.map(\.sessionId)) { _, ids in
            guard selectionActive else { return }
            let kept = selectedIds.filter { ids.contains($0) }
            if kept.isEmpty { exitSelection() } else if kept != selectedIds { selectedIds = kept }
        }
        .chatDeleteConfirm(targets: $deleteSelection) { ids in
            exitSelection()
            actions.deleteAll(rootIds: ids)
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

    // MARK: - List

    private var list: Chats.ChatList {
        Chats.chatList(
            groups: lists.teamGroups(teamState.activeTeamId),
            messages: lists.chatMessages,
            reads: lists.reads,
            prStates: lists.prStates,
            words: lists.words,
            filter: filter,
            view: view,
            now: Self.iso.string(from: now),
            utcOffsetMinutes: TimeZone.current.secondsFromGMT(for: now) / 60,
            // #EXP-1399 review: an ask mirror of a session I cannot see shows no text.
            visible: Set(lists.sessions.map(\.id))
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
        let bubbles = view == .archived ? [] : bubbles
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                // S-3 STORIES: the story rings, above the bubbles (hidden without a ring).
                if view == .list {
                    StoryRingRow(teamId: teamState.activeTeamId).padding(.horizontal, -16)
                }
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
                // #EXP-1388 THE ARCHIVED ENTRY: first while archived > 0 and
                // the filter is All.
                if view == .list, list.archived > 0, filter == .all {
                    ChatArchivedRow(count: list.archived)
                        .glassLongPress(enabled: false, onTap: {
                            guard !selectionActive else { return }
                            pushRoute(.archivedChats(accountId: accountId))
                        }, onHold: {})
                }
                if list.rows.isEmpty {
                    Group {
                        if view == .archived {
                            EmptyState(title: Chats.Copy.Archived.empty, icon: AppIcons.uiArchive)
                        } else {
                            EmptyState(.threads)
                        }
                    }
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
        let selected = selectionActive && selectedIds.contains(row.sessionId)
        return ChatRowView(
            title: row.title,
            time: row.time,
            preview: row.preview.text,
            previewLabel: label?.trimmingCharacters(in: .whitespaces),
            previewItalic: row.preview.kind == .working,
            needsYou: row.needsYou,
            unreadCount: row.unreadCount,
            word: row.word?.text,
            wordIcon: row.word == .stack ? AppIcons.prStack : nil,
            selected: selected
        ) {
            ChatLeadMark(lead: row.lead, cli: lists.clis[row.sessionId])
        }
        .chatSelectedTint(selected)
        // The issue row's tap + long-press (`bulkSelectable`): the same hold,
        // the release never toggles the row back out, ONE haptic (the list's).
        .bulkSelectable(
            selecting: selectionActive,
            onTap: {
                if selectionActive {
                    toggleSelection(row.sessionId)
                } else {
                    pushRoute(.thread(accountId: accountId, threadId: row.sessionId))
                }
            },
            onLongPress: { enterSelection(with: row.sessionId) }
        )
        .accessibilityIdentifier(Chats.TestIds.chatRow)
    }

    // MARK: - Selection (#EXP-1388)

    private func enterSelection(with rootId: String) {
        guard !selectionActive else { return }
        withAnimation(motion.standard) {
            selectionActive = true
            selectedIds = [rootId]
        }
    }

    private func toggleSelection(_ rootId: String) {
        if let index = selectedIds.firstIndex(of: rootId) {
            selectedIds.remove(at: index)
        } else {
            selectedIds.append(rootId)
        }
        // Deselecting the last row leaves the selection.
        if selectedIds.isEmpty { exitSelection() }
    }

    private func exitSelection() {
        withAnimation(motion.standard) {
            selectionActive = false
            selectedIds = []
        }
    }

    /// `chatSelectionActions` over the selected rows (selection order).
    private var selectionItems: [Chats.ChatSelectionItem] {
        let rows = list.rows
        let selection: [Chats.ChatSelectionRow] = selectedIds.compactMap { id in
            guard let row = rows.first(where: { $0.sessionId == id }) else { return nil }
            return Chats.ChatSelectionRow(
                sessionId: id,
                rootStatus: lists.rootStatus[id] ?? "",
                unreadCount: row.unreadCount,
                canSteer: canSteer(id),
                canDelete: canDelete(id)
            )
        }
        return Chats.chatSelectionActions(rows: selection, view: view)
    }

    /// Every action leaves the selection; Delete asks first.
    private func runSelection(_ item: Chats.ChatSelectionItem) {
        if item.action == .delete {
            deleteSelection = item.sessionIds
            return
        }
        exitSelection()
        actions.run(item)
    }

    // MARK: - Menus

    /// The viewer started the chat.
    private func isStarter(_ rootId: String) -> Bool {
        lists.rootStarter[rootId] != nil && lists.rootStarter[rootId] == deps.auth.userId
    }

    /// web `canSteerSession`: a shared chat, or the viewer's own; anyone
    /// else gets no menu.
    private func canSteer(_ rootId: String) -> Bool {
        isStarter(rootId) || lists.sessions.contains { $0.id == rootId && $0.shared }
    }

    /// The starter or a team owner deletes a chat (`sessions.delete`).
    private func canDelete(_ rootId: String) -> Bool {
        isOwner || isStarter(rootId)
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
