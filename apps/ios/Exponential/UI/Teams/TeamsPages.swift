import ExpCore
import ExpUI
import SwiftUI

// #EXP-2109 (sidebar-tree.json `phone`): THE TEAMS TAB, one page per level:
// Teams › a team (the mixed list in the member's own order) › a folder.
// Read-only: no drag, no folder creation on phones.

/// A tree row's face: the glyph, the name, THE count badge, the chevron (the
/// settings rows' metrics).
struct TreeRowLabel<Glyph: View>: View {
    let title: String
    var detail: String?
    var badge: NavBadges.Value = .none
    @ViewBuilder let glyph: Glyph

    var body: some View {
        HStack(spacing: 12) {
            glyph
                .frame(width: 24, height: 24)
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.body)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                if let detail {
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            CountBadge(badge)
            AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }
}

/// THE Menu in its sheet presentation (the app row's menu alike): a list of
/// rows, the pick handed back; the host acts once the sheet has dismissed.
private struct TeamMenuSheet<Pick: Hashable>: View {
    struct Item: Identifiable {
        let pick: Pick
        let label: String
        let icon: String
        let testId: String
        var children: [Item] = []
        var id: Pick { pick }
    }

    let title: String
    let items: [Item]
    let onPick: (Pick) -> Void

    /// The submenu up in place of the items.
    @State private var open: Item?

