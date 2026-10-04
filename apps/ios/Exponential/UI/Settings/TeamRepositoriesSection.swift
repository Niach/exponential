import Combine
import ExpCore
import ExpUI
import SwiftUI

/// The server-only repositories registry (masterplan §6 / §5.3). v4: a pure
/// registry — each row shows `owner/name`, the default branch, and the boards
/// it backs ("used by" chips from `repositories.list().boards`). Member-visible
/// since EXP-557 (per-user sharing): any member adds a repo (adding SHARES it
/// with the team), removal is sharer-or-owner per row and blocked server-side
/// (CONFLICT) while any board still points at it — that message is surfaced
/// inline.
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
    // Mutation failures (remove/CONFLICT/disconnect). Kept SEPARATE from load
    // failures so a failed mutation survives its own post-mutation reload
    // (EXP-365).
    @State private var errorText: String?
    // repositories.list failures — rendered from the same inline slot.
    @State private var loadErrorText: String?
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
    // A failed GitHub connect hop's message — separate from `errorText`,
    // which belongs to mutations and is cleared by `mutate`.
    @State private var connectError: String?
    // "Add repository" picker sheet (EXP-225): the add runs inside the sheet
    // (FEED-42), which shows its own failures.
    @State private var showAddRepo = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            // EXP-721/818: the ONE shared header band. "Add repository" is
            // always offered (FEED-42) — the picker itself handles the
            // not-configured and not-connected states.
            GlassSectionBand(GithubCopy.sectionTitle) {
                GlassPill(GithubCopy.addRepository, icon: AppIcons.uiGithub, mode: .action {
                    showAddRepo = true
                })
            }

            // The status block sits BEFORE the list (FEED-42).
            githubStatusBlock

            // Above the list so a failure is on-screen even for teams with
            // many repos (EXP-365). `connectError` is the failed connect hop.
            if let message = connectError ?? errorText ?? loadErrorText {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }

            if !loading && repos.isEmpty {
                Text(GithubCopy.noRepositories)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            }

            ForEach(repos) { repo in
                repoRow(repo)
            }
        }
        .task(id: team?.id) { await reload() }
        // An install/connect that finishes in an EXTERNAL browser comes back
        // via the app-level `exponential://github-connected` deep link. An
        // error slug means the connect FAILED: surface it (EXP-390).
        .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { notification in
            if notification.userInfo?["error"] != nil {
                connectError = GithubCopy.linkFailed
            } else {
                connectError = nil
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
                GithubRepoPicker(
                    accountId: accountId,
                    teamId: teamId,
                    integrationsApi: integrationsApi
                ) { repo in
                    try await repositoriesApi.add(
                        accountId: accountId,
                        teamId: teamId,
                        fullName: repo.fullName,
                        defaultBranch: repo.defaultBranch,
                        isPrivate: repo.`private`
                    )
                    // Registry changed — drop the per-team name cache.
                    RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
                    errorText = nil
                    Task { await reload() }
                }
            }
        }
        .alert("Remove repository", isPresented: Binding(
            get: { removeTarget != nil },
            set: { if !$0 { removeTarget = nil } }
        )) {
            Button("Cancel", role: .cancel) { removeTarget = nil }
            Button("Remove", role: .destructive) {
                if let repo = removeTarget {
                    Task { await mutate { try await repositoriesApi.remove(accountId: accountId, repositoryId: repo.id) } }
                }
            }
        } message: {
            Text("This disconnects \(removeTarget?.fullName ?? "this repository") from the team.")
        }
        // Confirm-first disconnect of the viewer's GitHub account (web
        // `GH_DISCONNECT_CONFIRM_TITLE`): repositories already added keep
        // working, their tokens mint off the App installation.
        .alert(GithubCopy.disconnectTitle, isPresented: $disconnectConfirm) {
            Button(GithubCopy.cancel, role: .cancel) { disconnectConfirm = false }
            Button(GithubCopy.disconnect, role: .destructive) {
                Task {
                    await mutate {
                        try await integrationsApi.githubDisconnect(accountId: accountId)
                    }
                }
            }
        } message: {
            Text(GithubCopy.disconnectBody)
        }
    }

    // MARK: - Row

    @ViewBuilder
    private func repoRow(_ repo: TeamRepo) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiRepository, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(repo.fullName)
                    .font(.subheadline.monospaced())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                Text(repo.defaultBranch)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                if repo.isPrivate {
                    AppIcon(AppIcons.uiPrivate, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                // Sharer-or-owner removal (EXP-557; the server refuses it
                // while any board still points at the repo); the tap opens a
                // confirmation.
                // EXP-721: the shared chromed circle (Boards/Labels parity).
                if canManage(repo) {
                    GhostIconButton(
                        AppIcons.uiDelete,
                        accessibilityLabel: "Remove repository",
                        tint: DesignTokens.Palette.destructive.opacity(0.7)
                    ) {
                        removeTarget = repo
                    }
                }
            }

            // Who shared the repo with the team (EXP-557, web parity).
            if let sharer = repo.sharedBy {
                Text("Shared by \(sharerLabel(sharer))")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }

            // "Used by" board chips (v4 — computed from boards.repositoryId).
            if !repo.boards.isEmpty {
                Text("Used by")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            FlowLayout(spacing: 6) {
                if repo.boards.isEmpty {
                    Text("Not used by any board")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.vertical, 4)
                }
                ForEach(repo.boards) { ref in
                    // EXP-862: the board's OWN icon, tinted with its colour
                    // (×4 — web `getBoardIcon`, desktop `icons::board_icon`,
                    // Android's `BoardIcon`). A board the synced set has not
                    // caught up with still draws the shared fallback glyph
                    // rather than a bare chip.
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
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        // EXP-818: the repositories are a LIST — flat rows under the band. The
        // GitHub status/notice rows below stay carded: they are notices, not
        // items of this list.
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
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer()
                GlassPill(GithubCopy.retry, icon: AppIcons.uiRefresh, mode: .action {
                    Task { await reload() }
                })
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
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
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // Not connected: ONE primary fix, the link-ticket hop.
    private func notLinkedLine(_ status: GithubStatusResult) -> some View {
        HStack(spacing: 8) {
            githubGlyph
            Text(GithubCopy.notLinked)
                .font(.caption)
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
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
        .accessibilityIdentifier("github-connection-unlinked")
    }

    // Linked, token dead: Reconnect (the same hop) or the ✕ to drop the link.
    private func expiredLine(_ status: GithubStatusResult) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                .foregroundStyle(DesignTokens.Semantic.yellow)
            Text(GithubCopy.reconnectNeeded)
                .font(.caption)
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
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
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
                    .font(.caption)
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
                            .font(.caption)
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
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
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
                .font(.caption)
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
        connectError = nil
        connecting = true
        connectSession.start(deps: deps, accountId: accountId, connectUrl: status.connectUrl) { outcome in
            connecting = false
            switch outcome {
            case .connected, .cancelled:
                Task { await reload() }
            case let .failed(message):
                connectError = message
            }
        }
    }

    // GitHub's install page in the system browser: the App's setup URL lands
    // on the web page, whose "Return to the app" deep-links back here.
    private func openInstall(_ urlString: String?) {
        guard let urlString, let url = URL(string: urlString) else { return }
        connectError = nil
        openURL(url)
    }

    // MARK: - Data (server-only registry; refetched after every mutation)

    private func reload() async {
        guard let teamId = team?.id else { return }
        loading = repos.isEmpty
        defer { loading = false }
        do {
            repos = try await repositoriesApi.list(accountId: accountId, teamId: teamId)
            // Only the LOAD error clears here — a mutation failure must
            // survive its own post-mutation reload (EXP-365).
            loadErrorText = nil
        } catch {
            loadErrorText = error.trpcUserMessage
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
        }
    }

    private func mutate(_ operation: () async throws -> Void) async {
        errorText = nil
        do {
            try await operation()
            // Registry changed — drop the per-team name cache used by chips.
            if let teamId = team?.id {
                RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
            }
        } catch {
            // Surfaces the server message (remove CONFLICT "repository backs N
            // boards", unlink CONFLICT). Stays visible through the reload.
            errorText = error.trpcUserMessage
        }
        removeTarget = nil
        await reload()
    }
}
