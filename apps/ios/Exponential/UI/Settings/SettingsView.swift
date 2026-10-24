import ExpUI
import ExpCore
import SwiftUI

import GRDB

/// The Team tab's counts (its meta line and rows).
private struct TeamCounts: Equatable, Sendable {
    var members = 0
    var boards = 0
    var agents = 0
    /// The viewer owns the team (`owner` rows).
    var isOwner = false
}

/// EXP-1373 (chats.json `shell`): the phone's TEAM tab (the old settings
/// root moved here; the board header lost its gear). The ACTIVE team's
/// avatar (84) + name + `copy.team.meta`, a glass group of the viewer's
/// teams (the active one checked, a team with an unread notification an
/// amber dot; a tap switches — the ONLY place a phone switches team), then
/// the team's settings rows, the account rows (each server's signed-in
/// account) and the rest of the old root (Getting started, About).
struct TeamTabView: View {
    /// Teams holding an unread notification (their amber dots).
    var unreadTeamIds: Set<String> = []
    /// A team row's tap: `(accountId, teamId)`.
    var onSwitchTeam: (String, String) -> Void = { _, _ in }

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    /// EXP-1270: Getting started for the ACTIVE team, behind the General
    /// section's row (the phones' entry; web's sidebar entry).
    @State private var gettingStarted = GettingStartedProgress()
    @State private var showGettingStarted = false
    @State private var teamLoader: MultiAccountTeamLoader?
    @State private var showAddServer = false
    @State private var showTeamSetup = false
    @State private var counts = TeamCounts()
    // The account that was active before Add-server opened, so a cancelled /
    // incomplete add can be rolled back off the tokenless pending account.
    @State private var previousActiveAccountId: String?

