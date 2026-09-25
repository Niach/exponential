import ExpCore
import Foundation
import GRDB

/// EXP-982 — one node's coding run as the graph and the sheet read it: where a
/// tap goes, and what the dot says (the ONE session tone table, the same one
/// every session list paints with).
struct WorkflowNodeRun: Equatable {
    let sessionId: String
    /// Still up — what the Running strip lists.
    let live: Bool
    let tone: SessionDotTone
    /// The agent is mid-turn: the dot pulses (EXP-848).
    let busy: Bool
}

/// EXP-981 — one workflow's detail: the synced row, its nodes (the server's
/// `wave`/`lane` layout), the issues they cover and the `blocks` relations that
/// ARE the edges, all LIVE off the two new shapes, plus the member-gated writes
/// (`workflows.update` / `.updateNode` / `.delete`).
///
/// EXP-1014: the screen configures NOTHING about the run any more. The stored
/// launch (EXP-1029: one agent, a cheap model and a strong one) is CARRIED —
/// read to say what a node runs on, never written — and the only settable
/// field left is the runner machine, which a draft needs before it can start.
/// Only the name is typed, so only the name is drafted.
@MainActor @Observable
final class WorkflowDetailModel {
    let workflowId: String
    private let accountId: String
    private let deps: AppDependencies

    private(set) var workflow: WorkflowEntity?
    private(set) var nodes: [WorkflowNodeEntity] = []
    /// The covered issues by id — a node whose row has not synced renders
    /// without one rather than disappearing.
    private(set) var issues: [String: IssueEntity] = [:]
    /// The `blocks` relations between covered issues — the graph's EDGES, and
    /// the only dependency a phone can draw (`Blocked by` chips under a node).
    private(set) var relations: [IssueRelationEntity] = []
    /// The nodes' coding sessions by SESSION id — what the graph's live dots
    /// and the Running strip read. A session that has not synced is absent.
    private(set) var sessions: [String: CodingSessionEntity] = [:]
    /// EVERY machine of the team, offline included: a workflow BINDS its runner
    /// the way an automation does — a sleeping box still owns the binding.
    private(set) var devices: [SteerDevice] = []
    /// The first emission has landed (an absent row then means deleted).
    private(set) var loaded = false
    /// A write is in flight; the synced row echoes the result back.
    var busy = false
    /// The server's refusal, verbatim.
    var error: String?

    private var observationTask: Task<Void, Never>?

    init(workflowId: String, accountId: String, deps: AppDependencies) {
        self.workflowId = workflowId
        self.accountId = accountId
        self.deps = deps
    }

    // MARK: - Derived

    var metrics: WorkflowMetrics { workflow?.parsedMetrics ?? WorkflowMetrics() }

    /// The stored launch, CARRIED (EXP-1029): the phone reads it to say which
    /// model a node's run takes and never writes a byte of it back.
    var launch: WorkflowLaunch { workflow?.parsedLaunch ?? WorkflowLaunch() }

    /// The runner machine is DRAFT-only server-side; the picker says so by
    /// going inert rather than by bouncing on submit.
    var isDraft: Bool { workflow?.status == DomainContract.wfStatusDraft }

    var status: String { workflow?.status ?? DomainContract.wfStatusDraft }

    /// Why Start is disabled, or nil when the draft can start — the shared
    /// rule, so the caption says exactly what the server would refuse with.
    var startBlocker: String? {
        workflow.flatMap(WorkflowView.startBlocker)
    }

    /// The merge train: the nodes whose PR is up, in landing order. Empty on a
    /// draft — the strip is hidden there anyway.
    var mergeTrain: [WorkflowView.TrainEntry] {
        WorkflowView.mergeTrain(nodes)
    }

    /// EXP-984 — the run's counters as the detail's Metrics rows. Empty-ish on
    /// a draft, where the section is hidden anyway.
    var metricRows: [WorkflowView.MetricRow] {
        WorkflowView.metricRows(metrics)
    }

    /// The final-PR node's caption, or nil while that node is not drawn.
    var finalPrCaption: String? {
        WorkflowView.finalPrCaption(
            states: nodes.map(\.state),
            finalPrState: workflow?.finalPrState,
            finalPrNumber: workflow?.finalPrNumber
        )
    }

