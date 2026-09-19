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

        public init(id: String, issueId: String, memberIssueIds: [String]) {
            self.id = id
            self.issueId = issueId
            self.memberIssueIds = memberIssueIds
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

        public init(from: String, to: String, cycle: Bool) {
            self.from = from
            self.to = to
            self.cycle = cycle
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
                EdgeNode(id: $0.id, issueId: $0.issueId, memberIssueIds: $0.memberIssueIds)
            },
            relations: relations.map {
                EdgeRelation(
                    type: $0.type, issueId: $0.issueId, relatedIssueId: $0.relatedIssueId
                )
            },
            cycleEdges: cycleEdges
        )
    }
}
