import ExpCore
import ExpUI
import SwiftUI

/// EXP-981 — a workflow's graph on a phone. Web and the IDE draw the grid; a
/// phone has no room for one, so it renders the SAME nodes as the wave-grouped
/// LIST the blocks graph uses (`IssueGraphView`, EXP-980): a `Wave 1` /
/// `Wave 2` … band per column and one row per node, in the server's own
/// (`wave`, `lane`) order.
///
/// EXP-1014 — a node row IS the app's established issue chip: the glyph slot
/// carries the node's own state (its live run's dot while the run is up, the
/// state's glyph in the state's tone once it has one, the issue's status glyph
/// before that), the identifier and title read exactly as they do in every
/// other chip, and the state CAPTION sits at the row's trailing edge in the
/// same tone. No kind or risk subtitle — what the plan declares is the node
/// sheet's business. A compound node (a parent run as one batch with its
/// sub-issues) is the STACKED chip.
///
/// Under a node that has blockers sits the one dependency a phone CAN draw:
/// the `Blocked by` chips, which ARE the edges the grid clients draw as lines,
/// so they take the edge's own style (EXP-983 `WorkflowView.edgeStyle`) —
/// landed green, stale or cyclic red, speculative a dashed border, plain the
/// chip's default. Android's list draws the same row.
///
/// Nothing is laid out here: `wave` and `lane` come off the synced rows, and
/// the edges come from `WorkflowView.edges` over the already synced `blocks`
/// relations. A node whose issue row has not synced names itself by id rather
/// than crashing.
struct WorkflowGraphView: View {
    let nodes: [WorkflowNodeEntity]
    /// The edges between the nodes (`WorkflowView.edges`) — what the
    /// `Blocked by` chips under each row stand for.
    let edges: [WorkflowView.Edge]
    /// contract `wfStatus` — a draft's nodes have no state worth a caption, so
    /// the rule gives them none.
    let workflowStatus: String
    /// The synced rows the nodes are named from.
    let issues: [String: IssueEntity]
    /// EXP-982 — `WorkflowView.finalPrCaption`, nil while the final-PR node is
    /// not drawn (a wave of its own after the last one).
    let finalPrCaption: String?
    let finalPrUrl: String?
    /// contract `prState` — while it is `open` the final-PR row carries the
    /// Merge control (EXP-1033).
    var finalPrState: String? = nil
    /// EXP-982 — `node id → its run`. A node that is UP wears its session's own
    /// dot instead of the state glyph, and the strip above the waves opens it.
    var runs: [String: WorkflowNodeRun] = [:]
    /// A write is in flight upstairs; the Merge control goes inert rather than
    /// double-firing.
    var busy = false
    let onSelect: (WorkflowNodeEntity) -> Void
    /// Steer the node's run — the Running strip's tap.
    var onOpenRun: (String) -> Void = { _ in }
    /// Squash-merge the final pull request (`workflows.mergeFinalPr`), once
    /// the confirmation is through.
    var onMergeFinalPr: () -> Void = {}

    /// The final-PR merge asks first: it is the one irreversible step of the
    /// whole run, so it confirms like Cancel workflow does.
    @State private var showMergeConfirm = false

    /// The nodes by id — an edge names NODES, so a blocker chip resolves its
    /// issue through this.
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
                                Self.nodeIdentifier(node, issue: issues[node.issueId]),
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

