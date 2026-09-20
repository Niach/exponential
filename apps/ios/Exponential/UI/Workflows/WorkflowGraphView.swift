import ExpCore
import ExpUI
import SwiftUI

/// EXP-981 — a workflow's graph on a phone. Web and the IDE draw the grid with
/// its edges; a phone has no room for one, so it renders the SAME nodes as the
/// wave-grouped LIST the blocks graph uses (`IssueGraphView`, EXP-980): a
/// `Wave 1` / `Wave 2` … band per column, one row per node, and under each row
/// the chips of the nodes that block it. Those chips ARE the edges the grid
/// clients draw, so they take the edge's own style (EXP-983
/// `WorkflowView.edgeStyle`): landed green, stale or cyclic red, speculative a
/// dashed border, plain the chip's default.
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
    /// EXP-982 — `node id → its run`. A node that is UP wears its session's own
    /// dot instead of the state glyph, and the strip above the waves opens it.
    var runs: [String: WorkflowNodeRun] = [:]
    let onSelect: (WorkflowNodeEntity) -> Void
    /// Steer the node's run — the Running strip's tap.
    var onOpenRun: (String) -> Void = { _ in }

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
                runningStrip
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

    /// The runs that are up right now, one tap away: a node's row says THAT it
    /// runs, this strip is the way in. A wide run set scrolls sideways rather
    /// than wrapping the header off a phone.
    @ViewBuilder
    private var runningStrip: some View {
        let live = nodes.compactMap { node -> (WorkflowNodeEntity, WorkflowNodeRun)? in
            guard let run = runs[node.id], run.live else { return nil }
            return (node, run)
        }
        if !live.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                Text(WorkflowView.runningNowLabel)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(live, id: \.0.id) { node, run in
                            GlassPill(
                                WorkflowView.nodeTitle(
                                    identifier: issues[node.issueId]?.identifier ?? node.issueId,
                                    memberCount: node.memberIssueIds.count
                                ),
                                mode: .action { onOpenRun(run.sessionId) }
                            ) {
                                SessionStateDot(tone: run.tone, pulsing: run.busy, size: 8)
                            }
                            .accessibilityIdentifier("workflow-running-\(node.id)")
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.bottom, 8)
            .accessibilityIdentifier("workflow-running-strip")
        }
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
        // EXP-982: a node whose run is UP reads off the session itself — the
        // ×4 dot table, pulsing only while the agent is mid-turn — rather than
        // off the node state alone, so "running" on a phone says as much as the
        // circle does on the grid clients.
        let run = runs[node.id].flatMap { $0.live ? $0 : nil }
        let captionColor = run.map { SessionStateDot.color($0.tone) } ?? Self.color(tone)
        Button { onSelect(node) } label: {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    chip(for: node, cycle: node.onCycle)
                    Spacer(minLength: 0)
                }
                // EXP-982: the caption painted by its tone PLUS a glyph, so a
                // state reads by shape as well as by colour.
                HStack(spacing: 5) {
                    if let run {
                        SessionStateDot(tone: run.tone, pulsing: run.busy, size: 8)
                            .accessibilityIdentifier("workflow-node-run-dot")
                    } else if let icon = Self.stateIcon(node.state) {
                        AppIcon(icon, size: AppIcon.Size.small)
                            .foregroundStyle(Self.color(tone))
                    }
                    Text(caption)
                        .font(.caption2)
                        .foregroundStyle(captionColor)
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
                                        chip(
                                            for: blocker,
                                            cycle: edge.cycle,
                                            style: WorkflowView.edgeStyle(
                                                edge,
                                                fromState: blocker.state,
                                                toState: node.state
                                            )
                                        )
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
            // EXP-984: a `proposed` node is not part of the run until a member
            // admits it. Where the grid clients draw the NODE dashed, a phone
            // row wears the dash as its own border.
            .overlay {
                if node.isProposed { ProposedRowBorder() }
            }
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
    /// the bare identifier otherwise. A blocker chip stands in for the EDGE the
    /// grid clients draw, so it is handed that edge's `style` and wears it.
    @ViewBuilder
    private func chip(
        for node: WorkflowNodeEntity, cycle: Bool, style: WorkflowView.EdgeStyle? = nil
    ) -> some View {
        let issue = issues[node.issueId]
        let status = IssueStatus.from(issue?.status)
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
                : (Self.edgeColor(style) ?? status.color)
        )
        // The dashed hairline says "speculative" the way a dashed edge does.
        .overlay {
            if style == .speculative { SpeculativeChipBorder() }
        }
    }

    /// What an edge's style paints its chip in, or nil where the chip keeps the
    /// issue's own status colour (`plain` and `speculative` — the latter says
    /// what it is with the dash instead).
    static func edgeColor(_ style: WorkflowView.EdgeStyle?) -> Color? {
        switch style {
        case .cycle, .stale: DesignTokens.Semantic.red
        case .landed: DesignTokens.Semantic.green
        default: nil
        }
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

/// EXP-984 — the dashed outline a `proposed` node wears: a follow-up filed
/// during the run that a member has yet to admit, so it is drawn but is not
/// part of the run. Web and the IDE dash the node's own outline; a phone row's
/// outline IS its border.
struct ProposedRowBorder: View {
    var body: some View {
        RoundedRectangle(cornerRadius: GlassTokens.rowRadius, style: .continuous)
            .strokeBorder(
                Color.white.opacity(TextOpacity.tertiary),
                style: StrokeStyle(lineWidth: GlassTokens.hairline, dash: [4, 3])
            )
    }
}

/// EXP-983 — the hairline a speculative edge's chip wears: the chip's own
/// border, redrawn as a dash. A phone draws no edges, so the chip standing in
/// for one has to carry the dash itself.
struct SpeculativeChipBorder: View {
    var body: some View {
        RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
            .strokeBorder(
                Color.white.opacity(TextOpacity.secondary),
                style: StrokeStyle(lineWidth: IssueChipTokens.borderWidth, dash: [3, 2])
            )
    }
}
