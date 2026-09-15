import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// Backs the dedicated diff page (EXP-34). Observes the issue row (so the diff
/// source flips live when a PR opens on a watched branch) and loads the changed
/// files from the tier that applies: PR present → `issues.prFiles`, otherwise
/// the pushed branch via `repositories.branchDiff`. Mirrors the Android
/// ChangesViewModel.
@MainActor @Observable
final class ChangesViewModel {
    enum LoadState {
        case loading
        case failed(String)
        case loaded([PrFile])
    }

    private(set) var issue: IssueEntity?
    private(set) var load: LoadState = .loading
    /// Filenames whose patch is expanded — ≤3 files start expanded, more start
    /// collapsed (reset on every reload).
    private(set) var expanded: Set<String> = []

    /// Membership gates the Merge / Close affordances (resolved from the issue's
    /// board → team, like IssueDetailViewModel.refreshPermissions). The
    /// server enforces the rule too; this just hides controls a viewer can't use.
    private(set) var permissions: TeamPermissions = .denied
    /// The issue's own team — the scope the "Fix conflicts" launcher runs in
    /// (EXP-323); not necessarily the tab bar's active team.
    private(set) var teamId: String?
    private(set) var merging = false
    private(set) var closing = false
    private(set) var actionError: String?
    /// Which action produced `actionError` — merge and close share the caption.
    /// The "Fix conflicts" run rebases, force-pushes and then MERGES the pull
    /// request, so it may only be offered after a failed MERGE: a user who
    /// asked to CLOSE a PR must never be handed a button that merges it.
    enum PrAction { case merge, close }
    private(set) var actionErrorFrom: PrAction?
    /// Whether `actionError` is a REAL content conflict (EXP-533): the server
    /// answers `CONFLICT` / HTTP 409 only for a conflict it diagnosed, and a
    /// rebase-and-merge run cannot fix any of the other refusals.
    private(set) var actionErrorIsConflict = false

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let repositoriesApi: RepositoriesApi
    private let auth: AuthRepository

    private var observationTask: Task<Void, Never>?
    /// nil until the first issue row arrives; a flip re-fetches (Android's
    /// `distinctUntilChanged` on hasPr).
    private var hadPr: Bool?

    init(
        accountId: String,
        issueId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        repositoriesApi: RepositoriesApi,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.repositoriesApi = repositoriesApi
        self.auth = auth
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
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
                    self.refreshPermissions(for: row)
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
            // Every file starts collapsed (EXP-248) — uniform with the web
            // and Android review detail.
            expanded = []
            load = .loaded(files)
        } catch {
            load = .failed(error.userFacingMessage)
        }
    }

    func toggle(_ filename: String) {
        if expanded.contains(filename) {
            expanded.remove(filename)
        } else {
            expanded.insert(filename)
        }
    }

    /// Resolve membership from the issue's board → team (mirror of
    /// IssueDetailViewModel.refreshPermissions) so the review actions only show
    /// for members.
    private func refreshPermissions(for issue: IssueEntity) {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let team: TeamEntity? = (try? pool.read { db -> TeamEntity? in
            let board = try BoardEntity.fetchOne(db, key: issue.boardId)
            return try board.flatMap { try TeamEntity.fetchOne(db, key: $0.teamId) }
        }) ?? nil
        teamId = team?.id
        permissions = TeamPermissions.resolve(
            team: team,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
    }

    /// Squash-merge the PR via the GitHub App (EXP-131). Success needs no local
    /// write — Electric echoes the prState/status flips.
    func mergePr() {
        guard !merging else { return }
        merging = true
        actionError = nil
        actionErrorFrom = nil
        actionErrorIsConflict = false
        Task {
            do {
                try await issuesApi.mergePr(accountId: accountId, issueId: issueId)
            } catch {
                actionError = error.userFacingMessage
                actionErrorFrom = .merge
                actionErrorIsConflict = error.isMergeConflict
            }
            merging = false
        }
    }

    /// Close the PR WITHOUT merging (EXP-100 — the drop path). The prState flip
    /// arrives through Electric sync; failures caption the floating action bar.
    func closePr() {
        guard !closing else { return }
        closing = true
        actionError = nil
        actionErrorFrom = nil
        actionErrorIsConflict = false
        Task {
            do {
                try await issuesApi.closePr(accountId: accountId, issueId: issueId)
            } catch {
                actionError = error.userFacingMessage
                actionErrorFrom = .close
            }
            closing = false
        }
    }
}

/// The dedicated diff + review page (EXP-34/156): pushed from the Reviews
/// list. EXP-893: the content, the merge confirm, the recovery run and the
/// floating bar are `PrChangesFace` — shared with the Work screen's Changes
/// face — and this page adds the "Review" title and the review-only
/// close-PR circle + dialog (`reviewMode`).
struct ChangesView: View {
    let issueId: String

    var body: some View {
        ZStack {
            AppBackground()
            PrChangesFace(issueId: issueId, reviewMode: true) { EmptyView() }
        }
        .navigationTitle("Review")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }
}
