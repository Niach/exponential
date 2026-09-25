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

    /// EXP-1029 — the model ONE node's run spawns on: the workflow's STRONG
    /// model for a `contract` or `integration` node and for any `risk: high`
    /// node, else its cheap one. Mirrors web `modelForNode`
    /// (`lib/workflow-launch.ts`) and Rust `coding::workflows::launch`.
    public static func modelForNode(
        _ launch: WorkflowLaunch, kind: String, risk: String
    ) -> String {
        let strict = launch.normalized
        let strong = kind == DomainContract.wfNodeKindContract
            || kind == DomainContract.wfNodeKindIntegration
            || risk == DomainContract.wfRiskHigh
        return strong ? strict.strongModel : strict.model
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
    public static let cancelConfirm =
        "Its live runs end and its branch is deleted. Nothing reached the default branch."
    public static let finalPrTitle = "Final pull request"
    /// EXP-1033: the ONE human review of the whole run — squash-merging the
    /// workflow's final pull request from the workflow screen.
    public static let mergeFinalPrLabel = "Merge"
    public static let mergeFinalPrConfirm =
        "The workflow's branch is squash-merged into the default branch and the run is done."
    /// What Start reads off the workflow row.
    public struct StartableWorkflow: Sendable, Equatable {
        public let status: String
        public let deviceId: String?
        public let repositoryId: String?

        public init(status: String, deviceId: String?, repositoryId: String?) {
            self.status = status
            self.deviceId = deviceId
            self.repositoryId = repositoryId
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
                repositoryId: workflow.repositoryId
            ),
            metrics: workflow.parsedMetrics
        )
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

    // MARK: - Review gate, dynamic graphs (EXP-984)

    public static let admitNodeLabel = "Admit"
    public static let dismissNodeLabel = "Dismiss"
    public static let proposedNodeNote =
        "Filed during the run. Admit it into the workflow or dismiss it."
    /// EXP-1014: the chip of a node whose issue row has not synced yet — the
    /// identifier slot shows the first 8 characters of the issue id, the title
    /// this line. Byte-identical ×4.
    public static let nodeUnsyncedTitle = "Not synced yet"
}

// MARK: - EXP-1082 workflow contract: display states, strip, header, actions

/// EXP-1082: the FIVE states a node chip shows — the engine's ten internal
/// `wfNodeState`s fold into these (contract `wfNodeDisplayState`), locked by
/// the fixture's `displayStates` ×4 (web `lib/workflow-view.ts`).
public enum WorkflowNodeDisplayState: String, Sendable, CaseIterable {
    case queued
    case running
    case done
    case failed
    case skipped

    /// The chip's caption, byte-identical ×4.
    public var label: String {
        switch self {
        case .queued: "Queued"
        case .running: "Running"
        case .done: "Done"
        case .failed: "Failed"
        case .skipped: "Skipped"
        }
    }
}

/// EXP-1082: a node as the strip reads it (EXP-1066 implements the strip).
public struct StripNodeInput: Sendable, Equatable {
    public let id: String
    public let identifier: String
    public let state: String
    public let wave: Int
    public let lane: Int
    public let members: Int
    public let live: Bool
    public let needsYou: Bool
    public let note: String?

    public init(
        id: String, identifier: String, state: String, wave: Int, lane: Int,
        members: Int = 0, live: Bool = false, needsYou: Bool = false, note: String? = nil
    ) {
        self.id = id
        self.identifier = identifier
        self.state = state
        self.wave = wave
        self.lane = lane
        self.members = members
        self.live = live
        self.needsYou = needsYou
        self.note = note
    }
}

/// One chip of the strip.
public struct NodeChip: Sendable, Equatable {
    public let id: String
    public let title: String
    public let display: WorkflowNodeDisplayState
    public let caption: String
    /// A compound node (members > 0) draws as a stacked chip.
    public let stacked: Bool
    public let members: Int
    public let live: Bool
    public let needsYou: Bool

    public init(
        id: String, title: String, display: WorkflowNodeDisplayState, caption: String,
        stacked: Bool, members: Int, live: Bool, needsYou: Bool
    ) {
        self.id = id
        self.title = title
        self.display = display
        self.caption = caption
        self.stacked = stacked
        self.members = members
        self.live = live
        self.needsYou = needsYou
    }
}

/// One column of the strip: the chips of one wave, lane order.
public struct StripWave: Sendable, Equatable {
    public let wave: Int
    public let nodes: [NodeChip]

    public init(wave: Int, nodes: [NodeChip]) {
        self.wave = wave
        self.nodes = nodes
    }
}

/// A node as the header caption counts it.
public struct HeaderNode: Sendable, Equatable {
    public let state: String
    public let members: Int

    public init(state: String, members: Int = 0) {
        self.state = state
        self.members = members
    }
}

/// The workflow page's ONE primary button (fixture `primaryActions`).
public enum WorkflowPrimaryAction: String, Sendable {
    case pickDevice = "pick_device"
    case start
    case pause
    case resume
    case reviewFinalPr = "review_final_pr"
}

