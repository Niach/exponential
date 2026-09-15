import Combine
import ExpCore
import ExpUI
import SwiftUI

/// The server-only repositories registry (masterplan §6 / §5.3). v4: a pure
/// registry — each row shows `owner/name`, the default branch, and the boards
/// it backs ("used by" chips from `repositories.list().boards`). Member-visible
/// since EXP-557 (per-user sharing): the status block and the picker show the
/// VIEWER's own GitHub connections/repos (the server scopes them), any member
/// connects GitHub / adds a repo (connecting SHARES it with the team), and
/// removal is sharer-or-owner per row. Removal is blocked server-side
/// (CONFLICT) while any board still points at it, and that message is surfaced
/// inline. Connecting GitHub runs fully IN-APP (EXP-45), same
/// ASWebAuthenticationSession flow as GithubRepoPicker.
///
/// FEED-42: the connection block follows the canonical ×4 spec (copy in ExpCore
/// `GithubCopy`): header pill, intro, the status block BEFORE the list, then
/// the list. Accounts, re-auth and stale marks all come from
/// `integrations.github.status`, exactly as web and desktop read them.
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

    @State private var repos: [TeamRepo] = []
    @State private var loading = true
    // Mutation failures (remove/CONFLICT/unlink). Kept SEPARATE from load
    // failures so a failed mutation survives its own post-mutation reload
    // (EXP-365).
    @State private var errorText: String?
    // repositories.list failures — rendered from the same inline slot.
    @State private var loadErrorText: String?
    @State private var removeTarget: TeamRepo?
    // Account disconnect confirmation — live (✕) and stale rows share it.
    @State private var disconnectTarget: GithubInstallation?
    // The ONE GitHub data source (FEED-42): `status` with `platform: mobile`,
    // so its connect URL deep-links back via `exponential://github-connected`.
    @State private var githubStatus: GithubStatusResult?
    // A failed probe with nothing loaded renders the failed line + Retry;
    // a later failure keeps the last good value.
    @State private var githubStatusFailed = false
    @Environment(\.scenePhase) private var scenePhase
    @State private var connectSession = InstallWebAuthSession()
    // A failed GitHub connect hop's message (EXP-390) — separate from
    // `errorText`, which belongs to mutations and is cleared by `mutate`.
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

            Text(GithubCopy.intro)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 4)

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
            if let slug = notification.userInfo?["error"] as? String {
                connectError = GithubConnect.errorMessage(for: slug)
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
        // Confirm-first account disconnect (EXP-557 stale rows, FEED-31 ✕).
        .alert(GithubCopy.disconnectTitle, isPresented: Binding(
            get: { disconnectTarget != nil },
            set: { if !$0 { disconnectTarget = nil } }
        )) {
            Button(GithubCopy.cancel, role: .cancel) { disconnectTarget = nil }
            Button(GithubCopy.disconnect, role: .destructive) {
                if let installation = disconnectTarget, let teamId = team?.id {
                    Task {
                        await mutate {
                            try await integrationsApi.githubUnlink(
                                accountId: accountId,
                                teamId: teamId,
                                installationId: installation.installationId
                            )
                        }
                    }
                }
            }
        } message: {
            Text(disconnectMessage)
        }
    }

    // A LIVE link gets honest copy (the server refuses while a connected repo
    // still rides it); a stale one the "nothing is lost" copy.
    private var disconnectMessage: String {
        let label = disconnectTarget.map { installationLabel($0) } ?? "this account"
        let stale = disconnectTarget.map { target in
            staleAccounts.contains { $0.installationId == target.installationId }
        } ?? false
        return stale ? GithubCopy.staleConfirm(label) : GithubCopy.unlinkConfirm(label)
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

    // MARK: - GitHub status block (FEED-42 spec A)

    @ViewBuilder
    private var githubStatusBlock: some View {
        if let status = githubStatus {
            let suspended = status.installations.filter { $0.isSuspended }
            if !status.configured {
                githubLine(GithubCopy.notConfigured)
            } else if !status.installed {
                notInstalledLine(status)
            } else if !suspended.isEmpty {
                suspendedLine(status, suspended: suspended)
            } else {
                installedAccountsBlock(status)
            }
        } else if githubStatusFailed {
            // The probe is best-effort — say nothing definite, offer a retry.
            HStack(spacing: 8) {
                githubGlyph
                Text(GithubCopy.statusFailed)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer()
                GlassPill(GithubCopy.retry, mode: .action {
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

    // Primary = the OAuth hop (finds installations the viewer already
    // controls); the secondary goes straight to the account picker — only
    // worth a second pill when both URLs exist (FEED-31).
    private func notInstalledLine(_ status: GithubStatusResult) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                githubGlyph
                Text(GithubCopy.notInstalled)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer(minLength: 0)
            }
            if connectHopUrl(status) != nil {
                FlowLayout(spacing: 8) {
                    GlassPill(GithubCopy.connectGithub, mode: .action { openHop(connectHopUrl(status)) })
                    if status.connectUrl != nil, status.installUrl != nil {
                        GlassPill(GithubCopy.installOnAnAccount, mode: .action { openHop(status.installUrl) })
                    }
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // Suspension outranks everything (REV2-29): a reconnect CANNOT fix it —
    // only unsuspending on GitHub can. The whole line is destructive and no
    // stale lines render under it.
    private func suspendedLine(_ status: GithubStatusResult, suspended: [GithubInstallation]) -> some View {
        let manage = suspended[0].manageUrl.isEmpty ? status.installUrl : suspended[0].manageUrl
        return VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                    .padding(.top, 1)
                Text(GithubCopy.suspendedLine(suspended.map { installationLabel($0) }))
                    .font(.caption)
                Spacer(minLength: 0)
            }
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
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // FEED-31 (web GithubStatusLine, installed state): header, one indented
    // row per account (glyph, login, Configure, ✕ + confirm), the caption, the
    // two SEPARATE actions, then the re-auth line and the stale lines.
    private func installedAccountsBlock(_ status: GithubStatusResult) -> some View {
        let stale = staleAccounts
        let staleIds = Set(stale.map { $0.installationId })
        let reauthInstalls = status.installations.filter {
            $0.needsReauth && !staleIds.contains($0.installationId)
        }
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                if reauthInstalls.isEmpty {
                    Circle()
                        .fill(DesignTokens.Semantic.green)
                        .frame(width: 8, height: 8)
                } else {
                    AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                        .foregroundStyle(DesignTokens.Semantic.yellow)
                }
                Text(GithubCopy.accountsHeader)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            VStack(alignment: .leading, spacing: 8) {
                ForEach(status.installations) { installation in
                    accountRow(installation)
                }
                Text(GithubCopy.installationsCaption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                if status.installUrl != nil || status.connectUrl != nil {
                    FlowLayout(spacing: 8) {
                        if status.installUrl != nil {
                            GlassPill(GithubCopy.connectAnotherAccount, icon: AppIcons.uiAdd, mode: .action { openHop(status.installUrl) })
                        }
                        if status.connectUrl != nil {
                            GlassPill(GithubCopy.refreshAccess, icon: AppIcons.uiRefresh, mode: .action { openHop(status.connectUrl) })
                        }
                    }
                }
                if !reauthInstalls.isEmpty {
                    HStack(spacing: 8) {
                        Text(GithubCopy.reauthLine(reauthInstalls.map { installationLabel($0) }))
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Spacer(minLength: 8)
                        if connectHopUrl(status) != nil {
                            GlassPill(GithubCopy.reconnect, mode: .action { openHop(connectHopUrl(status)) })
                        }
                    }
                }
                // Stale accounts (EXP-557): no reconnect can ever refresh them
                // — a Disconnect instead of a permanent nag (EXP-556).
                ForEach(stale) { installation in
                    HStack(alignment: .top, spacing: 8) {
                        AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                            .foregroundStyle(DesignTokens.Semantic.yellow)
                            .padding(.top, 1)
                        Text(GithubCopy.staleLine(installationLabel(installation)))
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Spacer(minLength: 8)
                        GlassPill(GithubCopy.disconnectAccount, mode: .action {
                            disconnectTarget = installation
                        })
                    }
                }
            }
            .padding(.leading, 16)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

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
            }
            GhostIconButton(
                AppIcons.uiClose,
                accessibilityLabel: GithubCopy.disconnectAccessibility,
                glyphSize: AppIcon.Size.small,
                tint: .white.opacity(TextOpacity.tertiary)
            ) {
                disconnectTarget = installation
            }
        }
    }

    // Linked installations with zero grants from ANY member — only the
    // `status` endpoint carries the mark. Suspended installs are excluded
    // (their fix is an unsuspend, REV2-29).
    private var staleAccounts: [GithubInstallation] {
        (githubStatus?.installations ?? []).filter { $0.isStale && !$0.isSuspended }
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

    // The OAuth connect hop re-captures grants; `installUrl` is only the
    // no-OAuth-secret fallback.
    private func connectHopUrl(_ status: GithubStatusResult) -> String? {
        status.connectUrl ?? status.installUrl
    }

    // The in-app hop (ASWebAuthenticationSession, same flow as
    // GithubRepoPicker) for an explicit URL. The completion fires on callback
    // AND manual dismissal, so re-query regardless.
    private func openHop(_ urlString: String?) {
        guard let urlString, let url = URL(string: urlString) else { return }
        connectError = nil
        connectSession.start(url: url) { errorSlug in
            connectError = errorSlug.map { GithubConnect.errorMessage(for: $0) }
            Task { await reload() }
        }
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
        disconnectTarget = nil
        await reload()
    }
}
