import Foundation

/// #EXP-2204 (packages/exponential-ui/docs/ci-components-contract.md §5):
/// the JobGraph layout, the SDK's src/job-graph.ts (eui-core
/// `job_graph.rs`, Kotlin `JobGraphLayout.kt`); `CiLogTests` replays
/// fixtures/job-graph.json through it. Stages as columns (else the needs
/// depth), a matrix as one node, the need edges as cubics; `compact` = one
/// full-width row per node (the surface below `md`).
public enum JobGraph {
    /// style.json `jobGraph` (px); `rowHeight` = a compact list row.
    public struct Geometry: Equatable {
        public var nodeWidth: Double = 200
        public var nodeHeight: Double = 40
        public var columnGap: Double = 48
        public var rowGap: Double = 12
        public var stageHeight: Double = 24
        public var stageGap: Double = 8
        public var edgeWidth: Double = 1.5
        public var rowHeight: Double = 40
        public init() {}
    }

    public static let geometry = Geometry()

    public struct Node: Equatable {
        /// The job id, or the matrix key of a collapsed matrix.
        public let id: String
        public let name: String
        public var status: String
        public let duration: String?
        /// A matrix: (passed, total); else nil.
        public var count: (passed: Int, total: Int)?
        public var members: [String]
        /// Known upstream node ids, first-seen order, never itself.
        public var needs: [String]
        public let stage: String
        public var column = 0
        public var row = 0
        public var x: Double = 0
        public var y: Double = 0
        public var width: Double = 0
        public var height: Double = 0

        /// The meta text: `3/5` for a matrix, else the duration.
        public var meta: String? { count.map { "\($0.passed)/\($0.total)" } ?? duration }

        public static func == (a: Node, b: Node) -> Bool {
            a.id == b.id && a.name == b.name && a.status == b.status && a.duration == b.duration && a.count?.passed == b.count?.passed && a.count?.total == b.count?.total
                && a.members == b.members && a.needs == b.needs && a.stage == b.stage && a.column == b.column && a.row == b.row && a.x == b.x && a.y == b.y && a.width == b.width && a.height == b.height
        }
    }

    public struct Edge: Equatable {
        public let from: String
        public let to: String
        public let start: CGPoint
        public let c1: CGPoint
        public let c2: CGPoint
        public let end: CGPoint
    }

    public struct Layout: Equatable {
        /// `graph` | `list`.
        public let mode: String
        public let width: Double
        public let height: Double
        public let stages: [(title: String, x: Double)]
        public let nodes: [Node]
        public let edges: [Edge]

        public static func == (a: Layout, b: Layout) -> Bool {
            a.mode == b.mode && a.width == b.width && a.height == b.height && a.stages.map(\.title) == b.stages.map(\.title) && a.stages.map(\.x) == b.stages.map(\.x) && a.nodes == b.nodes && a.edges == b.edges
        }
    }

    /// The aggregate status of a matrix: the first present, in this order.
    static let matrixOrder = ["failed", "running", "queued", "cancelled", "passed", "skipped"]

    private static func nonEmpty(_ v: Any?) -> String? {
        guard let s = v as? String, !s.isEmpty else { return nil }
        return s
    }

    private struct Draft {
        var node: Node
        var rawNeeds: [String]
        var statuses: [String]
        let matrix: Bool
    }

