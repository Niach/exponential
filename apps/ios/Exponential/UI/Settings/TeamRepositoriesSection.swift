import Combine
import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// The server-only repositories registry (masterplan §6 / §5.3). v4: a pure
/// registry — each row shows `owner/name`, the default branch, and the boards
/// it backs ("used by" chips from `repositories.list().boards`). Member-visible
/// since EXP-557 (per-user sharing): any member adds a repo (adding SHARES it
/// with the team), removal is sharer-or-owner per row and blocked server-side
/// (CONFLICT) while any board still points at it — that message rides a
/// toast (FEED-79).
///
/// SLOP-7/SLOP-26: the connection block above the list is the VIEWER's OWN
/// GitHub connection (web `GithubStatusLine`, copy in ExpCore `GithubCopy`):
/// not connected → Connect GitHub (the link-ticket hop, `GithubConnectSession`);
/// connected as `login` with Disconnect; an expired token → Reconnect; then
/// the accounts where the app is installed, one row each with a Configure
/// link, and Install on another account (`installUrl` in the system
/// browser). Nothing is per team any more — no stale or re-auth lines.
struct TeamRepositoriesSection: View {
    let accountId: String
    let team: TeamEntity?
    /// The viewer — sharer-or-owner row gating needs it (EXP-557).
    let currentUserId: String?
    let isOwner: Bool
    /// Synced boards — the "Used by" chips draw each board's own glyph +
    /// color from them (Android parity, EXP-577).
    var boards: [BoardEntity] = []
    let repositoriesApi: RepositoriesApi
    let integrationsApi: IntegrationsApi
    let instanceBaseURL: URL?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL
    @State private var repos: [TeamRepo] = []
    @State private var loading = true
    // EXP-1274: a reload over an already-loaded list spins the band's
    // loading glyph instead of blanking the rows.
    @State private var refreshing = false
    // FEED-79: every failure (load, remove CONFLICT, disconnect, connect hop)
    // is a toast, never a rose line in the section.
    @Environment(\.toaster) private var toaster
    @State private var removeTarget: TeamRepo?
    // Disconnecting GitHub (unlinking the viewer's account) confirms first.
    @State private var disconnectConfirm = false
    // The ONE GitHub data source (FEED-42): `status` with `platform: mobile`,
    // so its guided-page URL deep-links back via `exponential://github-connected`.
    @State private var githubStatus: GithubStatusResult?
    // A failed probe with nothing loaded renders the failed line + Retry;
    // a later failure keeps the last good value.
    @State private var githubStatusFailed = false
    @Environment(\.scenePhase) private var scenePhase
    @State private var connectSession = GithubConnectSession()
    @State private var connecting = false
    // "Add repository" picker sheet (EXP-225): the add runs inside the sheet
    // (FEED-42), which shows its own failures.
    @State private var showAddRepo = false
    // #EXP-1616: each row's checks chip rolls up its live PRs' checks
    // (`RepoPage.checksRollup` over the synced `pull_requests`).
    @State private var checksByRepo: [String: String] = [:]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            // EXP-721/818: the ONE shared header band. "Add repository" is
            // always offered (FEED-42) — the picker itself handles the
            // not-configured and not-connected states.
            GlassSectionBand(GithubCopy.sectionTitle, loading: refreshing) {
                GlassPill(GithubCopy.addRepository, icon: AppIcons.uiGithub, mode: .action {
                    showAddRepo = true
                })
            }

            // The status block sits BEFORE the list (FEED-42).
            githubStatusBlock

            if !loading && repos.isEmpty {
                EmptyState(title: GithubCopy.noRepositories, icon: AppIcons.settingsRepositories, size: .list)
            }

