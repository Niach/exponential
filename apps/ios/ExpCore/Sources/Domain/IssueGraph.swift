import Foundation

/// EXP-980 — the `blocks` graph behind the list badge, the mini-graph overlay
/// and the blocked-start dialog.
///
/// ONE pure rule, mirrored ×4 (web `lib/issue-graph.ts`, Android
/// `domain/IssueGraph.kt`, desktop `domain::issue_graph`) and locked by the
/// contract fixture `domain-contract/fixtures/issue-graph.json` — same cases,
/// same test names.
///
/// A canonical `blocks` row is `issueId` BLOCKS `relatedIssueId` (EXP-736). An
/// edge counts only while BOTH ends are synced and OPEN (anchor status not
/// done / cancelled / duplicate): a finished blocker is in nobody's way, and a
/// finished issue is blocked by nothing. A SUBJECT is always kept, open or not.
public enum IssueGraph {

    /// The most nodes one graph draws; the rest is cut and `truncated` says so.
    public static let maxNodes = 60

    /// Under a graph the node cap cut. Byte-identical ×4.
    public static let truncatedNote = "Showing the nearest \(maxNodes) issues."
    /// Under a graph that holds a cycle. Byte-identical ×4.
    public static let cycleNote = "Red issues block each other in a cycle."

    /// One row's badge numbers.
    public struct BlockCounts: Equatable, Sendable {
        /// Open issues that block this one.
        public let blockedBy: Int
        /// Open issues this one blocks.
        public let blocking: Int

        public init(blockedBy: Int, blocking: Int) {
            self.blockedBy = blockedBy
            self.blocking = blocking
        }
    }

    public struct Node: Equatable, Sendable {
        public let id: String
        /// The column: 0 = blocked by nothing in the graph; every edge points
        /// to a higher wave (cycle edges aside).
        public let wave: Int
        /// The row inside the wave, by identifier.
        public let lane: Int
        public let subject: Bool

        public init(id: String, wave: Int, lane: Int, subject: Bool) {
            self.id = id
            self.wave = wave
            self.lane = lane
            self.subject = subject
        }
    }

    public struct Edge: Equatable, Sendable {
        /// The blocker.
        public let from: String
        /// The blocked issue.
        public let to: String
        /// Part of a blocking cycle: drawn red, and nothing on it can start.
        public let cycle: Bool

        public init(from: String, to: String, cycle: Bool) {
            self.from = from
            self.to = to
            self.cycle = cycle
        }
    }

    public struct Graph: Equatable, Sendable {
        public let nodes: [Node]
        public let edges: [Edge]
        public let hasCycle: Bool
        public let truncated: Bool

        public init(nodes: [Node], edges: [Edge], hasCycle: Bool, truncated: Bool) {
            self.nodes = nodes
            self.edges = edges
            self.hasCycle = hasCycle
            self.truncated = truncated
        }

        public var isEmpty: Bool { nodes.isEmpty }
    }

    /// The badge's accessible label: `Blocked by 2`, `Blocking 1` or
    /// `Blocked by 2, blocking 1`. Byte-identical ×4.
    public static func blocksBadgeLabel(_ counts: BlockCounts) -> String {
        var parts: [String] = []
        if counts.blockedBy > 0 { parts.append("Blocked by \(counts.blockedBy)") }
        if counts.blocking > 0 {
            parts.append("\(parts.isEmpty ? "Blocking" : "blocking") \(counts.blocking)")
        }
        return parts.joined(separator: ", ")
    }

    /// The badge numbers of every issue that has any; an issue with neither
    /// count is absent.
    public static func blockCounts(
        relations: [IssueRelationEntity],
        issues: [IssueEntity]
    ) -> [String: BlockCounts] {
        var blockedBy: [String: Int] = [:]
        var blocking: [String: Int] = [:]
        for (from, to) in openEdges(relations, issues, keep: []) {
            blocking[from, default: 0] += 1
            blockedBy[to, default: 0] += 1
        }
        var counts: [String: BlockCounts] = [:]
        for id in Set(blockedBy.keys).union(blocking.keys) {
            counts[id] = BlockCounts(
                blockedBy: blockedBy[id] ?? 0, blocking: blocking[id] ?? 0
            )
        }
        return counts
    }

    /// The open issues that block any of `ids` from OUTSIDE the set, by
    /// identifier: what a batch start has to ask about (a blocker picked into
    /// the same batch is not in its way).
    public static func openBlockersOfSet(
        ids: [String],
        relations: [IssueRelationEntity],
        issues: [IssueEntity]
    ) -> [IssueEntity] {
        let picked = Set(ids)
        let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var seen = Set<String>()
        var out: [IssueEntity] = []
        for (from, to) in openEdges(relations, issues, keep: []) {
            guard picked.contains(to), !picked.contains(from) else { continue }
            guard !seen.contains(from), let blocker = byId[from] else { continue }
            seen.insert(from)
            out.append(blocker)
        }
        return out.sorted { sortKey($0) < sortKey($1) }
    }

