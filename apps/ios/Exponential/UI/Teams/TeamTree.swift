import ExpCore
import ExpUI
import SwiftUI
import UIKit

// #EXP-2109 (sidebar-tree.json `phone`): the team trees, one nested list the
// phone's Teams tab and the iPad's sidebar both draw: teams and folders fold
// in place, a leaf opens its target, a long press opens the row's menu
// (sidebar-menus.json `phone`), a team's row carries + and `…`
// (header-actions.json `rows.team`).

/// What a pick of a tree menu does.
enum TeamTreeVerb: Hashable {
    case action(SidebarMenus.Action)
    case new(SidebarMenus.NewKind)
    case moveInto(String)
    case moveNewFolder
    case moveTopLevel(String)
    /// A divider (by its place).
    case separator(Int)
}

/// THE Menu in its sheet presentation: a list of rows, the pick handed back;
/// the host acts once the sheet has dismissed.
struct TeamMenuSheet<Pick: Hashable>: View {
    struct Item: Identifiable {
        let pick: Pick
        let label: String
        let icon: String?
        let testId: String
        var destructive = false
        var separator = false
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
                    if item.separator {
                        GlassDivider().padding(.vertical, 4)
                    } else {
                        Button {
                            if item.children.isEmpty { onPick(item.pick) } else { open = item }
                        } label: {
                            GlassMenuItemLabel(
                                item.label, icon: item.icon, destructive: item.destructive, chevron: !item.children.isEmpty
                            )
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier(item.testId)
                    }
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("team-menu")
        }
    }
}

/// The trees of `teams`, one row list (the host draws the group around it).
struct TeamTreeList: View {
    let teams: [TeamEntity]
    /// The iPad sidebar's density (36pt rows) instead of the phone's 48.
    var compact = false
    var isSelected: (SidebarTree.PhoneRow) -> Bool = { _ in false }
    /// A leaf's tap.
    let onOpen: (SidebarTree.PhoneRow) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(SessionListsModel.self) private var lists
    @Environment(SidebarTreeModel.self) private var tree
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster
    @Environment(\.motion) private var motion

    private struct MenuTarget: Identifiable {
        enum Part { case menu, add, more }
        let row: SidebarTree.PhoneRow
        let part: Part
        var id: String { "\(part)-\(row.id)" }
    }

    @State private var menu: MenuTarget?
    @State private var pending: (SidebarTree.PhoneRow, TeamTreeVerb)?
    @State private var createBoardIn: TeamTarget?
    @State private var newAgentIn: TeamTarget?
    @State private var newAppIn: TeamTarget?
    @State private var pendingNewApp: (String, AppsNewMenuSheet.Action)?
    @State private var importIn: TeamTarget?
    @State private var archiving: BoardEntity?
    @State private var creatingDraft = false
    @State private var draftName = ""
    @FocusState private var naming: Bool
    @State private var now = Date()

    private struct TeamTarget: Identifiable, Hashable { let id: String }

    private var rows: [SidebarTree.PhoneRow] { tree.rows(teams: teams, teamState: teamState, lists: lists) }

