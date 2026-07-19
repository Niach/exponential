import ExpUI
import ExpCore
import SwiftUI

/// Resolves the server-only repositories registry once per team and caches
/// it for the app's lifetime (mirrors SteerConfigCache's fetch-once pattern).
/// The synced `boards.repositoryId` is a uuid; the fullName/defaultBranch live
/// only behind the repositories tRPC API, so every surface that shows a repo
/// name (board header, issue coding section) reads through this cache.
@MainActor
enum RepositoryDirectory {
    private static var cache: [String: [TeamRepo]] = [:]

    private static func key(_ accountId: String, _ teamId: String) -> String {
        "\(accountId)|\(teamId)"
    }

    static func repos(accountId: String, teamId: String, api: RepositoriesApi) async -> [TeamRepo] {
        let k = key(accountId, teamId)
        if let cached = cache[k] { return cached }
        let list = (try? await api.list(accountId: accountId, teamId: teamId)) ?? []
        // Only cache non-empty results so a transient failure retries next time.
        if !list.isEmpty { cache[k] = list }
        return list
    }

    static func repo(
        accountId: String,
        teamId: String,
        repositoryId: String,
        api: RepositoriesApi
    ) async -> TeamRepo? {
        await repos(accountId: accountId, teamId: teamId, api: api)
            .first { $0.id == repositoryId }
    }

    /// Drop the cached list so the next read re-fetches (after a create/retarget).
    static func invalidate(accountId: String, teamId: String) {
        cache.removeValue(forKey: key(accountId, teamId))
    }
}

/// A tappable `owner/name` chip for a board's backing repo. Resolves the uuid
/// via `RepositoryDirectory`; renders nothing until (and unless) it resolves.
struct RepoNameChip: View {
    let accountId: String
    let teamId: String
    let repositoryId: String?

    @Environment(AppDependencies.self) private var deps
    @State private var repo: TeamRepo?

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
        .task(id: "\(accountId)|\(teamId)|\(repositoryId ?? "")") { await resolve() }
    }

    private func resolve() async {
        guard let repositoryId, !repositoryId.isEmpty else {
            repo = nil
            return
        }
        repo = await RepositoryDirectory.repo(
            accountId: accountId,
            teamId: teamId,
            repositoryId: repositoryId,
            api: deps.repositoriesApi
        )
    }
}
