import ExpCore
import Foundation
import GRDB

/// Backs the Agents tab: the signed-in user's own live coding sessions in the
/// ACTIVE TEAM of the active account (the synced `coding_sessions` shape) —
/// running AND in_review (EXP-194), joined to their issues for display. Desktop
/// is the only session runner — this list is the mobile window into what YOU are
/// coding right now; teammates' runs are owner-only (EXP-312) and never listed
/// here, and the team scoping mirrors web's `use-agents-data.ts`.
@MainActor @Observable
final class AgentsViewModel {
    struct Row: Identifiable {
        let session: CodingSessionEntity
        let issue: IssueEntity?
        /// EXP-535: a batch session's resolved open PR, as a representative
        /// linked issue (merging through it merges the ONE batch PR — Reviews
        /// pattern). Set only on issueless batch rows in review with an
        /// UNAMBIGUOUS match.
        let batchPrIssue: IssueEntity?
        /// EXP-549/550: the host machine as it presents right now — the LIVE
        /// devices row's label (a rename never rewrites the session's
        /// snapshot) plus whether that machine stopped heartbeating, which
        /// makes a still-coding run read "Paused" instead of live.
        let device: SessionDevicePresentation
        var id: String { session.id }
    }

    /// EXP-637: one finished run in "Recent runs" — an ended row the AGENT
    /// closed out itself, so it carries a summary worth expanding to.
    struct RecentRun: Identifiable {
        let session: CodingSessionEntity
        let issue: IssueEntity?
        /// The host machine as it presents right now (the live devices row's
        /// label, not the session's start-time snapshot).
        let device: SessionDevicePresentation
        /// Non-nil when a Resume would be accepted: the run's OWN machine,
        /// online and advertising `resume-run`.
        let resumeDevice: SteerDevice?
        var id: String { session.id }
    }

    var rows: [Row] = []

    /// EXP-637: the caller's most recent agent-ended runs in the active team,
    /// newest first, capped at 10 (web's `use-agents-data.ts` `recent`).
    var recentRuns: [RecentRun] = []

    /// EXP-481: the machines list, composed from the synced `devices` shape
    /// (own rows + the active team's shared servers; online-ness derives from
    /// last_seen_at freshness) — the sync-fed replacement for the old 15s
    /// `devices.list` poll. nil until the first observation emission, so the
    /// view can tell "loading" from "no machines".
    var devices: [SteerDevice]?
    /// EXP-481: the synced worktree inventory (shape 18) — the Start-coding
    /// sheet's resume probe and the device-settings worktree list.
    var worktrees: [DeviceWorktreeEntity] = []

    /// The team the surrounding view currently shows — kept current by
    /// `AgentsView` (the sessions observation is account-wide, the list is
    /// team-scoped). nil until the team state resolves: no rows.
    var activeTeamId: String? {
        didSet {
            guard oldValue != activeTeamId else { return }
            rebuild()
            rebuildDevices()
        }
    }

    private let accountId: String
    private let userId: String?
    private let db: DatabaseManager
    // Stored and cancelled individually — a single wrapper task would not
    // propagate cancellation into unstructured inner loops, and the view
    // re-arms on every appear.
    private var sessionTask: Task<Void, Never>?
    private var endedTask: Task<Void, Never>?
    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?
    private var livenessTask: Task<Void, Never>?
    private var deviceTask: Task<Void, Never>?
    private var worktreeTask: Task<Void, Never>?
    private var userTask: Task<Void, Never>?
    /// EXP-656: wakes when our own `devices` shape completes a poll — the
    /// missing foreground re-derivation hook. Presence is only as current as
    /// that cursor, so a machine's badge must repaint the moment it advances
    /// (and not before: an unrefreshed cursor renders presence as unknown).
    private var freshnessTask: Task<Void, Never>?

    private var sessions: [CodingSessionEntity] = []
    // EXP-637: agent-ended rows, observed separately from the live ones — the
    // live query filters on status and would otherwise have to carry them
    // through every liveness rule that only makes sense for a running run.
    private var endedSessions: [CodingSessionEntity] = []
    private var issues: [IssueEntity] = []
    // Observed so the Start-coding picker can resolve repo-backed boards
    // (EXP-156) and so the batch-PR resolution can scope issues to the
    // active team (EXP-535 — issues don't sync team_id).
    private var boards: [BoardEntity] = []
    // EXP-481: raw synced rows behind `devices` (users resolve shared-row
    // owner names — a sharing owner is always inside the users shape).
    private var deviceEntities: [DeviceEntity]?
    private var users: [UserEntity] = []

    init(accountId: String, userId: String?, db: DatabaseManager) {
        self.accountId = accountId
        self.userId = userId
        self.db = db
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        let sessionObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                // Every live status — an in_review session is still watchable
                // (EXP-194); `rebuild()`'s liveness filter drops stale rows.
                .filter([
                    DomainContract.codingSessionStatusRunning,
                    DomainContract.codingSessionStatusInReview,
                ].contains(Column("status")))
                .fetchAll(db)
        }
        sessionTask = Task { [weak self] in
            do {
                for try await sessions in sessionObservation.values(in: pool) {
                    self?.sessions = sessions
                    self?.rebuild()
                }
            } catch {}
        }

