import Foundation

/// EXP-897 Part 4 — ONE model for everything a piece of work is entangled
/// with, mirrored ×4 (web `lib/pr-graph.ts`, desktop `pr_graph.rs`, Android
/// `PrGraph.kt`). The Work header's badge and its overlay read nothing else.
///
/// Three relationships, all derived from ALREADY SYNCED rows — no new column,
/// no request:
///
/// - **stack**: pull requests chained by `prBaseBranch → branch` (`PrStack`).
/// - **batch**: the issues sharing ONE `prUrl` — a batch run's combined PR.
///   A batch is an ENTRY like any other, so a batch can be a stack member.
/// - **tree**: the session tree off `parentSessionId` (`SessionTree`).
///
/// Plus the issue's OPEN blockers (`StackStart.openBlockers`), which is what
/// the Issue face's "Blocked by" section lists.
public enum PrGraph {

    /// ONE pull request: a single issue, or the issues of a batch PR.
    public struct Entry: Identifiable, Equatable {
        /// `prUrl` when there is one, else `issue:<id>` — the grouping key.
        public let id: String
        /// The issues on this PR, newest first; `representative` is the first.
        public let issues: [IssueEntity]

        public init(id: String, issues: [IssueEntity]) {
            self.id = id
            self.issues = issues
        }

        public var representative: IssueEntity { issues[0] }
        public var isBatch: Bool { issues.count > 1 }
        public var prUrl: String? { representative.prUrl }
        public var prNumber: Int? { representative.prNumber }
        public var prState: String? { representative.prState }
        public var branch: String? { representative.branch }
        public var prBaseBranch: String? { representative.prBaseBranch }
        public var identifiers: [String] { issues.compactMap(\.identifier) }

        public static func == (a: Entry, b: Entry) -> Bool { a.id == b.id }
    }

    /// One rung of the stack, bottom first.
    public struct StackEntry: Identifiable {
        public let entry: Entry
        /// 0 at the BOTTOM of the chain, +1 per rung up.
        public let depth: Int
        public var id: String { entry.id }
    }

    /// The batch the subject is part of — the issues sharing its PR.
    public struct BatchEntry {
        public let issues: [IssueEntity]
    }

    /// What the header badge says.
    public enum BadgeKind: Equatable {
        case stack
        case batch
        case stackAndBatch
    }

    public struct Graph {
        /// The chain this work sits in, bottom first. Empty when it is not
        /// stacked (a lone PR is not a stack).
        public let stack: [StackEntry]
        /// The batch this work rides, or nil when its PR is its own.
        public let batch: BatchEntry?
        /// The run tree the subject's session belongs to (roots first).
        public let tree: [SessionTree.Row<CodingSessionEntity>]
        /// The issue's OPEN blockers — the Issue face's "Blocked by" list.
        public let blockers: [IssueEntity]
        /// The subject's own entry, when it has one.
        public let entry: Entry?

        /// The subject's 1-based rung, counted from the bottom. Nil unstacked.
        public var position: Int? {
            guard let entry, !stack.isEmpty else { return nil }
            guard let index = stack.firstIndex(where: { $0.entry.id == entry.id }) else {
                return nil
            }
            return index + 1
        }

        public var size: Int { stack.count }

        /// `2 of 3` — the badge's caption, nil when there is no stack.
        public var positionLabel: String? {
            guard let position, size > 1 else { return nil }
            return "\(position) of \(size)"
        }

        /// The entry directly below the subject (its foundation).
        public var below: Entry? {
            guard let position, position > 1 else { return nil }
            return stack[position - 2].entry
        }

        /// The entry directly above the subject.
        public var above: Entry? {
            guard let position, position < size else { return nil }
            return stack[position].entry
        }

        /// The BOTTOM of the chain — where "Merge stack" lives.
        public var bottom: Entry? { stack.first?.entry }

        /// Nothing to show: no stack, no batch, no tree, no blockers.
        public var isEmpty: Bool {
            stack.count < 2 && batch == nil && tree.count < 2 && blockers.isEmpty
        }
    }

