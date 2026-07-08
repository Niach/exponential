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
/// NEW repos (the GitHub-App install flow) stays web-only.
struct WorkspaceRepositoriesSection: View {
    let accountId: String
    let workspace: WorkspaceEntity?
    let isOwner: Bool
    let repositoriesApi: RepositoriesApi
    let instanceBaseURL: URL?

    @State private var repos: [WorkspaceRepo] = []
    @State private var loading = true
    @State private var errorText: String?
    @State private var removeTarget: WorkspaceRepo?

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
                if isOwner {
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

    // MARK: - Data (server-only registry; refetched after every mutation)

    private var webSettingsURL: URL? {
        guard let base = instanceBaseURL, let slug = workspace?.slug else { return nil }
        let baseString = base.absoluteString.hasSuffix("/")
            ? String(base.absoluteString.dropLast())
            : base.absoluteString
        return URL(string: "\(baseString)/w/\(slug)/settings")
    }

    private func reload() async {
        guard let workspaceId = workspace?.id else { return }
        loading = repos.isEmpty
        defer { loading = false }
        do {
            repos = try await repositoriesApi.list(accountId: accountId, workspaceId: workspaceId)
            errorText = nil
        } catch {
            errorText = error.trpcUserMessage
        }
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
