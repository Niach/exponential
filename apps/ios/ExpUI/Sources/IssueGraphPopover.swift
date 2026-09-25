import ExpCore
import SwiftUI

/// EXP-1082 (workflow contract): the blocks MINI-GRAPH as a shared ExpUI
/// view — the same inputs the issue list's row-badge sheet draws today
/// (`IssueGraphSheet` / `IssueGraphView` in the app target): the issue's
/// transitive `IssueGraph.Graph`, the issues it names, and a tap-to-open.
/// STUB: renders nothing yet; EXP-1057 fills it by COPYING (not moving) the
/// drawing out of the app target's IssueGraphView, which IssueListView keeps
/// using until then.
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

    public var body: some View {
        EmptyView()
    }
}
