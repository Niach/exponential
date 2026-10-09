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

    /// EXP-1248 (rule 9, `_groupDoc`): one display item of a board band.
    public enum Item: Sendable, Identifiable {
        /// A lone PR (depth 0) or a member of a PR TREE, pre-order under its
        /// root.
        case pr(entry: Entry, depth: Int)
        /// A linear STACK: its entries TOP first, then the base-branch row.
        case stack(entries: [Entry], baseBranch: String?)

        public var id: String {
            switch self {
            case let .pr(entry, _): entry.key
            case let .stack(entries, _): "stack:" + (entries.first?.key ?? "")
            }
        }
    }

    public struct BoardGroup: Identifiable, Sendable {
        public let board: BoardEntity
        /// Every entry, flat, newest first (rule 3).
        public let entries: [Entry]
        /// The same entries as the band draws them (rule 9).
        public let items: [Item]
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
            .map {
                BoardGroup(
                    board: boardById[$0]!, entries: entriesByBoard[$0]!,
                    items: items(entriesByBoard[$0]!)
                )
            }
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

    /// (9) A band's entries as items (web `queueItems`, desktop
    /// `queue_items`, Android `ReviewsQueue.items`). Edge: an entry sits on
    /// the entry (same band) whose representative's `branch` is its
    /// representative's `prBaseBranch`. A component lists where its NEWEST
    /// entry would, walked from its ROOT (a cycle breaks where the climb first
    /// repeats). Any entry with two children = a TREE: pre-order, children in
    /// band order, depth = distance from the root. Otherwise 2+ entries = ONE
    /// stack item, top first, `baseBranch` = the root's `prBaseBranch`.
    public static func items(_ entries: [Entry]) -> [Item] {
        func edge(_ branch: String?) -> String? {
            guard let branch, !branch.isEmpty else { return nil }
            return branch
        }
        var owner: [String: Entry] = [:]
        for entry in entries {
            if let branch = edge(entry.representative.branch), owner[branch] == nil {
                owner[branch] = entry
            }
        }
        var parentOf: [String: Entry] = [:]
        var children: [String: [Entry]] = [:]
        for entry in entries {
            guard let base = edge(entry.representative.prBaseBranch), let parent = owner[base],
                  parent.key != entry.key
            else { continue }
            parentOf[entry.key] = parent
            children[parent.key, default: []].append(entry)
        }
        var placed = Set<String>()
        var out: [Item] = []
        for start in entries where !placed.contains(start.key) {
            // Climb to the root; a cycle stops where it first repeats.
            var root = start
            var climbed: Set<String> = [root.key]
            while let parent = parentOf[root.key], !climbed.contains(parent.key),
                  !placed.contains(parent.key) {
                climbed.insert(parent.key)
                root = parent
            }
            // The component under the root, pre-order, children in band order.
            var members: [(entry: Entry, depth: Int)] = []
            var fork = false
            func visit(_ entry: Entry, _ depth: Int) {
                guard !placed.contains(entry.key) else { return }
                placed.insert(entry.key)
                members.append((entry, depth))
                let below = (children[entry.key] ?? []).filter { !placed.contains($0.key) }
                if below.count > 1 { fork = true }
                for child in below { visit(child, depth + 1) }
            }
            visit(root, 0)
            if members.count > 1, !fork {
                out.append(.stack(
                    entries: members.map(\.entry).reversed(),
                    baseBranch: edge(root.representative.prBaseBranch)
                ))
            } else {
                out += members.map { .pr(entry: $0.entry, depth: $0.depth) }
            }
        }
        return out
    }

    // MARK: - EXP-1248: the Reviews page as drawn (web `reviews/index.tsx`)

    /// A PR row's mono label + title: the first issue's identifier (a batch
    /// PR = `EXP-874 +2`) beside the first issue's title (web
    /// `reviewRowLabel`).
    public static func reviewRowLabel(_ entry: Entry) -> (identifier: String, title: String) {
        let first = entry.issues[0]
        let more = entry.issues.count - 1
        let identifier = first.identifier ?? ""
        return (more > 0 ? "\(identifier) +\(more)" : identifier, first.title)
    }

    /// One drawn block of a board band (web `reviewBlocks`).
    public enum Block: Sendable, Identifiable {
        /// Consecutive `pr` items in ONE list, so a tree's guides span its
        /// rows.
        case list(rows: [(entry: Entry, depth: Int)])
        /// Each stack is its own rail.
        case stack(entries: [Entry], baseBranch: String?)

        public var id: String {
            switch self {
            case let .list(rows): "list:" + (rows.first?.entry.key ?? "")
            case let .stack(entries, _): "stack:" + (entries.first?.key ?? "")
            }
        }
    }

    public static func reviewBlocks(_ items: [Item]) -> [Block] {
        var blocks: [Block] = []
        for item in items {
            switch item {
            case let .stack(entries, baseBranch):
                blocks.append(.stack(entries: entries, baseBranch: baseBranch))
            case let .pr(entry, depth):
                if case let .list(rows)? = blocks.last {
                    blocks[blocks.count - 1] = .list(rows: rows + [(entry, depth)])
                } else {
                    blocks.append(.list(rows: [(entry, depth)]))
                }
            }
        }
        return blocks
    }

    /// The Reviews tab's state (fixture `_navDoc` + `navCases`, ×4: web
    /// `reviewsNav`, desktop `reviews_nav`, Android `ReviewsQueue.nav`).
    public struct Nav: Equatable, Sendable {
        /// The dot: the queue (unlinked pulls included) is non-empty.
        public let dot: Bool
        /// The tab exists: no team in scope, some team NOT in yolo mode, or
        /// the dot is lit (in yolo mode an open PR = a failed auto-merge).
        public let shows: Bool

        public init(dot: Bool, shows: Bool) {
            self.dot = dot
            self.shows = shows
        }
    }

    /// `yolo` = each in-scope team's `yolo_mode`; `count` = `build(...).count`
    /// over the same inputs the Reviews screen reads.
    public static func nav(yolo: [Bool], count: Int) -> Nav {
        let dot = count > 0
        return Nav(dot: dot, shows: yolo.isEmpty || yolo.contains(false) || dot)
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
