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
/// NEW repos (the GitHub-App install flow) stays web-only, but the grant-model
/// reconnect (re-capturing which repos the user can access) runs in-app — a
/// workspace linked before per-user grants existed lists zero repos until
/// someone re-runs the OAuth connect, and an iOS-only user must be able to do
/// that without a desktop (web parity: repositories-section.tsx).
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
    // GitHub grant state — drives the reconnect notice. Fetched via the `repos`
    // endpoint (not `status`) because only it accepts `platform: "mobile"`, so
    // the minted connect URL deep-links back via `exp://github-connected` and
    // auto-dismisses the in-app auth session.
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
            // captured grants yields zero repos everywhere until a member
            // re-runs the OAuth connect. Any member's reconnect captures THEIR
            // grants, so this isn't owner-gated.
            if let github, github.installations.contains(where: { $0.needsReauth }) {
                reconnectNotice(github)
            }

            if let errorText {
                Text(errorText)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }

            if isOwner, let url = webSettingsURL {
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
            Text("This disconnects \(removeTarget?.fullName ?? "this repository") from the workspace.")
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
                    reconnect(github)
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "arrow.triangle.2.circlepath")
                        Text("Reconnect GitHub")
                    }
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
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
    // GithubRepoPicker): re-captures which repos this user can access. It must
    // be `connectUrl` — the install page does NOT re-capture grants — with
    // `installUrl` only as the no-OAuth-secret fallback. The completion fires
    // on callback AND manual dismissal, so re-query regardless.
    private func reconnect(_ github: GithubReposResult) {
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
