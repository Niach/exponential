import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// #NEW-12: the iPad at REGULAR width (full screen, the ⅔ split) wears the
/// desktop app's shape: the SIDEBAR (the desktop rail's entries, in its
/// order), a LIST column and a DETAIL column. Compact width (Slide Over, the
/// ⅓ split, every iPhone) keeps the phone layout. Every column hosts the
/// pages the phone already has; only the arrangement is the iPad's.
enum SidebarItem: Hashable {
    case agent, needsYou, chats, devices, drafts, apps, settings
    case agentChannel(String)
    case app(String)
    case board(String)
    case chat(String)

    /// The phone root this entry stands for (the navigator's deep links land
    /// on a `MainTab`; the sidebar follows them).
    var tab: MainTab {
        switch self {
        case .agent: .agent
        case .needsYou, .chats, .agentChannel, .chat: .chats
        case .devices, .settings: .team
        case .drafts, .board: .issues
        case .apps, .app: .apps
        }
    }

    /// The entry a landing on `tab` lights.
    init(_ tab: MainTab, board: CurrentBoardRef?) {
        switch tab {
        case .agent: self = .agent
        case .issues: self = board.map { .board($0.boardId) } ?? .agent
        case .chats: self = .chats
        case .apps: self = .apps
        case .team: self = .settings
        }
    }

    /// The muted glyph of an empty detail column.
    var placeholderIcon: String {
        switch self {
        case .needsYou: AppIcons.navNeedsYou
        case .chats: AppIcons.navChats
        case .drafts: AppIcons.navDrafts
        case .apps: AppIcons.navApps
        case .settings: AppIcons.navSettings
        case .board: AppIcons.navIssues
        default: AppIcons.navIssues
        }
    }
}

/// The three-column layout: three sibling navigation stacks side by side
/// (a `NavigationSplitView` adopts the first stack in its detail column and
/// pushes over the whole column, list included), so the detail pushes on
/// its own and an entry without a list (Agent, Devices, an agent, an app, a
/// session) gives the detail the full width, the desktop main area's.
struct TabletShell: View {
    /// The sidebar's pick count (a pick folds an overlaid sidebar away).
    let picks: Int
    let sidebar: AnyView
    /// The list column (nil = the entry is one page).
    let listColumn: AnyView?
    /// The entry's own page under the detail stack (nil = the detail's root
    /// is the path's first route, else the empty glyph).
    let detailRoot: AnyView?
    let placeholderIcon: String
    @Binding var path: [AppRoute]
    /// A list-column row's open: REPLACES the detail.
    let listPush: PushRouteAction
    /// A route's page: `pushed` = above the detail's root (the bare back).
    let destination: (AppRoute, _ pushed: Bool) -> AnyView

    @Environment(\.motion) private var motion
    /// Wider than tall (landscape full screen): the sidebar stays beside the
    /// columns; else (portrait, the ⅔ split) it folds away behind the
    /// toggle so the list and the detail keep their width.
    @State private var wide = true
    /// The folded sidebar, slid in over the columns.
    @State private var sidebarOpen = false

    static let sidebarWidth: CGFloat = 280
    static let listWidth: CGFloat = 380

    var body: some View {
        HStack(spacing: 0) {
            if wide {
                sidebarStack
                hairline
            }
            if let listColumn {
                NavigationStack(path: listPath) {
                    listColumn
                        .toolbar(.visible, for: .navigationBar)
                        .toolbar { sidebarToggle }
                        .navigationDestination(for: AppRoute.self) { _ in EmptyView() }
                }
                .environment(\.pushRoute, listPush)
                .frame(width: Self.listWidth)
                hairline
            }
            detailStack
                .frame(maxWidth: .infinity)
        }
        .overlay(alignment: .leading) {
            if !wide, sidebarOpen {
                ZStack(alignment: .leading) {
                    Color.black.opacity(0.4)
                        .ignoresSafeArea()
                        .onTapGesture { sidebarOpen = false }
                        .accessibilityHidden(true)
                    HStack(spacing: 0) {
                        sidebarStack
                        hairline
                    }
                    .transition(.move(edge: .leading))
                }
            }
        }
        .animation(motion.standard, value: sidebarOpen)
        .onGeometryChange(for: Bool.self) { $0.size.width > $0.size.height } action: { isWide in
            wide = isWide
            if isWide { sidebarOpen = false }
        }
        .onChange(of: picks) { _, _ in sidebarOpen = false }
        .environment(\.tabBarShown, false)
        .environment(\.pageHeaderAsRoot, true)
    }