    var body: some View {
        GlassSheetChrome(title: open?.label ?? title) {
            VStack(spacing: 0) {
                ForEach(open?.children ?? items) { item in
                    Button {
                        if item.children.isEmpty { onPick(item.pick) } else { open = item }
                    } label: {
                        GlassMenuItemLabel(item.label, icon: item.icon)
                            .overlay(alignment: .trailing) {
                                if !item.children.isEmpty {
                                    AppIcon(AppIcons.uiChevronRight, size: 12)
                                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                        .padding(.trailing, GlassMenuTokens.itemHPadding)
                                }
                            }
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(item.testId)
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("team-menu")
        }
    }
}

// MARK: - Teams

/// TEAMS: the member teams by name, each its mark, its name, its needs-you
/// count and a chevron; the header's + opens `teamsMenu`. With several
/// servers signed in their teams follow, each naming its server; one of
/// another server's activates it first.
struct TeamsView: View {
    /// Another server's team: `(accountId, teamId)`.
    var onSwitchAccount: (String, String) -> Void = { _, _ in }

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(SessionListsModel.self) private var lists
    @Environment(SidebarTreeModel.self) private var tree
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var loader: MultiAccountTeamLoader?
    @State private var menuOpen = false
    @State private var setupPending = false
    @State private var showTeamSetup = false
    @State private var now = Date()

    private var otherServers: [ServerTeamGroup] {
        (loader?.groups ?? []).filter { $0.accountId != accountId }
    }

    var body: some View {
        ZStack {
            AppBackground()
            if teamState.teams.isEmpty, otherServers.isEmpty {
                if tree.loaded {
                    EmptyState(.noTeam, actionLabel: "Create or join a team") { showTeamSetup = true }
                } else {
                    SkeletonRows()
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                }
            } else {
                ScrollView {
                    VStack(spacing: 16) {
                        if !teamState.teams.isEmpty { section(teamState.orderedTeams) }
                        ForEach(otherServers) { group in
                            section(group.teams, server: group)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .padding(.bottom, 16)
                }
            }
        }
        .pageHeader(Chats.Copy.Headers.teams) {
            Button { menuOpen = true } label: { PageHeaderGlyph(icon: AppIcons.uiAdd) }
                .buttonStyle(.plain)
                .accessibilityLabel(SidebarTree.Copy.teams)
                .accessibilityIdentifier("teams-add")
        }
        .sheet(isPresented: $menuOpen, onDismiss: {
            if setupPending {
                setupPending = false
                showTeamSetup = true
            }
        }) {
            TeamMenuSheet(
                title: SidebarTree.Copy.teams,
                items: SidebarTree.teamsMenu.map { item in
                    .init(
                        pick: item,
                        label: item == .newTeam ? SidebarTree.Copy.newTeam : SidebarTree.Copy.joinTeam,
                        icon: item == .newTeam ? AppIcons.uiAdd : AppIcons.uiInvite,
                        testId: item.rawValue
                    )
                }
            ) { _ in
                // ONE create-or-join form answers both.
                setupPending = true
                menuOpen = false
            }
        }
        // Its own node: one presentation per node.
        .background {
            Color.clear.sheet(isPresented: $showTeamSetup) { TeamSetupSheet() }
        }
        .onAppear {
            if loader == nil { loader = MultiAccountTeamLoader(auth: deps.auth, db: deps.db) }
            now = Date()
        }
        .onChange(of: deps.auth.accounts) { _, _ in loader?.refresh() }
        .accessibilityIdentifier("teams-page")
    }

    private func section(_ teams: [TeamEntity], server: ServerTeamGroup? = nil) -> some View {
        VStack(spacing: 0) {
            ForEach(teams, id: \.id) { team in
                Button {
                    if let server {
                        onSwitchAccount(server.accountId, team.id)
                    } else {
                        pushRoute(.team(accountId: accountId, teamId: team.id))
                    }
                } label: {
                    TreeRowLabel(
                        title: team.name,
                        detail: server?.hostname,
                        badge: server == nil
                            ? NavBadges.Value(
                                count: lists.needsYou(teamId: team.id, me: deps.auth.userId, now: now).count,
                                tone: .attention
                            )
                            : .none
                    ) {
                        TeamAvatar(iconUrl: team.iconUrl, size: 24)
                            .foregroundStyle(.white)
                            // Another server's image loads with that account.
                            .environment(\.accountId, server?.accountId ?? accountId)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("team-row-\(team.id)")
            }
        }
        .glassSection()
    }
}

// MARK: - Team, folder

/// A TEAM (`folderId` nil: its `…` menu) or a FOLDER: one page of the mixed
/// list; a folder that is gone pops its page.
struct TeamTreePage: View {
    let teamId: String
    var folderId: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(SessionListsModel.self) private var lists
    @Environment(SidebarTreeModel.self) private var tree
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.dismiss) private var dismiss
    @Environment(\.toaster) private var toaster
    @State private var menuOpen = false
    @State private var pending: MenuPick?
    @State private var showCreateBoard = false
    @State private var newAgent = false
    @State private var newAppOpen = false
    @State private var pendingNewApp: AppsNewMenuSheet.Action?
    @State private var importing = false
    @State private var creatingDraft = false
    @State private var gettingStarted = GettingStartedProgress()

    private enum MenuPick: Hashable {
        case new, board, agent, app, settings
    }

    private var team: TeamEntity? { teamState.team(teamId) }
    private var owner: Bool { tree.isOwner(teamId) }

    private var rows: [SidebarTree.PhoneRow]? {
        tree.page(
            teamId: teamId, folderId: folderId, boards: teamState.boards(of: teamId), sessions: lists.sessions
        )
    }

    var body: some View {
        ZStack {
            AppBackground()
            if !tree.loaded {
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            } else if let rows, !rows.isEmpty {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(rows) { TeamTreeRow(row: $0, teamId: teamId) }
                    }
                    .glassSection()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .padding(.bottom, 16)
                }
            } else if folderId == nil {
                empty
            }
        }
        .pageHeader(
            folderId.map { tree.folderName(teamId: teamId, folderId: $0) } ?? team?.name ?? "",
            pushed: true,
            glyph: folderId == nil ? .team(iconUrl: team?.iconUrl) : .folder
        )
        .toolbar {
            if folderId == nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { menuOpen = true } label: { PageHeaderGlyph(icon: AppIcons.uiMore) }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Team actions")
                        .accessibilityIdentifier("team-actions")
                }
            }
        }
        .onChange(of: tree.loaded && rows == nil, initial: true) { _, gone in
            // The folder is gone (deleted, emptied).
            if gone { dismiss() }
        }
        .sheet(isPresented: $menuOpen, onDismiss: runPending) {
            TeamMenuSheet(title: team?.name ?? "", items: menuItems) { pick in
                pending = pick
                menuOpen = false
            }
        }
        .background {
            Color.clear.sheet(isPresented: $showCreateBoard) {
                CreateBoardSheet(accountId: accountId, teamId: teamId) { boardId in
                    pushRoute(.board(accountId: accountId, id: boardId))
                }
            }
        }
        .background {
            Color.clear.sheet(isPresented: $newAppOpen, onDismiss: runPendingNewApp) {
                AppsNewMenuSheet { action in
                    pendingNewApp = action
                    newAppOpen = false
                }
            }
        }
        .background {
            Color.clear.sheet(isPresented: $importing) {
                AppImportStep(teamId: teamId) { pushRoute(.app(accountId: accountId, appId: $0)) }
            }
        }
        .navigationDestination(isPresented: $newAgent) {
            AgentSettingsPage(teamId: teamId, agentId: nil, isOwner: owner)
                .environment(\.accountId, accountId)
                .bareBackButton()
        }
        .task(id: "\(accountId)/\(teamId)") {
            guard folderId == nil else { return }
            gettingStarted.bind(accountId: accountId, teamId: teamId, deps: deps)
        }
        .onDisappear { gettingStarted.stop() }
        .accessibilityIdentifier(folderId == nil ? "team-page" : "folder-page")
    }

    /// A team with nothing in it: the shared empty state over Getting started.
    private var empty: some View {
        ScrollView {
            VStack(spacing: 24) {
                if owner {
                    EmptyState(.boards) { showCreateBoard = true }
                } else {
                    EmptyState(title: EmptyStateCopy.State.boards.title, icon: AppIcons.navBoards)
                }
                GettingStartedList(
                    progress: gettingStarted,
                    accountId: accountId,
                    teamId: teamId,
                    onCreateBoard: owner ? { showCreateBoard = true } : nil
                )
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
        }
        .scrollBounceBehavior(.basedOnSize)
    }

    // MARK: - Menu (`phoneTeamMenu`)

    private var menuItems: [TeamMenuSheet<MenuPick>.Item] {
        SidebarTree.phoneTeamMenu(owner: owner).map { entry in
            switch entry.id {
            case .new:
                .init(
                    pick: .new, label: SidebarTree.Copy.new, icon: AppIcons.uiAdd, testId: "team-menu-new",
                    children: entry.children.compactMap(newItem)
                )
            case .settings:
                .init(
                    pick: .settings, label: SidebarTree.Copy.settings, icon: AppIcons.navSettings,
                    testId: "team-menu-settings"
                )
            }
        }
    }

    private func newItem(_ kind: SidebarTree.ItemKind) -> TeamMenuSheet<MenuPick>.Item? {
        switch kind {
        case .board:
            .init(pick: .board, label: SidebarTree.Copy.newBoard, icon: AppIcons.navBoards, testId: "team-menu-new-board")
        case .agent:
            .init(pick: .agent, label: SidebarTree.Copy.newAgent, icon: AppIcons.settingsAgents, testId: "team-menu-new-agent")
        case .app:
            .init(pick: .app, label: SidebarTree.Copy.newApp, icon: AppIcons.navApps, testId: "team-menu-new-app")
        case .session: nil
        }
    }

    private func runPending() {
        guard let pick = pending else { return }
        pending = nil
        switch pick {
        case .board: showCreateBoard = true
        case .agent: newAgent = true
        case .app: newAppOpen = true
        case .settings: pushRoute(.teamSettings(accountId: accountId, teamId: teamId))
        case .new: break
        }
    }

    /// VAPP-242: THE NEW MENU's pick, once its sheet is gone.
    private func runPendingNewApp() {
        guard let action = pendingNewApp else { return }
        pendingNewApp = nil
        switch action {
        case .newApp: createDraft()
        case .importApp: importing = true
        }
    }

    /// VAPP-176 NEW APP: a DRAFT (`apps.create {teamId}`, no other field),
    /// then its editor at once; a failure = the failed-action toast.
    private func createDraft() {
        guard !creatingDraft else { return }
        creatingDraft = true
        Task {
            defer { creatingDraft = false }
            do {
                let created = try await deps.appsApi.create(accountId: accountId, AppCreateInput(teamId: teamId))
                pushRoute(.app(accountId: accountId, appId: created.id))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "create", thing: "the app"),
                    description: error.trpcUserMessage
                )
            }
        }
    }
}

/// One row of a team's tree: its own glyph, its name, the agent's or
/// session's badge, a chevron; a tap opens the target.
struct TeamTreeRow: View {
    let row: SidebarTree.PhoneRow
    let teamId: String

