import ExpCore
import ExpUI
import SwiftUI

/// The server-only repositories registry (masterplan §7a): lists the
/// workspace's connected repos with their project links; owners can remove a
/// repo, link/unlink projects, and set the primary clone target. Connecting
/// NEW repos (the GitHub-App install flow) is web-only — this section links
/// out to the web workspace settings for that. Mirrors the Android
/// `RepositoriesSection` + the web repositories-section semantics.
struct WorkspaceRepositoriesSection: View {
    let accountId: String
    let workspace: WorkspaceEntity?
    let projects: [ProjectEntity]
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
            Text("This disconnects \(removeTarget?.fullName ?? "this repository") from the workspace. Project links are removed too.")
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

            FlowLayout(spacing: 6) {
                if repo.projectLinks.isEmpty {
                    Text("No projects linked")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.vertical, 4)
                }
                ForEach(repo.projectLinks, id: \.projectId) { link in
                    projectChip(repo: repo, link: link)
                }
                linkProjectMenu(repo)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    /// One linked project as a chip: star = primary clone target (owners tap
    /// to promote), x = unlink (web parity).
    @ViewBuilder
    private func projectChip(repo: WorkspaceRepo, link: RepoProjectLink) -> some View {
        let project = projects.first { $0.id == link.projectId }
        HStack(spacing: 5) {
            Button {
                guard isOwner, !link.isPrimary else { return }
                Task { await mutate { try await repositoriesApi.setPrimary(accountId: accountId, projectId: link.projectId, repositoryId: repo.id) } }
            } label: {
                Image(systemName: link.isPrimary ? "star.fill" : "star")
                    .font(.caption2)
                    .foregroundStyle(link.isPrimary ? Color.yellow : .white.opacity(TextOpacity.tertiary))
            }
            .buttonStyle(.plain)
            .disabled(!isOwner || link.isPrimary)

            Text(project?.name ?? "Unknown project")
                .font(.caption)
                .foregroundStyle(.white)
                .lineLimit(1)

            if isOwner {
                Button {
                    Task { await mutate { try await repositoriesApi.unlinkProject(accountId: accountId, projectId: link.projectId, repositoryId: repo.id) } }
                } label: {
                    Image(systemName: "xmark")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .glassButton(isActive: link.isPrimary)
    }

    /// Owner-only "+ Link project" chip offering the not-yet-linked projects.
    @ViewBuilder
    private func linkProjectMenu(_ repo: WorkspaceRepo) -> some View {
        let linkedIds = Set(repo.projectLinks.map(\.projectId))
        let unlinked = projects.filter { !linkedIds.contains($0.id) }
        if isOwner, !unlinked.isEmpty {
            Menu {
                ForEach(unlinked, id: \.id) { project in
                    Button(project.name) {
                        Task { await mutate { try await repositoriesApi.linkProject(accountId: accountId, projectId: project.id, repositoryId: repo.id) } }
                    }
                }
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "plus")
                        .font(.caption2)
                    Text("Link project")
                        .font(.caption)
                }
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .padding(.horizontal, 8)
                .padding(.vertical, 5)
                .glassButton()
            }
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

    private func reload() async {
        guard let workspaceId = workspace?.id else { return }
        loading = repos.isEmpty
        defer { loading = false }
        do {
            repos = try await repositoriesApi.list(accountId: accountId, workspaceId: workspaceId)
            errorText = nil
        } catch {
            errorText = error.localizedDescription
        }
    }

    private func mutate(_ operation: () async throws -> Void) async {
        do {
            try await operation()
            errorText = nil
        } catch {
            errorText = error.localizedDescription
        }
        removeTarget = nil
        await reload()
    }
}