            ForEach(repos) { repo in
                repoRow(repo)
            }
        }
        .task(id: team?.id) { await reload() }
        .task(id: team?.id) { await observeChecks() }
        // An install/connect that finishes in an EXTERNAL browser comes back
        // via the app-level `exponential://github-connected` deep link. An
        // error slug means the connect FAILED: surface it (EXP-390).
        .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { notification in
            if notification.userInfo?["error"] != nil {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "connect", thing: "GitHub"),
                    description: GithubCopy.linkFailed
                )
            } else {
                Task { await reload() }
            }
        }
        // Fallback when the deep link never arrives: returning to the
        // foreground re-detects (EXP-365).
        .onChange(of: scenePhase) { _, phase in
            if phase == .active {
                Task { await reload() }
            }
        }
        .onDisappear { connectSession.cancel() }
        // The add lands in the registry (repositories.add) INSIDE the sheet:
        // a throw keeps the picker open with the error inline.
        .sheet(isPresented: $showAddRepo) {
            if let teamId = team?.id {
                // #EXP-1616: owners pick GitHub | Exponential first.
                AddRepositorySheet(
                    accountId: accountId,
                    teamId: teamId,
                    isOwner: isOwner,
                    integrationsApi: integrationsApi,
                    repositoriesApi: repositoriesApi,
                    onAddGithub: { repo in
                        try await repositoriesApi.add(
                            accountId: accountId,
                            teamId: teamId,
                            fullName: repo.fullName,
                            defaultBranch: repo.defaultBranch,
                            isPrivate: repo.`private`
                        )
                        // Registry changed — drop the per-team name cache.
                        RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
                        Task { await reload() }
                    },
                    onCreated: {
                        RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
                        Task { await reload() }
                    }
                )
            }
        }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4. Each card on
        // a zero-size node of its own (EXP-240): stacked on the picker's
        // node SwiftUI drops them.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $removeTarget) { repo in
                    GlassAlert(prompt: Prompts.RemoveRepository.copy(fullName: repo.fullName), handlers: [
                        "remove": {
                            Task { await mutate(thing: "the repository") { try await repositoriesApi.remove(accountId: accountId, repositoryId: repo.id) } }
                        },
                    ])
                }
        }
        // Confirm-first disconnect of the viewer's GitHub account (web
        // `GH_DISCONNECT_CONFIRM_TITLE`): repositories already added keep
        // working, their tokens mint off the App installation.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $disconnectConfirm) {
                    GlassAlert(
                        title: GithubCopy.disconnectTitle,
                        message: GithubCopy.disconnectBody,
                        actions: [
                            GlassAlertAction(GithubCopy.cancel, role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                            GlassAlertAction(GithubCopy.disconnect, role: .destructive, id: "disconnect") {
                                Task {
                                    await mutate(verb: "disconnect", thing: "GitHub") {
                                        try await integrationsApi.githubDisconnect(accountId: accountId)
                                    }
                                }
                            },
                        ]
                    )
                }
        }
    }

    // MARK: - Row

    /// Web `RepoRow`: the GitHub glyph + name, the default-branch chip (a
    /// picker for the sharer-or-owner, EXP-462), Private, the grey trash
    /// (disabled while a board still uses the repo); under it, inset, "Used
    /// by" + the board chips on one wrapping line (or "Not used by any
    /// board"), then "· Shared by …".
    @ViewBuilder
    private func repoRow(_ repo: TeamRepo) -> some View {
        let inUse = !repo.boards.isEmpty
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                // #EXP-1616: the glyph + name open the repo page.
                NavigationLink(value: AppRoute.repo(
                    accountId: accountId, teamId: team?.id ?? "", repositoryId: repo.id, isOwner: isOwner
                )) {
                    HStack(spacing: 8) {
                        if repo.isEcode {
                            AppIcon(AppIcons.settingsRepositories, size: AppIcon.Size.small)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        } else {
                            githubGlyph
                        }
                        Text(repo.fullName)
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer(minLength: 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier(RepoPage.TestIds.rowOpen)
                Spacer(minLength: 8)
                if canManage(repo) {
                    RepoDefaultBranchChip(accountId: accountId, repo: repo) { branch in
                        Task {
                            await mutate(verb: "set", thing: "the default branch") {
                                try await repositoriesApi.setDefaultBranch(
                                    accountId: accountId,
                                    repositoryId: repo.id,
                                    branch: branch
                                )
                            }
                        }
                    }
                } else {
                    GlassPill(repo.defaultBranch)
                        .mono()
                        .fixedSize()
                }
                if repo.isPrivate {
                    GlassPill("Private", icon: AppIcons.uiPrivate)
                        .fixedSize()
                }
                // Sharer-or-owner removal (EXP-557); a repo still backing a
                // board cannot go (the server refuses it too), so its trash
                // is disabled, like web's.
                if canManage(repo) {
                    GhostIconButton(
                        AppIcons.uiDelete,
                        accessibilityLabel: "Remove repository",
                        enabled: !inUse
                    ) {
                        removeTarget = repo
                    }
                }
            }

            FlowLayout(spacing: 6, centersLines: true) {
                // #EXP-1616: the provider pill and, when its live PRs carry
                // checks, the rollup chip lead the row's second line.
                RepoProviderPill(provider: repo.provider)
                if let checks = checksByRepo[repo.id] {
                    RepoChecksPill(state: checks)
                }
                if inUse {
                    Text("Used by")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    ForEach(repo.boards) { ref in
                        // EXP-862: the board's OWN icon, tinted with its colour
                        // (×4). A board the synced set has not caught up with
                        // still draws the shared fallback glyph.
                        let board = boards.first { $0.id == ref.id }
                        GlassPill(ref.name) {
                            AppIcon(
                                board.map { BoardTypeDisplay.iconName(for: $0) } ?? "square-kanban",
                                size: GlassPillTokens.glyphSm
                            )
                            .foregroundStyle(
                                Color(hex: board?.color ?? "#888888") ?? .gray
                            )
                        }
                    }
                } else {
                    Text("Not used by any board")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                // Who shared the repo with the team (EXP-557, web parity).
                if let sharer = repo.sharedBy {
                    Text("· Shared by \(sharerLabel(sharer))")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .padding(.leading, 24)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        // EXP-818: the repositories are a LIST — flat rows under the band.
        .flatRow()
    }

    // MARK: - GitHub connection block (SLOP-26, web GithubStatusLine)

    @ViewBuilder
    private var githubStatusBlock: some View {
        if let status = githubStatus {
            if !status.configured {
                githubLine(GithubCopy.notConfigured)
            } else if !status.linked {
                notLinkedLine(status)
            } else if status.needsReconnect {
                expiredLine(status)
            } else {
                connectedBlock(status)
            }
        } else if githubStatusFailed {
            // The probe is best-effort — say nothing definite, offer a retry.
            HStack(spacing: 8) {
                githubGlyph
                Text(GithubCopy.statusFailed)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer()
                GlassPill(GithubCopy.retry, icon: AppIcons.uiRefresh, mode: .action {
                    Task { await reload(retried: true) }
                })
            }
            .padding(.vertical, 4)
        }
        // Loading → nothing.
    }

    private var githubGlyph: some View {
        AppIcon(AppIcons.uiGithub, size: AppIcon.Size.small)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
    }

    private func githubLine(_ text: String) -> some View {
        HStack(spacing: 8) {
            githubGlyph
            Text(text)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
    }

    // Not connected: ONE primary fix, the link-ticket hop.
    private func notLinkedLine(_ status: GithubStatusResult) -> some View {
        HStack(spacing: 8) {
            githubGlyph
            Text(GithubCopy.notLinked)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer(minLength: 8)
            GlassPill(
                GithubCopy.connectGithub,
                icon: AppIcons.uiGithub,
                mode: .action { connect(status) },
                primary: true,
                enabled: !connecting
            )
        }
        .padding(.vertical, 4)
        .accessibilityIdentifier("github-connection-unlinked")
    }

    // Linked, token dead: Reconnect (the same hop) or the ✕ to drop the link.
    private func expiredLine(_ status: GithubStatusResult) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                .foregroundStyle(DesignTokens.Semantic.yellow)
            Text(GithubCopy.reconnectNeeded)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer(minLength: 8)
            GlassPill(
                GithubCopy.reconnect,
                icon: AppIcons.uiRefresh,
                mode: .action { connect(status) },
                primary: true,
                enabled: !connecting
            )
            disconnectButton
        }
        .padding(.vertical, 4)
        .accessibilityIdentifier("github-connection-expired")
    }

    // Connected: "Connected as login" + ✕, the suspended line (REV2-29), then
    // the accounts where the app is installed (Configure each) and Install
    // on another account — or the install nudge when there are none.
    private func connectedBlock(_ status: GithubStatusResult) -> some View {
        let suspended = status.installations.filter { $0.isSuspended }
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Circle()
                    .fill(DesignTokens.Semantic.green)
                    .frame(width: 8, height: 8)
                Text(status.login.map { GithubCopy.connectedAs($0) } ?? GithubCopy.connected)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                disconnectButton
            }
            VStack(alignment: .leading, spacing: 8) {
                if !suspended.isEmpty {
                    suspendedLine(status, suspended: suspended)
                }
                if status.installations.isEmpty {
                    HStack(alignment: .top, spacing: 8) {
                        Text(GithubCopy.notInstalled)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Spacer(minLength: 8)
                        if status.installUrl != nil {
                            GlassPill(GithubCopy.installApp, icon: AppIcons.uiAdd, mode: .action {
                                openInstall(status.installUrl)
                            }, primary: true)
                        }
                    }
                } else {
                    Text(GithubCopy.accountsHeader)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    ForEach(status.installations) { installation in
                        accountRow(installation)
                    }
                    Text(GithubCopy.installationCaption)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    if status.installUrl != nil {
                        GlassPill(GithubCopy.installAnother, icon: AppIcons.uiAdd, mode: .action {
                            openInstall(status.installUrl)
                        })
                    }
                }
            }
            .padding(.leading, 16)
        }
        .padding(.vertical, 4)
        .accessibilityIdentifier("github-connection-linked")
    }

    private var disconnectButton: some View {
        GhostIconButton(
            AppIcons.uiClose,
            accessibilityLabel: GithubCopy.disconnect,
            glyphSize: AppIcon.Size.small,
            tint: .white.opacity(TextOpacity.tertiary)
        ) {
            disconnectConfirm = true
        }
    }

    // GitHub suspended the app for an account (REV2-29): until it is
    // unsuspended no token mints — say so instead of looking healthy. A
    // reconnect cannot fix it; Manage opens the installation on GitHub.
    private func suspendedLine(_ status: GithubStatusResult, suspended: [GithubInstallation]) -> some View {
        let manage = suspended[0].manageUrl.isEmpty ? status.installUrl : suspended[0].manageUrl
        return HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                .padding(.top, 1)
            Text(GithubCopy.suspendedLine(suspended.map { installationLabel($0) }))
                .font(.subheadline)
            Spacer(minLength: 8)
            if let manage, let url = URL(string: manage) {
                Link(destination: url) {
                    GlassPill(GithubCopy.manage, tint: DesignTokens.Palette.destructive) {
                        EmptyView()
                    } trailing: {
                        AppIcon(AppIcons.uiExternalLink, size: GlassPillTokens.glyphSm)
                    }
                    .contentShape(Capsule())
                }
            }
        }
        .foregroundStyle(DesignTokens.Palette.destructive)
    }

    // One row per account with the app installed: glyph, login, Configure
    // (that installation's GitHub settings page, in the system browser).
    private func accountRow(_ installation: GithubInstallation) -> some View {
        HStack(spacing: 8) {
            AppIcon(
                installation.accountType == "Organization" ? AppIcons.uiOrganization : AppIcons.uiAvatarPlaceholder,
                size: AppIcon.Size.small
            )
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(installationLabel(installation))
                .font(.subheadline)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            if !installation.manageUrl.isEmpty, let url = URL(string: installation.manageUrl) {
                Link(destination: url) {
                    HStack(spacing: 4) {
                        Text(GithubCopy.configure)
                            .font(.caption.weight(.medium))
                        AppIcon(AppIcons.uiExternalLink, size: 11)
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                .accessibilityLabel(GithubCopy.configureTitle(installationLabel(installation)))
            }
        }
    }

    private func installationLabel(_ installation: GithubInstallation) -> String {
        GithubCopy.installationLabel(login: installation.accountLogin, installationId: installation.installationId)
    }

    // Sharer-or-owner (EXP-557): who may remove the row. Mirrors the server's
    // assertCanManageRepository (sharedBy.id == viewer OR team owner).
    private func canManage(_ repo: TeamRepo) -> Bool {
        if isOwner { return true }
        guard let me = currentUserId, let sharer = repo.sharedBy?.id else { return false }
        return sharer == me
    }

    private func sharerLabel(_ sharer: RepoSharer) -> String {
        if let name = sharer.name, !name.isEmpty { return name }
        return sharer.email ?? "a teammate"
    }

    // Connect (or reconnect) the viewer's GitHub account: the link-ticket
    // hop in the in-app auth sheet, the guided page as the fallback. Every
    // outcome but a failure re-probes.
    private func connect(_ status: GithubStatusResult) {
        guard !connecting else { return }
        connecting = true
        connectSession.start(deps: deps, accountId: accountId, connectUrl: status.connectUrl) { outcome in
            connecting = false
            switch outcome {
            case .connected, .cancelled:
                Task { await reload() }
            case let .failed(message):
                toaster.error(FeedbackStates.Errors.title(verb: "connect", thing: "GitHub"), description: message)
            }
        }
    }

    // GitHub's install page in the system browser: the App's setup URL lands
    // on the web page, whose "Return to the app" deep-links back here.
    private func openInstall(_ urlString: String?) {
        guard let urlString, let url = URL(string: urlString) else { return }
        openURL(url)
    }

    // MARK: - Data (server-only registry; refetched after every mutation)

    /// The team's synced PRs → each repo's checks rollup.
    private func observeChecks() async {
        guard let teamId = team?.id, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try PullRequestEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }
        do {
            for try await rows in observation.removeDuplicates().values(in: pool) {
                let byRepo = Dictionary(grouping: rows, by: \.repositoryId)
                checksByRepo = byRepo.compactMapValues { RepoPage.checksRollup($0.map(RepoPage.Pr.init)) }
            }
        } catch {}
    }

    /// `retried` = the person's own Retry tap: its failure toasts (D10);
    /// the loads on appear and after writes stay quiet.
    private func reload(retried: Bool = false) async {
        guard let teamId = team?.id else { return }
        loading = repos.isEmpty
        refreshing = !repos.isEmpty
        defer {
            loading = false
            refreshing = false
        }
        do {
            repos = try await repositoriesApi.list(accountId: accountId, teamId: teamId)
        } catch {
            // FEED-79: the list loads on appear and after every write, so a
            // failure is quiet: the empty state (or the last good rows).
        }
        // Non-fatal: keep the last good value on failure; with nothing loaded
        // the failed line + Retry renders.
        do {
            githubStatus = try await integrationsApi.githubStatus(
                accountId: accountId,
                teamId: teamId,
                mobile: true
            )
            githubStatusFailed = false
        } catch {
            githubStatusFailed = true
            if retried {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "load", thing: "GitHub"),
                    description: error.trpcUserMessage,
                    action: ToastAction(label: FeedbackStates.Errors.retry) {
                        Task { @MainActor in await reload(retried: true) }
                    }
                )
            }
        }
    }

    private func mutate(verb: String = "remove", thing: String, _ operation: () async throws -> Void) async {
        do {
            try await operation()
            // Registry changed — drop the per-team name cache used by chips.
            if let teamId = team?.id {
                RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
            }
        } catch {
            // FEED-79: the server message (remove CONFLICT "repository backs N
            // boards", unlink CONFLICT) rides the toast's description.
            toaster.error(FeedbackStates.Errors.title(verb: verb, thing: thing), description: error.trpcUserMessage)
        }
        removeTarget = nil
        await reload()
    }
}

