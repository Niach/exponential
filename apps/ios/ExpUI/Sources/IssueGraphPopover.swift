import ExpCore
import SwiftUI

/// EXP-1057 — THE blocks MINI-GRAPH, drawn exactly like web (`@exp/ui`
/// `WaveGraph`), desktop and Android: a GRID whose columns are waves and rows
/// lanes, one issue-chip box per node, a cubic edge from the blocker's
/// right-middle to the blocked box's left-middle — grey, RED on a blocking
/// cycle. Subjects wear the primary ring, cycle members a destructive one;
/// the two byte-locked `IssueGraph` notes sit underneath.
///
/// Every number comes from `IssueGraph.Geometry` (locked ×4 by
/// `issue-graph-geometry.json`); the inset is part of the grid, so the rings
/// are never clipped. Past `maxViewWidth` / `maxViewHeight` (or the width the
/// parent offers) the grid scrolls both ways.
public struct IssueGraphPopover: View {
    public let graph: IssueGraph.Graph
    public let issues: [IssueEntity]
    public let onOpenIssue: (String) -> Void

    public init(
        graph: IssueGraph.Graph,
        issues: [IssueEntity],
        onOpenIssue: @escaping (String) -> Void
    ) {
        self.graph = graph
        self.issues = issues
        self.onOpenIssue = onOpenIssue
    }

    private typealias G = IssueGraph.Geometry

    /// The chip text measure inside a 28pt box.
    private static let chipBodySize: CGFloat = 13

    private var issuesById: [String: IssueEntity] {
        Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// Nodes touched by a cycle edge: the destructive ring.
    private var onCycle: Set<String> {
        var ids = Set<String>()
        for edge in graph.edges where edge.cycle {
            ids.insert(edge.from)
            ids.insert(edge.to)
        }
        return ids
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !graph.nodes.isEmpty {
                grid
            }
            if graph.hasCycle {
                note(IssueGraph.cycleNote, color: DesignTokens.Palette.destructive)
            }
            if graph.truncated {
                note(IssueGraph.truncatedNote, color: .white.opacity(TextOpacity.tertiary))
            }
        }
        .accessibilityIdentifier("issue-graph")
    }

    // MARK: - Grid

    private var grid: some View {
        let size = G.size(of: graph)
        let byId = issuesById
        let cycleIds = onCycle
        return ScrollView([.horizontal, .vertical], showsIndicators: false) {
            ZStack(alignment: .topLeading) {
                edges
                ForEach(graph.nodes, id: \.id) { node in
                    nodeBox(node, issue: byId[node.id], cycle: cycleIds.contains(node.id))
                }
            }
            .frame(width: CGFloat(size.width), height: CGFloat(size.height), alignment: .topLeading)
        }
        .frame(maxWidth: CGFloat(size.viewWidth), alignment: .leading)
        .frame(height: CGFloat(size.viewHeight))
    }

    /// Every edge, behind the boxes.
    private var edges: some View {
        let cells = Dictionary(
            graph.nodes.map { ($0.id, (wave: $0.wave, lane: $0.lane)) },
            uniquingKeysWith: { a, _ in a }
        )
        let edges = graph.edges
        return Canvas { context, _ in
            for edge in edges {
                guard let from = cells[edge.from], let to = cells[edge.to] else { continue }
                let curve = G.edge(from: from, to: to)
                var path = Path()
                path.move(to: point(curve.start))
                path.addCurve(
                    to: point(curve.end),
                    control1: point(curve.control1),
                    control2: point(curve.control2)
                )
                context.stroke(
                    path,
                    with: .color(edge.cycle ? DesignTokens.Palette.destructive : GlassTokens.strokeStrong),
                    lineWidth: CGFloat(G.edgeStroke)
                )
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func point(_ p: G.Point) -> CGPoint {
        CGPoint(x: p.x, y: p.y)
    }

    /// One node: the shared issue chip in a `nodeWidth` × `nodeHeight` box at
    /// its origin, ringed when it is a subject or on a cycle.
    @ViewBuilder
    private func nodeBox(_ node: IssueGraph.Node, issue: IssueEntity?, cycle: Bool) -> some View {
        let origin = G.origin(wave: node.wave, lane: node.lane)
        let status = IssueStatus.from(issue?.status)
        let identifier = issue?.identifier ?? node.id
        let ring: Color? = cycle
            ? DesignTokens.Palette.destructive
            : (node.subject ? DesignTokens.Palette.primary : nil)
        let shape = RoundedRectangle(cornerRadius: CGFloat(G.nodeRadius), style: .continuous)
        Button { onOpenIssue(node.id) } label: {
            IssueChip(
                identifier: identifier,
                title: issue?.title,
                iconName: status.iconName,
                statusColor: status.color,
                bodySize: Self.chipBodySize
            )
            .frame(
                width: CGFloat(G.nodeWidth),
                height: CGFloat(G.nodeHeight),
                alignment: .leading
            )
            .clipShape(shape)
            .overlay {
                if let ring {
                    shape.strokeBorder(ring, lineWidth: CGFloat(G.ringWidth))
                }
            }
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("issue-graph-node-\(identifier)")
        .offset(x: CGFloat(origin.x), y: CGFloat(origin.y))
    }

    private func note(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.caption2)
            .foregroundStyle(color)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
