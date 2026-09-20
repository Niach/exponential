import Foundation

/// EXP-981 — what every client SAYS about a workflow. The graph's geometry is
/// the server's (`wave`/`lane` on the synced nodes); this is the rest — bands,
/// captions, the edges between nodes — mirrored ×4 (web `lib/workflow-view.ts`,
/// Android `domain/WorkflowView.kt`, desktop `domain::workflow_view`) and
/// locked by the contract fixture `domain-contract/fixtures/workflow-view.json`.
/// All strings byte-identical.
public enum WorkflowView {

    public enum Band: String, Sendable {
        case running
        case draft
        case done
    }

    /// The list's three bands, in order. Flat rows under each, no row buttons.
    public static let bands: [(key: Band, title: String)] = [
        (key: .running, title: "Running"),
        (key: .draft, title: "Draft"),
        (key: .done, title: "Done"),
    ]

    public static let title = "Workflows"
    public static let emptyTitle = "No workflows yet"
    public static let emptyBody =
        "Select backlog issues on a board and choose Create workflow to plan them as one parallel run."
    public static let planLabel = "Plan"
    public static let deleteLabel = "Delete workflow"
    /// The bulk bar's play menu (EXP-981), in order.
    public static let startAsBatchLabel = "Start as batch"
    public static let startAsStackLabel = "Start as stack"
    public static let createWorkflowLabel = "Create workflow…"

    /// `paused` is a running workflow someone held; `cancelled` is over. An
    /// unknown status (a newer server) lands in Done rather than vanishing.
    public static func band(_ status: String) -> Band {
        if status == DomainContract.wfStatusRunning || status == DomainContract.wfStatusPaused {
            return .running
        }
        if status == DomainContract.wfStatusDraft { return .draft }
        return .done
    }

    /// `12 nodes · depth 3 · width 8`; `1 node · depth 1 · width 1`.
    public static func shapeLine(_ metrics: WorkflowMetrics) -> String {
        let nodes = metrics.nodes == 1 ? "1 node" : "\(metrics.nodes) nodes"
        return "\(nodes) · depth \(metrics.depth) · width \(metrics.width)"
    }

    /// Nil while the workflow could start; else the cycles spelled out.
    public static func cycleNote(_ metrics: WorkflowMetrics) -> String? {
        if metrics.cycles.isEmpty { return nil }
        let spelled = metrics.cycles.map { $0.joined(separator: ", ") }.joined(separator: "; ")
        return "These issues block each other in a cycle: \(spelled). Remove one relation to start."
    }

    private static let stateLabels: [String: String] = [
        "proposed": "Proposed",
        "blocked": "Blocked",
        "ready": "Ready",
        "running": "Running",
        "waiting": "Waiting",
        "in_review": "In review",
        "updating": "Updating",
        "landed": "Landed",
        "failed": "Failed",
        "skipped": "Skipped",
        "paused": "Paused",
    ]

    private static let kindLabels: [String: String] = [
        "contract": "Contract",
        "leaf": "Leaf",
        "integration": "Integration",
    ]

    public static func nodeStateLabel(_ state: String) -> String {
        stateLabels[state] ?? state
    }

    public static func nodeKindLabel(_ kind: String) -> String {
        kindLabels[kind] ?? kind
    }

    /// The tone a node's state paints in. `waiting` is the ONLY amber one (and
    /// the only one that pushes): amber means "a person is needed".
    public enum Tone: String, Sendable {
        case muted
        case active
        case amber
        case success
        case danger
    }

    public static func nodeTone(_ state: String) -> Tone {
        if state == "waiting" { return .amber }
        if state == "failed" { return .danger }
        if state == "landed" { return .success }
        if state == "running" || state == "updating" || state == "in_review" { return .active }
        return .muted
    }

    /// What a caption is derived from — the node's plan and its state.
    public struct CaptionNode: Sendable, Equatable {
        public let kind: String
        public let state: String
        public let risk: String

        public init(kind: String, state: String, risk: String) {
            self.kind = kind
            self.state = state
            self.risk = risk
        }
    }