    /// The graph around `subjectIds`: the subjects, everything that
    /// transitively blocks them and everything they transitively block, with
    /// every open edge among those nodes.
    ///
    /// Layout: an edge is a CYCLE edge when its blocker is reachable from its
    /// blocked end; `wave` is the longest path over the remaining (acyclic)
    /// edges, `lane` the identifier order inside a wave. Nodes come back by
    /// (wave, lane), edges by (from, to) identifier.
    public static func blockGraph(
        subjectIds: [String],
        relations: [IssueRelationEntity],
        issues: [IssueEntity]
    ) -> Graph {
        let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // The identifier is the ordering key everywhere; an id stands in for a
        // row that never stamped one, so the order stays total.
        func key(_ id: String) -> (String, String) { (byId[id]?.identifier ?? id, id) }

        let subjects = Set(subjectIds).filter { byId[$0] != nil }
            .sorted { key($0) < key($1) }
        let subjectSet = Set(subjects)
        let all = openEdges(relations, issues, keep: subjectSet)
        var blockersOf: [String: [String]] = [:]
        var blockedBy: [String: [String]] = [:]
        for (from, to) in all {
            blockersOf[to, default: []].append(from)
            blockedBy[from, default: []].append(to)
        }

        // The closure, blockers first and then the blocked side, level by
        // level and in identifier order, so the node cap cuts the same nodes
        // everywhere.
        var picked = Set<String>()
        var truncated = false
        func admit(_ id: String) -> Bool {
            if picked.contains(id) { return false }
            if picked.count >= maxNodes {
                truncated = true
                return false
            }
            picked.insert(id)
            return true
        }
        for id in subjects { _ = admit(id) }
        for next in [blockersOf, blockedBy] {
            var frontier = subjects
            while !frontier.isEmpty {
                var found = Set<String>()
                for id in frontier {
                    for other in next[id] ?? [] where !picked.contains(other) {
                        found.insert(other)
                    }
                }
                frontier = found.sorted { key($0) < key($1) }.filter { admit($0) }
            }
        }

        let inside = all.filter { picked.contains($0.0) && picked.contains($0.1) }
        var out: [String: [String]] = [:]
        for (from, to) in inside { out[from, default: []].append(to) }
        func reaches(_ start: String, _ goal: String) -> Bool {
            var seen: Set<String> = [start]
            var stack = [start]
            while let id = stack.popLast() {
                if id == goal { return true }
                for other in out[id] ?? [] where !seen.contains(other) {
                    seen.insert(other)
                    stack.append(other)
                }
            }
            return false
        }

        let edges: [Edge] = inside
            .map { Edge(from: $0.0, to: $0.1, cycle: reaches($0.1, $0.0)) }
            .sorted { a, b in
                if key(a.from) != key(b.from) { return key(a.from) < key(b.from) }
                return key(a.to) < key(b.to)
            }

        // Longest path over the acyclic edges (Kahn).
        var wave: [String: Int] = [:]
        var pending: [String: Int] = [:]
        for id in picked {
            wave[id] = 0
            pending[id] = 0
        }
        var forward: [String: [String]] = [:]
        for edge in edges where !edge.cycle {
            pending[edge.to, default: 0] += 1
            forward[edge.from, default: []].append(edge.to)
        }
        var ready = Array(picked).filter { pending[$0] == 0 }
        while let id = ready.popLast() {
            for other in forward[id] ?? [] {
                wave[other] = max(wave[other] ?? 0, (wave[id] ?? 0) + 1)
                let left = (pending[other] ?? 0) - 1
                pending[other] = left
                if left == 0 { ready.append(other) }
            }
        }

        let ordered = picked.sorted { a, b in
            let (wa, wb) = (wave[a] ?? 0, wave[b] ?? 0)
            if wa != wb { return wa < wb }
            return key(a) < key(b)
        }
        var laneAt: [Int: Int] = [:]
        let nodes: [Node] = ordered.map { id in
            let w = wave[id] ?? 0
            let lane = laneAt[w] ?? 0
            laneAt[w] = lane + 1
            return Node(id: id, wave: w, lane: lane, subject: subjectSet.contains(id))
        }

        return Graph(
            nodes: nodes,
            edges: edges,
            hasCycle: edges.contains { $0.cycle },
            truncated: truncated
        )
    }

    // MARK: - Geometry

    /// EXP-1057: THE mini-graph's geometry, identical ×4 and locked by
    /// `domain-contract/fixtures/issue-graph-geometry.json` (web
    /// `lib/issue-graph.ts` `ISSUE_GRAPH_GEOMETRY`, desktop
    /// `domain::issue_graph::geometry`, Android `IssueGraph.Geometry`). Points:
    /// web px = desktop px = iOS pt = Android dp. The grid sits `inset` inside
    /// its scroll box so the rings are never clipped.
    public enum Geometry {
        public static let nodeWidth: Double = 176
        public static let nodeHeight: Double = 28
        public static let waveGap: Double = 40
        public static let laneGap: Double = 8
        public static let inset: Double = 4
        public static let maxViewWidth: Double = 520
        public static let maxViewHeight: Double = 320
        public static let edgeStroke: Double = 1.25
        public static let ringWidth: Double = 1
        public static let nodeRadius: Double = 6
        public static let railGutter: Double = 8
        public static let railNodeWidth: Double = 24
        public static let railDot: Double = 10
        public static let railDotRing: Double = 2

