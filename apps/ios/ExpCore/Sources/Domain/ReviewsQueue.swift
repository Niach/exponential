import Foundation

/// EXP-1244: the Reviews queue, ONE pure function ×4 (web
/// `lib/reviews-queue.ts` `reviewsQueue`, desktop `domain::reviews_queue`,
/// Android `ReviewsQueue.build`), locked by
/// `packages/domain-contract/fixtures/reviews-queue.json` (its `_doc` = the
/// rules) and `ReviewsQueueTests`. Synced issues + runs + the
/// `repositories.openPulls` results → board bands, "Agent runs" bands and the
/// unlinked-PR repository bands.
public enum ReviewsQueue {
    /// A repository band's trailing caption on a single-team list ×4
    /// (fixture `labels.repoBandCaption`).
    public static let repoBandCaption = "not linked to an issue"
    /// The "Agent runs" band's trailing caption on a single-team list ×4
    /// (fixture `labels.runBandCaption`).
    public static let runBandCaption = "opened by a coding run"

    /// One team's `repositories.openPulls` repo, tagged with that team.
    public struct PullRepo: Equatable, Sendable, Identifiable {
        public let teamId: String
        public let repositoryId: String
        public let fullName: String
        public let pulls: [OpenPull]

        public var id: String { repositoryId }

        public init(teamId: String, repositoryId: String, fullName: String, pulls: [OpenPull]) {
            self.teamId = teamId
            self.repositoryId = repositoryId
            self.fullName = fullName
            self.pulls = pulls
        }
    }

    /// One open PR: every in-scope issue it links, `issues[0]` = the
    /// representative (merge target, row tap).
    public struct Entry: Identifiable, Sendable {
        /// `prUrl`, or `issue:<id>` when it has none.
        public let key: String
        public let issues: [IssueEntity]
        public var id: String { key }
        public var representative: IssueEntity { issues[0] }
    }

    public struct BoardGroup: Identifiable, Sendable {
        public let board: BoardEntity
        public let entries: [Entry]
        public var id: String { board.id }
    }

    public struct RunGroup: Identifiable, Sendable {
        public let teamId: String
        public let sessions: [CodingSessionEntity]
        public var id: String { teamId }
    }

    public struct Result: Sendable {
        public let boardGroups: [BoardGroup]
        public let runGroups: [RunGroup]
        public let repoGroups: [PullRepo]
        public let count: Int
    }

