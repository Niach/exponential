import Combine
import ExpCore
import ExpUI
import SwiftUI

/// The server-only repositories registry (masterplan §6 / §5.3). v4: a pure
/// registry — each row shows `owner/name`, the default branch, and the boards
/// it backs ("used by" chips from `repositories.list().boards`). Owners can
/// remove a repo; removal is blocked server-side (CONFLICT) while any board
/// still points at it, and that message is surfaced inline. The primary-star and
/// per-board link/unlink UI is gone (a board now owns exactly one repo, set
/// at creation or via the boards section's "Change repository"). Connecting
/// GitHub (the App install / grant-capture OAuth hop) runs fully IN-APP
/// (EXP-45), same ASWebAuthenticationSession flow as GithubRepoPicker — the
/// old "connect on the web" Safari bounce survives only as a fallback when the
/// server has no GitHub App configured. The grant-model reconnect (re-capturing
/// which repos the user can access) uses the same hop — a team linked
/// before per-user grants existed lists zero repos until the owner re-runs the
/// OAuth connect (web parity: repositories-section.tsx).
struct TeamRepositoriesSection: View {
    let accountId: String
    let team: TeamEntity?
    let isOwner: Bool
    let repositoriesApi: RepositoriesApi
    let integrationsApi: IntegrationsApi
    let instanceBaseURL: URL?

    @State private var repos: [TeamRepo] = []
    @State private var loading = true
    @State private var errorText: String?
    @State private var removeTarget: TeamRepo?
    // GitHub grant state — drives the connect button + reconnect notice.
    // Fetched via the `repos` endpoint (not `status`) because only it accepts
    // `platform: "mobile"`, so the minted connect URL deep-links back via
    // `exponential://github-connected` and auto-dismisses the in-app session.
    @State private var github: GithubReposResult?
    @State private var connectSession = InstallWebAuthSession()
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
                // pattern, EXP-228). Owner-gated like the connect hop
                // (repositories.add is assertCanManageRepos server-side) and
                // only once the server has a GitHub App — the picker itself
                // handles the not-yet-connected case with its inline connect hop.
                if isOwner, let github, github.configured {
                    Button {
                        showAddRepo = true
                    } label: {
                        HStack(spacing: 4) {
                            AppIcon(AppIcons.uiAdd, size: 11, weight: .semibold)
                            Text("Add repository")
                                .font(.caption.weight(.medium))
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .glassButton()
                    }
                    .buttonStyle(.plain)
                }
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
            // flicker-free behavior. Visible when any account is linked (every
            // member) OR the viewer is an owner (owners always see the connect
            // entry point); hidden for non-owners with zero installations.
            if let github, (!github.installations.isEmpty || isOwner) {
                githubStatusLine(github)
            }

            if let errorText {
                Text(errorText)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }
        }
        .task(id: team?.id) { await reload() }
        // An install/connect that finishes in an EXTERNAL browser comes back
        // via the app-level `exponential://github-connected` deep link instead
        // of the auth-session callback — re-query so the new grants appear
        // (GithubRepoPicker parity).
        .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { _ in
            Task { await reload(refreshGithub: true) }
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
                .presentationBackground(.ultraThinMaterial)
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
                // Owner-only removal (the server refuses it while any board
                // still points at the repo); the tap opens a confirmation.
                if isOwner {
                    Button {
                        removeTarget = repo
                    } label: {
                        AppIcon(AppIcons.uiDelete, size: AppIcon.Size.small)
                            .foregroundStyle(.red.opacity(0.5))
                    }
                    .buttonStyle(.plain)
                }
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
                ForEach(repo.boards) { board in
                    Text(board.name)
                        .font(.caption)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 5)
                        .glassButton()
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
    // stale) and the single owner action on the right. It replaces the old
    // accounts card — caption, installation chips and reconnect explainer all
    // collapse into this one row.
    @ViewBuilder
    private func githubStatusLine(_ github: GithubReposResult) -> some View {
        // A linked installation with no captured grants yields zero repos until
        // the owner re-runs the OAuth connect (grant-model fail-closed state).
        let needsReauth = github.installations.contains(where: { $0.needsReauth })
        let logins = github.installations
            .map { $0.accountLogin ?? "Installation \($0.installationId)" }
            .joined(separator: ", ")
        let status: String = { () -> String in
            if !github.configured { return "GitHub isn't configured on this server." }
            if needsReauth { return "Reconnect GitHub to refresh which repositories you can access." }
            if github.installations.isEmpty { return "No GitHub account connected" }
            return "GitHub: \(logins)"
        }()
        HStack(spacing: 8) {
            if github.configured, needsReauth {
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
                .lineLimit(2)
            Spacer()
            // Owner-gated action: the connect hop always ends in the owner-only
            // team claim (assertCanManageRepos), so a member would dead-end on a
            // forbidden page — non-owners see just the status. Nothing to offer
            // when the server has no GitHub App at all.
            if isOwner, github.configured {
                if (github.connectUrl ?? github.installUrl) != nil {
                    Button {
                        openConnect(github)
                    } label: {
                        HStack(spacing: 4) {
                            AppIcon(needsReauth ? AppIcons.uiRefresh : AppIcons.uiGithub,
                                    size: 11, weight: .semibold)
                            Text(needsReauth ? "Reconnect" : (github.installations.isEmpty ? "Connect GitHub" : "Manage"))
                                .font(.caption.weight(.medium))
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .glassButton()
                    }
                    .buttonStyle(.plain)
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
        connectSession.start(url: url) {
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
            errorText = nil
        } catch {
            errorText = error.trpcUserMessage
        }
        // Non-fatal: the grant state only powers the reconnect notice. Bypass
        // the server's repo cache right after a reconnect hop.
        github = try? await integrationsApi.githubRepos(
            accountId: accountId,
            teamId: teamId,
            refresh: refreshGithub
        )
    }

    private func mutate(_ operation: () async throws -> Void) async {
        do {
            try await operation()
            errorText = nil
            // Registry changed — drop the per-team name cache used by chips.
            if let teamId = team?.id {
                RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
            }
        } catch {
            // Surfaces the server CONFLICT ("repository backs N boards") message.
            errorText = error.trpcUserMessage
        }
        removeTarget = nil
        await reload()
    }
}