    /// Build the graph for a subject: an issue, a session, or both (the Work
    /// screen usually has both). `issues` is every synced issue in scope,
    /// `sessions` every synced session, `relations` every synced relation row.
    public static func build(
        issue: IssueEntity?,
        session: CodingSessionEntity?,
        issues: [IssueEntity],
        sessions: [CodingSessionEntity],
        relations: [IssueRelationEntity]
    ) -> Graph {
        let subject = issue ?? session?.issueId.flatMap { id in issues.first { $0.id == id } }
        let entries = prEntries(issues)
        let subjectEntry = subject.flatMap { subject in
            entries.first { entry in entry.issues.contains { $0.id == subject.id } }
        } ?? batchSessionEntry(session, issues: issues, entries: entries)

        let chain: [StackEntry]
        if let subjectEntry {
            let members = PrStack.stackChain(
                subjectEntry,
                in: entries,
                id: { $0.id },
                branch: { $0.branch },
                base: { $0.prBaseBranch }
            )
            chain = members.count > 1
                ? members.enumerated().map { StackEntry(entry: $0.element, depth: $0.offset) }
                : []
        } else {
            chain = []
        }

        let batch = subjectEntry.flatMap { $0.isBatch ? BatchEntry(issues: $0.issues) : nil }

        let blockers = subject.map {
            StackStart.openBlockers(issueId: $0.id, relations: relations, issues: issues)
        } ?? []

        return Graph(
            stack: chain,
            batch: batch,
            tree: session != nil
                ? sessionTree(session: session, sessions: sessions)
                : issueTree(subject, sessions: sessions),
            blockers: blockers,
            entry: subjectEntry
        )
    }

    /// The badge a graph earns: a stack, a batch, both, or nothing at all.
    public static func badgeKind(_ graph: Graph) -> BadgeKind? {
        let stacked = graph.stack.count > 1
        let batched = graph.batch != nil
        if stacked && batched { return .stackAndBatch }
        if stacked { return .stack }
        if batched { return .batch }
        return nil
    }

    /// EXP-1079/EXP-1097, SLOP-16 r5: what the header chip DRAWS, the SAME on
    /// every face — Issue, Run and Changes alike. First match wins:
    ///
    /// 1. a PR relation (`badgeKind`: stack, batch, stack+batch);
    /// 2. `blocked` — the subject issue has OPEN blockers (`graph.blockers`);
    /// 3. nil = no chip. A run family alone earns NO badge.
    ///
    /// Byte-identical ×4: web `badgeShape` (`pr-graph.ts`), desktop
    /// `pr_graph::badge_shape`, Android `PrGraph.badgeShape`.
    public enum BadgeShape: Equatable {
        case stack
        case batch
        case stackAndBatch
        case blocked
    }

    public static func badgeShape(_ graph: Graph) -> BadgeShape? {
        switch badgeKind(graph) {
        case .stack: return .stack
        case .batch: return .batch
        case .stackAndBatch: return .stackAndBatch
        case nil: break
        }
        if !graph.blockers.isEmpty { return .blocked }
        return nil
    }

    /// EXP-1058: what the header's badge counts — the front issue and how
    /// many ride behind it (`+N`).
    ///
    /// - stack / batch: `issue` = the subject PR's representative row, `count`
    ///   = every OTHER issue on the stack (all its entries) or batch;
    /// - `blocked` (EXP-1097): `issue` = the FIRST open blocker, `count` = the
    ///   other open blockers.
    public struct BadgeChip: Equatable {
        public let issue: IssueEntity?
        public let count: Int

        public static func == (a: BadgeChip, b: BadgeChip) -> Bool {
            a.issue?.id == b.issue?.id && a.count == b.count
        }
    }

    /// The chip, nil exactly when `badgeShape` is nil. Mirrors web
    /// `badgeChip` (`pr-graph.ts`), desktop `pr_graph::badge_chip`, Android
    /// `PrGraph.badgeChip`.
    public static func badgeChip(_ graph: Graph) -> BadgeChip? {
        guard let shape = badgeShape(graph) else { return nil }
        if shape == .blocked {
            return BadgeChip(issue: graph.blockers.first, count: graph.blockers.count - 1)
        }
        let issue = graph.entry?.representative
        if graph.stack.count >= 2 {
            let total = graph.stack.reduce(0) { $0 + $1.entry.issues.count }
            return BadgeChip(issue: issue, count: total - 1)
        }
        return BadgeChip(issue: issue, count: (graph.batch?.issues.count ?? 1) - 1)
    }

    /// One band of the "Related work" sheet.
    public enum OverlaySection: String, Equatable, Sendable {
        case blocked, batch, stack
    }

    /// SLOP-16 r5: the "Related work" view's copy, byte-identical ×4 (web
    /// `RELATED_WORK_TITLE` + `OVERLAY_COPY`, desktop, Android). ONE title,
    /// one relations-card band per section, ONE empty note.
    public enum OverlayCopy {
        public static let relatedWorkTitle = "Related work"
        public static let blocked = IssueRelationsView.Copy.blockedBy
        /// The subject's batch PARTNERS (everything on its PR but itself).
        public static let batch = "Same pull request"
        /// The OTHER pull requests of the subject's stack, bottom-up.
        public static let stack = "Pull request stack"
        public static let empty = "Nothing else is linked to this issue."
    }

    /// A section's band title.
    public static func overlayBandTitle(_ section: OverlaySection) -> String {
        switch section {
        case .blocked: OverlayCopy.blocked
        case .batch: OverlayCopy.batch
        case .stack: OverlayCopy.stack
        }
    }