        public struct Point: Equatable, Sendable {
            public let x: Double
            public let y: Double

            public init(x: Double, y: Double) {
                self.x = x
                self.y = y
            }
        }

        /// The grid's natural size (insets included) and the viewport it shows
        /// before scrolling.
        public struct Size: Equatable, Sendable {
            public let width: Double
            public let height: Double
            public let viewWidth: Double
            public let viewHeight: Double

            public init(width: Double, height: Double, viewWidth: Double, viewHeight: Double) {
                self.width = width
                self.height = height
                self.viewWidth = viewWidth
                self.viewHeight = viewHeight
            }
        }

        /// One edge as a cubic: blocker's right-middle → blocked box's left-middle.
        public struct EdgeCurve: Equatable, Sendable {
            public let start: Point
            public let control1: Point
            public let control2: Point
            public let end: Point

            public init(start: Point, control1: Point, control2: Point, end: Point) {
                self.start = start
                self.control1 = control1
                self.control2 = control2
                self.end = end
            }
        }

        /// A node box's top-left inside the grid.
        public static func origin(wave: Int, lane: Int) -> Point {
            Point(
                x: inset + Double(wave) * (nodeWidth + waveGap),
                y: inset + Double(lane) * (nodeHeight + laneGap)
            )
        }

        /// The grid for `waves` × `lanes`; nothing at all when either is 0.
        public static func size(waves: Int, lanes: Int) -> Size {
            guard waves > 0, lanes > 0 else {
                return Size(width: 0, height: 0, viewWidth: 0, viewHeight: 0)
            }
            let width = 2 * inset + Double(waves) * (nodeWidth + waveGap) - waveGap
            let height = 2 * inset + Double(lanes) * (nodeHeight + laneGap) - laneGap
            return Size(
                width: width,
                height: height,
                viewWidth: min(width, maxViewWidth),
                viewHeight: min(height, maxViewHeight)
            )
        }

        /// The grid a graph needs: its highest wave and lane, plus one.
        public static func size(of graph: Graph) -> Size {
            size(
                waves: (graph.nodes.map(\.wave).max() ?? -1) + 1,
                lanes: (graph.nodes.map(\.lane).max() ?? -1) + 1
            )
        }

        /// Forward, the curve bends on the gap's middle; a backward (cycle)
        /// edge bows by max(waveGap / 2, |dx| / 2).
        public static func edge(
            from: (wave: Int, lane: Int),
            to: (wave: Int, lane: Int)
        ) -> EdgeCurve {
            let a = origin(wave: from.wave, lane: from.lane)
            let b = origin(wave: to.wave, lane: to.lane)
            let start = Point(x: a.x + nodeWidth, y: a.y + nodeHeight / 2)
            let end = Point(x: b.x, y: b.y + nodeHeight / 2)
            let bend = end.x > start.x
                ? (end.x - start.x) / 2
                : max(waveGap / 2, abs(end.x - start.x) / 2)
            return EdgeCurve(
                start: start,
                control1: Point(x: start.x + bend, y: start.y),
                control2: Point(x: end.x - bend, y: end.y),
                end: end
            )
        }
    }

    // MARK: - Pieces

    /// The `blocks` edges both of whose ends are synced and open, deduped and
    /// in row order. `keep` are the subjects, which stay whatever their status.
    private static func openEdges(
        _ relations: [IssueRelationEntity],
        _ issues: [IssueEntity],
        keep: Set<String>
    ) -> [(String, String)] {
        let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        func usable(_ id: String) -> Bool {
            guard let issue = byId[id] else { return false }
            return keep.contains(id) || !isFinished(issue)
        }
        var seen = Set<String>()
        var edges: [(String, String)] = []
        for relation in relations
        where relation.type == IssueRelationType.blocks.rawValue {
            if relation.issueId == relation.relatedIssueId { continue }
            guard usable(relation.issueId), usable(relation.relatedIssueId) else { continue }
            let key = "\(relation.issueId)\n\(relation.relatedIssueId)"
            if seen.contains(key) { continue }
            seen.insert(key)
            edges.append((relation.issueId, relation.relatedIssueId))
        }
        return edges
    }

    /// The terminal anchors: an issue in one of them blocks nothing, and is
    /// blocked by nothing.
    private static func isFinished(_ issue: IssueEntity) -> Bool {
        switch IssueStatus.from(issue.status) {
        case .done, .cancelled, .duplicate: return true
        case .backlog, .inProgress, .inReview: return false
        }
    }

    /// Identifier order, id as the deterministic tie-break.
    private static func sortKey(_ issue: IssueEntity) -> (String, String) {
        (issue.identifier ?? issue.id, issue.id)
    }
}
