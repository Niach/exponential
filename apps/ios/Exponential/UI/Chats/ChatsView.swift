import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// Session 58cb32df (chats.json, #EXP-1373): the phone's CHATS tab — the
/// page header (#EXP-1384 `shell.header`: the large title "Chats", the
/// native search and INBOX circles), then #FEED-191 the Needs you entry
/// (needs-you.json, none = no row), the Agents section, (#EXP-1733) the Apps
/// section and the ACTIVE team's
/// chats grouped (`Chats.chatSections` over `Chats.chatList`: Needs you ·
/// Working · Unread · Chats, no bands, no nesting, each newest activity first).
/// A row opens the chat; an agent row's hold THE session screen menu
/// (#FEED-239, kind agent / assistant; its settings open that agent's chat
/// with its settings over it). #EXP-1388 SELECTION + ARCHIVE: a row's long press ENTERS the
/// multi-selection with that row (the row's touch menu is gone); while
/// selecting nothing shifts, a selected row is a full-width tint with the
/// check badge on its disc, a tap
/// toggles and THE BAR (the issue list's bulk bar, in the tab bar's slot)
/// acts on the selection (`chatSelectionActions`). The Archived entry row,
/// first under the Chats header, pushes the same list in `view: .archived`
/// (the Archived page: flat, no headers).
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

    @State private var isOwner = false
    /// A row's or an agent row's menu sheet, and its pick (run on dismiss).
    @State private var menu: ChatMenuTarget?
    @State private var pendingPick: (target: ChatMenuTarget, entry: ChatMenuSheet.Entry)?
    @State private var deleteTarget: String?
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
        .task(id: teamState.activeTeamId) { await resolveOwner() }
        // S-3: the viewer's Assistant exists once the list mounts (once per
        // team per app run).
        .task(id: "\(accountId)/\(teamState.activeTeamId ?? "")") {
            if view == .list, let teamId = teamState.activeTeamId {
                await AssistantEnsure.run(deps, accountId: accountId, teamId: teamId)
            }
        }
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
        lists.chatList(
            teamId: teamState.activeTeamId,
            view: view,
            now: Self.iso.string(from: now),
            utcOffsetMinutes: TimeZone.current.secondsFromGMT(for: now) / 60
        )
    }

    /// The active team's agents in `listAgents` order (the viewer's
    /// Assistant first; another member's never). #EXP-1714: the viewer's
    /// global Assistant (its team never syncs) lists in every team.
    private var teamAgents: [AgentRow] {
        AgentSettings.agentsInTeam(
            lists.agents, teamId: teamState.activeTeamId, teamIds: teamState.teams.map(\.id)
        )
    }

    /// S-3: the AGENTS section pinned on top (`agentChatRows`).
    private var agentRows: [Chats.AgentChatRow] {
        Chats.agentChatRows(
            agents: teamAgents,
            sessions: lists.sessions,
            messages: lists.chatMessages,
            reads: lists.reads,
            me: deps.auth.userId,
            view: view,
            now: Self.iso.string(from: now),
            utcOffsetMinutes: TimeZone.current.secondsFromGMT(for: now) / 60
        )
    }

    /// #EXP-1733: the APPS section between the Agents and the sections
    /// (`appChatRows`): the active team's live apps; none on Archived.
    private var appRows: [Chats.AppChatRow] {
        Chats.appChatRows(
            apps: teamState.activeTeamId.flatMap { lists.appsByTeam[$0] } ?? [],
            sessions: lists.sessions,
            messages: lists.chatMessages,
            reads: lists.reads,
            me: deps.auth.userId,
            view: view,
            now: Self.iso.string(from: now),
            utcOffsetMinutes: TimeZone.current.secondsFromGMT(for: now) / 60
        )
    }

    /// #FEED-191: the displayed order: the sections', or the Archived page's flat
    /// rows. Selection reads it too.
    private func displayed(_ list: Chats.ChatList) -> [Chats.ChatRow] {
        view == .archived ? list.rows : Chats.chatSections(list.rows).flatMap(\.rows)
    }

    @ViewBuilder
    private var content: some View {
        let list = list
        let agentRows = agentRows
        let appRows = appRows
        let archivedEntry = view == .list && list.archived > 0
        let needsYouCount = view == .list
            ? lists.needsYou(teamId: teamState.activeTeamId, me: deps.auth.userId, now: now).count
            : 0
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                // #FEED-191 (needs-you.json THE ENTRY): the first row; none = no row.
                if needsYouCount > 0 {
                    NeedsYouEntryRow(count: needsYouCount)
                        .glassLongPress(enabled: false, onTap: {
                            guard !selectionActive else { return }
                            pushRoute(.needsYou(accountId: accountId))
                        }, onHold: {})
                }
                if !agentRows.isEmpty {
                    ChatSectionHeader(Chats.Copy.Sections.agents)
                    ForEach(agentRows) { row in
                        agentChatRow(row)
                    }
                }
                if !appRows.isEmpty {
                    ChatSectionHeader(Chats.Copy.Sections.apps)
                    ForEach(appRows) { row in
                        appChatRow(row)
                    }
                }
                if list.rows.isEmpty, agentRows.isEmpty, appRows.isEmpty, !archivedEntry {
                    Group {
                        if view == .archived {
                            EmptyState(title: Chats.Copy.Archived.empty, icon: AppIcons.uiArchive)
                        } else {
                            EmptyState(.chats)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.top, 48)
                } else if view == .archived {
                    ForEach(list.rows) { row in
                        chatRow(row)
                    }
                } else {
                    let sections = Chats.chatSections(list.rows)
                    ForEach(sections) { section in
                        ChatSectionHeader(Chats.Copy.Sections.title(section.key))
                        if section.key == .chats, archivedEntry {
                            archivedRow(list.archived)
                        }
                        ForEach(section.rows) { row in
                            chatRow(row)
                        }
                    }
                    // The Archived entry stays first under its header, also
                    // with no other chat in that section.
                    if archivedEntry, !sections.contains(where: { $0.key == .chats }) {
                        ChatSectionHeader(Chats.Copy.Sections.chats)
                        archivedRow(list.archived)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .scrollContentBackground(.hidden)
        .tabBarBottomInset()
    }

    /// #EXP-1388 THE ARCHIVED ENTRY: first under the Chats header.
    private func archivedRow(_ count: Int) -> some View {
        ChatArchivedRow(count: count)
            .glassLongPress(enabled: false, onTap: {
                guard !selectionActive else { return }
                pushRoute(.archivedChats(accountId: accountId))
            }, onHold: {})
    }

    /// The preview's leading `Question:` / `Plan:` label.
    private func previewLabel(_ kind: Chats.ChatPreviewKind) -> String? {
        let label: String? = switch kind {
        case .question: Chats.Copy.Preview.question.components(separatedBy: "{text}").first
        case .plan: Chats.Copy.Preview.plan.components(separatedBy: "{text}").first
        default: nil
        }
        return label?.trimmingCharacters(in: .whitespaces)
    }

    /// S-3: an agent row = a chat row whose lead disc holds the agent's
    /// avatar (the Assistant = the brand mark), the viewer's conversation's
    /// preview / time / unread; never dim, never selectable. A tap opens
    /// the conversation, a hold the agent menu.
    private func agentChatRow(_ row: Chats.AgentChatRow) -> some View {
        ChatRowView(
            title: row.name,
            time: row.time,
            preview: row.preview.text,
            previewLabel: previewLabel(row.preview.kind),
            previewItalic: row.preview.kind == .working || row.preview.kind == .locked,
            needsYou: row.needsYou,
            unreadCount: row.unreadCount
        ) {
            AgentChatLead(row: row)
        }
        .glassLongPress(onTap: {
            guard !selectionActive else { return }
            pushRoute(.agentChannel(accountId: accountId, agentId: row.agentId))
        }, onHold: {
            guard !selectionActive else { return }
            openAgentMenu(row.agentId)
        })
        .accessibilityIdentifier(Chats.TestIds.chatRow)
    }

    /// #EXP-1733: an app row = the agent row's look with the app's glyph in
    /// the disc, the viewer's session with the app for preview / time /
    /// unread; never selectable, no menu. A tap opens the app's chat (its
    /// editor, the Apps menu's Edit).
    private func appChatRow(_ row: Chats.AppChatRow) -> some View {
        ChatRowView(
            title: row.name,
            time: row.time,
            preview: row.preview.text,
            previewLabel: previewLabel(row.preview.kind),
            previewItalic: row.preview.kind == .working || row.preview.kind == .locked,
            needsYou: row.needsYou,
            unreadCount: row.unreadCount
        ) {
            AgentChatLead(app: row)
        }
        .glassLongPress(enabled: false, onTap: {
            guard !selectionActive else { return }
            pushRoute(.app(accountId: accountId, appId: row.appId))
        }, onHold: {})
        .accessibilityIdentifier(Chats.TestIds.appRow)
    }

    private func chatRow(_ row: Chats.ChatRow) -> some View {
        let selected = selectionActive && selectedIds.contains(row.sessionId)
        return ChatRowView(
            title: row.title,
            time: row.time,
            preview: row.preview.text,
            previewLabel: previewLabel(row.preview.kind),
            previewItalic: row.preview.kind == .working || row.preview.kind == .locked,
            needsYou: row.needsYou,
            unreadCount: row.unreadCount,
            word: row.word?.text,
            wordIcon: row.word == .stack ? AppIcons.prStack : nil,
            selected: selected
        ) {
            ChatLeadMark(lead: row.lead, cli: lists.clis[row.sessionId])
        }
        // S-3: a DONE chat (every PR merged, nothing works) stays, dimmed.
        .opacity(row.dim ? Chats.Shell.dimOpacity : 1)
        .chatSelectedTint(selected)
        // The issue row's tap + long-press (`bulkSelectable`): the same hold,
        // the release never toggles the row back out, ONE haptic (the list's).
        .bulkSelectable(
            selecting: selectionActive,
            onTap: {
                if selectionActive {
                    toggleSelection(row.sessionId)
                } else {
                    pushRoute(.chat(accountId: accountId, chatId: row.sessionId))
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

    /// `chatSelectionActions` over the selected rows (#FEED-191: chats.json
    /// selection order = the displayed order).
    private var selectionItems: [Chats.ChatSelectionItem] {
        let selection: [Chats.ChatSelectionRow] = displayed(list).compactMap { row in
            guard selectedIds.contains(row.sessionId) else { return nil }
            let id = row.sessionId
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

    /// #FEED-239: an agent row's menu = THE session screen menu (kind agent
    /// / assistant, never a phone's Publish): Compact · New conversation for
    /// the member, the settings for an agent's owners / the Assistant's.
    private func openAgentMenu(_ agentId: String) {
        guard let agent = lists.agents.first(where: { $0.agentId == agentId }) else { return }
        let kind: SessionScreen.Kind = agent.assistant ? .assistant : .agent
        let entries = SessionScreen.menu(
            kind: kind, phone: false, member: true, linkIssue: false, settings: agent.assistant || isOwner,
            publish: false, export: false, delete: false
        )
        menu = ChatMenuTarget(
            id: agentId, title: agent.name, entries: ChatMenuSheet.screenEntries(entries, kind: kind), agent: true
        )
    }

    private func runPendingPick() {
        guard let pick = pendingPick else { return }
        pendingPick = nil
        switch pick.entry {
        case .chat(.delete):
            deleteTarget = pick.target.id
        case let .agent(entry):
            let agentId = pick.target.id
            switch entry {
            case .compact, .newConversation:
                actions.sendToAgent(agentId: agentId, entry: entry)
            case .settings:
                pushRoute(.agentChannel(accountId: accountId, agentId: agentId, settings: true))
            }
        case let .screen(entry, _):
            let agentId = pick.target.id
            switch entry {
            case .compact, .newConversation:
                if let send = Chats.AgentMenuEntry(rawValue: entry.rawValue) {
                    actions.sendToAgent(agentId: agentId, entry: send)
                }
            case .settings:
                // #FEED-239: never the Settings screen: the agent's chat
                // with its settings opened over it.
                pushRoute(.agentChannel(accountId: accountId, agentId: agentId, settings: true))
            case .delete:
                deleteTarget = pick.target.id
            case .linkIssue, .publish, .export, .separator:
                break
            }
        case .checks, .mergeNow, .hold, .resume, .closePr:
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

/// A chat row's lead in its 52pt disc (`lead`): needs you = the chat glyph
/// (`shell.chatLead`) with the amber badge, working = the working mark, else
/// the root's PR glyph (open emerald, merged violet), else the bare chat
/// glyph. S-3: never the brand mark (that is the Assistant's avatar now).
struct ChatLeadMark: View {
    let lead: Chats.ChatLead
    var cli: String?
    /// The glyph's side: the list's 52pt disc, or the header's smaller one.
    var size: CGFloat = 24

    static var chatGlyph: String {
        AppIcons.conceptOrName(Chats.Shell.chatLead) ?? AppIcons.actionChat
    }

    var body: some View {
        switch lead {
        case .needsYou:
            AppIcon(Self.chatGlyph, size: size)
                .foregroundStyle(.white)
                .needsYouBadge()
        case .working:
            AgentWorkingMark(agent: cli)
                .frame(width: size * 22 / 24, height: size * 22 / 24)
        case .open:
            AppIcon(AppIcons.prOpen, size: size)
                .foregroundStyle(DesignTokens.Semantic.green)
        case .merged:
            AppIcon(AppIcons.prMerged, size: size)
                .foregroundStyle(DesignTokens.Avatar.violet)
        case .waiting:
            AppIcon(Self.chatGlyph, size: size)
                .foregroundStyle(.white)
        }
    }
}

/// S-3: an agent row's lead = the agent's AVATAR (the Assistant = the
/// Exponential brand mark in the foreground colour, never its stored
/// icon/colour; an agent = its icon in its colour), the amber badge while
/// it needs the viewer. #EXP-1733: an app row's lead = the app's board
/// glyph in its colour (the Apps list's `AppListEntry.glyph` fallback and
/// `AppGlyph.neutralColor` when unset), the same badge.
struct AgentChatLead: View {
    private let icon: String
    private let color: Color
    private let assistant: Bool
    private let needsYou: Bool

    init(row: Chats.AgentChatRow) {
        icon = BoardTypeDisplay.iconName(for: row.icon) ?? SessionTree.ListCopy.agentDefaultIcon
        color = Color(hex: row.color) ?? Color(hex: SessionTree.ListCopy.agentDefaultColor) ?? .white
        assistant = row.assistant
        needsYou = row.needsYou
    }

    init(app row: Chats.AppChatRow) {
        icon = BoardTypeDisplay.iconName(for: row.icon) ?? AppIcons.navApps
        color = Color(hex: row.color ?? AppGlyph.neutralColor) ?? .white
        assistant = false
        needsYou = row.needsYou
    }

    var body: some View {
        Group {
            if assistant {
                ExpLogoMark(size: 26, color: .white)
            } else {
                AppIcon(icon, size: 26, weight: .medium)
                    .foregroundStyle(color)
            }
        }
        .needsYouBadge(needsYou)
    }
}

private extension View {
    /// The amber needs-you badge on a lead's top-trailing corner.
    func needsYouBadge(_ shown: Bool = true) -> some View {
        overlay(alignment: .topTrailing) {
            if shown {
                Circle()
                    .fill(DesignTokens.Semantic.yellow)
                    .frame(width: 10, height: 10)
                    .offset(x: 4, y: -4)
            }
        }
    }
}