    /// The ONE caption under a node. A draft has no states worth reading yet,
    /// so it names the plan (`Contract`, `Leaf · high risk`); a started
    /// workflow names the state, prefixed by the kind only for the two special
    /// nodes (`Contract · Running`, `In review`).
    public static func nodeCaption(_ node: CaptionNode, workflowStatus: String) -> String {
        if workflowStatus == DomainContract.wfStatusDraft {
            let kind = nodeKindLabel(node.kind)
            return node.risk == DomainContract.wfRiskHigh ? "\(kind) · high risk" : kind
        }
        let state = nodeStateLabel(node.state)
        return node.kind == DomainContract.wfNodeKindLeaf
            ? state
            : "\(nodeKindLabel(node.kind)) · \(state)"
    }

    /// `EXP-14 +3` for a compound node (a parent run as one batch with its
    /// sub-issues), the bare identifier otherwise.
    public static func nodeTitle(identifier: String, memberCount: Int) -> String {
        memberCount > 0 ? "\(identifier) +\(memberCount)" : identifier
    }

    // MARK: - Edges

    /// One node as the edge rule reads it.
    public struct EdgeNode: Sendable, Equatable {
        public let id: String
        public let issueId: String
        public let memberIssueIds: [String]
        /// EXP-983: engine-written serialization edges (`after_node_ids`).
        public let afterNodeIds: [String]

        public init(
            id: String,
            issueId: String,
            memberIssueIds: [String],
            afterNodeIds: [String] = []
        ) {
            self.id = id
            self.issueId = issueId
            self.memberIssueIds = memberIssueIds
            self.afterNodeIds = afterNodeIds
        }
    }

    /// One `issue_relations` row as the edge rule reads it.
    public struct EdgeRelation: Sendable, Equatable {
        public let type: String
        public let issueId: String
        public let relatedIssueId: String

        public init(type: String, issueId: String, relatedIssueId: String) {
            self.type = type
            self.issueId = issueId
            self.relatedIssueId = relatedIssueId
        }
    }

    public struct Edge: Sendable, Equatable {
        public let from: String
        public let to: String
        /// Inside a blocking cycle (`metrics.cycleEdges`): drawn red.
        public let cycle: Bool
        /// EXP-983: not a `blocks` relation but a SERIALIZATION edge the engine
        /// added after two siblings' work collided: `to` merges `from` in first.
        public let serial: Bool

        public init(from: String, to: String, cycle: Bool, serial: Bool = false) {
            self.from = from
            self.to = to
            self.cycle = cycle
            self.serial = serial
        }
    }

    /// The edges between a workflow's nodes, from the synced `blocks`
    /// relations: a relation between ANY two covered issues of two different
    /// nodes (the server's `nodeEdges`). One edge per node pair, ordered by
    /// (from, to) node id. `cycleEdges` = the workflow's `metrics.cycleEdges`
    /// (`<from>\n<to>`).
    public static func edges(
        nodes: [EdgeNode],
        relations: [EdgeRelation],
        cycleEdges: [String] = []
    ) -> [Edge] {
        var nodeOf: [String: String] = [:]
        for node in nodes {
            nodeOf[node.issueId] = node.id
            for member in node.memberIssueIds { nodeOf[member] = node.id }
        }
        let onCycle = Set(cycleEdges)
        var seen = Set<String>()
        var edges: [Edge] = []
        for relation in relations {
            guard relation.type == IssueRelationType.blocks.rawValue else { continue }
            guard let from = nodeOf[relation.issueId],
                  let to = nodeOf[relation.relatedIssueId],
                  from != to
            else { continue }
            let key = "\(from)\n\(to)"
            if seen.contains(key) { continue }
            seen.insert(key)
            edges.append(Edge(from: from, to: to, cycle: onCycle.contains(key)))
        }
        // Serialization edges, unless a real edge already joins the pair.
        let known = Set(nodes.map(\.id))
        for node in nodes {
            for from in node.afterNodeIds {
                guard known.contains(from), from != node.id else { continue }
                let key = "\(from)\n\(node.id)"
                if seen.contains(key) { continue }
                seen.insert(key)
                edges.append(Edge(from: from, to: node.id, cycle: false, serial: true))
            }
        }
        return edges.sorted { ($0.from, $0.to) < ($1.from, $1.to) }
    }

