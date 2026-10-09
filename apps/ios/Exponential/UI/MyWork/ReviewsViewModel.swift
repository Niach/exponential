import ExpUI
import ExpCore
import Foundation
import GRDB

/// EXP-734: one AGENT RUN's own open pull request — the chore PR an action or
/// chat run opened through `exponential_pr_open({repositoryId, head})`, which
/// links no issue at all and so appears in no board group.
struct RunReviewEntry: Identifiable {
    let session: CodingSessionEntity
    /// The run's own name: its action, or "Chat" for a chat run.
    let title: String
    var id: String { session.id }
    var prUrl: String? { session.prUrl }
    var prNumber: Int? { session.prNumber }
    var branch: String? { session.branch }
}

/// One board's band — Reviews groups by board like the other cross-board
/// lists group by status. EXP-1248: one line per open pull request; a PR TREE
/// nests under its root, a linear STACK hangs off one rail
/// (`ReviewsQueue.items`, rule 9), drawn as `blocks`.
struct ReviewGroup: Identifiable {
    let board: BoardEntity
    /// EXP-1186: the board's team — the header names it when the caller is
    /// in more than one.
    let team: TeamEntity
    let blocks: [ReviewsQueue.Block]
    var id: String { board.id }
}

/// EXP-1244: one team repo's open pull requests NO issue or run links
/// (`repositories.openPulls`), listed after the "Agent runs" bands.
struct RepoReviewGroup: Identifiable {
    let team: TeamEntity
    let repositoryId: String
    let fullName: String
    let pulls: [OpenPull]
    var id: String { "\(team.id):\(repositoryId)" }
}

/// The whole queue, in display order (`ReviewsQueue.build`).
struct ReviewsSnapshot {
    var groups: [ReviewGroup] = []
    var runs: [TeamGroups.Group<RunReviewEntry>] = []
    var repos: [RepoReviewGroup] = []
    var isEmpty: Bool { groups.isEmpty && runs.isEmpty && repos.isEmpty }
}

/// "Reviews" (EXP-131): every open PR across EVERY member team (EXP-1186,
/// cross-team like the Inbox). EXP-1244: the queue itself is ONE pure
/// function ×4, `ReviewsQueue.build` (ExpCore, fixture-locked); this model
/// only observes its inputs — GRDB loops over issues, boards and sessions,
/// plus one `repositories.openPulls` fetch per team.
@MainActor @Observable
final class ReviewsViewModel {
    /// Every issue carrying a pr_url or an open pr_state: the open ones are
    /// entries, every pr_url (any state) LINKS a pull request.
    var issues: [IssueEntity] = []
    var boards: [BoardEntity] = []
    /// Every run carrying a pr_url: issue-less open ones are run entries,
    /// every pr_url (any state) links a pull request.
    var sessions: [CodingSessionEntity] = []
    private let accountId: String
    private let db: DatabaseManager
    /// EXP-1244: the app-wide openPulls store the tab bar's dot reads too.
    private let openPulls: OpenPullsStore
    private let repositoriesApi: RepositoriesApi

    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?
    private var sessionTask: Task<Void, Never>?

    init(
        accountId: String, db: DatabaseManager, openPulls: OpenPullsStore,
        repositoriesApi: RepositoriesApi
    ) {
        self.accountId = accountId
        self.db = db
        self.openPulls = openPulls
        self.repositoriesApi = repositoriesApi
    }

    /// The loops are team-agnostic and scope at read time to the member
    /// teams handed in (`snapshot(teams:)`).
    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        // EXP-1244: the open ones AND every linked one — a merged or closed
        // issue's pr_url still links its pull request (the queue filters).
        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity
                .filter(Column("pr_state") == DomainContract.prStateOpen || Column("pr_url") != nil)
                .fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues
                }
            } catch {}
        }

        // Boards resolve each entry's board (name/section) and scope the
        // list to the member teams (issues carry no team_id).
        let boardObservation = ValueObservation.tracking { db in
            try BoardEntity.fetchAll(db)
        }
        boardTask = Task { [weak self] in
            do {
                for try await boards in boardObservation.values(in: pool) {
                    self?.boards = boards
                }
            } catch {}
        }

        // EXP-734: an action or chat run's own PR links no issue, so it can
        // only be found on the session row the server stamped it on.
        // EXP-1244: every run's pr_url, any state, links its pull request.
        let sessionObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("pr_url") != nil)
                .fetchAll(db)
        }
        sessionTask = Task { [weak self] in
            do {
                for try await sessions in sessionObservation.values(in: pool) {
                    self?.sessions = sessions
                }
            } catch {}
        }
    }

    func stopObserving() {
        issueTask?.cancel()
        issueTask = nil
        boardTask?.cancel()
        boardTask = nil
        sessionTask?.cancel()
        sessionTask = nil
    }

    /// EXP-1244: a FORCED openPulls refetch per team, in parallel (joined
    /// with any in-flight one); a failure means that team lists nothing.
    /// Called on appear, on refresh and whenever the team set changes.
    func refreshPulls(teamIds: [String]) async {
        await openPulls.refresh(
            accountId: accountId, teamIds: teamIds, api: repositoriesApi, force: true
        )
    }

    /// The queue across `teams` (EXP-1186), in the team order
    /// (`TeamGroups.ordered`): board bands, then one "Agent runs" band per
    /// team, then the repository bands (EXP-1244). Empty when no team has
    /// synced.
    func snapshot(teams: [TeamEntity]) -> ReviewsSnapshot {
        let orderedTeams = TeamGroups.ordered(teams)
        let teamById = Dictionary(orderedTeams.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let queue = ReviewsQueue.build(
            teamIds: orderedTeams.map(\.id),
            boards: boards,
            issues: issues,
            sessions: sessions,
            pulls: openPulls.pulls(accountId: accountId, teamIds: orderedTeams.map(\.id))
        )
        let groups = queue.boardGroups.compactMap { group -> ReviewGroup? in
            guard let team = teamById[group.board.teamId] else { return nil }
            return ReviewGroup(
                board: group.board, team: team,
                blocks: ReviewsQueue.reviewBlocks(group.items)
            )
        }
        let runs = queue.runGroups.compactMap { group -> TeamGroups.Group<RunReviewEntry>? in
            guard let team = teamById[group.teamId] else { return nil }
            return TeamGroups.Group(team: team, items: group.sessions.map {
                RunReviewEntry(
                    session: $0,
                    title: PastRuns.chatSubject($0) ?? $0.actionName ?? PastRuns.chatRunName
                )
            })
        }
        let repos = queue.repoGroups.compactMap { repo -> RepoReviewGroup? in
            guard let team = teamById[repo.teamId] else { return nil }
            return RepoReviewGroup(
                team: team, repositoryId: repo.repositoryId,
                fullName: repo.fullName, pulls: repo.pulls
            )
        }
        return ReviewsSnapshot(groups: groups, runs: runs, repos: repos)
    }
}
