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
/// EXP-1057: it draws the SAME grid web, desktop and Android draw — the shared
/// ExpUI `IssueGraphPopover` (waves × lanes, chip boxes, cubic edges, red on a
/// cycle, geometry locked by `issue-graph-geometry.json`). An empty graph
/// says so instead of drawing nothing.
struct IssueGraphView: View {
    let graph: IssueGraph.Graph
    /// The synced rows the nodes are named from; a node with no row prints its
    /// id, exactly like the rule's ordering fallback.
    let issues: [IssueEntity]
    let onOpenIssue: (String) -> Void

    var body: some View {
        if graph.nodes.isEmpty {
            Text("Nothing blocks this work.")
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.top, 8)
                .accessibilityIdentifier("issue-graph")
        } else {
            IssueGraphPopover(graph: graph, issues: issues, onOpenIssue: onOpenIssue)
        }
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