    @Environment(TeamState.self) private var teamState
    @Environment(SessionListsModel.self) private var lists
    @Environment(SidebarTreeModel.self) private var tree
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute

    var body: some View {
        Button(action: open) {
            TreeRowLabel(title: title, badge: badge) { TeamTreeGlyph(row: row, size: 20) }
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("tree-row-\(row.kind.rawValue)")
    }

    private var title: String {
        TeamTreeRowValues.title(row, teamState: teamState, lists: lists, tree: tree)
    }

    private var badge: NavBadges.Value {
        TeamTreeRowValues.badge(row, lists: lists)
    }

    private func open() {
        switch row.kind {
        case .folder: pushRoute(.folder(accountId: accountId, teamId: teamId, folderId: row.id))
        case .item(.board): pushRoute(.board(accountId: accountId, id: row.id))
        case .item(.agent): pushRoute(.agentChannel(accountId: accountId, agentId: row.id))
        case .item(.app): pushRoute(.appRun(accountId: accountId, appId: row.id))
        case .item(.session): pushRoute(.chat(accountId: accountId, chatId: row.id))
        }
    }
}

/// What a tree row draws, off the synced rows (the phone's pages and the
/// iPad's sidebar).
@MainActor
enum TeamTreeRowValues {
    static func title(
        _ row: SidebarTree.PhoneRow, teamState: TeamState, lists: SessionListsModel, tree: SidebarTreeModel
    ) -> String {
        switch row.kind {
        case .folder: row.name ?? SidebarFolders.defaultFolderName
        case .item(.board): teamState.boards.first { $0.id == row.id }?.name ?? ""
        case .item(.agent): tree.agent(row.id)?.name ?? ""
        case .item(.app): tree.app(row.id)?.name ?? ""
        case .item(.session): lists.chatRoots[row.id]?.title ?? ""
        }
    }