    /// The final-PR node — one more chip after the last wave, reading like
    /// every other row. It links out to the pull request once there is one;
    /// until then it is the caption alone ("Opening the pull request").
    ///
    /// EXP-1033 — while that pull request is OPEN the row also carries the one
    /// human review of the whole run: Merge, confirmed, which squash-merges the
    /// workflow's branch into the default branch and completes the run. A phone
    /// finishes a workflow without leaving for GitHub.
    @ViewBuilder
    private func finalPrRow(_ caption: String) -> some View {
        let body = HStack(spacing: 8) {
            IssueChip(
                identifier: nil,
                title: WorkflowView.finalPrTitle,
                iconName: AppIcons.notificationPrMerged,
                statusColor: .white.opacity(TextOpacity.secondary)
            )
            Spacer(minLength: 8)
            Text(caption)
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
        }

        HStack(spacing: 8) {
            if let url = finalPrUrl.flatMap(URL.init(string:)) {
                Link(destination: url) { body.contentShape(Rectangle()) }
                    .buttonStyle(.plain)
            } else {
                body
            }
            if finalPrState == DomainContract.prStateOpen {
                GlassPill(
                    WorkflowView.mergeFinalPrLabel,
                    icon: AppIcons.notificationPrMerged,
                    mode: .action { showMergeConfirm = true },
                    primary: true,
                    enabled: !busy
                )
                .accessibilityIdentifier("workflow-final-pr-merge")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()
        .accessibilityIdentifier("workflow-final-pr-row")
        .confirmationDialog(
            WorkflowView.mergeFinalPrLabel,
            isPresented: $showMergeConfirm,
            titleVisibility: .visible
        ) {
            Button(WorkflowView.mergeFinalPrLabel) { onMergeFinalPr() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(WorkflowView.mergeFinalPrConfirm)
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
                    nodeChip(node, tone: tone, run: run)
                    Spacer(minLength: 8)
                    if !caption.isEmpty {
                        Text(caption)
                            .font(.caption2)
                            .foregroundStyle(captionColor)
                            .lineLimit(1)
                            .accessibilityIdentifier("workflow-node-caption")
                    }
                }
                blockedBy(node, incoming: incoming)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .flatRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("workflow-node-row")
    }

    /// What blocks this node inside the workflow, chipped. A phone draws no
    /// lines, so each chip IS an edge and wears that edge's style; a wide
    /// fan-in would push the row off the screen, so the chips scroll sideways
    /// rather than wrapping.
    @ViewBuilder
    private func blockedBy(
        _ node: WorkflowNodeEntity, incoming: [WorkflowView.Edge]
    ) -> some View {
        if !incoming.isEmpty {
            HStack(spacing: 6) {
                Text("Blocked by")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 4) {
                        ForEach(incoming, id: \.from) { edge in
                            if let blocker = nodesById[edge.from] {
                                blockerChip(
                                    blocker,
                                    cycle: edge.cycle,
                                    style: WorkflowView.edgeStyle(
                                        edge, fromState: blocker.state, toState: node.state
                                    )
                                )
                            }
                        }
                    }
                }
            }
            .accessibilityIdentifier("workflow-node-blockers")
        }
    }

    /// One blocker, as the shared issue chip standing in for the EDGE the grid
    /// clients draw: the edge's colour where it has one (red on a cycle or a
    /// stale upstream, green once the blocker landed), and the dashed hairline
    /// a speculative edge wears.
    @ViewBuilder
    private func blockerChip(
        _ node: WorkflowNodeEntity, cycle: Bool, style: WorkflowView.EdgeStyle
    ) -> some View {
        let issue = issues[node.issueId]
        let status = issue.map { IssueStatus.from($0.status) }
        IssueChip(
            identifier: Self.nodeIdentifier(node, issue: issue),
            title: Self.nodeChipTitle(issue),
            iconName: status?.iconName,
            statusColor: cycle
                ? DesignTokens.Semantic.red
                : (Self.edgeColor(style) ?? status?.color)
        )
        // The dashed hairline says "speculative" the way a dashed edge does.
        .overlay {
            if style == .speculative {
                WorkflowChipRing(
                    color: .white.opacity(TextOpacity.secondary), dashed: true
                )
            }
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

    /// One node as the shared issue chip, with the workflow's own ring around
    /// it — dashed for a `proposed` node (filed mid-run, not part of it until a
    /// member admits it), destructive for one inside a blocking cycle. A
    /// COMPOUND node is the stacked chip: one run, one branch, one pull
    /// request over several issues.
    @ViewBuilder
    private func nodeChip(
        _ node: WorkflowNodeEntity, tone: WorkflowView.Tone, run: WorkflowNodeRun?
    ) -> some View {
        let chip = chipFace(node, tone: tone, run: run).overlay { Self.ring(node) }
        if node.memberIssueIds.isEmpty {
            chip
        } else {
            IssueChipStack { chip }
                .accessibilityIdentifier("workflow-node-chip-stack")
        }
    }

    /// The chip itself. The glyph slot is the node's STATE: its live run's dot
    /// while the run is up, the state's glyph in the state's tone once the
    /// state has one, and the issue's own status glyph before that — a node
    /// that has not started reads exactly like the same issue anywhere else.
    @ViewBuilder
    private func chipFace(
        _ node: WorkflowNodeEntity, tone: WorkflowView.Tone, run: WorkflowNodeRun?
    ) -> some View {
        let issue = issues[node.issueId]
        // A node whose issue has not synced still names itself — by the head
        // of its issue id, over the line that says why there is no title.
        let identifier = Self.nodeIdentifier(node, issue: issue)
        let title = Self.nodeChipTitle(issue)
        // No row, no status: an unsynced node leaves the glyph slot EMPTY
        // rather than inventing a backlog circle (×4).
        let status = issue.map { IssueStatus.from($0.status) }
        // A DRAFT's nodes have not run, so they keep the issue's own status
        // glyph whatever their state says — the same guard ×4.
        let stateIcon = workflowStatus == DomainContract.wfStatusDraft
            ? nil
            : Self.stateIcon(node.state)
        if let run {
            ChipBox(identifier: identifier, title: Self.chipTitle(title)) {
                SessionStateDot(tone: run.tone, pulsing: run.busy, size: 8)
                    .accessibilityIdentifier("workflow-node-run-dot")
            }
        } else if let icon = stateIcon {
            IssueChip(
                identifier: identifier,
                title: title,
                iconName: icon,
                statusColor: Self.color(tone)
            )
        } else {
            IssueChip(
                identifier: identifier,
                title: title,
                iconName: status?.iconName,
                statusColor: status?.color
            )
        }
    }

    /// The same 60-character cut `IssueChip` takes, for the one face that
    /// builds its box by hand (the live dot is not a glyph name).
    private static func chipTitle(_ title: String?) -> String? {
        guard let title else { return nil }
        let cut = IssueRefs.chipTitle(title)
        return cut.isEmpty ? nil : cut
    }

    /// What a node's chip is CALLED: the issue's own identifier once its row
    /// has synced, else the first 8 characters of the issue id — never the
    /// bare 36-character uuid (EXP-1014, ×4). The compound suffix rides
    /// either, so an unsynced batch reads `abcd1234 +2`.
    static func nodeIdentifier(_ node: WorkflowNodeEntity, issue: IssueEntity?) -> String {
        WorkflowView.nodeTitle(
            identifier: issue?.identifier ?? String(node.issueId.prefix(8)),
            memberCount: node.memberIssueIds.count
        )
    }

    /// The chip's title: the issue's own, or the ONE line that says why there
    /// is none yet (`WorkflowView.nodeUnsyncedTitle`, byte-identical ×4).
    static func nodeChipTitle(_ issue: IssueEntity?) -> String {
        issue?.title ?? WorkflowView.nodeUnsyncedTitle
    }

    /// The ring a node's chip wears, or nothing at all: a `proposed` node is
    /// drawn but is not part of the run (EXP-984), and a node inside a blocking
    /// cycle cannot start until a relation goes.
    @ViewBuilder
    static func ring(_ node: WorkflowNodeEntity) -> some View {
        if node.isProposed {
            WorkflowChipRing(color: .white.opacity(TextOpacity.tertiary), dashed: true)
        } else if node.onCycle {
            WorkflowChipRing(color: DesignTokens.Palette.destructive)
        }
    }

    /// The glyph a state wears in the chip's glyph slot — existing icon
    /// CONCEPTS only. The states that have not started (blocked / ready /
    /// proposed / skipped) get none: the chip then keeps the issue's own status
    /// glyph, which is what a node that has not run yet actually is.
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

/// EXP-1014 — the ring a workflow node's chip wears: the chip's own border,
/// redrawn in a colour that says something. Dashed = `proposed` (drawn, but not
/// part of the run until a member admits it), solid destructive = inside a
/// blocking cycle. Web and the IDE ring the node in the grid; a phone rings the
/// chip that IS the node.
struct WorkflowChipRing: View {
    let color: Color
    var dashed = false

    var body: some View {
        RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
            .strokeBorder(
                color,
                style: StrokeStyle(
                    lineWidth: IssueChipTokens.borderWidth, dash: dashed ? [3, 2] : []
                )
            )
    }
}
