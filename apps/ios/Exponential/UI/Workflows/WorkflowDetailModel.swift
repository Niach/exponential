import ExpCore
import Foundation
import GRDB

/// EXP-1086 — one workflow's page: the synced row, its nodes (the server's
/// `wave`/`lane` layout), the issues they cover, the relations among them, the
/// workflow's RUNS and its event log, all LIVE off the synced store, plus the
/// member-gated writes. What the page SAYS comes out of the shared view model
/// (`WorkflowView.headerCaption` / `nodeStrip` / `primaryAction` /
/// `nodeChipMenu`, ×4); which chip is picked is `WorkflowSelection`.
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
    /// `blocks` + `parent` relations among the covered issues: the mini-graph
    /// and the nested issue list.
    private(set) var relations: [IssueRelationEntity] = []
    /// The workflow's runs (`coding_sessions.workflow_id`, plus every node's
    /// own run).
    private(set) var sessions: [CodingSessionEntity] = []
    private(set) var events: [WorkflowEventEntity] = []
    /// The synced machine rows — what a run row's device line resolves from.
    private(set) var deviceRows: [DeviceEntity] = []
    /// Own + team-shared machines (offline included: a bound runner keeps its
    /// name while it sleeps).
    private(set) var devices: [SteerDevice] = []
    /// The first emission has landed (an absent row then means deleted).
    private(set) var loaded = false
    /// A write is in flight; the synced row echoes the result back.
    var busy = false
    /// The server's refusal, verbatim (a notice toast).
    var error: String?
    /// The chip strip's pick: All, or the picked nodes (the page shows the
    /// cursor's).
    var selection = WorkflowSelection()

    private var observationTask: Task<Void, Never>?
    /// The asking runs that are MINE, attached while the banner shows so an
    /// answer goes out on the run's own steer socket — how a `needs_input`
    /// run is answered everywhere else. Keyed by session id.
    private var answerModels: [String: AgentSessionModel] = [:]

    init(workflowId: String, accountId: String, deps: AppDependencies) {
        self.workflowId = workflowId
        self.accountId = accountId
        self.deps = deps
    }

    // MARK: - Derived

    var status: String { workflow?.status ?? DomainContract.wfStatusDraft }
    var isDraft: Bool { status == DomainContract.wfStatusDraft }
    var isLive: Bool {
        status == DomainContract.wfStatusRunning || status == DomainContract.wfStatusPaused
    }

    private var boundDevice: SteerDevice? {
        guard let deviceId = workflow?.deviceId else { return nil }
        return devices.first { $0.deviceId == deviceId }
    }

    /// The runner's plain name; its id while its row has not synced.
    var deviceLabel: String? {
        guard let deviceId = workflow?.deviceId, !deviceId.isEmpty else { return nil }
        if let label = boundDevice?.deviceLabel, !label.isEmpty { return label }
        return deviceId
    }

    /// The machines a draft may bind: own + team-shared, online.
    var runnerChoices: [SteerDevice] { devices.filter(\.isOnline) }

    var caption: String {
        WorkflowView.headerCaption(
            status: status,
            nodes: nodes.map { HeaderNode(state: $0.state, members: $0.memberIssueIds.count) },
            deviceLabel: deviceLabel
        )
    }

    var primaryAction: WorkflowPrimaryAction? {
        WorkflowView.primaryAction(
            status: status, deviceLabel: deviceLabel, finalPrState: workflow?.finalPrState
        )
    }

    /// Why Start is disabled — the server's own sentence.
    var startBlocker: String? { workflow.flatMap(WorkflowView.startBlocker) }

    var openQuestions: [WorkflowOpenQuestion] {
        WorkflowQuestions.open(sessions, workflowId: workflowId)
    }

    private var sessionsById: [String: CodingSessionEntity] {
        Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    func session(_ id: String?) -> CodingSessionEntity? {
        guard let id else { return nil }
        return sessions.first { $0.id == id }
    }

    func identifier(of node: WorkflowNodeEntity) -> String {
        issues[node.issueId]?.identifier ?? String(node.issueId.prefix(8))
    }

    var strip: [StripWave] {
        let byId = sessionsById
        let asking = Set(openQuestions.map(\.nodeId))
        return WorkflowView.nodeStrip(
            nodes: nodes.map { node in
                StripNodeInput(
                    id: node.id,
                    identifier: identifier(of: node),
                    state: node.state,
                    wave: node.wave,
                    lane: node.lane,
                    members: node.memberIssueIds.count,
                    live: node.sessionId.flatMap { byId[$0] }?.agentBusy ?? false,
                    needsYou: asking.contains(node.id),
                    note: node.note
                )
            }
        )
    }

    var order: [String] { WorkflowSelection.order(strip) }

    var selectedNode: WorkflowNodeEntity? {
        guard let id = selection.nodeId else { return nil }
        return nodes.first { $0.id == id }
    }

    func node(_ id: String) -> WorkflowNodeEntity? { nodes.first { $0.id == id } }

    /// Every covered issue in strip order, nested under its parent (the ×4
    /// `IssueNesting` rule).
    var nestedIssues: [(issue: IssueEntity, depth: Int)] {
        let byNode = Dictionary(nodes.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var ids: [String] = []
        for id in order {
            for issueId in byNode[id]?.coveredIssueIds ?? [] where !ids.contains(issueId) {
                ids.append(issueId)
            }
        }
        let rows = IssueNesting.nestIssueRows(
            groups: [ids.filter { issues[$0] != nil }],
            relations: relations,
            identifierOf: { self.issues[$0]?.identifier ?? $0 }
        ).first ?? []
        return rows.compactMap { row in issues[row.id].map { ($0, row.depth) } }
    }

    /// The node an issue belongs to (a member resolves to its compound node).
    func node(coveringIssue issueId: String) -> WorkflowNodeEntity? {
        nodes.first { $0.coveredIssueIds.contains(issueId) }
    }

    /// The mini-graph a chip's long-press opens.
    func blockGraph(for node: WorkflowNodeEntity) -> IssueGraph.Graph {
        IssueGraph.blockGraph(
            subjectIds: node.coveredIssueIds, relations: relations, issues: Array(issues.values)
        )
    }

    /// The picked node's embedded Work screen (its subject) and the faces it
    /// will offer, by the SAME run lookup the screen uses: the page opens it
    /// on its face only when that face can show (a missing one falls back
    /// like the Work screen's own switch).
    func work(for node: WorkflowNodeEntity) -> WorkflowNodeWork {
        let issue = issues[node.issueId]
        return WorkflowView.nodeWork(
            issueId: node.issueId,
            sessionId: node.sessionId,
            issuePushed: issue?.prUrl?.isEmpty == false || issue?.branch?.isEmpty == false,
            sessions: sessions,
            me: deps.auth.userId,
            now: Date()
        )
    }

    /// The nodes in the page's scope, in strip (DAG) order.
    var orderedNodes: [WorkflowNodeEntity] {
        order.compactMap { id in nodes.first { $0.id == id } }
    }

    /// Every run's published screenshots, by topic.
    var resultGroups: [SessionResultGroup] {
        groupSessionResults(sessions.flatMap { parseSessionResults($0.results) })
    }

    func devicePresentation(_ session: CodingSessionEntity) -> SessionDevicePresentation {
        SessionDevicePresentation.resolve(session: session, devices: deviceRows)
    }

    func batchIssues(_ session: CodingSessionEntity) -> [IssueEntity] {
        BatchRun.issueIds(session.batchIssueIds).compactMap { issues[$0] }
    }

    // MARK: - Observation

    func observe() {
        guard observationTask == nil, let pool = try? deps.db.pool(forAccountId: accountId)
        else { return }
        let id = workflowId
        let observation = ValueObservation.tracking { db -> Snapshot in
            let workflow = try WorkflowEntity.fetchOne(db, key: id)
            let nodes = try WorkflowNodeEntity.filter(Column("workflow_id") == id).fetchAll(db)
            let sessionIds = Array(Set(nodes.compactMap(\.sessionId)))
            let sessions = try CodingSessionEntity
                .filter(Column("workflow_id") == id || sessionIds.contains(Column("id")))
                .fetchAll(db)
            let events = try WorkflowEventEntity
                .filter(Column("workflow_id") == id)
                .order(Column("at").desc)
                .fetchAll(db)
            let deviceRows = try DeviceEntity.fetchAll(db)
            let covered = Array(Set(nodes.flatMap(\.coveredIssueIds)))
            guard !covered.isEmpty else {
                return Snapshot(
                    workflow: workflow, nodes: nodes, issues: [], relations: [],
                    sessions: sessions, events: events, deviceRows: deviceRows
                )
            }
            let issues = try IssueEntity.filter(covered.contains(Column("id"))).fetchAll(db)
            let relations = try IssueRelationEntity
                .filter([IssueRelationType.blocks.rawValue, IssueRelationType.parent.rawValue]
                    .contains(Column("type")))
                .filter(covered.contains(Column("issue_id")))
                .filter(covered.contains(Column("related_issue_id")))
                .fetchAll(db)
            return Snapshot(
                workflow: workflow, nodes: nodes, issues: issues, relations: relations,
                sessions: sessions, events: events, deviceRows: deviceRows
            )
        }
        observationTask = Task { [weak self] in
            do {
                for try await snapshot in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    self.apply(snapshot)
                    // The machine list follows the team; the picker refreshes
                    // it on open, so a session tick never re-reads it.
                    if self.devices.isEmpty { await self.loadDevices() }
                }
            } catch {
                self?.loaded = true
            }
        }
    }

    private struct Snapshot {
        let workflow: WorkflowEntity?
        let nodes: [WorkflowNodeEntity]
        let issues: [IssueEntity]
        let relations: [IssueRelationEntity]
        let sessions: [CodingSessionEntity]
        let events: [WorkflowEventEntity]
        let deviceRows: [DeviceEntity]
    }

    private func apply(_ snapshot: Snapshot) {
        workflow = snapshot.workflow
        nodes = snapshot.nodes
        issues = Dictionary(snapshot.issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        relations = snapshot.relations
        sessions = snapshot.sessions
        events = snapshot.events
        deviceRows = snapshot.deviceRows
        loaded = true
        selection.prune(with: order)
        syncAnswerChannel()
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
        releaseAnswerChannel()
    }

    /// The machine pool follows the workflow's TEAM (it arrives with the row).
    func loadDevices() async {
        guard let teamId = workflow?.teamId else { return }
        devices = await DeviceQueries.devices(
            db: deps.db, accountId: accountId, teamId: teamId, userId: deps.auth.userId
        )
    }

    // MARK: - The open questions

    /// Every open question renders; only the ones whose run is MINE take an
    /// answer — a live run is steerable only by its owner (EXP-312), and a
    /// teammate's question never hides mine.
    func isAnswerable(_ question: WorkflowOpenQuestion) -> Bool {
        guard let run = session(question.sessionId) else { return false }
        return CodingSessionOwnership.isOwn(run, userId: deps.auth.userId)
    }

    private func syncAnswerChannel() {
        let wanted = Set(openQuestions.filter(isAnswerable).map(\.sessionId))
        for id in answerModels.keys where !wanted.contains(id) {
            deps.steerSessions.detach(accountId: accountId, sessionId: id)
            answerModels[id] = nil
        }
        for id in wanted where answerModels[id] == nil {
            guard let run = session(id) else { continue }
            answerModels[id] = deps.steerSessions.attach(accountId: accountId, sessionId: id) {
                AgentSessionModel(
                    accountId: accountId,
                    session: run,
                    currentUserId: deps.auth.userId,
                    steerApi: deps.steerApi,
                    attachmentsApi: deps.attachmentsApi,
                    issuesApi: deps.issuesApi,
                    db: deps.db
                )
            }
        }
    }

    private func releaseAnswerChannel() {
        for id in answerModels.keys {
            deps.steerSessions.detach(accountId: accountId, sessionId: id)
        }
        answerModels = [:]
    }

    /// Send the answer as a message to the asking run. False while its socket
    /// is still connecting.
    func answer(_ text: String, to sessionId: String) -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let answerModel = answerModels[sessionId] else { return false }
        let sent = answerModel.sendMessage(trimmed)
        if !sent { error = "The run is not connected yet. Try again in a moment." }
        return sent
    }

    // MARK: - Writes

    /// Save the edited name (commit or blur); blank or unchanged = nothing.
    /// A rename never conflicts with a start/pause, so it goes out even while
    /// another write is in flight (the `busy` guard would drop it silently)
    /// and never flips `busy` itself.
    func rename(_ value: String) {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, trimmed != workflow?.name else { return }
        let accountId = accountId
        let workflowId = workflowId
        Task {
            do {
                _ = try await self.deps.workflowsApi.update(
                    accountId: accountId, id: workflowId, patch: WorkflowPatch(name: trimmed)
                )
            } catch {
                self.error = error.userFacingMessage
            }
        }
    }

    func setDevice(_ deviceId: String?) {
        run { accountId, id in
            _ = try await self.deps.workflowsApi.update(
                accountId: accountId, id: id, patch: WorkflowPatch(deviceId: .some(deviceId))
            )
        }
    }

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

    func mergeFinalPr() {
        run { accountId, id in
            _ = try await self.deps.workflowsApi.mergeFinalPr(accountId: accountId, id: id)
        }
    }

    func perform(_ action: NodeChipAction, on nodeId: String) {
        run { accountId, _ in
            let api = self.deps.workflowsApi
            switch action {
            case .retry:
                try await api.resolveNode(accountId: accountId, nodeId: nodeId, action: .retry)
            case .skip:
                try await api.resolveNode(accountId: accountId, nodeId: nodeId, action: .skip)
            case .admit:
                try await api.admitNode(accountId: accountId, nodeId: nodeId, admit: true)
            case .dismiss:
                try await api.admitNode(accountId: accountId, nodeId: nodeId, admit: false)
            }
        }
    }

    /// Delete the whole workflow; `onDeleted` pops back to the list.
    func delete(onDeleted: @escaping () -> Void) {
        run { accountId, id in
            try await self.deps.workflowsApi.delete(accountId: accountId, id: id)
            onDeleted()
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
}
