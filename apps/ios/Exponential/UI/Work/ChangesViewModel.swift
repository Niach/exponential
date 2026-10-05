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
///
/// EXP-1194: a RUN's own issue-less PR (Reviews → Agent runs) loads through
/// the same model off `codingSessions.prFiles` — `.session(id)` observes the
/// run row instead and never falls back to the branch diff.
@MainActor @Observable
final class ChangesViewModel {
    /// What the model reads — an issue's PR (or pushed branch), or a run's own PR.
    enum Source: Equatable {
        case issue(String)
        case session(String)
    }

    enum LoadState {
        case loading
        case failed(String)
        /// EXP-895: the SHARED diff model, never `PrFile` — every Changes
        /// surface on every client renders `Diff.File` and nothing else.
        case loaded([Diff.File])
    }

    private(set) var issue: IssueEntity?
    /// EXP-1194: the run row for a `.session` source (title, PR url + state).
    private(set) var session: CodingSessionEntity?
    private(set) var load: LoadState = .loading

    /// The loaded files, or nil while the fetch is out / failed.
    var loadedFiles: [Diff.File]? {
        if case let .loaded(files) = load { return files }
        return nil
    }

    private let accountId: String
    let source: Source
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let repositoriesApi: RepositoriesApi
    private let codingSessionsApi: CodingSessionsApi

    private var observationTask: Task<Void, Never>?
    /// nil until the first subject row arrives; a flip re-fetches (Android's
    /// `distinctUntilChanged` on hasPr).
    private var hadPr: Bool?

    init(
        accountId: String,
        source: Source,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        repositoriesApi: RepositoriesApi,
        codingSessionsApi: CodingSessionsApi
    ) {
        self.accountId = accountId
        self.source = source
        self.db = db
        self.issuesApi = issuesApi
        self.repositoriesApi = repositoriesApi
        self.codingSessionsApi = codingSessionsApi
    }

    func startObserving() {
        stopObserving() // restartable: the host re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        switch source {
        case let .issue(issueId):
            let observation = ValueObservation.tracking { db in
                try IssueEntity.filter(Column("id") == issueId).fetchOne(db)
            }
            observationTask = Task { [weak self] in
                do {
                    for try await row in observation.values(in: pool) {
                        guard let self, let row else { continue }
                        self.issue = row
                        self.prPresence(row.prUrl?.isEmpty == false)
                    }
                } catch {}
            }
        case let .session(sessionId):
            let observation = ValueObservation.tracking { db in
                try CodingSessionEntity.filter(Column("id") == sessionId).fetchOne(db)
            }
            observationTask = Task { [weak self] in
                do {
                    for try await row in observation.values(in: pool) {
                        guard let self, let row else { continue }
                        self.session = row
                        self.prPresence(row.prUrl?.isEmpty == false)
                    }
                } catch {}
            }
        }
    }

    /// Re-fetch when the diff source flips (a PR opens on a watched branch)
    /// — and once on the first row.
    private func prPresence(_ hasPr: Bool) {
        guard hadPr != hasPr else { return }
        hadPr = hasPr
        Task { await refresh() }
    }

    func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
    }

    func refresh() async {
        load = .loading
        do {
            let files: [PrFile]
            switch source {
            case let .issue(issueId):
                if issue?.prUrl?.isEmpty == false {
                    files = try await issuesApi.prFiles(accountId: accountId, issueId: issueId).files
                } else {
                    files = try await repositoriesApi.branchDiff(accountId: accountId, issueId: issueId)?.files ?? []
                }
            case let .session(sessionId):
                // EXP-1194: the run's own PR only — no branch fallback.
                files = try await codingSessionsApi.prFiles(accountId: accountId, sessionId: sessionId).files
            }
            load = .loaded(files.map(\.diffFile))
        } catch {
            load = .failed(error.userFacingMessage)
        }
    }
}
