import ExpCore
import ExpUI
import SwiftUI

/// EXP-980 — one rendered issue-list row: the issue and how deep it hangs
/// under its root (`IssueNesting`). Shared by every list that nests — the
/// board list and My Issues — so the row and its `TreeGuides` connector are
/// described the same way on both.
struct NestedIssueRow: Identifiable {
    let issue: IssueEntity
    /// 0 = a root row, 1 = its sub-issue, …
    let depth: Int
    var id: String { issue.id }
}

/// EXP-980 — the ONE blocks-graph surface on iOS, used in three places: the
/// list row's badge, the Work header overlay's Issue face (where it replaced
/// the flat "Blocked by" chip section) and the blocked-start sheet.
///
/// A phone has no room for the grid web and the IDE draw, so it renders the
/// SAME `IssueGraph.Graph` as a LIST grouped by wave (the ×4 rule): a
/// `Wave 1` / `Wave 2` … band per column, one row per node, and under each row
/// the chips of its direct blockers INSIDE the graph — red where the edge is
/// part of a cycle. Subject rows are outlined. The two notes under the graph
/// are the byte-locked `IssueGraph` copy.
struct IssueGraphView: View {
    let graph: IssueGraph.Graph
    /// The synced rows the nodes are named from; a node with no row prints its
    /// id, exactly like the rule's ordering fallback.
    let issues: [IssueEntity]
    let onOpenIssue: (String) -> Void

    private var issuesById: [String: IssueEntity] {
        Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// The nodes grouped by column, in the rule's order (nodes already come
    /// back sorted by wave then lane).
    private var waves: [(wave: Int, nodes: [IssueGraph.Node])] {
        var order: [Int] = []
        var byWave: [Int: [IssueGraph.Node]] = [:]
        for node in graph.nodes {
            if byWave[node.wave] == nil { order.append(node.wave) }
            byWave[node.wave, default: []].append(node)
        }
        return order.map { (wave: $0, nodes: byWave[$0] ?? []) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if graph.nodes.isEmpty {
                note("Nothing blocks this work.")
            } else {
                ForEach(waves, id: \.wave) { entry in
                    GlassSectionBand("Wave \(entry.wave + 1)")
                    ForEach(entry.nodes, id: \.id) { node in
                        nodeRow(node)
                    }
                }
            }
            if graph.hasCycle { note(IssueGraph.cycleNote) }
            if graph.truncated { note(IssueGraph.truncatedNote) }
        }
        .accessibilityIdentifier("issue-graph")
    }

    @ViewBuilder
    private func nodeRow(_ node: IssueGraph.Node) -> some View {
        // The edges pointing AT this node: its direct blockers inside the
        // graph, in the rule's edge order.
        let incoming = graph.edges.filter { $0.to == node.id }
        Button { onOpenIssue(node.id) } label: {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    chip(for: node.id, cycle: false)
                    Spacer(minLength: 0)
                }
                if !incoming.isEmpty {
                    HStack(spacing: 6) {
                        Text("Blocked by")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        // A wide fan-in would push the row off a phone: the
                        // chips scroll sideways instead of wrapping.
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 4) {
                                ForEach(incoming, id: \.from) { edge in
                                    chip(for: edge.from, cycle: edge.cycle)
                                }
                            }
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .flatRow()
            // The picked work reads as picked, the way a selected issue row
            // does — the graph is otherwise all the same weight.
            .overlay {
                if node.subject {
                    RoundedRectangle(cornerRadius: 10)
                        .stroke(DesignTokens.Palette.primary.opacity(0.45), lineWidth: 1)
                        .allowsHitTesting(false)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// One node, as the shared issue badge. `cycle` paints its status glyph
    /// red — the list's stand-in for the red EDGE the grid clients draw.
    @ViewBuilder
    private func chip(for id: String, cycle: Bool) -> some View {
        let issue = issuesById[id]
        let status = IssueStatus.from(issue?.status)
        IssueChip(
            identifier: issue?.identifier ?? id,
            title: issue?.title,
            iconName: status.iconName,
            statusColor: cycle ? DesignTokens.Semantic.red : status.color
        )
    }

    @ViewBuilder
    private func note(_ text: String) -> some View {
        Text(text)
            .font(.caption2)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.top, 8)
    }
}

/// The graph as its own sheet — what a list row's blocks badge opens.
struct IssueGraphSheet: View {
    let graph: IssueGraph.Graph
    let issues: [IssueEntity]
    let onOpenIssue: (String) -> Void

    var body: some View {
        GlassSheetChrome(title: "Blocked work", height: .fitted) {
            IssueGraphView(graph: graph, issues: issues, onOpenIssue: onOpenIssue)
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
        }
        .accessibilityIdentifier("issue-graph-sheet")
    }
}

/// The issue a list is showing the graph for (`.sheet(item:)`).
struct IssueGraphTarget: Identifiable {
    let id: String
}

/// EXP-980 — the row badge: one small pill after the title carrying the
/// `blocked-by` count in the destructive tone and/or the `blocks` count in the
/// muted one. Absent when both are 0; its accessible label is the byte-locked
/// `IssueGraph.blocksBadgeLabel`.
///
/// Not a `Button`: the badge lives inside a row that is itself one (the board
/// list) or a `NavigationLink` (My Issues), and a nested button would hand its
/// taps to the row. A high-priority tap gesture wins over both, so tapping the
/// pill opens the graph and tapping anywhere else still opens the issue.
struct BlocksBadge: View {
    let counts: IssueGraph.BlockCounts
    let action: () -> Void

    var body: some View {
        HStack(spacing: 5) {
            if counts.blockedBy > 0 {
                side(
                    icon: AppIcons.relationBlockedBy,
                    count: counts.blockedBy,
                    color: DesignTokens.Palette.destructive
                )
            }
            if counts.blocking > 0 {
                side(
                    icon: AppIcons.relationBlocks,
                    count: counts.blocking,
                    color: .white.opacity(TextOpacity.secondary)
                )
            }
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 2)
        .background(Capsule().fill(DesignTokens.Glass.backgroundTop.opacity(0.9)))
        .overlay(Capsule().stroke(.white.opacity(0.12), lineWidth: 1))
        .contentShape(Capsule())
        .highPriorityGesture(TapGesture().onEnded { action() })
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(IssueGraph.blocksBadgeLabel(counts))
        .accessibilityIdentifier("blocks-badge")
    }

    @ViewBuilder
    private func side(icon: String, count: Int, color: Color) -> some View {
        HStack(spacing: 2) {
            AppIcon(icon, size: 10)
            Text("\(count)")
                .font(.caption2.weight(.medium))
                .lineLimit(1)
        }
        .foregroundStyle(color)
    }
}