/// The repo row's default-branch chip as the shared `BranchPicker` (web
/// `DefaultBranchMenu`, EXP-462): the monospace branch + a chevron; the
/// branches load when the sheet opens. Picking GitHub's own default reports
/// nil (the pin clears, the repo follows GitHub again).
private struct RepoDefaultBranchChip: View {
    let accountId: String
    let repo: TeamRepo
    let onPick: (String?) -> Void

    @Environment(AppDependencies.self) private var deps
    // FEED-79: a failed listing is the picker's empty state + Retry; only a
    // failed Retry tap toasts.
    @Environment(\.toaster) private var toaster
    @State private var open = false
    @State private var branches: [String]?
    @State private var loadFailed = false

    var body: some View {
        BranchPicker(
            branches: branches,
            value: repo.defaultBranch,
            repoDefault: repo.githubDefaultBranch,
            onChange: onPick,
            onRetry: loadFailed ? { Task { await load(retried: true) } } : nil,
            open: $open
        ) {
            // The pill's own tap is off; the trigger takes it.
            ZStack {
                GlassPill(repo.defaultBranch) {
                    EmptyView()
                } trailing: {
                    AppIcon(AppIcons.uiChevronDown, size: 12)
                }
                .mono()
                .fixedSize()
                .allowsHitTesting(false)
            }
            .contentShape(Capsule())
        }
        .accessibilityLabel("Default branch for \(repo.fullName)")
        .onChange(of: open) { _, isOpen in
            if isOpen { Task { await load() } }
        }
    }

    private func load(retried: Bool = false) async {
        if branches?.isEmpty == true { branches = nil }
        loadFailed = false
        do {
            branches = try await deps.repositoriesApi.listBranches(accountId: accountId, repositoryId: repo.id)
        } catch {
            branches = []
            loadFailed = true
            guard retried else { return }
            toaster.error(
                FeedbackStates.Errors.title(verb: "load", thing: "branches"),
                description: error.trpcUserMessage
            )
        }
    }
}
