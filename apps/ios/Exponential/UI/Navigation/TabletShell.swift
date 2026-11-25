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
    case agent, needsYou, chats, devices, drafts, settings
    case agentChannel(String)
    case app(String)
    case board(String)
    case chat(String)

    /// The phone root this entry stands for (the navigator's deep links land
    /// on a `MainTab`; the sidebar follows them).
    var tab: MainTab {
        switch self {
        case .agent, .needsYou, .chats, .chat, .drafts: .chats
        case .agentChannel: .chats
        case .app, .board: .teams
        case .devices, .settings: .settings
        }
    }

    /// The entry a landing on `tab` lights.
    init(_ tab: MainTab, assistantId: String?) {
        switch tab {
        case .chats, .teams: self = .chats
        case .assistant: self = assistantId.map { .agentChannel($0) } ?? .chats
        case .settings: self = .settings
        }
    }

    /// The muted glyph of an empty detail column.
    var placeholderIcon: String {
        switch self {
        case .needsYou: AppIcons.navNeedsYou
        case .chats: AppIcons.navChats
        case .drafts: AppIcons.navDrafts
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
                    // A one-page entry's root (the Assistant): nothing to go back to.
                    .environment(\.detailColumnRoot, true)
                    .navigationDestination(for: AppRoute.self) { destination($0, true).environment(\.detailColumnRoot, false) }
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
/// while any, Chats, Devices, Drafts while any), the viewer's Assistant, then
/// the team trees (#EXP-2109, sidebar-tree.json: `TeamTreeList`, the phone's
/// own) and the Active sessions; the account at the
/// bottom.
struct TabletSidebar: View {
    let selection: SidebarItem
    let draftCount: Int
    let onSelect: (SidebarItem) -> Void
    let onSearch: () -> Void
    let onNewIssue: (() -> Void)?

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(SessionListsModel.self) private var lists
    @Environment(SidebarTreeModel.self) private var tree
    @State private var now = Date()

    var body: some View {
        ZStack {
            AppBackground()
            VStack(spacing: 0) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 2) {
                        entries
                        if !teamState.teams.isEmpty {
                            section(SidebarTree.Copy.teams, rows: [AnyView(TeamTreeList(
                                teams: teamState.orderedTeams,
                                compact: true,
                                isSelected: { item($0) == selection },
                                onOpen: { if let item = item($0) { onSelect(item) } }
                            ))])
                        }
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
        .navigationTitle(Chats.Copy.Headers.teams)
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
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                now = Date()
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("tablet-sidebar")
    }

    // MARK: - Entries

    private var needsYouCount: Int {
        // #EXP-1950: installs waiting for approval count as Needs you rows.
        lists.needsYou(me: deps.auth.userId, now: now).count
            + deps.installTrust.snapshot(deps.auth.activeAccountId ?? "").rows.count
    }

    @ViewBuilder
    private var entries: some View {
        row(.agent, title: Chats.Copy.Headers.agent) { glyph(AppIcons.actionChat) }
        if needsYouCount > 0 || selection == .needsYou {
            row(.needsYou, title: NeedsYou.Copy.entry, badge: NavBadges.Value(count: needsYouCount, tone: .attention)) {
                glyph(AppIcons.navNeedsYou, color: DesignTokens.Semantic.yellow)
            }
        }
        row(.chats, title: Chats.Copy.Headers.chats, badge: lists.chatsBadge) {
            glyph(AppIcons.navChats)
        }
        row(.devices, title: Chats.Copy.Headers.devices) { glyph(AppIcons.navDevices) }
        if draftCount > 0 || selection == .drafts {
            row(.drafts, title: "Drafts", badge: NavBadges.Value(count: draftCount, tone: .muted)) { glyph(AppIcons.navDrafts) }
        }
        if let assistant = lists.assistant {
            row(.agentChannel(assistant.agentId), title: assistant.name, badge: lists.agentBadge(assistant)) {
                ExpLogoMark(size: 18, color: .white)
            }
        }
    }

    /// The entry a tree leaf selects.
    private func item(_ node: SidebarTree.PhoneRow) -> SidebarItem? {
        switch node.kind {
        case .team, .folder: nil
        case .item(.board): .board(node.id)
        case .item(.agent): .agentChannel(node.id)
        case .item(.app): .app(node.id)
        case .item(.session): .chat(node.id)
        }
    }

    /// The desktop rail's Active band (`SessionTree.railChats`).
    private var activeRows: [ChatsListRow] {
        SessionTree.railChats(lists.groups) { lists.words[$0.rootId] }
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

    private func section(_ title: String, rows: [AnyView]) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Rectangle()
                .fill(GlassTokens.strokeCard)
                .frame(height: 1)
                .padding(.vertical, 8)
            ChatSectionHeader(title)
                .padding(.horizontal, 8)
            ForEach(rows.indices, id: \.self) { index in rows[index] }
        }
    }

    private func glyph(_ icon: String, color: Color = .white) -> some View {
        AppIcon(icon, size: 18).foregroundStyle(color)
    }

    /// One rail row: the glyph, the name, THE count badge (#FEED-232); the open entry's
    /// row is filled (`flatRow(isActive:)`, a master-detail's open item).
    private func row<Glyph: View>(
        _ item: SidebarItem,
        title: String,
        badge: NavBadges.Value = .none,
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
                CountBadge(badge)
            }
            .padding(.horizontal, 8)
            .frame(height: 36)
            .contentShape(Rectangle())
            .flatRow(isActive: selection == item)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(testId(item))
    }

    /// The phone bar's ids on their sidebar twins (the capture walks drive
    /// both), else `sidebar-<entry>`.
    private func testId(_ item: SidebarItem) -> String {
        switch item {
        case .agent: "sidebar-agent"
        case .chats: Chats.TestIds.tabChats
        case let .board(id): "sidebar-board-\(id)"
        case .needsYou: "sidebar-needs-you"
        case .devices: "sidebar-devices"
        case .drafts: "sidebar-drafts"
        case .settings: Chats.TestIds.tabSettings
        case let .agentChannel(id): lists.assistant?.agentId == id ? Chats.TestIds.tabAssistant : "sidebar-agent-\(id)"
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
        .accessibilityIdentifier(Chats.TestIds.tabSettings)
    }
}

/// The sidebar's Drafts entry: every team's drafts as a column.
struct TabletDraftsColumn: View {
    @Environment(AppDependencies.self) private var deps
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
            drafts?.startObserving()
        }
        .onDisappear { drafts?.stopObserving() }
    }
}