    /// The edges between the nodes, from the synced `blocks` relations — the
    /// shared rule, cycle edges included.
    var edges: [WorkflowView.Edge] {
        WorkflowView.edges(
            nodes: nodes, relations: relations, cycleEdges: metrics.cycleEdges
        )
    }

    var boundDevice: SteerDevice? {
        guard let deviceId = workflow?.deviceId else { return nil }
        return devices.first { $0.deviceId == deviceId }
    }

    /// The node covering [issueId] — a member's id resolves to its compound
    /// node, exactly as the router addresses one.
    func node(coveringIssue issueId: String) -> WorkflowNodeEntity? {
        nodes.first { $0.coveredIssueIds.contains(issueId) }
    }

    /// EXP-982 — `node id → its run`, for every node whose session row has
    /// synced. A node's marker and the Running strip both read this, so the
    /// graph never invents a liveness the session row does not claim.
    var runs: [String: WorkflowNodeRun] {
        var runs: [String: WorkflowNodeRun] = [:]
        for node in nodes {
            guard let sessionId = node.sessionId, let session = sessions[sessionId] else {
                continue
            }
            // The row's status alone — a run the engine lost is reported by
            // the NODE's own state, never by a stale dot. The same rule ×4.
            let live = session.status == DomainContract.codingSessionStatusRunning
                || session.status == DomainContract.codingSessionStatusInReview
            let state = CodingSessionDisplayState.of(
                session: session, prState: issues[node.issueId]?.prState
            )
            runs[node.id] = WorkflowNodeRun(
                sessionId: sessionId,
                live: live,
                // An ended run keeps its row (Open run still works) but never a
                // live tone: the strip lists what is up, nothing else.
                tone: live ? SessionStateDot.tone(of: state) : .muted,
                busy: CodingSessionDisplayState.pulses(
                    state: state, agentBusy: session.agentBusy, live: live
                )
            )
        }
        return runs
    }

    // MARK: - Observation

    func observe() {
        guard observationTask == nil, let pool = try? deps.db.pool(forAccountId: accountId)
        else { return }
        let id = workflowId
        let observation = ValueObservation.tracking {
            db -> (
                WorkflowEntity?, [WorkflowNodeEntity], [IssueEntity], [IssueRelationEntity],
                [CodingSessionEntity]
            ) in
            let workflow = try WorkflowEntity.fetchOne(db, key: id)
            let nodes = try WorkflowNodeEntity
                .filter(Column("workflow_id") == id)
                .fetchAll(db)
            // EXP-982: the run each started node is in — the graph marks a node
            // that is up with its session's own dot, and the Running strip
            // opens it.
            let sessionIds = Array(Set(nodes.compactMap(\.sessionId)))
            let sessions = sessionIds.isEmpty
                ? []
                : try CodingSessionEntity.filter(sessionIds.contains(Column("id"))).fetchAll(db)
            let covered = Array(Set(nodes.flatMap(\.coveredIssueIds)))
            guard !covered.isEmpty else { return (workflow, nodes, [], [], sessions) }
            let issues = try IssueEntity.filter(covered.contains(Column("id"))).fetchAll(db)
            // Only the relations between COVERED issues can be edges; the rule
            // drops the rest anyway, so they never need reading.
            let relations = try IssueRelationEntity
                .filter(Column("type") == IssueRelationType.blocks.rawValue)
                .filter(covered.contains(Column("issue_id")))
                .filter(covered.contains(Column("related_issue_id")))
                .fetchAll(db)
            return (workflow, nodes, issues, relations, sessions)
        }
        observationTask = Task { [weak self] in
            do {
                for try await (workflow, nodes, issues, relations, sessions)
                    in observation.values(in: pool)
                {
                    guard let self, !Task.isCancelled else { return }
                    self.workflow = workflow
                    self.nodes = nodes.sorted { ($0.wave, $0.lane) < ($1.wave, $1.lane) }
                    self.issues = Dictionary(
                        issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }
                    )
                    self.relations = relations
                    self.sessions = Dictionary(
                        sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }
                    )
                    self.loaded = true
                    await self.loadDevices()
                }
            } catch {
                self?.loaded = true
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
    }

    /// The machine pool follows the workflow's TEAM (it arrives with the row).
    private func loadDevices() async {
        guard let teamId = workflow?.teamId, devices.isEmpty else { return }
        devices = await DeviceQueries.devices(
            db: deps.db, accountId: accountId, teamId: teamId, userId: deps.auth.userId
        )
    }