/// What the page's overflow offers (fixture `overflowMenus`).
public enum WorkflowOverflowItem: String, Sendable {
    case plan
    case runsOn = "runs_on"
    case stop
    case delete
}

/// What a node chip's menu offers (fixture `chipMenus`).
public enum NodeChipAction: String, Sendable {
    case retry
    case skip
    case admit
    case dismiss
}

extension WorkflowView {
    /// The badge a node that waits on a person wears, byte-identical ×4.
    public static let needsYouLabel = "needs you"

    /// Fold an internal `wfNodeState` into what the chip shows; an unknown
    /// state (a newer server) reads queued rather than vanishing.
    public static func nodeDisplayState(_ state: String) -> WorkflowNodeDisplayState {
        switch state {
        case DomainContract.wfNodeStateProposed,
             DomainContract.wfNodeStateBlocked,
             DomainContract.wfNodeStateReady:
            return .queued
        case DomainContract.wfNodeStateRunning,
             DomainContract.wfNodeStateWaiting,
             DomainContract.wfNodeStateInReview,
             DomainContract.wfNodeStateUpdating:
            return .running
        case DomainContract.wfNodeStateLanded: return .done
        case DomainContract.wfNodeStateFailed: return .failed
        case DomainContract.wfNodeStateSkipped: return .skipped
        default: return .queued
        }
    }

    /// The strip IS the graph: waves left to right (only the waves that hold
    /// a node), lanes top to bottom within a wave, ties by id. The edges are
    /// the mini-graph popover's business; the strip only orders. A compound
    /// node is `stacked` (the `IssueChipStack`); the caption is the node's
    /// note while it has one, a `proposed` node's `proposedNodeNote`, else its
    /// display label (fixture `nodeStrips`; nodes only).
    public static func nodeStrip(nodes: [StripNodeInput]) -> [StripWave] {
        let byWave = Dictionary(grouping: nodes, by: \.wave)
        return byWave.keys.sorted().map { wave in
            let chips = (byWave[wave] ?? [])
                .sorted { ($0.lane, $0.id) < ($1.lane, $1.id) }
                .map { node in
                    let display = nodeDisplayState(node.state)
                    let note = node.note?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                    return NodeChip(
                        id: node.id,
                        title: nodeTitle(identifier: node.identifier, memberCount: node.members),
                        display: display,
                        caption: !note.isEmpty
                            ? note
                            : node.state == DomainContract.wfNodeStateProposed
                            ? proposedNodeNote : display.label,
                        stacked: node.members > 0,
                        members: node.members,
                        live: node.live,
                        needsYou: node.needsYou
                    )
                }
            return StripWave(wave: wave, nodes: chips)
        }
    }

    private static let statusWords: [String: String] = [
        "draft": "Draft",
        "done": "Done",
        "failed": "Failed",
        "cancelled": "Cancelled",
    ]

    /// The one line under the workflow's name. A draft counts its ISSUES
    /// (members included): `Draft · 8 issues`. A started workflow names its
    /// runner and counts NODES: `on MacBook · 5 of 8 done · 2 running` (the
    /// running tail only while something runs). Over, it counts what happened:
    /// `Done · 2 done · 1 skipped`. A `proposed` node was never admitted and is
    /// not counted (fixture `headerCaptions`).
    public static func headerCaption(status: String, nodes: [HeaderNode], deviceLabel: String?) -> String {
        let admitted = nodes.filter { $0.state != DomainContract.wfNodeStateProposed }
        if status == DomainContract.wfStatusDraft {
            let issues = admitted.reduce(0) { $0 + 1 + $1.members }
            return "Draft · \(issues == 1 ? "1 issue" : "\(issues) issues")"
        }
        func tally(_ display: WorkflowNodeDisplayState) -> Int {
            admitted.filter { nodeDisplayState($0.state) == display }.count
        }
        let done = tally(.done)
        if status == DomainContract.wfStatusRunning || status == DomainContract.wfStatusPaused {
            var parts = ["\(done) of \(admitted.count) done"]
            let running = tally(.running)
            if running > 0 { parts.append("\(running) running") }
            if let deviceLabel { parts.insert("on \(deviceLabel)", at: 0) }
            return parts.joined(separator: " · ")
        }
        let word = statusWords[status] ?? (status.prefix(1).uppercased() + status.dropFirst())
        var parts = [word, "\(done) done"]
        let failed = tally(.failed)
        if failed > 0 { parts.append("\(failed) failed") }
        let skipped = tally(.skipped)
        if skipped > 0 { parts.append("\(skipped) skipped") }
        return parts.joined(separator: " · ")
    }

