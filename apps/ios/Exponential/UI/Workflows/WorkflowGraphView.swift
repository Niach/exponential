import ExpCore
import ExpUI
import SwiftUI

/// EXP-981 — a workflow's graph on a phone. Web and the IDE draw the grid with
/// its edges; a phone has no room for one, so it renders the SAME nodes as the
/// wave-grouped LIST the blocks graph uses (`IssueGraphView`, EXP-980): a
/// `Wave 1` / `Wave 2` … band per column, one row per node, and under each row
/// the chips of the nodes that block it — red where the edge is part of a
/// cycle.
///
/// Nothing is laid out here: `wave` and `lane` come off the synced rows (the
/// server computes them), and the edges come from `WorkflowView.edges` over the
/// already synced `blocks` relations. A node whose issue row has not synced
/// renders its caption alone rather than crashing.
struct WorkflowGraphView: View {
    let nodes: [WorkflowNodeEntity]
    let edges: [WorkflowView.Edge]
    /// contract `wfStatus` — the caption reads the plan on a draft and the
    /// state once it runs.
    let workflowStatus: String
    /// The synced rows the nodes are named from.
    let issues: [String: IssueEntity]
    /// EXP-982 — `WorkflowView.finalPrCaption`, nil while the final-PR node is
    /// not drawn (a wave of its own after the last one).
    let finalPrCaption: String?
    let finalPrUrl: String?
    let onSelect: (WorkflowNodeEntity) -> Void

    private var nodesById: [String: WorkflowNodeEntity] {
        Dictionary(nodes.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// The nodes grouped by column, in (wave, lane) order — the server's
    /// layout, read straight off the rows.
    private var waves: [(wave: Int, nodes: [WorkflowNodeEntity])] {
        var order: [Int] = []
        var byWave: [Int: [WorkflowNodeEntity]] = [:]
        for node in nodes.sorted(by: { ($0.wave, $0.lane) < ($1.wave, $1.lane) }) {
            if byWave[node.wave] == nil { order.append(node.wave) }
            byWave[node.wave, default: []].append(node)
        }
        return order.map { (wave: $0, nodes: byWave[$0] ?? []) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if nodes.isEmpty {
                Text("This workflow has no issues yet.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.top, 8)
            } else {
                ForEach(waves, id: \.wave) { entry in
                    GlassSectionBand("Wave \(entry.wave + 1)")
                    ForEach(entry.nodes) { node in
                        nodeRow(node)
                    }
                }
                // The final PR is the node after the last wave: the ONE pull
                // request integration → default branch.
                if let caption = finalPrCaption {
                    GlassSectionBand("Wave \((waves.last?.wave ?? 0) + 2)")
                    finalPrRow(caption)
                }
            }
        }
        .accessibilityIdentifier("workflow-graph")
    }

    /// The final-PR node. It links out to the pull request once there is one;
    /// until then it is the caption alone ("Opening the pull request").
    @ViewBuilder
    private func finalPrRow(_ caption: String) -> some View {
        let row = VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.navReviews, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(WorkflowView.finalPrTitle)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                Spacer(minLength: 0)
            }
            Text(caption)
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()

        if let url = finalPrUrl.flatMap(URL.init(string:)) {
            Link(destination: url) { row.contentShape(Rectangle()) }
                .buttonStyle(.plain)
                .accessibilityIdentifier("workflow-final-pr-row")
        } else {
            row.accessibilityIdentifier("workflow-final-pr-row")
        }
    }

    @ViewBuilder
    private func nodeRow(_ node: WorkflowNodeEntity) -> some View {
        // The edges pointing AT this node: what blocks it inside the workflow,
        // in the rule's edge order.
        let incoming = edges.filter { $0.to == node.id }
        let caption = WorkflowView.nodeCaption(
            WorkflowView.CaptionNode(kind: node.kind, state: node.state, risk: node.risk),
            workflowStatus: workflowStatus
        )
        let tone = WorkflowView.nodeTone(node.state)
        Button { onSelect(node) } label: {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    chip(for: node, cycle: node.onCycle)
                    Spacer(minLength: 0)
                }
                // EXP-982: the caption painted by its tone PLUS a glyph, so a
                // state reads by shape as well as by colour.
                HStack(spacing: 5) {
                    if let icon = Self.stateIcon(node.state) {
                        AppIcon(icon, size: AppIcon.Size.small)
                            .foregroundStyle(Self.color(tone))
                    }
                    Text(caption)
                        .font(.caption2)
                        .foregroundStyle(Self.color(tone))
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
                                    if let blocker = nodesById[edge.from] {
                                        chip(for: blocker, cycle: edge.cycle, edge: true)
                                    }
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
            // A compound node (a parent run as one batch with its sub-issues)
            // is drawn as a STACKED card: a second card edge peeking out
            // behind it, the ×4 rule's shorthand for "this is several issues".
            .background(alignment: .bottom) {
                if !node.memberIssueIds.isEmpty {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(GlassTokens.fillRow)
                        .overlay(
                            RoundedRectangle(cornerRadius: 10)
                                .stroke(GlassTokens.strokeRow, lineWidth: GlassTokens.hairline)
                        )
                        .padding(.horizontal, 6)
                        .offset(y: 5)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("workflow-node-row")
    }

    /// One node, as the shared issue badge: `EXP-14 +3` for a compound node,
    /// the bare identifier otherwise. A blocker chip (`edge`) stands in for the
    /// EDGE the grid clients draw, so it takes the edge's own paint: red on a
    /// cycle, green once the node it comes FROM has landed.
    @ViewBuilder
    private func chip(
        for node: WorkflowNodeEntity, cycle: Bool, edge: Bool = false
    ) -> some View {
        let issue = issues[node.issueId]
        let status = IssueStatus.from(issue?.status)
        let landed = node.state == DomainContract.wfNodeStateLanded
        IssueChip(
            identifier: WorkflowView.nodeTitle(
                // A node whose issue has not synced still names itself.
                identifier: issue?.identifier ?? node.issueId,
                memberCount: node.memberIssueIds.count
            ),
            title: issue?.title,
            iconName: status.iconName,
            statusColor: cycle
                ? DesignTokens.Semantic.red
                : (edge && landed ? DesignTokens.Semantic.green : status.color)
        )
    }

    /// The glyph a state wears beside its caption — existing icon CONCEPTS
    /// only. The quiet states (blocked / ready / proposed / skipped / paused)
    /// get none: the caption alone is the whole message.
    static func stateIcon(_ state: String) -> String? {
        switch state {
        case DomainContract.wfNodeStateRunning: AppIcons.codingRunning
        case DomainContract.wfNodeStateWaiting: AppIcons.uiWarning
        case DomainContract.wfNodeStateLanded: AppIcons.notificationPrMerged
        case DomainContract.wfNodeStateFailed: AppIcons.uiError
        case DomainContract.wfNodeStateInReview, DomainContract.wfNodeStateUpdating:
            AppIcons.navReviews
        default: nil
        }
    }

    /// The shared tone in this app's own state vocabulary — the same colours
    /// `SessionStateDot` paints a run with, so a running node and a running run
    /// read alike.
    static func color(_ tone: WorkflowView.Tone) -> Color {
        switch tone {
        case .muted: .white.opacity(TextOpacity.tertiary)
        case .active: DesignTokens.Semantic.green
        case .amber: DesignTokens.Semantic.yellow
        case .success: DesignTokens.Semantic.blue
        case .danger: DesignTokens.Semantic.red
        }
    }
}
