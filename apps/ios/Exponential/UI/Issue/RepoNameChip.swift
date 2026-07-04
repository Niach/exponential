import ExpUI
import ExpCore
import SwiftUI

/// Resolves the server-only repositories registry once per workspace and caches
/// it for the app's lifetime (mirrors SteerConfigCache's fetch-once pattern).
/// The synced `projects.repositoryId` is a uuid; the fullName/defaultBranch live
/// only behind the repositories tRPC API, so every surface that shows a repo
/// name (project header, issue coding section) reads through this cache.
@MainActor
enum RepositoryDirectory {
    private static var cache: [String: [WorkspaceRepo]] = [:]

    private static func key(_ accountId: String, _ workspaceId: String) -> String {
        "\(accountId)|\(workspaceId)"
    }

    static func repos(accountId: String, workspaceId: String, api: RepositoriesApi) async -> [WorkspaceRepo] {
        let k = key(accountId, workspaceId)
        if let cached = cache[k] { return cached }
        let list = (try? await api.list(accountId: accountId, workspaceId: workspaceId)) ?? []
        // Only cache non-empty results so a transient failure retries next time.
        if !list.isEmpty { cache[k] = list }
        return list
    }

    static func repo(
        accountId: String,
        workspaceId: String,
        repositoryId: String,
        api: RepositoriesApi
    ) async -> WorkspaceRepo? {
        await repos(accountId: accountId, workspaceId: workspaceId, api: api)
            .first { $0.id == repositoryId }
    }

    /// Drop the cached list so the next read re-fetches (after a create/retarget).
    static func invalidate(accountId: String, workspaceId: String) {
        cache.removeValue(forKey: key(accountId, workspaceId))
    }
}

/// A tappable `owner/name` chip for a project's backing repo. Resolves the uuid
/// via `RepositoryDirectory`; renders nothing until (and unless) it resolves.
struct RepoNameChip: View {
    let accountId: String
    let workspaceId: String
    let repositoryId: String?

    @Environment(AppDependencies.self) private var deps
    @State private var repo: WorkspaceRepo?

    var body: some View {
        Group {
            if let repo {
                Button {
                    if let url = URL(string: "https://github.com/\(repo.fullName)") {
                        Platform.open(url)
                    }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                            .font(.caption2)
                        Text(repo.fullName)
                            .font(.caption.monospaced())
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Image(systemName: "arrow.up.right")
                            .font(.caption2)
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .glassButton()
                }
                .buttonStyle(.plain)
            }
        }
        .task(id: "\(accountId)|\(workspaceId)|\(repositoryId ?? "")") { await resolve() }
    }

    private func resolve() async {
        guard let repositoryId, !repositoryId.isEmpty else {
            repo = nil
            return
        }
        repo = await RepositoryDirectory.repo(
            accountId: accountId,
            workspaceId: workspaceId,
            repositoryId: repositoryId,
            api: deps.repositoriesApi
        )
    }
}