    /// The header's ONE primary button: a draft without a runner picks one, a
    /// draft starts; a started workflow whose final PR is OPEN reviews it,
    /// else running pauses and paused resumes; done reviews the final PR.
    /// Failed and cancelled offer nothing (fixture `primaryActions`).
    public static func primaryAction(
        status: String, deviceLabel: String?, finalPrState: String? = nil
    ) -> WorkflowPrimaryAction? {
        switch status {
        case DomainContract.wfStatusDraft: return deviceLabel == nil ? .pickDevice : .start
        case DomainContract.wfStatusRunning, DomainContract.wfStatusPaused:
            if finalPrState == "open" { return .reviewFinalPr }
            return status == DomainContract.wfStatusRunning ? .pause : .resume
        case DomainContract.wfStatusDone: return .reviewFinalPr
        default: return nil
        }
    }

    /// The header's status glyph, as the node display state it reads like:
    /// draft → queued, running/paused → running, done → done, failed →
    /// failed, cancelled → skipped, anything newer → queued (fixture
    /// `statusGlyphs`).
    public static func statusGlyph(status: String) -> WorkflowNodeDisplayState {
        switch status {
        case DomainContract.wfStatusRunning, DomainContract.wfStatusPaused: .running
        case DomainContract.wfStatusDone: .done
        case "failed": .failed
        case DomainContract.wfStatusCancelled: .skipped
        default: .queued
        }
    }

    /// What a node chip's menu offers: Retry / Skip on a `failed` node, Admit /
    /// Dismiss on a `proposed` one, nothing else anywhere (fixture
    /// `chipMenus`).
    public static func nodeChipMenu(state: String) -> [NodeChipAction] {
        if state == DomainContract.wfNodeStateFailed { return [.retry, .skip] }
        if state == DomainContract.wfNodeStateProposed { return [.admit, .dismiss] }
        return []
    }

    // MARK: - Page labels + overflow (fixture `pageLabels` / `overflowMenus`)

    public static let allNodesLabel = "All"
    public static let decisionsLabel = "Decisions"
    public static let stopWorkflowLabel = "Stop"
    public static let pickDeviceLabel = "Pick device"
    public static let runsOnLabel = "Runs on"
    public static let reviewFinalPrLabel = "Review final PR"
    /// All × Changes: a node with nothing pushed yet.
    public static let noChangesLabel = "No changes yet"
    /// The Run(s) face when no run is in scope yet.
    public static let noRunsLabel = "No runs yet"
    /// The Results face when no run in scope published a screenshot.
    public static let noResultsLabel = "No results yet"
    /// The Dismiss confirm's one sentence (a `proposed` node's chip menu).
    public static let dismissNodeConfirm = "The node is removed from the workflow."

    /// The header's overflow: a draft plans, re-binds its runner or is
    /// deleted; a live workflow stops; anything else is deleted.
    public static func overflowMenu(status: String) -> [WorkflowOverflowItem] {
        switch status {
        case DomainContract.wfStatusDraft: [.plan, .runsOn, .delete]
        case DomainContract.wfStatusRunning, DomainContract.wfStatusPaused: [.stop]
        default: [.delete]
        }
    }
}

// MARK: - A picked node's embedded Work screen (iOS)

/// What a picked node's embedded Work screen is ABOUT: its issue, or — for a
/// compound (batch) node whose run is mine and issue-less — that run itself
/// (an issue subject would look the run up by `issue_id`, which a batch row
/// does not carry, and show a spinner forever).
public enum WorkflowNodeSubject: Hashable, Sendable {
    case issue(id: String)
    case session(id: String)
}

public struct WorkflowNodeWork: Equatable, Sendable {
    public let subject: WorkflowNodeSubject
    /// The faces that screen can SHOW, by the screen's own run lookup — the
    /// page offers a face only when this list has it.
    public let faces: [WorkFaceKind]
}

public extension WorkflowView {
    /// The embedded Work screen for a node, and the faces it will offer.
    /// Mirrors the screen: an issue subject shows `WorkFaces.codingTarget`
    /// (runs of MINE on the issue), a session subject its own row (no issue
    /// face for an issue-less run, Changes only once its live diff lands — so
    /// never promised here). A teammate's run shows on neither: no Run face.
    static func nodeWork(
        issueId: String,
        sessionId: String?,
        issuePushed: Bool,
        sessions: [CodingSessionEntity],
        me: String?,
        now: Date
    ) -> WorkflowNodeWork {
        if let sessionId,
           let row = sessions.first(where: { $0.id == sessionId }),
           row.issueId == nil,
           CodingSessionOwnership.isOwn(row, userId: me)
        {
            return WorkflowNodeWork(
                subject: .session(id: sessionId),
                faces: WorkFaces.availableFaces(
                    hasIssue: false,
                    hasRun: true,
                    hasChanges: false,
                    hasResults: !parseSessionResults(row.results).isEmpty
                )
            )
        }
        let target = WorkFaces.codingTarget(
            sessions, issueId: issueId, boundId: nil, me: me, now: now
        )
        return WorkflowNodeWork(
            subject: .issue(id: issueId),
            faces: WorkFaces.availableFaces(
                hasIssue: true,
                hasRun: target != nil,
                hasChanges: issuePushed,
                hasResults: !parseSessionResults(target?.results).isEmpty
            )
        )
    }
}
