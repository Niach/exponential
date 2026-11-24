import ExpUI
import ExpCore
import SwiftUI

import GRDB

/// A settings row: the glyph, the title, the count, the chevron.
struct SettingsLinkRow<Leading: View>: View {
    let title: String
    var count: Int?
    var chevron = true
    @ViewBuilder let leading: Leading

    var body: some View {
        HStack(spacing: 12) {
            leading
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

/// A settings row's glyph at the list's size.
struct SettingsRowGlyph: View {
    let icon: String

    var body: some View {
        AppIcon(icon, size: AppIcon.Size.medium)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
    }
}

/// #EXP-2109 (settings-nav.json `nav.phone.settings`): the SETTINGS TAB: the
/// viewer's card (opens Account), account / assistant / notifications /
/// security, Devices, About, then the app's own rows (each server's signed-in
/// account and Add server, the staging Sync diagnostics, the device's Keep
/// tool steps). Nothing team related.
struct SettingsTabView: View {
    @Environment(AppDependencies.self) private var deps
    @State private var showAddServer = false
    // The account that was active before Add-server opened, so a cancelled /
    // incomplete add can be rolled back off the tokenless pending account.
    @State private var previousActiveAccountId: String?
    /// #EDE-114 THE STORAGE SETTING (this device): `Keep tool steps`.
    @State private var keepToolSteps = KeepToolSteps.windowDays()
    /// The viewer's synced row (the card's image).
    @State private var user: UserEntity?

    private var accountId: String { deps.auth.activeAccountId ?? "" }
    private var account: ServerAccount? { deps.auth.accounts.first { $0.id == accountId } }

    /// `phone.settings` split into its cards.
    private static let groups = [["account", "assistant", "notifications", "security"], ["devices"], ["about"]]

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    viewerCard
                    ForEach(Self.groups, id: \.self) { group in
                        VStack(spacing: 0) {
                            ForEach(SettingsNav.phoneSettings.filter(group.contains), id: \.self) { row($0) }
                        }
                        .glassSection()
                    }
                    serversSection
                    deviceSection
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 16)
            }
        }
        .pageHeader(Chats.Copy.Headers.settings)
        .task(id: accountId) { await observeUser() }
        .accessibilityIdentifier("settings-tab")
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

    // MARK: - Viewer

    @ViewBuilder
    private var viewerCard: some View {
        if let account {
            let name = user?.name ?? account.userName ?? account.userEmail ?? account.displayName
            NavigationLink(value: AppRoute.serverDetail(accountId: accountId)) {
                HStack(spacing: 14) {
                    UserAvatar(
                        user: user ?? UserEntity(
                            id: account.userId ?? "", name: account.userName, email: account.userEmail ?? "", image: nil,
                            createdAt: "", updatedAt: ""
                        ),
                        id: account.userId, size: 56
                    )
                    VStack(alignment: .leading, spacing: 2) {
                        Text(name)
                            .font(.system(size: 17, weight: .semibold))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if let email = account.userEmail, email != name {
                            Text(email)
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                        }
                    }
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 4)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("settings-viewer")
        }
    }

    private func observeUser() async {
        user = nil
        guard let userId = account?.userId, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in try UserEntity.fetchOne(db, key: userId) }.removeDuplicates()
        do {
            for try await next in observation.values(in: pool) { user = next }
        } catch {}
    }

    // MARK: - Rows

    @ViewBuilder
    private func row(_ id: String) -> some View {
        switch id {
        case "account":
            link(.serverDetail(accountId: accountId), id) { SettingsRowGlyph(icon: icon(id)) }
        case "assistant":
            // #EXP-1987: the viewer's ONE Assistant, its brand mark.
            link(.assistantSettings(accountId: accountId), id, title: AgentSettings.AssistantCopy.title) {
                AssistantAvatar(size: 22)
            }
            .accessibilityIdentifier("assistant-settings-row")
        case "notifications":
            link(.notificationSettings(accountId: accountId), id) { SettingsRowGlyph(icon: icon(id)) }
                .accessibilityIdentifier("settings-notifications-row")
        case "security":
            link(.securitySettings(accountId: accountId), id) { SettingsRowGlyph(icon: icon(id)) }
                .accessibilityIdentifier("settings-security-row")
        case "devices":
            link(.devices(accountId: accountId), id) { SettingsRowGlyph(icon: icon(id)) }
                .accessibilityIdentifier("settings-devices-row")
        case "about":
            link(.about, id) { SettingsRowGlyph(icon: icon(id)) }
        default:
            EmptyView()
        }
    }

    private func icon(_ id: String) -> String {
        SettingsNav.page(id).flatMap { AppIcons.conceptOrName($0.icon) } ?? AppIcons.uiChevronRight
    }

    private func link<Leading: View>(
        _ route: AppRoute, _ id: String, title: String? = nil, @ViewBuilder leading: () -> Leading
    ) -> some View {
        NavigationLink(value: route) {
            SettingsLinkRow(title: title ?? SettingsNav.page(id)?.label ?? "", leading: leading)
        }
        .buttonStyle(.plain)
    }

