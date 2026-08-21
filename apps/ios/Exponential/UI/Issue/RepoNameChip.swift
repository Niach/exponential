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
///
/// `headerStrip` wraps the chip in the board header's full-width band
/// (16pt gutter, 8pt vertical padding, faint tint). The band lives HERE, not
/// around the call site, so it only exists once the repo resolved — the
/// repositories registry is tRPC-only, so the lookup is async on every cold
/// open and can fail outright, and an empty band then sat between the nav
/// bar and the pinned status header as a stray ~16pt lighter strip (EXP-592,
/// the gap EXP-578/EXP-590 kept chasing inside the List).
struct RepoNameChip: View {
    let accountId: String
    let teamId: String
    let repositoryId: String?
    var headerStrip = false

    @Environment(AppDependencies.self) private var deps
    @State private var repo: TeamRepo?

    var body: some View {
        Group {
            if let repo {
                if headerStrip {
                    HStack {
                        chip(repo)
                        Spacer()
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(Color.white.opacity(0.04))
                } else {
                    chip(repo)
                }
            }
        }
        .task(id: "\(accountId)|\(teamId)|\(repositoryId ?? "")") { await resolve() }
    }

    private func chip(_ repo: TeamRepo) -> some View {
        Button {
            if let url = URL(string: "https://github.com/\(repo.fullName)") {
                Platform.open(url)
            }
        } label: {
            HStack(spacing: 6) {
                AppIcon(AppIcons.uiRepository, size: 11)
                Text(repo.fullName)
                    .font(.caption.monospaced())
                    .lineLimit(1)
                    .truncationMode(.middle)
                AppIcon(AppIcons.uiExternalLink, size: 11)
            }
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .glassButton()
        }
        .buttonStyle(.plain)
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
