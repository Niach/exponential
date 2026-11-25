import ExpCore
import ExpUI
import SwiftUI

// #EXP-2109 (sidebar-tree.json `phone`): THE TEAMS TAB, the sidebar's nested
// tree on one page (`TeamTreeList`).

/// Another server's team: the mark, the name over its server, the chevron.
private struct ServerTeamRow: View {
    let team: TeamEntity
    let hostname: String

    var body: some View {
        HStack(spacing: 12) {
            TeamAvatar(iconUrl: team.iconUrl, size: 18)
                .foregroundStyle(.white)
            VStack(alignment: .leading, spacing: 1) {
                Text(team.name)
                    .font(.body)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(hostname)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .frame(minHeight: 48)
        .contentShape(Rectangle())
    }
}

/// TEAMS: every member team's tree in one group; the header's + opens
/// `teamsMenu`. With several servers signed in their teams follow, each
/// naming its server; one of another server's activates it first.
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
    @State private var showCreateBoard = false
    @State private var gettingStarted = GettingStartedProgress()

    private var otherServers: [ServerTeamGroup] {
        (loader?.groups ?? []).filter { $0.accountId != accountId }
    }

    /// `phone.empty`: the only team holds nothing yet.
    private var emptyTeamId: String? {
        SidebarTree.phoneTeamsEmpty(teamState.orderedTeams.map { team in
            (id: team.id, count: tree.items(team.id, teamState: teamState, lists: lists).count)
        })
    }

    var body: some View {
        ZStack {
            AppBackground()
            if !tree.loaded {
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            } else if teamState.teams.isEmpty, otherServers.isEmpty {
                EmptyState(.noTeam, actionLabel: "Create or join a team") { showTeamSetup = true }
            } else {
                list
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
        .background {
            Color.clear.sheet(isPresented: $showCreateBoard) {
                if let emptyTeamId {
                    CreateBoardSheet(accountId: accountId, teamId: emptyTeamId) { boardId in
                        pushRoute(.board(accountId: accountId, id: boardId))
                    }
                }
            }
        }
        .task(id: "\(accountId)/\(emptyTeamId ?? "")") {
            if let emptyTeamId { gettingStarted.bind(accountId: accountId, teamId: emptyTeamId, deps: deps) }
        }
        .onDisappear { gettingStarted.stop() }
        .onAppear {
            if loader == nil { loader = MultiAccountTeamLoader(auth: deps.auth, db: deps.db) }
        }
        .onChange(of: deps.auth.accounts) { _, _ in loader?.refresh() }
        .accessibilityIdentifier("teams-page")
    }

    private var list: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 16) {
                    if !teamState.teams.isEmpty {
                        TeamTreeList(teams: teamState.orderedTeams, onOpen: open)
                            .padding(.vertical, 4)
                            .glassSection()
                    }
                    ForEach(otherServers) { group in
                        VStack(spacing: 0) {
                            ForEach(group.teams, id: \.id) { team in
                                Button { onSwitchAccount(group.accountId, team.id) } label: {
                                    ServerTeamRow(team: team, hostname: group.hostname)
                                        // Another server's image loads with that account.
                                        .environment(\.accountId, group.accountId)
                                }
                                .buttonStyle(.plain)
                                .accessibilityIdentifier("team-row-\(team.id)")
                            }
                        }
                        .padding(.vertical, 4)
                        .glassSection()
                    }
                    if let emptyTeamId { empty(emptyTeamId) }
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 16)
            }
            // A link to a team: unfolded and scrolled to, once.
            .onChange(of: tree.revealTeam, initial: true) { _, teamId in
                guard let teamId else { return }
                tree.revealTeam = nil
                tree.setTeamOpen(teamId, true)
                proxy.scrollTo(teamId, anchor: .top)
            }
        }
    }

    private func open(_ row: SidebarTree.PhoneRow) {
        switch row.kind {
        case .team, .folder: break
        case .item(.board): pushRoute(.board(accountId: accountId, id: row.id))
        case .item(.agent): pushRoute(.agentChannel(accountId: accountId, agentId: row.id))
        case .item(.app): pushRoute(.appRun(accountId: accountId, appId: row.id))
        case .item(.session): pushRoute(.chat(accountId: accountId, chatId: row.id))
        }
    }

    /// The shared empty state over Getting started.
    private func empty(_ teamId: String) -> some View {
        let owner = tree.isOwner(teamId)
        return VStack(spacing: 24) {
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
        .padding(.bottom, 24)
    }
}