    private var sidebarStack: some View {
        NavigationStack { sidebar }
            .frame(width: Self.sidebarWidth)
    }

    private var hairline: some View {
        Rectangle()
            .fill(GlassTokens.strokeCard)
            .frame(width: 1)
            .ignoresSafeArea()
    }

    /// The folded sidebar's way back.
    @ToolbarContentBuilder
    private var sidebarToggle: some ToolbarContent {
        if !wide {
            ToolbarItem(placement: .topBarLeading) {
                Button {
                    sidebarOpen = true
                } label: {
                    AppIcon(AppIcons.navRailExpand, size: AppIcon.Size.medium)
                }
                .accessibilityLabel("Show sidebar")
                .accessibilityIdentifier("tablet-sidebar-toggle")
            }
        }
    }

    /// The list column never pushes onto itself: a `NavigationLink(value:)`
    /// in it (Settings' rows, the header's Search and Inbox) opens in the
    /// detail through `listPush`.
    private var listPath: Binding<[AppRoute]> {
        Binding(get: { [] }, set: { routes in
            if let route = routes.last { listPush(route) }
        })
    }

    /// The pushes above the detail's root route.
    private var tail: Binding<[AppRoute]> {
        Binding(get: { Array(path.dropFirst()) }, set: { rest in
            if let first = path.first { path = [first] + rest } else { path = rest }
        })
    }

    @ViewBuilder
    private var detailStack: some View {
        if let detailRoot {
            NavigationStack(path: $path) {
                detailRoot
                    .toolbar(.visible, for: .navigationBar)
                    .toolbar { sidebarToggle }
                    .navigationDestination(for: AppRoute.self) { destination($0, true) }
            }
        } else {
            NavigationStack(path: tail) {
                Group {
                    if let first = path.first {
                        destination(first, false)
                            .id(first)
                            // Nothing under it to go back to (a chat's ghost back hides).
                            .environment(\.detailColumnRoot, true)
                    } else {
                        ZStack {
                            AppBackground()
                            AppIcon(placeholderIcon, size: 44)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        .toolbar(.hidden, for: .navigationBar)
                    }
                }
                .navigationDestination(for: AppRoute.self) { destination($0, true).environment(\.detailColumnRoot, false) }
            }
        }
    }
}

/// The sidebar: the desktop rail's entries in its order (Agent, Needs you
/// while any, Chats, Devices, Drafts while any), then the team's Agents, its
/// Apps, its Boards and the Active sessions; the account at the bottom.
struct TabletSidebar: View {
    let selection: SidebarItem
    /// The Board tab's board: its row carries the tab's test id.
    let currentBoardId: String?
    let draftCount: Int
    let onSelect: (SidebarItem) -> Void
    let onSearch: () -> Void
    let onNewIssue: (() -> Void)?

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(SessionListsModel.self) private var lists
    @State private var apps = AppsListModel()
    @State private var now = Date()