    /// The synced rows as the rule reads them — what every workflow surface
    /// actually holds.
    public static func edges(
        nodes: [WorkflowNodeEntity],
        relations: [IssueRelationEntity],
        cycleEdges: [String] = []
    ) -> [Edge] {
        edges(
            nodes: nodes.map {
                EdgeNode(
                    id: $0.id,
                    issueId: $0.issueId,
                    memberIssueIds: $0.memberIssueIds,
                    afterNodeIds: $0.afterNodeIds
                )
            },
            relations: relations.map {
                EdgeRelation(
                    type: $0.type, issueId: $0.issueId, relatedIssueId: $0.relatedIssueId
                )
            },
            cycleEdges: cycleEdges
        )
    }

    // MARK: - Running a workflow (EXP-982)

    public static let startLabel = "Start"
    public static let pauseLabel = "Pause"
    public static let resumeLabel = "Resume"
    public static let cancelLabel = "Cancel workflow"
    public static let cancelConfirm =
        "Its live runs end and its branch is deleted. Nothing reached the default branch."
    public static let approveNodeLabel = "Approve and land"
    public static let withdrawApprovalLabel = "Withdraw approval"
    public static let mergeTrainTitle = "Merge train"
    public static let mergeTrainEmpty = "Nothing is waiting to land."
    public static let finalPrTitle = "Final pull request"
    /// The strip over the graph that lists the runs that are up, one tap away.
    public static let runningNowLabel = "Running now"

    /// What Start reads off the workflow row.
    public struct StartableWorkflow: Sendable, Equatable {
        public let status: String
        public let deviceId: String?
        public let repositoryId: String?
        public let startOn: String

        public init(status: String, deviceId: String?, repositoryId: String?, startOn: String) {
            self.status = status
            self.deviceId = deviceId
            self.repositoryId = repositoryId
            self.startOn = startOn
        }
    }

    /// Why Start is disabled, or nil when the draft can start. One reason, the
    /// most fundamental first; the server refuses with the same sentences.
    public static func startBlocker(
        _ workflow: StartableWorkflow, metrics: WorkflowMetrics
    ) -> String? {
        if workflow.status != DomainContract.wfStatusDraft {
            return "The workflow has already started."
        }
        if metrics.nodes == 0 { return "The workflow has no issues." }
        if let cycle = cycleNote(metrics) { return cycle }
        if workflow.repositoryId == nil { return "The workflow's repository is gone." }
        if workflow.deviceId == nil { return "Pick the device that runs this workflow first." }
        return nil
    }

    /// The synced row as the rule reads it.
    public static func startBlocker(_ workflow: WorkflowEntity) -> String? {
        startBlocker(
            StartableWorkflow(
                status: workflow.status,
                deviceId: workflow.deviceId,
                repositoryId: workflow.repositoryId,
                startOn: workflow.startOn
            ),
            metrics: workflow.parsedMetrics
        )
    }

    /// A node lands without a person only when the workflow has no gate AND it
    /// is not the contract (always human-gated). Mirrors the server.
    public static func nodeNeedsApproval(gate: String, kind: String) -> Bool {
        kind == DomainContract.wfNodeKindContract || gate != DomainContract.wfGateNone
    }

    /// One node as the merge train reads it.
    public struct TrainNode: Sendable, Equatable {
        public let id: String
        public let kind: String
        public let state: String
        public let wave: Int
        public let lane: Int
        /// `approved_at` — nil until a member approved the node's PR.
        public let approvedAt: String?

        public init(
            id: String, kind: String, state: String, wave: Int, lane: Int, approvedAt: String?
        ) {
            self.id = id
            self.kind = kind
            self.state = state
            self.wave = wave
            self.lane = lane
            self.approvedAt = approvedAt
        }
    }

    public enum TrainStep: String, Sendable {
        case next
        case queued
        case needsApproval = "needs-approval"
        case updating
    }

    public struct TrainEntry: Sendable, Equatable {
        public let id: String
        public let step: TrainStep

        public init(id: String, step: TrainStep) {
            self.id = id
            self.step = step
        }
    }