    private var serversSection: some View {
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
                SettingsLinkRow(title: "Add server", chevron: false) { SettingsRowGlyph(icon: AppIcons.uiAdd) }
            }
            .buttonStyle(.plain)
        }
        .glassSection()
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

    /// The device's own rows (the staging Sync diagnostics, Keep tool
    /// steps); gone when it has none.
    @ViewBuilder
    private var deviceSection: some View {
        let showStorage = ChatStoreHub.shared.isAvailable
        if showStorage || AppConstants.isStaging {
            VStack(spacing: 0) {
                // EXP-232: staging-only — exposes live shape status and a
                // "Resync now" data-wipe hatch, not for store builds.
                if AppConstants.isStaging {
                    NavigationLink(value: AppRoute.syncDebug) {
                        SettingsLinkRow(title: "Sync diagnostics") { SettingsRowGlyph(icon: AppIcons.settingsSync) }
                    }
                    .buttonStyle(.plain)
                }
                if showStorage {
                    keepToolStepsRow
                }
            }
            .glassSection()
        }
    }

    /// #EDE-114 (chat-store.json THE STORAGE SETTING): how long this device
    /// keeps tool steps in its chat store; a shorter window applies at once.
    private var keepToolStepsRow: some View {
        let options = KeepToolSteps.options
        return GlassPicker(
            items: options.indices.map { PickerItem(value: $0, label: KeepToolSteps.optionLabel(options[$0])) },
            mode: .single,
            value: options.firstIndex(of: keepToolSteps).map { [$0] } ?? [],
            onChange: { picked in
                guard let index = picked.first, options.indices.contains(index) else { return }
                keepToolSteps = options[index]
                ChatStoreHub.shared.setWindowDays(options[index])
            },
            title: KeepToolSteps.label
        ) {
            // THE picker row (label, value, chevron; Android `PickerValueRow`).
            GlassPickerRowLabel(KeepToolSteps.label, value: KeepToolSteps.optionLabel(keepToolSteps))
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .contentShape(Rectangle())
        }
        .accessibilityIdentifier("settings-keep-tool-steps")
    }
}

/// #EXP-2109 (settings-nav.json `nav.phone`): Settings › Security: how the
/// account signs in (methods, passkeys; EXP-1126).
struct SecuritySettingsPage: View {
    let accountId: String

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    SignInMethodsSection(accountId: accountId)
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 24)
            }
        }
        .pageHeader(SettingsNav.page("security")?.label ?? "", pushed: true)
        .accessibilityIdentifier("security-settings")
    }
}

/// #EXP-2109 (settings-nav.json `nav.phone.team`): a team's SETTINGS page
/// (its `…` menu): `phone.team` in order, each row as before (visibility,
/// count, its own page); then the team's Getting started while it has steps.
struct TeamSettingsView: View {
    private struct Counts: Equatable, Sendable {
        var members = 0
        var boards = 0
        var agents = 0
        var isOwner = false
    }

    let teamId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var counts = Counts()
    @State private var gettingStarted = GettingStartedProgress()
    @State private var showGettingStarted = false

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(spacing: 18) {
                    VStack(spacing: 0) {
                        // Visibility is the fixture's: owner rows for the
                        // team's owner; the natives are never "cloud".
                        ForEach(SettingsNav.visiblePhoneTeam(isOwner: counts.isOwner), id: \.self) { id in
                            if let section = TeamSettingsSection(id) { link(section, row: SettingsNav.row(id)) }
                        }
                    }
                    .glassSection()
                    // EXP-1270: gone once every step is done.
                    if !gettingStarted.loading, !gettingStarted.complete, !gettingStarted.entries.isEmpty {
                        Button { showGettingStarted = true } label: {
                            SettingsLinkRow(title: GettingStarted.title) {
                                SettingsRowGlyph(icon: AppIcons.navGettingStarted)
                            }
                        }
                        .buttonStyle(.plain)
                        .glassSection()
                        .accessibilityIdentifier(GettingStarted.TestId.entry)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 16)
            }
        }
        .pageHeader(Chats.Copy.Headers.settings, pushed: true)
        .sheet(isPresented: $showGettingStarted) {
            GettingStartedSheet(progress: gettingStarted, accountId: accountId, teamId: teamId)
        }
        .task(id: "\(accountId)/\(teamId)") {
            gettingStarted.bind(accountId: accountId, teamId: teamId, deps: deps)
            await observeCounts()
        }
        .onDisappear { gettingStarted.stop() }
        .accessibilityIdentifier("team-settings")
    }

    private func count(_ count: SettingsNav.Count?) -> Int? {
        switch count {
        case .members: counts.members
        case .boards: counts.boards
        case .agents: counts.agents
        case nil: nil
        }
    }

    private func link(_ section: TeamSettingsSection, row: SettingsNav.Row) -> some View {
        NavigationLink(value: AppRoute.teamSettingsSection(accountId: accountId, teamId: teamId, section: section)) {
            SettingsLinkRow(title: row.label, count: count(row.count)) {
                SettingsRowGlyph(icon: AppIcons.conceptOrName(row.icon) ?? AppIcons.uiChevronRight)
            }
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(SettingsNav.rowTestId)
    }

    private func observeCounts() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let teamId = teamId
        let userId = deps.auth.userId
        let observation = ValueObservation.tracking { db in
            Counts(
                members: try TeamMemberEntity.filter(Column("team_id") == teamId).fetchCount(db),
                boards: try BoardEntity.filter(Column("team_id") == teamId).fetchCount(db),
                // S-3: the team agents the Agents page lists; #EXP-1987: the
                // viewer's own Assistant is Settings › Assistant, never here.
                agents: try SessionQueries.teamAgents(db: db, teamId: teamId, me: userId)
                    .filter { !$0.isAssistant && AgentSettings.visibleAgent($0, me: userId) }.count,
                isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
            )
        }.removeDuplicates()
        do {
            for try await next in observation.values(in: pool) { counts = next }
        } catch {}
    }
}