    var body: some View {
        ZStack {
            AppBackground()
            VStack(spacing: 0) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 2) {
                        entries
                        section(SessionTree.ListCopy.agents, rows: agentRows)
                        section(Apps.Copy.title, onTitle: { onSelect(.apps) }, rows: appRows)
                        section("Boards", rows: boardRows)
                        if !activeRows.isEmpty {
                            section(SessionTree.ListCopy.rail, rows: activeSessionRows)
                        }
                    }
                    .padding(.horizontal, 10)
                    .padding(.bottom, 12)
                }
                .scrollContentBackground(.hidden)
                accountRow
            }
        }
        .navigationTitle(teamState.activeTeam?.name ?? Chats.Copy.Tabs.team)
        .toolbarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button(action: onSearch) {
                    AppIcon(AppIcons.navSearch, size: AppIcon.Size.medium)
                }
                .accessibilityLabel("Search")
                Button { onNewIssue?() } label: {
                    AppIcon(AppIcons.navCreateIssue, size: AppIcon.Size.medium)
                }
                .disabled(onNewIssue == nil)
                .accessibilityLabel("New issue")
            }
        }
        .onAppear { bindApps() }
        .onChange(of: teamState.activeTeamId) { _, _ in bindApps() }
        .onDisappear { apps.stop() }
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                now = Date()
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("tablet-sidebar")
    }

    private func bindApps() {
        apps.bind(
            accountId: deps.auth.activeAccountId ?? "",
            teamId: teamState.activeTeamId,
            userId: deps.auth.userId,
            db: deps.db
        )
    }

    // MARK: - Entries

    private var needsYouCount: Int {
        lists.needsYou(teamId: teamState.activeTeamId, me: deps.auth.userId, now: now).count
    }

    @ViewBuilder
    private var entries: some View {
        row(.agent, title: Chats.Copy.Headers.agent) { glyph(AppIcons.actionChat) }
        if needsYouCount > 0 || selection == .needsYou {
            row(.needsYou, title: NeedsYou.Copy.entry, count: needsYouCount) {
                glyph(AppIcons.navNeedsYou, color: DesignTokens.Semantic.yellow)
            }
        }
        row(.chats, title: Chats.Copy.Headers.chats) { glyph(AppIcons.navChats) }
        row(.devices, title: Chats.Copy.Headers.devices) { glyph(AppIcons.navDevices) }
        if draftCount > 0 || selection == .drafts {
            row(.drafts, title: "Drafts", count: draftCount) { glyph(AppIcons.navDrafts) }
        }
    }

    private var agentRows: [AnyView] {
        // #EXP-1714: the viewer's global Assistant lists in every team.
        AgentSettings.agentsInTeam(
            lists.agents, teamId: teamState.activeTeamId, teamIds: teamState.teams.map(\.id)
        ).map { agent in
            AnyView(row(.agentChannel(agent.agentId), title: agent.name) {
                if agent.assistant {
                    ExpLogoMark(size: 18, color: .white)
                } else {
                    AgentRowIcon(icon: agent.icon, color: agent.color)
                }
            })
        }
    }

    private var appRows: [AnyView] {
        apps.apps.map { app in
            AnyView(row(.app(app.id), title: app.name) {
                AgentRowIcon(icon: app.glyph, color: app.color ?? AppGlyph.neutralColor)
            })
        }
    }

    private var boardRows: [AnyView] {
        teamState.filteredBoards.map { board in
            AnyView(row(.board(board.id), title: board.name) {
                AppIcon(BoardTypeDisplay.iconName(for: board), size: 18)
                    .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
            })
        }
    }

    /// The desktop rail's Active band (`SessionTree.railChats`).
    private var activeRows: [ChatsListRow] {
        SessionTree.railChats(lists.teamGroups(teamState.activeTeamId)) { lists.words[$0.rootId] }
            .filter { $0.depth == 0 }
    }

    private var activeSessionRows: [AnyView] {
        activeRows.map { chat in
            let lead: Chats.ChatLead = chat.needsYou ? .needsYou : chat.busy ? .working : .waiting
            return AnyView(row(.chat(chat.rootId), title: chat.title, bold: chat.unread) {
                ChatLeadMark(lead: lead, cli: lists.clis[chat.sessionId], size: 18)
            })
        }
    }

    // MARK: - Rows

    private func section(_ title: String, onTitle: (() -> Void)? = nil, rows: [AnyView]) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Rectangle()
                .fill(GlassTokens.strokeCard)
                .frame(height: 1)
                .padding(.vertical, 8)
            Group {
                if let onTitle {
                    Button(action: onTitle) { ChatSectionHeader(title) }
                        .buttonStyle(.plain)
                } else {
                    ChatSectionHeader(title)
                }
            }
            .padding(.horizontal, 8)
            ForEach(rows.indices, id: \.self) { index in rows[index] }
        }
    }

    private func glyph(_ icon: String, color: Color = .white) -> some View {
        AppIcon(icon, size: 18).foregroundStyle(color)
    }

    /// One rail row: the glyph, the name, a muted count; the open entry's
    /// row is filled (`flatRow(isActive:)`, a master-detail's open item).
    private func row<Glyph: View>(
        _ item: SidebarItem,
        title: String,
        count: Int? = nil,
        bold: Bool = false,
        @ViewBuilder glyph: () -> Glyph
    ) -> some View {
        Button { onSelect(item) } label: {
            HStack(spacing: 10) {
                glyph()
                    .frame(width: 20, height: 20)
                Text(title)
                    .font(.system(size: 15, weight: bold ? .semibold : .regular))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 4)
                if let count, count > 0 {
                    Text("\(count)")
                        .font(.system(size: 13))
                        .monospacedDigit()
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                }
            }
            .padding(.horizontal, 8)
            .frame(height: 36)
            .contentShape(Rectangle())
            .flatRow(isActive: selection == item)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(testId(item))
    }

    /// The phone tab bar's ids on their sidebar twins (the capture walks
    /// drive both), else `sidebar-<entry>`.
    private func testId(_ item: SidebarItem) -> String {
        switch item {
        case .agent: "tab-agent"
        case .chats: "tab-chats"
        case .apps: "tab-apps"
        case let .board(id) where id == currentBoardId: "tab-board"
        case let .board(id): "sidebar-board-\(id)"
        case .needsYou: "sidebar-needs-you"
        case .devices: "sidebar-devices"
        case .drafts: "sidebar-drafts"
        case .settings: "tab-team"
        case let .agentChannel(id): "sidebar-agent-\(id)"
        case let .app(id): "sidebar-app-\(id)"
        case let .chat(id): "sidebar-chat-\(id)"
        }
    }

    /// The account at the bottom: a tap opens Settings.
    private var accountRow: some View {
        let account = deps.auth.accounts.first { $0.id == deps.auth.activeAccountId }
        let name = account?.userName ?? account?.userEmail ?? account?.displayName ?? ""
        return Button { onSelect(.settings) } label: {
            HStack(spacing: 10) {
                UserAvatar(
                    image: nil,
                    initials: String(name.prefix(1)).uppercased(),
                    hueKey: avatarHueKey(id: account?.userId, name: account?.userName, email: account?.userEmail),
                    size: 26
                )
                VStack(alignment: .leading, spacing: 0) {
                    Text(name)
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if let email = account?.userEmail, email != name {
                        Text(email)
                            .font(.system(size: 12))
                            .foregroundStyle(DesignTokens.Palette.mutedForeground)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 4)
                AppIcon(AppIcons.navSettings, size: 16)
                    .foregroundStyle(DesignTokens.Palette.mutedForeground)
            }
            .padding(.horizontal, 8)
            .frame(height: 48)
            .contentShape(Rectangle())
            .flatRow(isActive: selection == .settings)
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 10)
        .padding(.bottom, 8)
        .accessibilityIdentifier("tab-team")
    }
}

/// The sidebar's Drafts entry: the board's Drafts chip list as a column.
struct TabletDraftsColumn: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @State private var drafts: DraftsViewModel?

    var body: some View {
        ZStack {
            AppBackground()
            if let drafts {
                DraftsListContent(viewModel: drafts)
            }
        }
        .pageHeader("Drafts")
        .onAppear {
            if drafts == nil, let accountId = deps.auth.activeAccountId {
                drafts = DraftsViewModel(accountId: accountId, db: deps.db, api: deps.issueDraftsApi)
            }
            drafts?.teamId = teamState.activeTeamId
            drafts?.startObserving()
        }
        .onDisappear { drafts?.stopObserving() }
        .onChange(of: teamState.activeTeamId) { _, teamId in drafts?.teamId = teamId }
    }
}
