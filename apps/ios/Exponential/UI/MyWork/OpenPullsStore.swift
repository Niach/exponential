import ExpCore
import Foundation

/// EXP-1244: the app-wide `repositories.openPulls` store (web
/// `lib/open-pulls-store.ts`). The Reviews screen AND the Reviews tab's dot
/// read it (`ReviewsQueue.build` rule 7 + fixture `_navDoc`), so the page and
/// its dot never disagree. One entry per account + team; a team whose fetch
/// fails lists nothing. The server caches 60 s too.
@MainActor @Observable
final class OpenPullsStore {
    private struct Key: Hashable {
        let accountId: String
        let teamId: String
    }

    private struct Entry {
        var repos: [ReviewsQueue.PullRepo]
        let fetchedAt: Date
    }

    /// The tab bar refetches a team older than this; the screen always does.
    static let staleAfter: TimeInterval = 60

    private var entries: [Key: Entry] = [:]
    @ObservationIgnored private var inFlight: [Key: Task<Void, Never>] = [:]

    /// Built as a default value of `AppDependencies`, whose init is not
    /// main-actor isolated (`SteerSessionStore` does the same).
    nonisolated init() {}

    /// `teamIds`' repos in that order; [] for a team not fetched yet.
    func pulls(accountId: String, teamIds: [String]) -> [ReviewsQueue.PullRepo] {
        teamIds.flatMap { entries[Key(accountId: accountId, teamId: $0)]?.repos ?? [] }
    }

    /// Fetch every team in parallel unless a fresh (`force` = never fresh)
    /// or in-flight one exists; an in-flight fetch is joined, never doubled.
    func refresh(
        accountId: String, teamIds: [String], api: RepositoriesApi, force: Bool = false
    ) async {
        var tasks: [Task<Void, Never>] = []
        for teamId in Set(teamIds) {
            let key = Key(accountId: accountId, teamId: teamId)
            if let running = inFlight[key] {
                tasks.append(running)
                continue
            }
            if !force, let entry = entries[key],
               Date().timeIntervalSince(entry.fetchedAt) < Self.staleAfter {
                continue
            }
            let task = Task { [weak self] in
                let repos = (try? await api.openPulls(accountId: accountId, teamId: teamId)) ?? []
                guard let self else { return }
                self.entries[key] = Entry(
                    repos: repos.map {
                        ReviewsQueue.PullRepo(
                            teamId: teamId, repositoryId: $0.repositoryId,
                            fullName: $0.fullName, pulls: $0.pulls
                        )
                    },
                    fetchedAt: Date()
                )
                self.inFlight[key] = nil
            }
            inFlight[key] = task
            tasks.append(task)
        }
        for task in tasks { await task.value }
    }
}
