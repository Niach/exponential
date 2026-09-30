import ExpCore
import SwiftUI

/// EXP-1057 — THE blocks MINI-GRAPH, drawn exactly like web (`@exp/ui`
/// `WaveGraph`), desktop and Android: a VERTICAL GRID (SLOP-16) whose rows
/// are waves (top = the first blockers, bottom = the blocked subject) and
/// columns lanes, one issue-chip box per node, a cubic edge from the
/// blocker's bottom-middle to the blocked box's top-middle — grey, RED on a
/// blocking cycle. Subjects wear the primary ring, cycle members a destructive one;
/// the two byte-locked `IssueGraph` notes sit underneath.
///
/// Every number comes from `IssueGraph.Geometry` (locked ×4 by
/// `issue-graph-geometry.json`); the inset is part of the grid, so the rings
/// are never clipped. Past `maxViewHeight` the grid scrolls down; only a
/// multi-lane grid wider than `maxViewWidth` (or the width the parent offers)
/// scrolls sideways, so a chain never does.
///
/// SLOP-16: `density: .compact` (the badge overlay, the phone's rail sheet)
/// draws the SMALL chip in `compactNodeWidth` boxes — the title rides in the
/// chip's tooltip and accessibility label.
public struct IssueGraphPopover: View {
    public let graph: IssueGraph.Graph
    public let issues: [IssueEntity]
    public let density: IssueGraph.Geometry.Density
    public let onOpenIssue: (String) -> Void

    public init(
        graph: IssueGraph.Graph,
        issues: [IssueEntity],
        density: IssueGraph.Geometry.Density = .full,
        onOpenIssue: @escaping (String) -> Void
    ) {
        self.graph = graph
        self.issues = issues
        self.density = density
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
        let size = G.size(of: graph, nodeWidth: density.nodeWidth)
        let byId = issuesById
        let cycleIds = onCycle
        let lanes = (graph.nodes.map(\.lane).max() ?? 0) + 1
        let axes: Axis.Set = lanes > 1 ? [.horizontal, .vertical] : .vertical
        return ScrollView(axes, showsIndicators: false) {
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
        let nodeWidth = density.nodeWidth
        return Canvas { context, _ in
            for edge in edges {
                guard let from = cells[edge.from], let to = cells[edge.to] else { continue }
                let curve = G.edge(from: from, to: to, nodeWidth: nodeWidth)
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

    /// One node: the shared issue chip FILLING a `nodeWidth` × `nodeHeight` box
    /// at its origin (so the edges meet its border), ringed when it is a subject or on a cycle.
    @ViewBuilder
    private func nodeBox(_ node: IssueGraph.Node, issue: IssueEntity?, cycle: Bool) -> some View {
        let origin = G.origin(wave: node.wave, lane: node.lane, nodeWidth: density.nodeWidth)
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
                bodySize: Self.chipBodySize,
                size: density == .compact ? .sm : .md,
                fillsFrame: true
            )
            .frame(
                width: CGFloat(density.nodeWidth),
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
