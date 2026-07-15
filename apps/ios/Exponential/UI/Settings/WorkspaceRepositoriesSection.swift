import Combine
import ExpCore
import ExpUI
import SwiftUI

/// The server-only repositories registry (masterplan §6 / §5.3). v4: a pure
/// registry — each row shows `owner/name`, the default branch, and the projects
/// it backs ("used by" chips from `repositories.list().projects`). Owners can
/// remove a repo; removal is blocked server-side (CONFLICT) while any project
/// still points at it, and that message is surfaced inline. The primary-star and
/// per-project link/unlink UI is gone (a project now owns exactly one repo, set
/// at creation or via the projects section's "Change repository"). Connecting
/// GitHub (the App install / grant-capture OAuth hop) runs fully IN-APP
/// (EXP-45), same ASWebAuthenticationSession flow as GithubRepoPicker — the
/// old "connect on the web" Safari bounce survives only as a fallback when the
/// server has no GitHub App configured. The grant-model reconnect (re-capturing
/// which repos the user can access) uses the same hop — a workspace linked
/// before per-user grants existed lists zero repos until the owner re-runs the
/// OAuth connect (web parity: repositories-section.tsx).
struct WorkspaceRepositoriesSection: View {
    let accountId: String
    let workspace: WorkspaceEntity?
    let isOwner: Bool
    let repositoriesApi: RepositoriesApi
    let integrationsApi: IntegrationsApi
    let instanceBaseURL: URL?
    // Repository ids backing a protected project (the dogfood board). Removal is
    // refused server-side while any project points at a repo, and doubly so for
    // a protected one — hide the affordance. Computed by the parent from the
    // already-observed workspace projects.
    var protectedRepositoryIds: Set<String> = []

    @State private var repos: [WorkspaceRepo] = []
    @State private var loading = true
    @State private var errorText: String?
    @State private var removeTarget: WorkspaceRepo?
    // GitHub grant state — drives the connect button + reconnect notice.
    // Fetched via the `repos` endpoint (not `status`) because only it accepts
    // `platform: "mobile"`, so the minted connect URL deep-links back via
    // `exponential://github-connected` and auto-dismisses the in-app session.
    @State private var github: GithubReposResult?
    @State private var connectSession = InstallWebAuthSession()

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Repositories")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("\(repos.count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Spacer()
                if loading {
                    ProgressView().controlSize(.small).tint(.white.opacity(0.5))
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

            // Grant-model fail-closed state: a linked installation with no
            // captured grants yields zero repos everywhere until the owner
            // re-runs the OAuth connect. Owner-gated like every connect
            // surface (web/Android parity) — the hop always runs the workspace
            // CLAIM, whose callback is assertCanManageRepos (owner-only), so a
            // member would finish the whole OAuth dance only to dead-end on a
            // forbidden page that never fires exponential://github-connected.
            if isOwner, let github, github.installations.contains(where: { $0.needsReauth }) {
                reconnectNotice(github)
            }

            if let errorText {
                Text(errorText)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }

            // In-app connect (EXP-45): the same ASWebAuthenticationSession hop
            // as GithubRepoPicker.openConnect. Owner-gated (web hides the whole
            // section behind canManageRepos; Android wraps this button in
            // isOwner): the connect hop always ends in the workspace claim,
            // which is owner-only server-side — see the reconnect note above.
            // The web link survives only as an owner fallback when the server
            // has no GitHub App configured / mints no URLs.
            if isOwner, let github, github.configured,
               (github.connectUrl ?? github.installUrl) != nil {
                Button {
                    openConnect(github)
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                            .font(.caption)
                        Text("Connect GitHub")
                            .font(.caption.weight(.medium))
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                }
                .glassButton()
                .buttonStyle(.plain)
            } else if isOwner, let url = webSettingsURL {
                Link(destination: url) {
                    HStack(spacing: 6) {
                        Image(systemName: "arrow.up.right.square")
                            .font(.caption)
                        Text("Connect repositories on the web")
                            .font(.caption.weight(.medium))
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
        }
        .task(id: workspace?.id) { await reload() }
        // An install/connect that finishes in an EXTERNAL browser comes back
        // via the app-level `exponential://github-connected` deep link instead
        // of the auth-session callback — re-query so the new grants appear
        // (GithubRepoPicker parity).
        .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { _ in
            Task { await reload(refreshGithub: true) }
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
    private func repoRow(_ repo: WorkspaceRepo) -> some View {
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

            // "Used by" project chips (v4 — computed from projects.repositoryId).
            FlowLayout(spacing: 6) {
                if repo.projects.isEmpty {
                    Text("Not used by any project")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.vertical, 4)
                }
                ForEach(repo.projects) { project in
                    Text(project.name)
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

    // MARK: - Reconnect (grant model)

    @ViewBuilder
    private func reconnectNotice(_ github: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 8) {
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
            if (github.connectUrl ?? github.installUrl) != nil {
                Button {
                    openConnect(github)
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "arrow.triangle.2.circlepath")
                        Text("Reconnect GitHub")
                    }
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    // .plain buttons hit-test only opaque label pixels — the
                    // stretched transparent frame ignored taps outside the
                    // text (the glassButton background lives on the Button,
                    // not the label). Cover the whole capsule.
                    .contentShape(Rectangle())
                }
                .glassButton()
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // The in-app OAuth connect hop (ASWebAuthenticationSession, same flow as
    // GithubRepoPicker.openConnect): claims a GitHub account for the workspace
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

    private var webSettingsURL: URL? {
        guard let base = instanceBaseURL, let slug = workspace?.slug else { return nil }
        let baseString = base.absoluteString.hasSuffix("/")
            ? String(base.absoluteString.dropLast())
            : base.absoluteString
        return URL(string: "\(baseString)/w/\(slug)/settings")
    }

    private func reload(refreshGithub: Bool = false) async {
        guard let workspaceId = workspace?.id else { return }
        loading = repos.isEmpty
        defer { loading = false }
        do {
            repos = try await repositoriesApi.list(accountId: accountId, workspaceId: workspaceId)
            errorText = nil
        } catch {
            errorText = error.trpcUserMessage
        }
        // Non-fatal: the grant state only powers the reconnect notice. Bypass
        // the server's repo cache right after a reconnect hop.
        github = try? await integrationsApi.githubRepos(
            accountId: accountId,
            workspaceId: workspaceId,
            refresh: refreshGithub
        )
    }

    private func mutate(_ operation: () async throws -> Void) async {
        do {
            try await operation()
            errorText = nil
            // Registry changed — drop the per-workspace name cache used by chips.
            if let workspaceId = workspace?.id {
                RepositoryDirectory.invalidate(accountId: accountId, workspaceId: workspaceId)
            }
        } catch {
            // Surfaces the server CONFLICT ("repository backs N projects") message.
            errorText = error.trpcUserMessage
        }
        removeTarget = nil
        await reload()
    }
}
