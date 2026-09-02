import Combine
import ExpCore
import ExpUI
import SwiftUI

/// The server-only repositories registry (masterplan §6 / §5.3). v4: a pure
/// registry — each row shows `owner/name`, the default branch, and the boards
/// it backs ("used by" chips from `repositories.list().boards`). Member-visible
/// since EXP-557 (per-user sharing): the status line and the pickers show the
/// VIEWER's own GitHub connections/repos (the server scopes them), any member
/// connects GitHub / adds a repo (connecting SHARES it with the team), and
/// removal is sharer-or-owner per row. Removal is blocked server-side
/// (CONFLICT) while any board still points at it, and that message is surfaced
/// inline. The primary-star and per-board link/unlink UI is gone (a board now
/// owns exactly one repo, set at creation or via the boards section's "Change
/// repository"). Connecting GitHub (the App install / grant-capture OAuth hop)
/// runs fully IN-APP (EXP-45), same ASWebAuthenticationSession flow as
/// GithubRepoPicker — the old "connect on the web" Safari bounce survives only
/// as a fallback when the server has no GitHub App configured. The grant-model
/// reconnect (re-capturing which repos the user can access) uses the same hop —
/// an installation whose grants were never captured lists zero repos until its
/// user re-runs the OAuth connect (web parity: repositories-section.tsx).
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
    // Mutation failures (add/remove/CONFLICT). Kept SEPARATE from load
    // failures: the post-mutation reload used to blank this on success, so a
    // failed `repositories.add` flashed for one round-trip and vanished
    // (EXP-365).
    @State private var errorText: String?
    // repositories.list failures — rendered from the same inline slot.
    @State private var loadErrorText: String?
    @State private var removeTarget: TeamRepo?
    // Stale-account disconnect confirmation (EXP-557).
    @State private var disconnectTarget: GithubInstallation?
    // GitHub grant state — drives the connect button + reconnect notice.
    // Fetched via the `repos` endpoint (not `status`) because only it accepts
    // `platform: "mobile"`, so the minted connect URL deep-links back via
    // `exponential://github-connected` and auto-dismisses the in-app session.
    @State private var github: GithubReposResult?
    // The `status` endpoint result rides along ONLY for its `stale` marks
    // (EXP-557) — the `repos` endpoint never emits them (an owner's stale
    // extras aren't part of the viewer's own connections).
    @State private var githubStatus: GithubStatusResult?
    // A failed grant query must not silently hide the whole GitHub surface
    // (connect button included) — keep the last good value and offer a retry.
    @State private var githubLoadFailed = false
    @Environment(\.scenePhase) private var scenePhase
    @State private var connectSession = InstallWebAuthSession()
    // A failed GitHub connect hop's message (EXP-390) — separate from
    // `errorText`, which belongs to mutations and is cleared by `mutate`.
    @State private var connectError: String?
    // "Add repository" picker sheet (EXP-225): registers a repo in the
    // server-only registry via repositories.add (web parity —
    // repositories-section.tsx's "Add repository" dialog).
    @State private var showAddRepo = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Repositories")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("\(repos.count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                if loading {
                    ProgressView().controlSize(.small).tint(.white.opacity(0.5))
                }

                Spacer()

                // "Add repository" moved into the header (Boards' "New board"
                // pattern, EXP-228). Member-level since EXP-557 (connecting a
                // repo SHARES it with the team) and only once the server has a
                // GitHub App — the picker itself handles the not-yet-connected
                // case with its inline connect hop.
                if let github, github.configured {
                    GlassPill("Add repository", icon: AppIcons.uiAdd, mode: .action {
                        showAddRepo = true
                    })
                }
            }

            // Above the list so a failure is on-screen even for teams with
            // many repos (EXP-365 — failed adds used to be invisible).
            // `connectError` is the failed GitHub connect hop (EXP-390).
            if let message = connectError ?? errorText ?? loadErrorText {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }

            if !loading && repos.isEmpty {
                Text("No repositories connected.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            }

            ForEach(repos) { repo in
                repoRow(repo)
            }

            // One GitHub status line (EXP-329, byte-identical to web/desktop).
            // Only rendered once `github` is loaded (non-nil) to keep the
            // flicker-free behavior. Every member sees it since EXP-557 — the
            // connect entry point is member-level (you connect your OWN
            // GitHub) and the line shows the viewer's own connections.
            if let github {
                githubStatusLine(github)
                // Stale accounts (EXP-557): linked installations no
                // reconnect can ever refresh (zero grants from anyone) — the
                // server scopes who sees them; render a Disconnect instead of
                // the permanent reconnect nag (EXP-556).
                ForEach(staleAccounts) { installation in
                    staleAccountRow(installation)
                }
            } else if github == nil, githubLoadFailed {
                // The grant query failed and nothing was ever loaded — say so
                // instead of silently hiding the connect entry point.
                HStack(spacing: 8) {
                    AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                        .foregroundStyle(.yellow.opacity(0.8))
                    Text("Couldn't load the GitHub connection state.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Spacer()
                    GlassPill("Retry", mode: .action {
                        Task { await reload(refreshGithub: true) }
                    })
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .glassRow()
            }
        }
        .task(id: team?.id) { await reload() }
        // An install/connect that finishes in an EXTERNAL browser comes back
        // via the app-level `exponential://github-connected` deep link instead
        // of the auth-session callback — re-query so the new grants appear
        // (GithubRepoPicker parity). An error slug means the connect FAILED:
        // surface it instead of refreshing (EXP-390).
        .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { notification in
            if let slug = notification.userInfo?["error"] as? String {
                connectError = GithubConnect.errorMessage(for: slug)
            } else {
                connectError = nil
                Task { await reload(refreshGithub: true) }
            }
        }
        // Fallback when the deep link never arrives (external-browser install,
        // swallowed handoff): returning to the foreground re-detects, same as
        // GithubRepoPicker (EXP-365).
        .onChange(of: scenePhase) { _, phase in
            if phase == .active {
                Task { await reload(refreshGithub: true) }
            }
        }
        // Same picker + presentation as RepositorySelector's add-by-name path;
        // here the pick lands in the registry directly (repositories.add). The
        // picker dismisses itself after onPick.
        .sheet(isPresented: $showAddRepo) {
            if let teamId = team?.id {
                GithubRepoPicker(
                    accountId: accountId,
                    teamId: teamId,
                    integrationsApi: integrationsApi
                ) { repo in
                    Task {
                        await mutate {
                            try await repositoriesApi.add(
                                accountId: accountId,
                                teamId: teamId,
                                fullName: repo.fullName,
                                defaultBranch: repo.defaultBranch,
                                isPrivate: repo.`private`
                            )
                        }
                    }
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
        // Confirm-first stale-account disconnect (EXP-557, web parity).
        .alert("Disconnect GitHub account", isPresented: Binding(
            get: { disconnectTarget != nil },
            set: { if !$0 { disconnectTarget = nil } }
        )) {
            Button("Cancel", role: .cancel) { disconnectTarget = nil }
            Button("Disconnect", role: .destructive) {
                if let installation = disconnectTarget, let teamId = team?.id {
                    Task {
                        await mutate(refreshGithub: true) {
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
            Text("This removes \(disconnectTarget.map { installationLabel($0) } ?? "this account") from the team. Nobody's GitHub connection covers it, so no repositories are lost.")
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
                if canManage(repo) {
                    Button {
                        removeTarget = repo
                    } label: {
                        AppIcon(AppIcons.uiDelete, size: AppIcon.Size.small)
                            .foregroundStyle(.red.opacity(0.5))
                    }
                    .buttonStyle(.plain)
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
                    GlassPill(ref.name) {
                        if let board = boards.first(where: { $0.id == ref.id }) {
                            AppIcon(BoardTypeDisplay.iconName(for: board), size: GlassPillTokens.glyphSm)
                                .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
                        }
                    }
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // MARK: - GitHub status line (EXP-329)

    // One status LINE, byte-identical to web and desktop: the connection state
    // on the left (green dot when connected, amber glyph when a grant went
    // stale) and the single member-level action on the right. It replaces the old
    // accounts card — caption, installation chips and reconnect explainer all
    // collapse into this one row.
    @ViewBuilder
    private func githubStatusLine(_ github: GithubReposResult) -> some View {
        // Suspension outranks everything (REV2-29): a suspended installation
        // mints no tokens and lists no repos, and a reconnect CANNOT fix it —
        // only unsuspending on GitHub can. Never nudge the wrong fix.
        let suspended = github.installations.filter { $0.isSuspended }
        // A linked installation with no captured grants yields zero repos
        // until its user re-runs the OAuth connect (grant-model fail-closed
        // state). STALE installations are excluded (EXP-557): reconnecting can
        // never refresh them — they get the Disconnect row instead.
        let staleIds = Set(staleAccounts.map { $0.installationId })
        let reauthInstalls = github.installations.filter {
            $0.needsReauth && !$0.isSuspended && !staleIds.contains($0.installationId)
        }
        let needsReauth = suspended.isEmpty && !reauthInstalls.isEmpty
        let label: (GithubInstallation) -> String = { installationLabel($0) }
        let logins = github.installations.map(label).joined(separator: ", ")
        let status: String = { () -> String in
            if !github.configured { return "GitHub isn't configured on this server." }
            if !suspended.isEmpty {
                return "GitHub suspended the Exponential app for \(suspended.map(label).joined(separator: ", ")). Unsuspend it on GitHub."
            }
            if needsReauth {
                return "Reconnect GitHub to refresh which repositories you can access from \(reauthInstalls.map(label).joined(separator: ", "))."
            }
            if github.installations.isEmpty { return "No GitHub account connected" }
            return "GitHub: \(logins)"
        }()
        HStack(spacing: 8) {
            if github.configured, !suspended.isEmpty {
                AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                    .foregroundStyle(.red.opacity(0.8))
            } else if github.configured, needsReauth {
                AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                    .foregroundStyle(.yellow.opacity(0.8))
            } else if github.configured, !github.installations.isEmpty {
                Circle()
                    .fill(Color.green)
                    .frame(width: 8, height: 8)
            }
            Text(status)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(3)
            Spacer()
            // Member-level action since EXP-557: any member connects their
            // OWN GitHub (connecting shares repos with the team). Nothing to
            // offer when the server has no GitHub App at all.
            if github.configured {
                if !suspended.isEmpty {
                    // Unsuspend happens on GitHub's installation settings page.
                    if let url = URL(string: suspended[0].manageUrl) {
                        Link(destination: url) {
                            GlassPill("Manage") {
                                AppIcon(AppIcons.uiExternalLink, size: GlassPillTokens.glyphSm)
                            }
                            .contentShape(Capsule())
                        }
                    }
                } else if (github.connectUrl ?? github.installUrl) != nil {
                    GlassPill(
                        needsReauth ? "Reconnect" : (github.installations.isEmpty ? "Connect GitHub" : "Manage"),
                        icon: needsReauth ? AppIcons.uiRefresh : AppIcons.uiGithub,
                        mode: .action { openConnect(github) }
                    )
                } else if let url = webRepositoriesURL {
                    // The server mints no connect/install URL — the web
                    // repositories page explains and handles it.
                    Link(destination: url) {
                        HStack(spacing: 4) {
                            AppIcon(AppIcons.uiExternalLink, size: 11)
                            Text("Connect on the web")
                                .font(.caption.weight(.medium))
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // MARK: - Stale accounts (EXP-557)

    // Linked installations with zero grants from ANY member — a reconnect can
    // never refresh them. Only the `status` endpoint carries the mark.
    // Suspended installs are excluded (their fix is an unsuspend on GitHub,
    // REV2-29).
    private var staleAccounts: [GithubInstallation] {
        (githubStatus?.installations ?? []).filter { $0.isStale && !$0.isSuspended }
    }

    // One row per stale account: what's wrong + the confirm-first Disconnect.
    // Visible to whoever the server sends the entry (owners see every stale
    // link; a member sees their own) — the unlink itself is server-enforced
    // link-creator-or-owner.
    @ViewBuilder
    private func staleAccountRow(_ installation: GithubInstallation) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                .foregroundStyle(.yellow.opacity(0.8))
            Text("No one's GitHub connection covers \(installationLabel(installation)) anymore — reconnecting can't refresh it.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(3)
            Spacer()
            GlassPill("Disconnect account", mode: .action {
                disconnectTarget = installation
            })
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    private func installationLabel(_ installation: GithubInstallation) -> String {
        installation.accountLogin ?? "Installation \(installation.installationId)"
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

    // The in-app OAuth connect hop (ASWebAuthenticationSession, same flow as
    // GithubRepoPicker.openConnect): claims a GitHub account for the team
    // and (re-)captures which repos this user can access. It must be
    // `connectUrl` — the install page does NOT re-capture grants — with
    // `installUrl` only as the no-OAuth-secret fallback. The completion fires
    // on callback AND manual dismissal, so re-query regardless. Shared by the
    // "Connect GitHub" button and the reconnect notice.
    private func openConnect(_ github: GithubReposResult) {
        guard let urlString = github.connectUrl ?? github.installUrl,
              let url = URL(string: urlString) else { return }
        connectError = nil
        connectSession.start(url: url) { errorSlug in
            connectError = errorSlug.map { GithubConnect.errorMessage(for: $0) }
            Task { await reload(refreshGithub: true) }
        }
    }

    // MARK: - Data (server-only registry; refetched after every mutation)

    // Deep-links straight at the repositories subpage rather than the settings
    // index: the index is a section menu, and landing an iOS user on a menu that
    // offers Plan & Billing is exactly the App Store 3.1.1 exposure we avoid.
    // Billing stays web-only and is never surfaced as a destination from here.
    private var webRepositoriesURL: URL? {
        guard let base = instanceBaseURL, let slug = team?.slug else { return nil }
        let baseString = base.absoluteString.hasSuffix("/")
            ? String(base.absoluteString.dropLast())
            : base.absoluteString
        return URL(string: "\(baseString)/t/\(slug)/settings/repositories")
    }

    private func reload(refreshGithub: Bool = false) async {
        guard let teamId = team?.id else { return }
        loading = repos.isEmpty
        defer { loading = false }
        do {
            repos = try await repositoriesApi.list(accountId: accountId, teamId: teamId)
            // Only the LOAD error clears here — a mutation failure set by
            // `mutate` must survive its own post-mutation reload (EXP-365:
            // failed adds were wiped after one round-trip).
            loadErrorText = nil
        } catch {
            loadErrorText = error.trpcUserMessage
        }
        // Non-fatal: the grant state only powers the reconnect notice + connect
        // button. Keep the previous value on failure — nilling it hid the
        // whole GitHub surface with no message. Bypass the server's repo cache
        // right after a reconnect hop.
        do {
            github = try await integrationsApi.githubRepos(
                accountId: accountId,
                teamId: teamId,
                refresh: refreshGithub
            )
            githubLoadFailed = false
        } catch {
            githubLoadFailed = true
        }
        // Stale marks ride ONLY the `status` endpoint (EXP-557) — fetched
        // separately and non-fatal: on failure the Disconnect rows simply
        // don't render, and the last good value is kept.
        do {
            githubStatus = try await integrationsApi.githubStatus(
                accountId: accountId,
                teamId: teamId
            )
        } catch {}
    }

    // `refreshGithub` bypasses the server's per-user repo cache on the
    // post-mutation reload — the stale-account unlink changes the linked set.
    private func mutate(refreshGithub: Bool = false, _ operation: () async throws -> Void) async {
        errorText = nil
        do {
            try await operation()
            // Registry changed — drop the per-team name cache used by chips.
            if let teamId = team?.id {
                RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
            }
        } catch {
            // Surfaces the server message (add FORBIDDEN/PRECONDITION_FAILED,
            // remove CONFLICT "repository backs N boards"). Stays visible
            // through the reload below.
            errorText = error.trpcUserMessage
        }
        removeTarget = nil
        disconnectTarget = nil
        await reload(refreshGithub: refreshGithub)
    }
}