    /// `teamIds` = the teams in DISPLAY order; `issues` = every synced issue
    /// carrying a pr_url or pr_state; `sessions` = every synced run carrying a
    /// pr_url; `pulls` = the openPulls results ([] until fetched).
    public static func build(
        teamIds: [String],
        boards: [BoardEntity],
        issues: [IssueEntity],
        sessions: [CodingSessionEntity],
        pulls: [PullRepo]
    ) -> Result {
        var teamOrder: [String: Int] = [:]
        for (index, id) in teamIds.enumerated() where teamOrder[id] == nil {
            teamOrder[id] = index
        }
        var boardById: [String: BoardEntity] = [:]
        for board in boards where teamOrder[board.teamId] != nil {
            boardById[board.id] = board
        }
        let scopedIssues = issues.filter { boardById[$0.boardId] != nil }
        let scopedSessions = sessions.filter { teamOrder[$0.teamId] != nil }

        // (2) One entry per open PR, issues newest first (id ascending on a tie).
        var buckets: [String: [IssueEntity]] = [:]
        var keyOrder: [String] = []
        for issue in scopedIssues where issue.prState == DomainContract.prStateOpen {
            let key = present(issue.prUrl) ? issue.prUrl! : "issue:\(issue.id)"
            if buckets[key] == nil { keyOrder.append(key); buckets[key] = [] }
            buckets[key]!.append(issue)
        }
        // (3) Newest representative first, under the representative's board.
        let entries = keyOrder
            .map { Entry(key: $0, issues: buckets[$0]!.sorted { newerFirst($0.createdAt, $0.id, $1.createdAt, $1.id) }) }
            .sorted {
                newerFirst(
                    $0.representative.createdAt, $0.representative.id,
                    $1.representative.createdAt, $1.representative.id
                )
            }
        var entriesByBoard: [String: [Entry]] = [:]
        var boardOrder: [String] = []
        for entry in entries {
            let boardId = entry.representative.boardId
            if entriesByBoard[boardId] == nil { boardOrder.append(boardId); entriesByBoard[boardId] = [] }
            entriesByBoard[boardId]!.append(entry)
        }
        // (4) Team order, sort_order (null last), name, id.
        let boardGroups = boardOrder
            .map { BoardGroup(board: boardById[$0]!, entries: entriesByBoard[$0]!) }
            .sorted { a, b in
                let ta = teamOrder[a.board.teamId]!, tb = teamOrder[b.board.teamId]!
                if ta != tb { return ta < tb }
                let la = a.board.sortOrder ?? .infinity, lb = b.board.sortOrder ?? .infinity
                if la != lb { return la < lb }
                let na = a.board.name.lowercased(), nb = b.board.name.lowercased()
                if na != nb { return na < nb }
                return a.board.id < b.board.id
            }

        // (5) Linked = any in-scope issue's or run's PR, whatever its state.
        let issueUrls = Set(scopedIssues.compactMap(\.prUrl).filter { !$0.isEmpty })
        let linked = issueUrls.union(scopedSessions.compactMap(\.prUrl).filter { !$0.isEmpty })

        // (6) A run's OWN PR: newest row per pr_url, banded per team.
        var runByUrl: [String: CodingSessionEntity] = [:]
        for session in scopedSessions {
            guard session.issueId == nil, session.prState == DomainContract.prStateOpen,
                  let url = session.prUrl, !url.isEmpty, !issueUrls.contains(url)
            else { continue }
            if let current = runByUrl[url],
               !newerFirst(session.createdAt, session.id, current.createdAt, current.id) {
                continue
            }
            runByUrl[url] = session
        }
        let runs = runByUrl.values.sorted { newerFirst($0.createdAt, $0.id, $1.createdAt, $1.id) }
        var seenTeams = Set<String>()
        let runGroups: [RunGroup] = teamIds.compactMap { teamId in
            guard seenTeams.insert(teamId).inserted else { return nil }
            let teamRuns = runs.filter { $0.teamId == teamId }
            return teamRuns.isEmpty ? nil : RunGroup(teamId: teamId, sessions: teamRuns)
        }

        // (7) The unlinked pulls, team order then fetch order.
        let repoGroups = pulls.enumerated()
            .filter { teamOrder[$0.element.teamId] != nil }
            .sorted { a, b in
                let ta = teamOrder[a.element.teamId]!, tb = teamOrder[b.element.teamId]!
                return ta != tb ? ta < tb : a.offset < b.offset
            }
            .map { repo in
                PullRepo(
                    teamId: repo.element.teamId, repositoryId: repo.element.repositoryId,
                    fullName: repo.element.fullName,
                    pulls: repo.element.pulls.filter { !linked.contains($0.url) }
                )
            }
            .filter { !$0.pulls.isEmpty }

        return Result(
            boardGroups: boardGroups,
            runGroups: runGroups,
            repoGroups: repoGroups,
            count: entries.count + runs.count + repoGroups.reduce(0) { $0 + $1.pulls.count }
        )
    }

    private static func present(_ url: String?) -> Bool {
        url.map { !$0.isEmpty } ?? false
    }

    /// created_at DESC as instants (either wire form), then id ASC.
    private static func newerFirst(_ aAt: String, _ aId: String, _ bAt: String, _ bId: String) -> Bool {
        let a = WireTimestamps.parse(aAt), b = WireTimestamps.parse(bAt)
        if let a, let b {
            if a != b { return a > b }
        } else if aAt != bAt {
            return aAt > bAt
        }
        return aId < bId
    }
}