    /// SLOP-16 r5: the sheet's bands, ONE fixed order on every face — Blocked
    /// by, Same pull request, Pull request stack — each only when the subject
    /// has that relation. Mirrors web `overlaySections`.
    public static func overlaySections(_ graph: Graph) -> [OverlaySection] {
        var sections: [OverlaySection] = []
        if !graph.blockers.isEmpty { sections.append(.blocked) }
        if graph.batch != nil { sections.append(.batch) }
        if graph.stack.count >= 2 { sections.append(.stack) }
        return sections
    }

    // MARK: - Pieces

    /// Every pull request among `issues`, one entry per distinct `prUrl`
    /// (issues sharing one = a batch), an issue with no PR keyed on its own
    /// id. First-seen order, newest issue first inside an entry — the Reviews
    /// rule, so the overlay and the list read the same.
    /// EXP-876: a BATCH run's own entry. A batch links no issue and stamps no
    /// `pr_url` of its own, so before this it resolved nothing at all — the
    /// pill and its sheet, the one surface built to name work that spans
    /// several issues, never appeared on the very run that spans them. Its
    /// covered set (`batch_issue_ids`) IS the entry.
    ///
    /// The PR-grouped entry wins whenever there is one: it carries the branch
    /// and the base the stack chains on, so a batch PR stacked on another
    /// still reads `stack+batch` and still offers Merge stack. The synthesized
    /// entry is what a batch wears BEFORE its PR exists.
    static func batchSessionEntry(
        _ session: CodingSessionEntity?,
        issues: [IssueEntity],
        entries: [Entry]
    ) -> Entry? {
        guard let session else { return nil }
        let covered = BatchRun.issues(session, issues: issues)
        guard let first = covered.first else { return nil }
        if let grouped = entries.first(where: { entry in
            entry.isBatch && entry.issues.contains { $0.id == first.id }
        }) {
            return grouped
        }
        return Entry(id: "run:\(session.id)", issues: covered)
    }

    public static func prEntries(_ issues: [IssueEntity]) -> [Entry] {
        var buckets: [String: [IssueEntity]] = [:]
        var keyOrder: [String] = []
        for issue in issues {
            let key = (issue.prUrl?.isEmpty == false) ? issue.prUrl! : "issue:\(issue.id)"
            if buckets[key] == nil {
                keyOrder.append(key)
                buckets[key] = []
            }
            buckets[key]?.append(issue)
        }
        return keyOrder.compactMap { key in
            guard let bucket = buckets[key], !bucket.isEmpty else { return nil }
            return Entry(id: key, issues: bucket.sorted(by: newerFirst))
        }
    }

    /// The run tree the subject's session belongs to: its whole ROOT subtree,
    /// nested. Empty without a session.
    private static func sessionTree(
        session: CodingSessionEntity?, sessions: [CodingSessionEntity]
    ) -> [SessionTree.Row<CodingSessionEntity>] {
        guard let session else { return [] }
        let byId = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Up to the root (defensively bounded by the row count).
        var root = byId[session.id] ?? session
        var seen: Set<String> = [root.id]
        while let parentId = root.parentSessionId, let parent = byId[parentId],
              !seen.contains(parentId) {
            seen.insert(parentId)
            root = parent
        }
        let rows = SessionTree.nest(sessions)
        guard let start = rows.firstIndex(where: { $0.session.id == root.id }) else {
            return SessionTree.nest([root])
        }
        var subtree = [rows[start]]
        for row in rows[(start + 1)...] {
            if row.depth <= rows[start].depth { break }
            subtree.append(row)
        }
        return subtree
    }

    /// No session to anchor on (an issue alone): every run OF the issue with
    /// its whole subtree, nested — web `prGraph`'s issue-only branch.
    private static func issueTree(
        _ issue: IssueEntity?, sessions: [CodingSessionEntity]
    ) -> [SessionTree.Row<CodingSessionEntity>] {
        guard let issue else { return [] }
        var childrenOf: [String: [CodingSessionEntity]] = [:]
        for row in sessions {
            guard let parent = row.parentSessionId, parent != row.id else { continue }
            childrenOf[parent, default: []].append(row)
        }
        var ids = Set<String>()
        for row in sessions where row.issueId == issue.id {
            var queue = [row.id]
            ids.insert(row.id)
            while !queue.isEmpty {
                let id = queue.removeFirst()
                for child in childrenOf[id] ?? [] where ids.insert(child.id).inserted {
                    queue.append(child.id)
                }
            }
        }
        guard !ids.isEmpty else { return [] }
        return SessionTree.nest(sessions.filter { ids.contains($0.id) })
    }

    /// Newest-first by `createdAt` (Postgres wire text compares
    /// chronologically), id as the deterministic tie-break.
    private static func newerFirst(_ a: IssueEntity, _ b: IssueEntity) -> Bool {
        if a.createdAt != b.createdAt { return a.createdAt > b.createdAt }
        return a.id > b.id
    }
}