    private var activeAccountId: String { deps.auth.activeAccountId ?? "" }

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    teamHeader
                    teamsSection
                    teamSettingsSection
                    serversSection
                    generalSection
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 16)
            }
            .tabBarBottomInset()
        }
        .navigationBarTitleDisplayMode(.inline)
        .task(id: "\(activeAccountId)/\(teamState.activeTeamId ?? "")") {
            if let teamId = teamState.activeTeamId, !activeAccountId.isEmpty {
                gettingStarted.bind(accountId: activeAccountId, teamId: teamId, deps: deps)
                await observeCounts(teamId: teamId)
            } else {
                gettingStarted.stop()
            }
        }
        .sheet(isPresented: $showGettingStarted) {
            if let teamId = teamState.activeTeamId {
                GettingStartedSheet(progress: gettingStarted, accountId: activeAccountId, teamId: teamId)
            }
        }
        .sheet(isPresented: $showTeamSetup) {
            TeamSetupSheet()
        }
        .onAppear {
            if teamLoader == nil {
                teamLoader = MultiAccountTeamLoader(auth: deps.auth, db: deps.db)
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in
            teamLoader?.refresh()
        }
        .accessibilityIdentifier("team-tab")
        .fullScreenCover(isPresented: $showAddServer) {
            InstanceView(showCancel: true) {
                // Add-server activates a tokenless pending account the moment a
                // URL is entered (setInstanceUrl → upsertAndActivate). If the
                // user backs out before login resolves a token, remove that
                // half-added pending and roll back to the account that was
                // active before — so the app is never left pointed at a
                // tokenless server, and a cancelled add leaves no phantom entry.
                if deps.auth.token == nil,
                   let prev = previousActiveAccountId,
                   let stranded = deps.auth.activeAccountId,
                   stranded != prev {
                    deps.auth.removeAccount(id: stranded)
                    deps.auth.switchAccount(id: prev)
                }
                showAddServer = false
            }
        }
    }

    // MARK: - Team

    @ViewBuilder
    private var teamHeader: some View {
        if let team = teamState.activeTeam {
            VStack(spacing: 10) {
                TeamAvatar(team: team, size: 84)
                Text(team.name)
                    .font(.title.weight(.bold))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(Chats.Copy.Team.metaLine(members: counts.members, boards: counts.boards))
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .frame(maxWidth: .infinity)
            .padding(.top, 24)
            .padding(.bottom, 4)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("team-tab-header")
        }
    }

    /// Every server's teams in ONE group (the server's name only
    /// disambiguates with several servers).
    private var teamsSection: some View {
        let groups = teamLoader?.groups ?? []
        return VStack(alignment: .leading, spacing: 4) {
            GlassSectionBand(Chats.Copy.Team.switch)
            VStack(spacing: 0) {
                ForEach(groups) { group in
                    ForEach(group.teams, id: \.id) { team in
                        teamRow(team, accountId: group.accountId, server: groups.count > 1 ? group.hostname : nil)
                    }
                }
                Button { showTeamSetup = true } label: {
                    settingsRow(icon: AppIcons.uiAdd, title: "New team", chevron: false)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("board-switcher-new-team")
            }
            .glassSection()
        }
    }

    private func teamRow(_ team: TeamEntity, accountId: String, server: String?) -> some View {
        let active = accountId == activeAccountId && team.id == teamState.activeTeamId
        return Button {
            onSwitchTeam(accountId, team.id)
        } label: {
            HStack(spacing: 12) {
                TeamAvatar(team: team, size: 24)
                VStack(alignment: .leading, spacing: 1) {
                    Text(team.name)
                        .font(.body)
                        .foregroundStyle(.white)
                    if let server {
                        Text(server)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                Spacer()
                if active {
                    AppIcon(AppIcons.uiCheck, size: AppIcon.Size.medium)
                        .foregroundStyle(.white)
                } else if accountId == activeAccountId, unreadTeamIds.contains(team.id) {
                    Circle()
                        .fill(DesignTokens.Semantic.yellow)
                        .frame(width: 8, height: 8)
                        .accessibilityLabel("Unread")
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(active ? .isSelected : [])
        .accessibilityIdentifier("team-row-\(team.id)")
    }

    /// #EXP-1381 (settings-nav.json): the team's settings rows, rendered
    /// FROM `SettingsNav.visiblePhoneRows` (order, label, icon concept,
    /// count, visibility); each opens that category's OWN page. Apps are not a row
    /// (the Apps tab).
    @ViewBuilder
    private var teamSettingsSection: some View {
        if let teamId = teamState.activeTeamId {
            VStack(spacing: 0) {
                // Visibility is the fixture's (`visiblePhoneRows`): owner
                // rows for the team's owner; the natives are never "cloud".
                ForEach(SettingsNav.visiblePhoneRows(isOwner: counts.isOwner), id: \.self) { id in
                    if let section = TeamSettingsSection(id) {
                        teamLink(teamId, section: section, row: SettingsNav.row(id))
                    }
                }
            }
            .glassSection()
        }
    }

    private func count(_ count: SettingsNav.Count?) -> Int? {
        switch count {
        case .members: counts.members
        case .boards: counts.boards
        case .agents: counts.agents
        case nil: nil
        }
    }

    private func teamLink(_ teamId: String, section: TeamSettingsSection, row: SettingsNav.Row) -> some View {
        NavigationLink(value: AppRoute.teamSettingsSection(accountId: activeAccountId, teamId: teamId, section: section)) {
            settingsRow(
                icon: AppIcons.conceptOrName(row.icon) ?? AppIcons.uiChevronRight,
                title: row.label,
                count: count(row.count)
            )
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(SettingsNav.rowTestId)
    }

    private func observeCounts(teamId: String) async {
        guard let pool = try? deps.db.pool(forAccountId: activeAccountId) else { return }
        let userId = deps.auth.userId
        let observation = ValueObservation.tracking { db in
            TeamCounts(
                members: try TeamMemberEntity.filter(Column("team_id") == teamId).fetchCount(db),
                boards: try BoardEntity.filter(Column("team_id") == teamId).fetchCount(db),
                agents: try AgentDefinitionEntity.filter(Column("team_id") == teamId).fetchCount(db),
                isOwner: try userId.map { userId in
                    try TeamMemberEntity
                        .filter(Column("team_id") == teamId && Column("user_id") == userId)
                        .filter(Column("role") == DomainContract.teamRoleOwner)
                        .fetchCount(db) > 0
                } ?? false
            )
        }.removeDuplicates()
        do {
            for try await next in observation.values(in: pool) {
                counts = next
            }
        } catch {}
    }

    // EXP-721: ONE grouped card per section (Android's SettingsScreen) — the
    // gapped stack of individually-filled rows is gone; rows sit flat inside
    // a single `.glassSection()`, no dividers (EXP-1320). "Add server" is the last row
    // of the Servers card, not a floating pill.
    private var serversSection: some View {
        sectionStack(title: "Account") {
            VStack(spacing: 0) {
                ForEach(deps.auth.accounts, id: \.id) { account in
                    NavigationLink(value: AppRoute.serverDetail(accountId: account.id)) {
                        serverRow(account)
                    }
                    .buttonStyle(.plain)
                }
                Button {
                    previousActiveAccountId = deps.auth.activeAccountId
                    showAddServer = true
                } label: {
                    HStack(spacing: 12) {
                        AppIcon(AppIcons.uiAdd, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 22)
                        Text("Add server")
                            .font(.body)
                            .foregroundStyle(.white)
                        Spacer()
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .glassSection()
        }
    }

    @ViewBuilder
    private func serverRow(_ account: ServerAccount) -> some View {
        // EXP-311: the row is always titled by the server ("Exponential
        // Cloud" / hostname) with the signed-in email below — the account
        // identity (avatar, name, email) lives in the detail view.
        HStack(spacing: 12) {
            AppIcon(AppIcons.settingsServers, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(account.displayName)
                    .font(.body)
                    .foregroundStyle(.white)
                if account.token == nil {
                    Text("Signed out")
                        .font(.caption)
                        .foregroundStyle(.orange.opacity(0.85))
                } else if let email = account.userEmail, !email.isEmpty {
                    Text(email)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            Spacer()
            AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }

    private var generalSection: some View {
        sectionStack(title: "General") {
            VStack(spacing: 0) {
                // EXP-1270: the block's entry, gone once every step is done.
                if !gettingStarted.loading, !gettingStarted.complete, !gettingStarted.entries.isEmpty {
                    Button {
                        showGettingStarted = true
                    } label: {
                        settingsRow(icon: AppIcons.navGettingStarted, title: GettingStarted.title)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(GettingStarted.TestId.entry)
                }
                // EXP-232: staging-only — exposes live shape status and a
                // "Resync now" data-wipe hatch, not for store builds.
                if AppConstants.isStaging {
                    NavigationLink(value: AppRoute.syncDebug) {
                        settingsRow(icon: AppIcons.settingsSync, title: "Sync diagnostics")
                    }
                    .buttonStyle(.plain)
                }
                NavigationLink(value: AppRoute.about) {
                    settingsRow(icon: AppIcons.settingsAbout, title: "About")
                }
                .buttonStyle(.plain)
            }
            .glassSection()
        }
    }

    /// The filled group band (`GlassSectionBand`) over each section — the
    /// team/account screens' and Android's settings root idiom. The band
    /// carries its own 4pt bottom inset; the stack adds the rest of the gap.
    @ViewBuilder
    private func sectionStack<Content: View>(title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            GlassSectionBand(title)
            content()
        }
    }

    private func settingsRow(icon: String, title: String, count: Int? = nil, chevron: Bool = true) -> some View {
        HStack(spacing: 12) {
            AppIcon(icon, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 22)
            Text(title)
                .font(.body)
                .foregroundStyle(.white)
            Spacer()
            if let count {
                Text("\(count)")
                    .font(.body)
                    .monospacedDigit()
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            if chevron {
                AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.quaternary))
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }
}
