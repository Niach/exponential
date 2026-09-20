import ExpCore
import Foundation
import GRDB

/// EXP-981 — one workflow's detail: the synced row, its nodes (the server's
/// `wave`/`lane` layout), the issues they cover and the `blocks` relations that
/// ARE the edges, all LIVE off the two new shapes, plus the member-gated writes
/// (`workflows.update` / `.updateNode` / `.delete`).
///
/// There are no local drafts for the configuration: every picker reads the
/// synced row and writes through the router, which echoes the change back — the
/// same "other fields mutate immediately" rule the issue detail follows. Only
/// the name is typed, so only the name is drafted.
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
    private(set) var relations: [IssueRelationEntity] = []
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
    var launch: WorkflowLaunch { workflow?.parsedLaunch ?? WorkflowLaunch() }

    /// Configuration is DRAFT-only server-side; the pickers say so by going
    /// inert rather than by bouncing on submit.
    var isDraft: Bool { workflow?.status == DomainContract.wfStatusDraft }

    var status: String { workflow?.status ?? DomainContract.wfStatusDraft }
    var gate: String { workflow?.gate ?? DomainContract.wfGateHuman }

    /// Why Start is disabled, or nil when the draft can start — the shared
    /// rule, so the caption says exactly what the server would refuse with.
    var startBlocker: String? {
        workflow.flatMap(WorkflowView.startBlocker)
    }

    /// The merge train: the nodes whose PR is up, in landing order. Empty on a
    /// draft — the strip is hidden there anyway.
    var mergeTrain: [WorkflowView.TrainEntry] {
        WorkflowView.mergeTrain(nodes, gate: gate)
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

    /// The bound machine's runnable agents; with no machine bound the contract
    /// list, so the pick can be made before the runner is.
    var availableAgents: [String] {
        let agents = LaunchVocabulary.agents(of: boundDevice)
        return agents.isEmpty ? DomainContract.codingAgentValues : agents
    }

    var agent: String {
        launch.agent ?? availableAgents.first ?? "claude"
    }

    /// EXP-872: the ONE account list — every signed-in login the RUNNER
    /// machine reports, across agents, its default first
    /// (`AccountOptions.flatten`). Picking one picks the agent too: the
    /// separate Agent row is gone.
    ///
    /// A machine that reports no login (and a draft with no runner bound yet)
    /// still offers a row per available agent, named by the agent — the pick
    /// has to be possible before the machine is.
    var accountOptions: [AccountOption] {
        let options = AccountOptions.flatten(
            accounts: boundDevice?.agentAccounts,
            usage: boundDevice?.agentUsage,
            launchDefaults: boundDevice?.launchDefaults
        )
        if !options.isEmpty { return options }
        let agents = availableAgents
        return agents.map { value in
            AccountOption(
                id: AgentAccountsRows.systemProfileId,
                agent: value,
                email: LaunchVocabulary.agentLabel(value),
                isDeviceDefault: value == agents.first
            )
        }
    }

    /// Which option the row reads back as: the stored (agent, account) pair,
    /// else that agent's first login (a stored profile the machine no longer
    /// reports), else the first row.
    var selectedAccount: AccountOption? {
        let options = accountOptions
        let id = launch.account ?? AgentAccountsRows.systemProfileId
        return options.first { $0.agent == agent && $0.id == id }
            ?? options.first { $0.agent == agent }
            ?? options.first
    }

    var maxParallel: Int {
        launch.maxParallel ?? DomainContract.workflowMaxParallelDefault
    }

    /// The nodes by id — the serialization edges (`after_node_ids`, EXP-983)
    /// name NODES, so the panel resolves them through this.
    var nodesById: [String: WorkflowNodeEntity] {
        Dictionary(nodes.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// The node covering [issueId] — a member's id resolves to its compound
    /// node, exactly as the router addresses one.
    func node(coveringIssue issueId: String) -> WorkflowNodeEntity? {
        nodes.first { $0.coveredIssueIds.contains(issueId) }
    }

    // MARK: - Observation

    func observe() {
        guard observationTask == nil, let pool = try? deps.db.pool(forAccountId: accountId)
        else { return }
        let id = workflowId
        let observation = ValueObservation.tracking {
            db -> (WorkflowEntity?, [WorkflowNodeEntity], [IssueEntity], [IssueRelationEntity]) in
            let workflow = try WorkflowEntity.fetchOne(db, key: id)
            let nodes = try WorkflowNodeEntity
                .filter(Column("workflow_id") == id)
                .fetchAll(db)
            let covered = Array(Set(nodes.flatMap(\.coveredIssueIds)))
            guard !covered.isEmpty else { return (workflow, nodes, [], []) }
            let issues = try IssueEntity.filter(covered.contains(Column("id"))).fetchAll(db)
            // Only the relations between COVERED issues can be edges; the rule
            // drops the rest anyway, so they never need reading.
            let relations = try IssueRelationEntity
                .filter(Column("type") == IssueRelationType.blocks.rawValue)
                .filter(covered.contains(Column("issue_id")))
                .filter(covered.contains(Column("related_issue_id")))
                .fetchAll(db)
            return (workflow, nodes, issues, relations)
        }
        observationTask = Task { [weak self] in
            do {
                for try await (workflow, nodes, issues, relations) in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    self.workflow = workflow
                    self.nodes = nodes.sorted { ($0.wave, $0.lane) < ($1.wave, $1.lane) }
                    self.issues = Dictionary(
                        issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }
                    )
                    self.relations = relations
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

    /// Bind (or unbind) the runner machine.
    func setDevice(_ deviceId: String?) {
        update(WorkflowPatch(deviceId: .some(deviceId)))
    }

    func setGate(_ value: String) {
        update(WorkflowPatch(gate: value))
    }

    func setStartOn(_ value: String) {
        update(WorkflowPatch(startOn: value))
    }

    /// Switching agent RESETS the per-agent vocabulary (model, subagent model,
    /// effort) and the login profile: they are per agent and per machine, and a
    /// stale value would hit a server refusal.
    func setModel(_ value: String?) {
        var next = launch
        next.agent = agent
        next.model = value
        update(WorkflowPatch(launch: next))
    }

    func setSubagentModel(_ value: String?) {
        var next = launch
        next.agent = agent
        next.subagentModel = value
        update(WorkflowPatch(launch: next))
    }

    func setEffort(_ value: String?) {
        var next = launch
        next.agent = agent
        next.effort = value
        update(WorkflowPatch(launch: next))
    }

    /// EXP-872: ONE pick for the agent AND its login (`system` = the machine's
    /// ambient login, which a launch never names). A DIFFERENT agent has a
    /// different model/effort vocabulary, so those clear — a stale value would
    /// only be refused by the router. Web `workflow-detail.tsx`'s account row,
    /// rule for rule.
    func selectAccount(_ option: AccountOption) {
        var next = launch
        let switched = option.agent != agent
        next.agent = option.agent
        next.account = option.id == AgentAccountsRows.systemProfileId ? nil : option.id
        if switched {
            next.model = nil
            next.effort = nil
            next.subagentModel = nil
        }
        update(WorkflowPatch(launch: next))
    }

    func setMaxParallel(_ value: Int) {
        var next = launch
        next.agent = agent
        next.maxParallel = value
        update(WorkflowPatch(launch: next))
    }

    /// EXP-984 — the model the gate's agent reviews run on; nil lets the engine
    /// pick one.
    func setReviewModel(_ value: String?) {
        var next = launch
        next.agent = agent
        next.reviewModel = value
        update(WorkflowPatch(launch: next))
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