    // MARK: - Writes

    /// Rename (a label, allowed at any status). Blank or unchanged is a no-op
    /// rather than a server refusal.
    func rename(_ value: String) {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, trimmed != workflow?.name else { return }
        update(WorkflowPatch(name: trimmed))
    }

    /// Bind (or unbind) the runner machine — EXP-1014: the ONE thing this
    /// screen still sets. The launch itself is carried, never written.
    func setDevice(_ deviceId: String?) {
        update(WorkflowPatch(deviceId: .some(deviceId)))
    }

    private func update(_ patch: WorkflowPatch) {
        guard !busy else { return }
        busy = true
        error = nil
        Task {
            do {
                try await deps.workflowsApi.update(
                    accountId: accountId, id: workflowId, patch: patch
                )
            } catch {
                self.error = error.userFacingMessage
            }
            busy = false
        }
    }

    /// What the plan declares per node — addressed by ISSUE, like the router.
    func updateNode(issueId: String, patch: WorkflowNodePatch) {
        guard !busy else { return }
        busy = true
        error = nil
        Task {
            do {
                try await deps.workflowsApi.updateNode(
                    accountId: accountId,
                    workflowId: workflowId,
                    issueId: issueId,
                    patch: patch
                )
            } catch {
                self.error = error.userFacingMessage
            }
            busy = false
        }
    }

    // MARK: - Running it (EXP-982)

    /// Start the run. The server only flips intent: the deterministic engine on
    /// the bound machine picks the row up off Electric and runs the nodes.
    func start() {
        run { accountId, id in
            _ = try await self.deps.workflowsApi.start(accountId: accountId, id: id)
        }
    }

    func pause() {
        run { try await self.deps.workflowsApi.pause(accountId: $0, id: $1) }
    }

    func resume() {
        run { try await self.deps.workflowsApi.resume(accountId: $0, id: $1) }
    }

    func cancel() {
        run { try await self.deps.workflowsApi.cancel(accountId: $0, id: $1) }
    }

    /// EXP-1033 — squash-merge the workflow's ONE final pull request, the one
    /// human review of the whole run: the server completes the workflow in the
    /// same call and the synced row carries that back, so a phone finishes a
    /// workflow without leaving for GitHub. A refusal lands in `error`.
    func mergeFinalPr() {
        run { accountId, id in
            _ = try await self.deps.workflowsApi.mergeFinalPr(accountId: accountId, id: id)
        }
    }

    /// The human gate: clear a node's open PR for the merge train, or take the
    /// approval back while the node has not landed.
    func approveNode(_ nodeId: String, approved: Bool) {
        run { accountId, _ in
            try await self.deps.workflowsApi.approveNode(
                accountId: accountId, nodeId: nodeId, approved: approved
            )
        }
    }

    /// Unstick a failed node: a fresh attempt, or out of the run entirely.
    func resolveNode(_ nodeId: String, action: WorkflowNodeResolution) {
        run { accountId, _ in
            try await self.deps.workflowsApi.resolveNode(
                accountId: accountId, nodeId: nodeId, action: action
            )
        }
    }

    /// EXP-984 — decide a `proposed` node: admit it into the run (the server
    /// re-plans) or dismiss it, which deletes the row.
    func admitNode(_ nodeId: String, admit: Bool) {
        run { accountId, _ in
            try await self.deps.workflowsApi.admitNode(
                accountId: accountId, nodeId: nodeId, admit: admit
            )
        }
    }

    /// One member-gated write: the synced row echoes the result back, so
    /// success needs no local write and a refusal is shown verbatim.
    private func run(_ body: @escaping (String, String) async throws -> Void) {
        guard !busy else { return }
        busy = true
        error = nil
        Task {
            do {
                try await body(accountId, workflowId)
            } catch {
                self.error = error.userFacingMessage
            }
            busy = false
        }
    }

    /// Delete the whole workflow; `onDeleted` pops back to the list. A live
    /// workflow has to be cancelled first and the server says so.
    func delete(onDeleted: @escaping () -> Void) {
        guard !busy else { return }
        busy = true
        error = nil
        Task {
            do {
                try await deps.workflowsApi.delete(accountId: accountId, id: workflowId)
                onDeleted()
            } catch {
                self.error = error.userFacingMessage
            }
            busy = false
        }
    }
}
