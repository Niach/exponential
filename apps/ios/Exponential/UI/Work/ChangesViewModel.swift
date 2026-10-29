import ExpCore
import GRDB
import SwiftUI

/// One pull request's changed files (EXP-34/952; EXP-1295: a `pull_requests`
/// row, never the issue's PR columns, which are gone). Observes the synced
/// row, so the files re-load when the PR's head or state moves, and loads them
/// through `pullRequests.files`.
@MainActor @Observable
final class ChangesViewModel {
    enum LoadState {
        case loading
        case failed(String)
        /// EXP-895: the SHARED diff model — every Changes surface on every
        /// client renders `Diff.File` and nothing else.
        case loaded([Diff.File])
    }

    let pullRequestId: String
    private(set) var pullRequest: PullRequestEntity?
    private(set) var load: LoadState = .loading

    /// The loaded files, or nil while the fetch is out / failed.
    var loadedFiles: [Diff.File]? {
        if case let .loaded(files) = load { return files }
        return nil
    }

    private let accountId: String
    private let db: DatabaseManager
    private let pullRequestsApi: PullRequestsApi

    private var observationTask: Task<Void, Never>?
    /// The head + state the files were last loaded for; a change re-fetches.
    private var loadedKey: String?

    init(
        accountId: String,
        pullRequestId: String,
        db: DatabaseManager,
        pullRequestsApi: PullRequestsApi
    ) {
        self.accountId = accountId
        self.pullRequestId = pullRequestId
        self.db = db
        self.pullRequestsApi = pullRequestsApi
    }

    func startObserving() {
        stopObserving() // restartable: the host re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let id = pullRequestId
        let observation = ValueObservation.tracking { db in
            try PullRequestEntity.fetchOne(db, key: id)
        }
        observationTask = Task { [weak self] in
            do {
                for try await row in observation.values(in: pool) {
                    guard let self, let row else { continue }
                    self.pullRequest = row
                    let key = "\(row.head)|\(row.state)"
                    if key != self.loadedKey {
                        self.loadedKey = key
                        Task { await self.refresh() }
                    }
                }
            } catch {}
        }
    }

    func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
    }

    func refresh() async {
        load = .loading
        do {
            let files = try await pullRequestsApi.files(accountId: accountId, pullRequestId: pullRequestId)
            load = .loaded(files.map {
                Diff.fromPullFile(
                    filename: $0.path,
                    previousFilename: nil,
                    status: Diff.Status.modified.rawValue,
                    additions: $0.additions,
                    deletions: $0.deletions,
                    patch: $0.patch
                )
            })
        } catch {
            load = .failed(error.userFacingMessage)
        }
    }
}
