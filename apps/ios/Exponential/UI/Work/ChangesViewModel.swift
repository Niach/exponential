import ExpCore
import GRDB
import SwiftUI

/// The issue's PR / pushed-branch files for the Work screen's Changes face
/// (EXP-34/952). Observes the issue row (so the diff source flips live when a
/// PR opens on a watched branch) and loads the changed files from the tier
/// that applies: PR present → `issues.prFiles`, otherwise the pushed branch
/// via `repositories.branchDiff`. Mirrors the Android `ChangesViewModel`.
///
/// EXP-1154: the review page is gone, and with it every action this model
/// used to carry: Merge PR, the stack choice and Fix conflicts are the Work
/// screen's `WorkMergePill`, Close PR its `…` menu.
@MainActor @Observable
final class ChangesViewModel {
    enum LoadState {
        case loading
        case failed(String)
        /// EXP-895: the SHARED diff model, never `PrFile` — every Changes
        /// surface on every client renders `Diff.File` and nothing else.
        case loaded([Diff.File])
    }

    private(set) var issue: IssueEntity?
    private(set) var load: LoadState = .loading

    /// The loaded files, or nil while the fetch is out / failed.
    var loadedFiles: [Diff.File]? {
        if case let .loaded(files) = load { return files }
        return nil
    }

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let repositoriesApi: RepositoriesApi

    private var observationTask: Task<Void, Never>?
    /// nil until the first issue row arrives; a flip re-fetches (Android's
    /// `distinctUntilChanged` on hasPr).
    private var hadPr: Bool?

    init(
        accountId: String,
        issueId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        repositoriesApi: RepositoriesApi
    ) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.repositoriesApi = repositoriesApi
    }

    func startObserving() {
        stopObserving() // restartable: the host re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let issueId = self.issueId
        let observation = ValueObservation.tracking { db in
            try IssueEntity.filter(Column("id") == issueId).fetchOne(db)
        }
        observationTask = Task { [weak self] in
            do {
                for try await row in observation.values(in: pool) {
                    guard let self, let row else { continue }
                    self.issue = row
                    // Re-fetch when the diff source flips (a PR opens on a
                    // watched branch) — and once on the first row.
                    let hasPr = row.prUrl?.isEmpty == false
                    if self.hadPr != hasPr {
                        self.hadPr = hasPr
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
            let files: [PrFile]
            if issue?.prUrl?.isEmpty == false {
                files = try await issuesApi.prFiles(accountId: accountId, issueId: issueId).files
            } else {
                files = try await repositoriesApi.branchDiff(accountId: accountId, issueId: issueId)?.files ?? []
            }
            load = .loaded(files.map(\.diffFile))
        } catch {
            load = .failed(error.userFacingMessage)
        }
    }
}