    /// The merge train: every node whose PR is up (`in_review`, or `updating`
    /// while it merges the trunk in), in landing order (wave, then lane). The
    /// FIRST node that is cleared to land is `next`; cleared ones behind it are
    /// `queued`; one still waiting for a person says so.
    public static func mergeTrain(_ nodes: [TrainNode], gate: String) -> [TrainEntry] {
        let waiting = nodes
            .filter {
                $0.state == DomainContract.wfNodeStateInReview
                    || $0.state == DomainContract.wfNodeStateUpdating
            }
            .sorted { ($0.wave, $0.lane, $0.id) < ($1.wave, $1.lane, $1.id) }
        var nextTaken = false
        return waiting.map { node in
            if node.state == DomainContract.wfNodeStateUpdating {
                return TrainEntry(id: node.id, step: .updating)
            }
            if nodeNeedsApproval(gate: gate, kind: node.kind), node.approvedAt == nil {
                return TrainEntry(id: node.id, step: .needsApproval)
            }
            if nextTaken { return TrainEntry(id: node.id, step: .queued) }
            nextTaken = true
            return TrainEntry(id: node.id, step: .next)
        }
    }

    /// The synced rows as the rule reads them.
    public static func mergeTrain(_ nodes: [WorkflowNodeEntity], gate: String) -> [TrainEntry] {
        mergeTrain(
            nodes.map {
                TrainNode(
                    id: $0.id, kind: $0.kind, state: $0.state,
                    wave: $0.wave, lane: $0.lane, approvedAt: $0.approvedAt
                )
            },
            gate: gate
        )
    }

    private static let trainStepLabels: [TrainStep: String] = [
        .next: "Landing next",
        .queued: "Queued",
        .needsApproval: "Needs approval",
        .updating: "Merging the trunk in",
    ]

    public static func trainStepLabel(_ step: TrainStep) -> String {
        trainStepLabels[step] ?? step.rawValue
    }

    /// The final-PR node's caption, or nil while the node is not drawn: it
    /// appears once every node landed (or was skipped), after the last wave.
    public static func finalPrCaption(
        states: [String], finalPrState: String?, finalPrNumber: Int?
    ) -> String? {
        // EXP-984: a `proposed` node was never admitted; it is not part of the
        // run.
        let states = states.filter { $0 != DomainContract.wfNodeStateProposed }
        if states.isEmpty { return nil }
        let allIn = states.allSatisfy {
            $0 == DomainContract.wfNodeStateLanded || $0 == DomainContract.wfNodeStateSkipped
        }
        if !allIn, finalPrNumber == nil { return nil }
        guard let number = finalPrNumber else { return "Opening the pull request" }
        let label =
            finalPrState == DomainContract.prStateMerged
                ? "Merged"
                : finalPrState == DomainContract.prStateClosed ? "Closed" : "Open"
        return "#\(number) · \(label)"
    }

    public static let retryNodeLabel = "Retry"
    public static let skipNodeLabel = "Skip"
    public static let skipNodeConfirm =
        "Its dependents go on without it. The node's work is not part of the final pull request."

    /// A list row's secondary text: the shape line, led by the status word for
    /// the two statuses a band alone does not tell apart.
    public static func rowSubtitle(status: String, metrics: WorkflowMetrics) -> String {
        let shape = shapeLine(metrics)
        if status == DomainContract.wfStatusPaused { return "Paused · \(shape)" }
        if status == DomainContract.wfStatusCancelled { return "Cancelled · \(shape)" }
        return shape
    }

    // MARK: - Speculative starts (EXP-983)

    /// How an edge is drawn. Grey solid is the default; the others say
    /// something.
    public enum EdgeStyle: String, Sendable {
        case plain
        case cycle
        case stale
        case landed
        case speculative
    }

    /// A dependent in one of these started before its blocker landed.
    private static let startedStates: Set<String> = [
        DomainContract.wfNodeStateRunning,
        DomainContract.wfNodeStateWaiting,
        DomainContract.wfNodeStateInReview,
        DomainContract.wfNodeStateUpdating,
    ]

