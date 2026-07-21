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
    // Repository ids backing a protected board (the dogfood board). Removal is
    // refused server-side while any board points at a repo, and doubly so for
    // a protected one — hide the affordance. Computed by the parent from the
    // already-observed team boards.
    var protectedRepositoryIds: Set<String> = []

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
    // repositories-section.tsx's "Connect repository" dialog).
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
                // only once a GitHub account is linked — before that the picker
                // has nothing to offer.
                if isOwner, let github, !github.installations.isEmpty {
                    Button {
                        showAddRepo = true
                    } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "plus")
                                .font(.caption2.weight(.semibold))
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

            // One grouped GitHub card (EXP-228): connect/reconnect entry point,
            // linked-account chips, and the reconnect explainer, in a single
            // glassRow. Only rendered once `github` is loaded (non-nil) to keep
            // the flicker-free behavior. Visible when any account is linked
            // (every member) OR the viewer is an owner (owners always see the
            // connect entry point); hidden for non-owners with zero
            // installations, matching today's behavior.
            if let github, (!github.installations.isEmpty || isOwner) {
                githubCard(github)
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
        .alert("Remove Repository", isPresented: Binding(
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
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .font(.caption)
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
                    Image(systemName: "lock.fill")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                if isOwner && !protectedRepositoryIds.contains(repo.id) {
                    Button {
                        removeTarget = repo
                    } label: {
                        Image(systemName: "trash")
                            .font(.caption)
                            .foregroundStyle(.red.opacity(0.5))
                    }
                    .buttonStyle(.plain)
                }
            }

            // "Used by" board chips (v4 — computed from boards.repositoryId).
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

    // MARK: - GitHub card (EXP-228)

    // One grouped card that replaces the old loose stack of connected-accounts
    // caption/chips + separate reconnect notice + connect button:
    //   • header row — caption + a compact Connect/Reconnect button (owner-gated;
    //     the header button doubles as the reconnect action, so there is no
    //     inner full-width button anymore),
    //   • the installation chips (visible to every member; a one-line hint for
    //     an owner with zero installations),
    //   • the reconnect explainer under the chips when a grant is missing.
    @ViewBuilder
    private func githubCard(_ github: GithubReposResult) -> some View {
        // A linked installation with no captured grants yields zero repos until
        // the owner re-runs the OAuth connect (grant-model fail-closed state).
        let needsReauth = github.installations.contains(where: { $0.needsReauth })
        VStack(alignment: .leading, spacing: 10) {
            // Header: caption + connect/reconnect entry point. Owner-gated: the
            // connect hop always ends in the owner-only team claim
            // (assertCanManageRepos), so a member would dead-end on a forbidden
            // page. The web link survives only as an owner fallback when the
            // server has no GitHub App configured / mints no URLs.
            HStack {
                Text("Connected GitHub accounts")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Spacer()
                if isOwner, github.configured, (github.connectUrl ?? github.installUrl) != nil {
                    Button {
                        openConnect(github)
                    } label: {
                        HStack(spacing: 4) {
                            Image(systemName: needsReauth
                                ? "arrow.triangle.2.circlepath"
                                : "chevron.left.forwardslash.chevron.right")
                                .font(.caption2.weight(.semibold))
                            Text(needsReauth ? "Reconnect" : "Connect GitHub")
                                .font(.caption.weight(.medium))
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .glassButton()
                    }
                    .buttonStyle(.plain)
                } else if isOwner, let url = webRepositoriesURL {
                    Link(destination: url) {
                        HStack(spacing: 4) {
                            Image(systemName: "arrow.up.right.square")
                                .font(.caption2)
                            Text("Connect on the web")
                                .font(.caption.weight(.medium))
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                }
            }

            // Linked-account chips (visible to every member). A yellow glyph
            // marks an installation whose grants were never captured; the
            // reconnect explainer below spells it out. An owner with zero
            // installations gets a one-line hint instead.
            if github.installations.isEmpty {
                if isOwner {
                    Text("No GitHub account connected yet.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            } else {
                FlowLayout(spacing: 6) {
                    ForEach(github.installations) { inst in
                        HStack(spacing: 6) {
                            Image(systemName: inst.accountType == "Organization" ? "building.2" : "person")
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            Text(inst.accountLogin ?? "Installation \(inst.installationId)")
                                .font(.caption.weight(.medium))
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            if inst.needsReauth {
                                Image(systemName: "exclamationmark.triangle")
                                    .font(.caption2)
                                    .foregroundStyle(.yellow.opacity(0.8))
                            }
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .glassButton()
                    }
                }
            }

            // Reconnect explainer — owner-gated, under the chips. The header
            // Reconnect button is the action.
            if isOwner, needsReauth {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 6) {
                        Image(systemName: "exclamationmark.triangle")
                            .font(.caption)
                            .foregroundStyle(.yellow.opacity(0.8))
                        Text("GitHub needs to be reconnected")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white)
                    }
                    Text("We only list repositories you can access on GitHub, so repos created or shared with you since your last connect won't appear until you reconnect.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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