    /// The jobs → stage columns (or needs depth), the node boxes and the need
    /// edges; `compact` = the list below `md`.
    public static func layout(_ jobs: Any?, compact: Bool, geometry g: Geometry = geometry) -> Layout {
        var drafts: [Draft] = []
        var byKey: [String: Int] = [:]
        var nodeOf: [String: String] = [:]
        var seen = Set<String>()
        for raw in jobs as? [Any] ?? [] {
            guard let job = raw as? [String: Any], let id = nonEmpty(job["id"]), !seen.contains(id) else { continue }
            seen.insert(id)
            let status = CiLog.status(job["status"])
            let needs = (job["needs"] as? [Any] ?? []).compactMap { $0 as? String }
            let stage = job["stage"] as? String ?? ""
            if let matrix = nonEmpty(job["matrix"]) {
                let existing = byKey[matrix]
                if existing == nil, nodeOf[matrix] == matrix { continue } // a plain job owns that id
                nodeOf[id] = matrix
                if let i = existing {
                    drafts[i].node.members.append(id)
                    drafts[i].statuses.append(status)
                    drafts[i].rawNeeds += needs
                    continue
                }
                byKey[matrix] = drafts.count
                drafts.append(Draft(node: Node(id: matrix, name: matrix, status: status, duration: nil, count: nil, members: [id], needs: [], stage: stage), rawNeeds: needs, statuses: [status], matrix: true))
                continue
            }
            if byKey[id] != nil { continue } // a matrix owns that id
            nodeOf[id] = id
            let name = nonEmpty(job["name"]) ?? id
            drafts.append(Draft(node: Node(id: id, name: name, status: status, duration: job["duration"] as? String, count: nil, members: [id], needs: [], stage: stage), rawNeeds: needs, statuses: [status], matrix: false))
        }
        let nodeIds = Set(drafts.map(\.node.id))
        for i in drafts.indices {
            if drafts[i].matrix {
                let s = drafts[i].statuses
                drafts[i].node.status = matrixOrder.first { s.contains($0) } ?? "queued"
                drafts[i].node.count = (s.filter { $0 == "passed" }.count, s.count)
            }
            var needs: [String] = []
            for need in drafts[i].rawNeeds {
                guard let target = nodeOf[need] ?? (byKey[need] != nil ? need : nil), nodeIds.contains(target), target != drafts[i].node.id, !needs.contains(target) else { continue }
                needs.append(target)
            }
            drafts[i].node.needs = needs
        }
        var nodes = drafts.map(\.node)
        var index: [String: Int] = [:]
        for (i, n) in nodes.enumerated() { index[n.id] = i }

        // Columns: the stages in first-appearance order, else the needs depth.
        let staged = nodes.contains { !$0.stage.isEmpty }
        var titles: [String] = []
        if staged {
            for i in nodes.indices {
                if let c = titles.firstIndex(of: nodes[i].stage) {
                    nodes[i].column = c
                } else {
                    titles.append(nodes[i].stage)
                    nodes[i].column = titles.count - 1
                }
            }
        } else {
            var depth = [Int?](repeating: nil, count: nodes.count)
            var onPath = [Bool](repeating: false, count: nodes.count)
            func visit(_ i: Int) -> Int {
                if let d = depth[i] { return d }
                onPath[i] = true
                var d = 0
                for need in nodes[i].needs {
                    let j = index[need]!
                    if onPath[j] { continue }
                    d = max(d, visit(j) + 1)
                }
                onPath[i] = false
                depth[i] = d
                return d
            }
            for i in nodes.indices {
                let d = visit(i)
                nodes[i].column = d
            }
        }

        // Column order: column 0 first, then input order.
        var ordered = nodes.enumerated().sorted { $0.element.column != $1.element.column ? $0.element.column < $1.element.column : $0.offset < $1.offset }.map(\.element)

        if compact {
            for i in ordered.indices {
                ordered[i].row = i
                ordered[i].x = 0
                ordered[i].y = Double(i) * g.rowHeight
                ordered[i].width = 0
                ordered[i].height = g.rowHeight
            }
            return Layout(mode: "list", width: 0, height: Double(ordered.count) * g.rowHeight, stages: [], nodes: ordered, edges: [])
        }

        let top = staged ? g.stageHeight + g.stageGap : 0
        let pitchX = g.nodeWidth + g.columnGap
        var rows: [Int: Int] = [:]
        for i in ordered.indices {
            let r = rows[ordered[i].column] ?? 0
            rows[ordered[i].column] = r + 1
            ordered[i].row = r
            ordered[i].x = Double(ordered[i].column) * pitchX
            ordered[i].y = top + Double(r) * (g.nodeHeight + g.rowGap)
            ordered[i].width = g.nodeWidth
            ordered[i].height = g.nodeHeight
        }
        let columns = (ordered.map(\.column).max() ?? -1) + 1
        let maxRows = rows.values.max() ?? 0
        let width = columns > 0 ? Double(columns) * pitchX - g.columnGap : 0
        let height = maxRows > 0 ? top + Double(maxRows) * (g.nodeHeight + g.rowGap) - g.rowGap : 0
        let stages = staged ? titles.enumerated().map { (title: $0.element, x: Double($0.offset) * pitchX) } : []
        var at: [String: Int] = [:]
        for (i, n) in ordered.enumerated() { at[n.id] = i }
        var edges: [Edge] = []
        for n in ordered {
            for need in n.needs {
                let up = ordered[at[need]!]
                let x1 = up.x + up.width, y1 = up.y + up.height / 2
                let x2 = n.x, y2 = n.y + n.height / 2
                let mx = (x1 + x2) / 2
                edges.append(Edge(from: up.id, to: n.id, start: CGPoint(x: x1, y: y1), c1: CGPoint(x: mx, y: y1), c2: CGPoint(x: mx, y: y2), end: CGPoint(x: x2, y: y2)))
            }
        }
        return Layout(mode: "graph", width: width, height: height, stages: stages, nodes: ordered, edges: edges)
    }

    /// The node `id` and every ancestor through `needs`, in the layout's
    /// node order; an unknown id → [].
    public static func upstream(_ layout: Layout, _ id: String) -> [String] {
        var byId: [String: Node] = [:]
        for n in layout.nodes { byId[n.id] = n }
        guard byId[id] != nil else { return [] }
        var set: Set<String> = [id]
        var stack = [id]
        while let top = stack.popLast() {
            for need in byId[top]!.needs where !set.contains(need) {
                set.insert(need)
                stack.append(need)
            }
        }
        return layout.nodes.map(\.id).filter { set.contains($0) }
    }
}