    static func badge(_ row: SidebarTree.PhoneRow, lists: SessionListsModel) -> NavBadges.Value {
        switch row.kind {
        case .item(.agent):
            lists.agents.first { $0.agentId == row.id }.map(lists.agentBadge) ?? .none
        case .item(.session):
            NavBadges.rowBadge(
                unreadCount: Chats.unreadCount(rootId: row.id, messages: lists.chatMessages, reads: lists.reads),
                needsYou: lists.chatRow(row.id)?.needsYou ?? false
            )
        default: .none
        }
    }
}

/// A tree row's own glyph.
struct TeamTreeGlyph: View {
    let row: SidebarTree.PhoneRow
    var size: CGFloat = 20

    @Environment(TeamState.self) private var teamState
    @Environment(SessionListsModel.self) private var lists
    @Environment(SidebarTreeModel.self) private var tree

    var body: some View {
        switch row.kind {
        case .folder:
            AppIcon(AppIcons.uiFolder, size: size)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        case .item(.board):
            if let board = teamState.boards.first(where: { $0.id == row.id }) {
                AppIcon(BoardTypeDisplay.iconName(for: board), size: size)
                    .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
            }
        case .item(.agent):
            let agent = tree.agent(row.id)
            AppIcon(
                agent.flatMap { BoardTypeDisplay.iconName(for: $0.icon) } ?? SessionTree.ListCopy.agentDefaultIcon, size: size
            )
            .foregroundStyle(agent.flatMap { Color(hex: $0.color) } ?? .white)
        case .item(.app):
            let app = tree.app(row.id)
            AppIcon(app?.icon ?? AppIcons.navApps, size: size)
                .foregroundStyle(app.flatMap { Color(hex: $0.color) } ?? .white)
        case .item(.session):
            let chat = lists.chatRow(row.id)
            ChatLeadMark(
                lead: chat?.needsYou == true ? .needsYou : chat?.busy == true ? .working : .waiting,
                cli: lists.clis[row.id],
                size: size
            )
        }
    }
}