    var body: some View {
        let rows = rows
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                rowView(row)
                    .treeGuides(guides[index], base: compact ? 8 : 16)
                    .id(row.id)
            }
        }
        .sheet(item: $menu, onDismiss: runPending) { target in
            TeamMenuSheet(title: title(target.row), items: items(target)) { verb in
                pending = (target.row, verb)
                menu = nil
            }
        }
        // Each presentation on its own node.
        .background {
            Color.clear.sheet(item: $createBoardIn) { team in
                CreateBoardSheet(accountId: accountId, teamId: team.id) { pushRoute(.board(accountId: accountId, id: $0)) }
            }
        }
        .background {
            Color.clear.sheet(item: $newAppIn, onDismiss: runPendingNewApp) { team in
                AppsNewMenuSheet { action in
                    pendingNewApp = (team.id, action)
                    newAppIn = nil
                }
            }
        }
        .background {
            Color.clear.sheet(item: $importIn) { team in
                AppImportStep(teamId: team.id) { pushRoute(.app(accountId: accountId, appId: $0)) }
            }
        }
        .background {
            Color.clear.sheet(item: $newAgentIn) { team in
                NavigationStack {
                    AgentSettingsPage(teamId: team.id, agentId: nil, isOwner: tree.isOwner(team.id))
                        .environment(\.accountId, accountId)
                        .bareBackButton()
                }
            }
        }
        .background {
            Color.clear.glassAlert(
                isPresented: Binding(get: { archiving != nil }, set: { if !$0 { archiving = nil } }),
                title: "Archive \"\(archiving?.name ?? "")\"?",
                message: "It and its issues disappear for the whole team. An owner can bring it back in team settings.",
                actions: [
                    GlassAlertAction("Cancel", role: .outline, isCancel: true, id: "cancel") {},
                    GlassAlertAction("Archive", id: "archive") { [archiving] in
                        if let archiving { archive(archiving) }
                    },
                ]
            )
        }
        .onAppear { now = Date() }
    }

    // MARK: - Rows

    private var glyphSize: CGFloat { 18 }

    @ViewBuilder
    private func rowView(_ row: SidebarTree.PhoneRow) -> some View {
        let editing = row.kind == .folder && tree.editingFolder == row.id
        HStack(spacing: 0) {
            HStack(spacing: compact ? 10 : 12) {
                glyph(row).frame(width: glyphSize, height: glyphSize)
                if editing {
                    nameField(row)
                } else {
                    Text(title(row))
                        .font(compact ? .system(size: 15) : .body)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                }
                Spacer(minLength: 4)
                if !editing { trailing(row) }
            }
            .padding(.leading, compact ? 8 : 16)
            .padding(.trailing, row.kind == .team ? 4 : compact ? 8 : 16)
            .padding(.vertical, compact ? 4 : 8)
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .glassLongPress(
                onTap: editing ? nil : { tap(row) },
                onHold: { if !editing { menu = MenuTarget(row: row, part: .menu) } }
            )
            .accessibilityElement(children: editing ? .contain : .combine)
            .accessibilityLabel(title(row))
            .accessibilityValue(value(row))
            .accessibilityAddTraits(.isButton)
            .accessibilityAction(named: "Actions") { menu = MenuTarget(row: row, part: .menu) }
            .accessibilityIdentifier(testId(row))
            if row.kind == .team { teamButtons(row) }
        }
        .frame(minHeight: compact ? 36 : 48)
        .fixedSize(horizontal: false, vertical: true)
        .flatRowIf(compact, isActive: isSelected(row))
    }

    private func testId(_ row: SidebarTree.PhoneRow) -> String {
        switch row.kind {
        case .team: "team-row-\(row.id)"
        case .folder: "sidebar-folder-\(row.id)"
        case .item(.session) where compact: "sidebar-chat-\(row.id)"
        case let .item(kind) where compact: "sidebar-\(kind.rawValue)-\(row.id)"
        case .item: "tree-row-\(row.kind.rawValue)"
        }
    }

    /// The fold state, then the row's count.
    private func value(_ row: SidebarTree.PhoneRow) -> String {
        let fold = row.kind == .team || row.kind == .folder ? (row.open ? "Expanded" : "Collapsed") : nil
        let count = badge(row).count
        return [fold, count > 0 ? "\(count)" : nil].compactMap { $0 }.joined(separator: ", ")
    }

    @ViewBuilder
    private func glyph(_ row: SidebarTree.PhoneRow) -> some View {
        if row.kind == .team {
            TeamAvatar(iconUrl: teamState.team(row.id)?.iconUrl, size: glyphSize)
                .foregroundStyle(.white)
        } else {
            TeamTreeGlyph(row: row, size: glyphSize)
        }
    }

    @ViewBuilder
    private func trailing(_ row: SidebarTree.PhoneRow) -> some View {
        switch row.kind {
        case .team:
            CountBadge(badge(row))
        case .folder:
            if !row.open {
                Text("\(row.count)")
                    .font(.subheadline)
                    .monospacedDigit()
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            chevron(row)
        case .item:
            CountBadge(badge(row))
        }
    }

    /// header-actions.json `rows.team`: +, `…`, then the fold chevron.
    private func teamButtons(_ row: SidebarTree.PhoneRow) -> some View {
        HStack(spacing: 4) {
            rowButton(AppIcons.uiAdd, label: "\(SidebarMenus.copy[.new] ?? ""): \(title(row))", testId: "team-new-\(row.id)") {
                menu = MenuTarget(row: row, part: .add)
            }
            rowButton(AppIcons.uiMore, label: "Team actions: \(title(row))", testId: "team-actions-\(row.id)") {
                menu = MenuTarget(row: row, part: .more)
            }
            chevron(row)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                .onTapGesture { tap(row) }
        }
        .padding(.trailing, compact ? 8 : 16)
    }

    private func chevron(_ row: SidebarTree.PhoneRow) -> some View {
        AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .rotationEffect(.degrees(row.open ? 90 : 0))
            .accessibilityHidden(true)
    }

    /// A team row's + and `…`: 18pt of ink, a 36pt target.
    private func rowButton(_ icon: String, label: String, testId: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            AppIcon(icon, size: 16)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 32, height: 36)
                .contentShape(Rectangle().inset(by: -4))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityIdentifier(testId)
    }

    /// Rename (and a new folder's name) in place: Return or leaving it saves.
    private func nameField(_ row: SidebarTree.PhoneRow) -> some View {
        TextField("", text: $draftName)
            .font(compact ? .system(size: 15) : .body)
            .foregroundStyle(.white)
            .focused($naming)
            .submitLabel(.done)
            .onSubmit { naming = false }
            .onAppear {
                draftName = title(row)
                naming = true
            }
            .onChange(of: naming) { _, focused in
                guard !focused, tree.editingFolder == row.id else { return }
                tree.editingFolder = nil
                let name = SidebarFolders.folderName(draftName)
                if name != row.name { write("rename", "the folder") { try await tree.rename(row.id, name, deps: deps, accountId: accountId) } }
            }
            .accessibilityLabel("Folder name")
            .accessibilityIdentifier("sidebar-folder-name")
    }

    private func title(_ row: SidebarTree.PhoneRow) -> String {
        row.kind == .team
            ? teamState.team(row.id)?.name ?? ""
            : TeamTreeRowValues.title(row, teamState: teamState, lists: lists, tree: tree)
    }

    private func badge(_ row: SidebarTree.PhoneRow) -> NavBadges.Value {
        row.kind == .team
            ? NavBadges.Value(count: lists.needsYou(teamId: row.id, me: deps.auth.userId, now: now).count, tone: .attention)
            : TeamTreeRowValues.badge(row, lists: lists)
    }

    private func tap(_ row: SidebarTree.PhoneRow) {
        switch row.kind {
        case .team: withAnimation(motion.standard) { tree.setTeamOpen(row.id, !row.open) }
        case .folder: withAnimation(motion.standard) { tree.setFolderOpen(row.id, teamId: row.teamId, !row.open) }
        case .item: onOpen(row)
        }
    }

    // MARK: - Menus

    private func kind(_ row: SidebarTree.PhoneRow) -> SidebarMenus.Kind {
        switch row.kind {
        case .team: .team
        case .folder: .folder
        case .item(.board): .board
        case .item(.agent): .agent
        case .item(.app): .app
        case .item(.session): .session
        }
    }

    private func items(_ target: MenuTarget) -> [TeamMenuSheet<TeamTreeVerb>.Item] {
        let row = target.row
        let owner = tree.isOwner(row.teamId)
        if target.part == .add { return newItems(row) }
        var actions = SidebarMenus.phoneMenu(kind(row), .init(owner: owner))
        if target.part == .more { actions = HeaderActions.menu(actions.map(\.rawValue)).compactMap(SidebarMenus.Action.init) }
        return actions.enumerated().map { index, action in
            if action == .separator {
                return .init(pick: .separator(index), label: "", icon: nil, testId: "", separator: true)
            }
            return .init(
                pick: .action(action),
                label: SidebarMenus.copy[action] ?? "",
                icon: SidebarMenus.icons[action].flatMap { AppIcons.conceptOrName($0) },
                testId: "sidebar-menu-\(action.rawValue)",
                destructive: SidebarMenus.destructive(kind(row), action),
                children: action == .new ? newItems(row) : action == .move ? moveItems(row) : []
            )
        }
    }

    private func newItems(_ row: SidebarTree.PhoneRow) -> [TeamMenuSheet<TeamTreeVerb>.Item] {
        SidebarMenus.newKinds(owner: tree.isOwner(row.teamId)).map { kind in
            .init(
                pick: .new(kind), label: SidebarMenus.newCopy[kind] ?? "",
                icon: SidebarMenus.newIcons[kind].flatMap { AppIcons.conceptOrName($0) },
                testId: "sidebar-new-\(kind.rawValue)"
            )
        }
    }

    private func moveItems(_ row: SidebarTree.PhoneRow) -> [TeamMenuSheet<TeamTreeVerb>.Item] {
        let targets = SidebarMenus.moveTargets(
            tree.folderRows(row.teamId, teamState: teamState, lists: lists), id: row.id
        )
        let folders = targets.folders.map { folder in
            TeamMenuSheet<TeamTreeVerb>.Item(
                pick: .moveInto(folder.id), label: folder.name ?? SidebarFolders.defaultFolderName,
                icon: AppIcons.uiFolder, testId: "sidebar-move-folder"
            )
        }
        return folders
            + (folders.isEmpty ? [] : [.init(pick: .separator(0), label: "", icon: nil, testId: "", separator: true)])
            + [.init(
                pick: .moveNewFolder, label: SidebarMenus.copy[.newFolder] ?? "", icon: AppIcons.uiFolder,
                testId: "sidebar-move-new-folder"
            )]
            + (targets.topLevel.map {
                [.init(pick: .moveTopLevel($0), label: SidebarMenus.topLevel, icon: nil, testId: "sidebar-move-top-level")]
            } ?? [])
    }

    private func runPending() {
        guard let (row, verb) = pending else { return }
        pending = nil
        switch verb {
        case let .action(action): run(action, row)
        case let .new(kind): runNew(kind, row.teamId)
        case let .moveInto(folder): move(row, .init(id: row.id, into: folder))
        case let .moveTopLevel(folder): move(row, .init(id: row.id, after: folder))
        case .moveNewFolder:
            layout(row.teamId) { SidebarFolders.moveToNewFolder(items: $0, entries: $1, id: row.id, newFolderId: newId()) }
        case .separator: break
        }
    }

    private func newId() -> String { UUID().uuidString.lowercased() }

    private func run(_ action: SidebarMenus.Action, _ row: SidebarTree.PhoneRow) {
        switch action {
        case .open: onOpen(row)
        case .newFolder: addFolder(row.teamId)
        case .rename: tree.editingFolder = row.id
        case .ungroup: layout(row.teamId) { SidebarFolders.ungroup(items: $0, entries: $1, folderId: row.id) }
        case .newIssue: pushRoute(.issueDraft(accountId: accountId, draftId: newId(), boardId: row.id))
        case .compact: send(row, .compact)
        case .newConversation: send(row, .newConversation)
        case .edit: pushRoute(.app(accountId: accountId, appId: row.id))
        case .copyLink: copyLink(row)
        case .removeFromSidebar:
            write("save", "the sidebar") { try await tree.remove(row.id, teamId: row.teamId, deps: deps, accountId: accountId) }
        case .settings: openSettings(row)
        case .archive: archiving = teamState.boards.first { $0.id == row.id }
        default: break
        }
    }

    private func runNew(_ kind: SidebarMenus.NewKind, _ teamId: String) {
        switch kind {
        case .session: pushRoute(.agent(accountId: accountId, seed: AgentComposerSeed(teamId: teamId)))
        case .board: createBoardIn = .init(id: teamId)
        case .agent: newAgentIn = .init(id: teamId)
        case .app: newAppIn = .init(id: teamId)
        }
    }

    /// VAPP-242: THE NEW MENU's pick, once its sheet is gone.
    private func runPendingNewApp() {
        guard let (teamId, action) = pendingNewApp else { return }
        pendingNewApp = nil
        switch action {
        case .newApp: createDraft(teamId)
        case .importApp: importIn = .init(id: teamId)
        }
    }

    /// VAPP-176 NEW APP: a DRAFT (`apps.create {teamId}`, no other field),
    /// then its editor at once; a failure = the failed-action toast.
    private func createDraft(_ teamId: String) {
        guard !creatingDraft else { return }
        creatingDraft = true
        Task {
            defer { creatingDraft = false }
            do {
                let created = try await deps.appsApi.create(accountId: accountId, AppCreateInput(teamId: teamId))
                pushRoute(.app(accountId: accountId, appId: created.id))
            } catch {
                failed("create", "the app", error)
            }
        }
    }

    private func openSettings(_ row: SidebarTree.PhoneRow) {
        switch row.kind {
        case .team: pushRoute(.teamSettings(accountId: accountId, teamId: row.teamId))
        case .item(.board): pushRoute(.teamSettingsSection(accountId: accountId, teamId: row.teamId, section: .boards))
        case .item(.agent): pushRoute(.agentChannel(accountId: accountId, agentId: row.id, settings: true))
        case .item(.app): pushRoute(.app(accountId: accountId, appId: row.id))
        default: break
        }
    }

    /// The page's URL, as the web copies it.
    private func copyLink(_ row: SidebarTree.PhoneRow) {
        guard let base = deps.auth.accounts.first(where: { $0.id == accountId })?.instanceUrl,
              let team = teamState.team(row.teamId) else { return }
        let path: String? = switch row.kind {
        case .item(.board): teamState.boards.first { $0.id == row.id }.map { "boards/\($0.slug)" }
        case .item(.app): tree.app(row.id).map { "apps/\($0.slug)" }
        case .item(.session): "sessions/\(row.id)"
        default: nil
        }
        guard let path else { return }
        UIPasteboard.general.string = "\(base.hasSuffix("/") ? String(base.dropLast()) : base)/t/\(team.slug)/\(path)"
        toaster.success(SidebarMenus.linkCopied)
    }

    private func send(_ row: SidebarTree.PhoneRow, _ entry: Chats.AgentMenuEntry) {
        guard let text = Chats.agentMenuSends[entry] else { return }
        write("send", "the message") {
            _ = try await deps.sessionsApi.send(accountId: accountId, .agent(agentId: row.id, text: text))
        }
    }

    private func archive(_ board: BoardEntity) {
        write("archive", "the board") { try await deps.boardsApi.archive(accountId: accountId, boardId: board.id) }
    }

    // MARK: - Layout writes

    private func addFolder(_ teamId: String) {
        layout(teamId) { SidebarFolders.appendFolder(items: $0, entries: $1, newFolderId: newId()) }
    }

    private func move(_ row: SidebarTree.PhoneRow, _ move: SidebarFolders.Move) {
        layout(row.teamId) { SidebarFolders.move(items: $0, entries: $1, move: move, newFolderId: newId()) }
    }

    private func layout(_ teamId: String, _ compute: ([String], [SidebarFolders.Entry]) -> SidebarFolders.Writes?) {
        let items = tree.items(teamId, teamState: teamState, lists: lists).map(\.id)
        guard let writes = compute(items, tree.entries(teamId)) else { return }
        if let created = writes.createdFolderId {
            tree.setTeamOpen(teamId, true)
            tree.editingFolder = created
        }
        write("save", "the sidebar") { try await tree.save(writes, teamId: teamId, deps: deps, accountId: accountId) }
    }

    private func write(_ verb: String, _ thing: String, _ call: @escaping @MainActor () async throws -> Void) {
        Task { @MainActor in
            do { try await call() } catch { failed(verb, thing, error) }
        }
    }

    private func failed(_ verb: String, _ thing: String, _ error: Error) {
        toaster.error(FeedbackStates.Errors.title(verb: verb, thing: thing), description: error.trpcUserMessage)
    }
}

private extension View {
    /// The iPad sidebar's row fill; the phone's rows sit on their group.
    @ViewBuilder
    func flatRowIf(_ on: Bool, isActive: Bool) -> some View {
        if on { flatRow(isActive: isActive) } else { self }
    }
}

/// What a tree row draws, off the synced rows.
@MainActor
enum TeamTreeRowValues {
    static func title(
        _ row: SidebarTree.PhoneRow, teamState: TeamState, lists: SessionListsModel, tree: SidebarTreeModel
    ) -> String {
        switch row.kind {
        case .team: teamState.team(row.id)?.name ?? ""
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
    var size: CGFloat = 18

    @Environment(TeamState.self) private var teamState
    @Environment(SessionListsModel.self) private var lists
    @Environment(SidebarTreeModel.self) private var tree

    var body: some View {
        switch row.kind {
        case .team:
            TeamAvatar(iconUrl: teamState.team(row.id)?.iconUrl, size: size).foregroundStyle(.white)
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