        // EXP-637: the finished runs behind "Recent runs" — only the ones
        // carrying the AGENT's close-out (`exponential_sessions_end` is the
        // sole writer of `outcome`). A run killed from the phone, closed with
        // its tab or ended by a merge WITHOUT a report has none and would list
        // as a bare "Ended". Keyed on the outcome, not `ended_by` (EXP-673):
        // a person-started run reports first and ends later, with its tab.
        // Same rule as `RunOutcomePresentation.hasCloseOut`.
        let endedObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("status") == DomainContract.codingSessionStatusEnded)
                .filter(Column("outcome") != nil)
                .fetchAll(db)
        }
        endedTask = Task { [weak self] in
            do {
                for try await sessions in endedObservation.values(in: pool) {
                    self?.endedSessions = sessions
                    self?.rebuildRecent()
                }
            } catch {}
        }

        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity.fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues
                    self?.rebuild()
                }
            } catch {}
        }

        // Boards back the Start-coding picker's eligibility filter AND scope
        // the batch-PR resolution (EXP-535: issues don't sync team_id), so
        // the running-session list rebuilds on these too.
        let boardObservation = ValueObservation.tracking { db in
            try BoardEntity.fetchAll(db)
        }
        boardTask = Task { [weak self] in
            do {
                for try await boards in boardObservation.values(in: pool) {
                    self?.boards = boards
                    self?.rebuild()
                }
            } catch {}
        }

        // EXP-481: the machines list — devices + worktrees + users off sync.
        let deviceObservation = ValueObservation.tracking { db in
            try DeviceEntity.fetchAll(db)
        }
        deviceTask = Task { [weak self] in
            do {
                for try await rows in deviceObservation.values(in: pool) {
                    self?.deviceEntities = rows
                    self?.rebuildDevices()
                    // EXP-549/550: the session rows carry a device label +
                    // liveness derived from these — a rename or a machine
                    // going quiet has to repaint the running list too.
                    self?.rebuild()
                }
            } catch {}
        }
        let worktreeObservation = ValueObservation.tracking { db in
            try DeviceWorktreeEntity.fetchAll(db)
        }
        worktreeTask = Task { [weak self] in
            do {
                for try await rows in worktreeObservation.values(in: pool) {
                    self?.worktrees = rows
                }
            } catch {}
        }
        let userObservation = ValueObservation.tracking { db in
            try UserEntity.fetchAll(db)
        }
        userTask = Task { [weak self] in
            do {
                for try await rows in userObservation.values(in: pool) {
                    self?.users = rows
                    self?.rebuildDevices()
                }
            } catch {}
        }

        // EXP-656: the devices cursor advancing is neither a row write nor a
        // clock tick, so nothing else in here notices it — and it is exactly
        // the moment a "Paused" row we couldn't vouch for becomes knowledge.
        freshnessTask = Task { [weak self] in
            for await polledAccountId in SyncFreshness.shared.updates() {
                guard let self, !Task.isCancelled else { return }
                guard polledAccountId == self.accountId else { continue }
                self.rebuild()
                self.rebuildDevices()
            }
        }

        // GRDB only re-fires on writes — this clock re-applies the staleness
        // filters so a phantom session clears once its liveness window
        // elapses (EXP-153) and a silent machine drops to "last seen" once
        // its heartbeat window does (EXP-481: 30s against the 90s contract
        // window, so the badge flips within one tick of the boundary).
        livenessTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                guard let self, !Task.isCancelled else { return }
                self.rebuild()
                self.rebuildDevices()
            }
        }
    }

    func stopObserving() {
        sessionTask?.cancel()
        sessionTask = nil
        endedTask?.cancel()
        endedTask = nil
        issueTask?.cancel()
        issueTask = nil
        boardTask?.cancel()
        boardTask = nil
        livenessTask?.cancel()
        livenessTask = nil
        deviceTask?.cancel()
        deviceTask = nil
        worktreeTask?.cancel()
        worktreeTask = nil
        userTask?.cancel()
        userTask = nil
        freshnessTask?.cancel()
        freshnessTask = nil
    }

    /// EXP-656: may a stale `last_seen_at` be read as "that machine is gone"?
    /// Only when our own `devices` shape polled within the contract window —
    /// otherwise the rows are pre-sleep knowledge and presence is unknown.
    private func devicesFresh(now: Date = Date()) -> Bool {
        DeviceFreshness.isTrustworthy(
            devicesPolledAt: SyncFreshness.shared.devicesPolledAt(accountId: accountId),
            now: now
        )
    }

    /// EXP-481: recompose the machines list from the observed rows — own
    /// machines first (most recently seen), then the active team's shared
    /// servers, exactly the `devices.list` ordering the view already renders.
    private func rebuildDevices() {
        guard let deviceEntities else { return }
        devices = DeviceQueries.compose(
            rows: deviceEntities,
            users: users,
            teamId: activeTeamId,
            userId: userId
        )
        // EXP-637: the Resume affordance is gated on the run's machine being
        // online and `resume-run`-capable, so a heartbeat repaints it too.
        rebuildRecent()
    }

    /// EXP-637: the "Recent runs" list — own, active-team, agent-ended rows,
    /// newest first, capped at 10. Ordered by when they ENDED (a long run that
    /// started yesterday and finished a minute ago is the most recent one),
    /// falling back to the start stamp for a row whose end never landed.
    private func rebuildRecent() {
        let now = Date()
        let fresh = devicesFresh(now: now)
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let deviceRows = deviceEntities ?? []
        let startTargets = devices ?? []
        recentRuns = endedSessions
            .filter {
                CodingSessionOwnership.isOwn($0, userId: userId, teamId: activeTeamId)
            }
            .sorted { ($0.endedAt ?? $0.startedAt) > ($1.endedAt ?? $1.startedAt) }
            .prefix(10)
            .map { session in
                RecentRun(
                    session: session,
                    issue: session.issueId.flatMap { issuesById[$0] },
                    device: SessionDevicePresentation.resolve(
                        session: session, devices: deviceRows, now: now, devicesFresh: fresh
                    ),
                    resumeDevice: RunResume.target(
                        for: session, devices: startTargets, currentUserId: userId
                    )
                )
            }
    }

    /// Candidate issues for the Agents-tab Start-coding sheet (EXP-156): every
    /// eligible issue in `teamId` (nil = across all synced teams),
    /// recency-ordered, no preselection. Same eligibility as the issue-detail
    /// card minus the current-issue exemption. Reads the already-observed
    /// boards/issues (no DB round-trip).
    func startCandidates(teamId: String?) -> [StartCodingSheet.IssueOption] {
        // Repo-backed boards only — boardId → repositoryId.
        var repoByBoard: [String: String] = [:]
        for board in boards {
            if let teamId, board.teamId != teamId { continue }
            if let repoId = board.repositoryId {
                repoByBoard[board.id] = repoId
            }
        }
        // ANCHOR set (EXP-314): custom statuses anchor to one of these enum
        // values, so the check keeps gating them correctly.
        let terminal: Set<String> = [
            IssueStatus.done.rawValue,
            IssueStatus.cancelled.rawValue,
            IssueStatus.duplicate.rawValue,
        ]
        return issues
            .filter { row in
                guard repoByBoard[row.boardId] != nil else { return false }
                if terminal.contains(row.status) { return false }
                if row.prState == DomainContract.prStateMerged { return false }
                return true
            }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map { row in
                StartCodingSheet.IssueOption(
                    id: row.id,
                    identifier: row.identifier,
                    title: row.title,
                    repositoryId: repoByBoard[row.boardId],
                    status: row.status,
                    priority: row.priority
                )
            }
    }

    private func rebuild() {
        // One clock for the whole pass (EXP-550): every row's offline-ness is
        // read against the same instant.
        let now = Date()
        let fresh = devicesFresh(now: now)
        let deviceRows = deviceEntities ?? []
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // EXP-535: the active team's open batch PRs, collapsed once per
        // rebuild — each in-review batch row then resolves ITS OWN PR by the
        // branch the pr_open flip stamped on it (EXP-545, see
        // BatchPrResolution).
        let teamBoardIds = Set(boards.filter { $0.teamId == activeTeamId }.map(\.id))
        let openBatchPrs = BatchPrResolution.openBatchPrs(
            issues: issues, teamBoardIds: teamBoardIds
        )
        rows = sessions
            // Own runs in the active team only: a teammate's session can't be
            // opened or steered (EXP-312), so listing it only read as "computer
            // not online" — and an own run in another team belongs under that
            // team (web parity, `use-agents-data.ts`).
            .filter {
                CodingSessionOwnership.isOwn($0, userId: userId, teamId: activeTeamId)
            }
            // Heartbeat-stale rows render as absent (EXP-153).
            .filter { CodingSessionLiveness.isLive($0) }
            .sorted { $0.startedAt > $1.startedAt }
            // issueId is nil for a desktop batch (multi-issue) run's session —
            // those rows render without an issue link, but an issueless,
            // actionless batch run whose PR is open (status in_review —
            // flipped in the pr_open transaction) gets the resolved batch PR
            // for its Merge button (EXP-535).
            .map { session in
                let isBatch = session.issueId == nil && session.actionName == nil
                return Row(
                    session: session,
                    issue: session.issueId.flatMap { issuesById[$0] },
                    batchPrIssue: isBatch
                        && session.status == DomainContract.codingSessionStatusInReview
                        ? BatchPrResolution.resolve(
                            sessionBranch: session.branch,
                            openBatchPrs: openBatchPrs
                        ) : nil,
                    device: SessionDevicePresentation.resolve(
                        session: session, devices: deviceRows, now: now, devicesFresh: fresh
                    )
                )
            }
        rebuildRecent()
    }
}