    /// - `cycle` (red): inside a blocking cycle.
    /// - `stale` (red): upstream moved and the dependent is merging it in (`to`
    ///   is `updating`).
    /// - `landed` (green): the blocker landed.
    /// - `speculative` (dashed): the dependent started before its blocker
    ///   landed, or the edge is a serialization edge.
    /// - `plain` (grey): nothing to say yet.
    public static func edgeStyle(
        _ edge: Edge, fromState: String, toState: String
    ) -> EdgeStyle {
        if edge.cycle { return .cycle }
        if fromState == DomainContract.wfNodeStateLanded { return .landed }
        if toState == DomainContract.wfNodeStateUpdating { return .stale }
        if edge.serial || startedStates.contains(toState) { return .speculative }
        return .plain
    }

    /// The node panel's line once a node announced its contract.
    public static let contractPublishedLabel = "Contract published"

    /// The node panel's chip line over `after_node_ids`. Byte-identical ×4.
    public static let mergesInFirstLabel = "Merges in first"

    // MARK: - Review gate, dynamic graphs, budgets, metrics (EXP-984)

    public static let admitNodeLabel = "Admit"
    public static let dismissNodeLabel = "Dismiss"
    public static let proposedNodeNote =
        "Filed during the run. Admit it into the workflow or dismiss it."
    public static let agentReviewTitle = "Agent review"
    public static let reviewModelLabel = "Review model"
    public static let budgetTitle = "Budget"
    public static let budgetMinutesLabel = "Minutes"
    public static let budgetTokensLabel = "Tokens"
    public static let metricsTitle = "Metrics"

    /// The node panel's one line about the latest agent review:
    /// `Approved · round 1 · checks passed`, `Approved · round 1 · advisory`,
    /// `Changes requested · round 2 · checks failed`,
    /// `Changes requested · round 2`.
    public static func reviewLine(_ review: WorkflowNodeReview) -> String {
        let approved = review.verdict == DomainContract.wfReviewVerdictApprove
        var parts = [approved ? "Approved" : "Changes requested", "round \(review.round)"]
        if let oracle = review.oracle {
            parts.append(oracle.passed ? "checks passed" : "checks failed")
        } else if approved {
            parts.append("advisory")
        }
        return parts.joined(separator: " · ")
    }

    public struct MetricRow: Sendable, Equatable {
        public let label: String
        public let value: String

        public init(label: String, value: String) {
            self.label = label
            self.value = value
        }
    }

    /// The detail's Metrics section for a STARTED workflow, in this order. A
    /// row appears only when it has something to say, except the critical path,
    /// which always does.
    public static func metricRows(_ metrics: WorkflowMetrics) -> [MetricRow] {
        func count(_ key: String) -> Int { metrics.counters[key] ?? 0 }
        var rows = [
            // The shape keys read off the typed fields; every other counter
            // comes out of the open set beside them.
            MetricRow(
                label: "Critical path",
                value: "\(metrics.depth) waves for \(metrics.nodes) nodes"
            )
        ]
        let landed = count("landed")
        if landed > 0 { rows.append(MetricRow(label: "Landed", value: "\(landed)")) }
        let mergeIns = count("mergeIns")
        let changes = count("contractChanges")
        if mergeIns > 0 {
            rows.append(
                changes > 0
                    ? MetricRow(
                        label: "Merge-ins per contract change",
                        value: String(format: "%.1f", Double(mergeIns) / Double(changes))
                    )
                    : MetricRow(label: "Merge-ins", value: "\(mergeIns)")
            )
        }
        let escalations = count("escalations")
        if escalations > 0 {
            rows.append(MetricRow(
                label: "Escalations",
                value: "\(escalations) (\(count("duplicateEscalations")) duplicate)"
            ))
        }
        let minutes = count("operatorMinutes")
        if minutes > 0 {
            rows.append(MetricRow(label: "Operator minutes", value: "\(minutes)"))
        }
        let rounds = count("reviewRounds")
        if rounds > 0 { rows.append(MetricRow(label: "Review rounds", value: "\(rounds)")) }
        let byOracle = count("defectsByOracle")
        let byAgent = count("defectsByAgentReview")
        if byOracle + byAgent > 0 {
            rows.append(MetricRow(
                label: "Defects found",
                value: "\(byOracle) by checks · \(byAgent) by agent review"
            ))
        }
        let pauses = count("budgetPauses")
        if pauses > 0 { rows.append(MetricRow(label: "Budget pauses", value: "\(pauses)")) }
        return rows
    }
}
