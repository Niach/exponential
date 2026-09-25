package com.exponential.app.ui.workflows

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.WorkflowsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.WorkflowEventEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.HeaderNode
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.domain.SessionResultGroup
import com.exponential.app.domain.StripNodeInput
import com.exponential.app.domain.StripWave
import com.exponential.app.domain.WorkflowOpenQuestion
import com.exponential.app.domain.WorkflowPrimaryAction
import com.exponential.app.domain.WorkflowQuestions
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.domain.coveredIssueIds
import com.exponential.app.domain.edgeNode
import com.exponential.app.domain.groupSessionResults
import com.exponential.app.domain.parseSessionResults
import com.exponential.app.domain.shape
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
import com.exponential.app.ui.components.deviceOptionLabel
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// EXP-1087: ONE workflow on the phone — a node strip (the graph as chips,
// `WorkflowView.nodeStrip`) over the Work screen's faces. Reads are the synced
// shapes (`workflows`, `workflow_nodes`, `workflow_events`, `coding_sessions`)
// joined against the issues and relations the client already holds; writes
// are the member-gated `workflows` router, whose refusals are human sentences
// shown verbatim ([error]).

/**
 * One node's coding run: where a tap goes and what the dot says (the ONE
 * session tone table). `live` is the row's status alone.
 */
data class WorkflowNodeRun(
    val sessionId: String,
    val live: Boolean,
    val tone: SessionDotTone,
    /** The agent is mid-turn: the dot pulses (EXP-848). */
    val busy: Boolean,
)

/** The workflow's nodes with everything a chip or a list row reads. */
data class WorkflowGraph(
    val nodes: List<WorkflowNodeEntity> = emptyList(),
    val edges: List<WorkflowView.Edge> = emptyList(),
    val issuesById: Map<String, IssueEntity> = emptyMap(),
    /** `node id → its run`, for every node whose session row has synced. */
    val runsByNodeId: Map<String, WorkflowNodeRun> = emptyMap(),
    /** Every covered issue resolved against the team's statuses. */
    val statusByIssueId: Map<String, ResolvedIssueStatus> = emptyMap(),
) {
    /** The runs that are UP, in the graph's own (wave, lane) order. */
    val liveRuns: List<Pair<WorkflowNodeEntity, WorkflowNodeRun>>
        get() = nodes.mapNotNull { node ->
            runsByNodeId[node.id]?.takeIf { it.live }?.let { node to it }
        }
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class WorkflowDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val workflowsApi: WorkflowsApi,
) : ViewModel() {

    val workflowId: String = savedStateHandle["workflowId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val currentUserId: StateFlow<String?> = auth.userId

    val workflow: StateFlow<WorkflowEntity?> = dbFlow
        .flatMapLatest { db ->
            if (db == null || workflowId.isEmpty()) flowOf(null) else db.workflowDao().observeById(workflowId)
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val nodes: StateFlow<List<WorkflowNodeEntity>> = dbFlow
        .flatMapLatest { db ->
            if (db == null || workflowId.isEmpty()) {
                flowOf(emptyList())
            } else {
                db.workflowNodeDao().observeByWorkflow(workflowId)
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** EXP-1082: the workflow's event log, newest first. */
    val events: StateFlow<List<WorkflowEventEntity>> = dbFlow
        .flatMapLatest { db ->
            if (db == null || workflowId.isEmpty()) {
                flowOf(emptyList())
            } else {
                db.workflowEventDao().observeByWorkflow(workflowId)
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Every synced issue and relation: a node's issue can live on any board of
    // the team, so neither pool may be scoped to one.
    private val allIssues: StateFlow<List<IssueEntity>> =
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val relations: StateFlow<List<IssueRelationEntity>> =
        dbFlow.scopedQuery(emptyList<IssueRelationEntity>()) { it.issueRelationDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val sessions: StateFlow<List<CodingSessionEntity>> =
        dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val deviceRows: StateFlow<List<DeviceEntity>> =
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val teamStatuses: StateFlow<List<ResolvedIssueStatus>> =
        combine(dbFlow, workflow.map { it?.teamId }.distinctUntilChanged()) { db, teamId -> db to teamId }
            .flatMapLatest { (db, teamId) ->
                if (db == null || teamId.isNullOrEmpty()) {
                    flowOf(emptyList())
                } else {
                    db.issueStatusDao().observeByTeam(teamId).map { IssueStatusResolver.teamStatuses(it) }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Every run of THIS workflow — its membership column first (EXP-1082),
     * plus a node's recorded session for a row that predates it — oldest
     * first.
     */
    val workflowSessions: StateFlow<List<CodingSessionEntity>> = combine(sessions, nodes) { rows, nodeRows ->
        val nodeSessionIds = nodeRows.mapNotNullTo(HashSet()) { it.sessionId?.takeIf(String::isNotBlank) }
        rows.filter { it.workflowId == workflowId || it.id in nodeSessionIds }
            .sortedWith(compareBy({ it.createdAt }, { it.id }))
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The open questions of the workflow's live runs (EXP-1065 fills the selector). */
    val openQuestions: StateFlow<List<WorkflowOpenQuestion>> = sessions
        .map { rows -> WorkflowQuestions.open(rows, workflowId) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val graph: StateFlow<WorkflowGraph> = combine(
        nodes,
        relations,
        allIssues,
        workflow,
        sessions,
    ) { nodeRows, relationRows, issues, row, sessionRows ->
        val covered = HashSet<String>()
        nodeRows.forEach { covered.addAll(it.coveredIssueIds) }
        val issuesById = issues.filter { it.id in covered }.associateBy { it.id }
        val sessionsById = sessionRows.associateBy { it.id }
        val runs = HashMap<String, WorkflowNodeRun>()
        nodeRows.forEach { node ->
            val session = node.sessionId?.takeIf { it.isNotBlank() }?.let { sessionsById[it] }
                ?: sessionRows.lastOrNull { it.workflowNodeId == node.id }
                ?: return@forEach
            val live = session.status == DomainContract.codingSessionStatusRunning ||
                session.status == DomainContract.codingSessionStatusInReview
            val state = codingSessionDisplayState(session, issuesById[node.issueId]?.prState)
            runs[node.id] = WorkflowNodeRun(
                sessionId = session.id,
                live = live,
                tone = if (!live) {
                    SessionDotTone.Muted
                } else {
                    when (state) {
                        CodingSessionDisplayState.Running -> SessionDotTone.Running
                        CodingSessionDisplayState.NeedsInput -> SessionDotTone.NeedsInput
                        CodingSessionDisplayState.Review -> SessionDotTone.Review
                        CodingSessionDisplayState.Done -> SessionDotTone.Done
                    }
                },
                busy = live && session.agentBusy,
            )
        }
        WorkflowGraph(
            nodes = nodeRows,
            edges = WorkflowView.edges(
                nodes = nodeRows.map { it.edgeNode },
                relations = relationRows
                    .filter { it.issueId in covered && it.relatedIssueId in covered }
                    .map { WorkflowView.EdgeRelation(it.type, it.issueId, it.relatedIssueId) },
                cycleEdges = row?.shape?.cycleEdges.orEmpty(),
            ),
            issuesById = issuesById,
            runsByNodeId = runs,
        )
    }
        .combine(teamStatuses) { graph, statuses ->
            graph.copy(
                statusByIssueId = graph.issuesById.mapValues { (_, issue) -> IssueStatusResolver.resolve(issue, statuses) },
            )
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), WorkflowGraph())

    /** The chip strip: one row per wave, `All` is the page's own first chip. */
    val strip: StateFlow<List<StripWave>> = combine(graph, openQuestions) { g, questions ->
        workflowStrip(g, questions)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * EXP-312: the runs the caller OWNS — only those are steerable, so only
     * their open questions offer an answer field.
     */
    val ownSessionIds: StateFlow<Set<String>> = combine(workflowSessions, auth.userId) { rows, userId ->
        if (userId == null) emptySet() else rows.filter { it.userId == userId }.mapTo(HashSet()) { it.id }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptySet())

    /** Every run's screenshots, grouped by topic in first-published order. */
    val results: StateFlow<List<SessionResultGroup>> = workflowSessions
        .map { rows -> groupSessionResults(rows.flatMap { parseSessionResults(it.results) }) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * The machines the workflow may be bound to: the caller's own plus the
     * team's shared runners (what the devices shape syncs), ONLINE only.
     */
    val devices: StateFlow<List<SteerDevice>> = combine(
        deviceRows,
        DeviceLiveness.ticker(),
        auth.userId,
    ) { rows, nowMs, userId ->
        rows.sortedWith(stableDeviceOrder(nowMs)).map { it.toSteerDevice(nowMs, userId) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The bound machine's row, when it is still in the registry. */
    val device: StateFlow<SteerDevice?> = combine(devices, workflow) { rows, row ->
        row?.deviceId?.let { id -> rows.firstOrNull { it.deviceId == id } }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** Why Start is refused, as the notice above the strip shows it (null = none). */
    val startNotice: StateFlow<String?> = combine(workflow, device) { row, dev ->
        row?.let { workflowStartNotice(it, dev?.let(::deviceOptionLabel) ?: it.deviceId?.take(8)) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The header's node counts (`workflowHeaderCaption`). */
    val headerNodes: StateFlow<List<HeaderNode>> = nodes
        .map { rows -> rows.map { HeaderNode(it.state, it.memberIssueIds.size) } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error

    /** Set once the delete landed — the screen pops back to the list. */
    private val _deleted = MutableStateFlow(false)
    val deleted: StateFlow<Boolean> = _deleted

    fun clearError() {
        _error.value = null
    }

    /** The header's name field: saves on Done or blur; blank or unchanged = nothing. */
    fun rename(name: String) {
        val trimmed = name.trim()
        if (trimmed.isEmpty() || trimmed == workflow.value?.name) return
        mutate("The workflow could not be renamed") { accountId ->
            workflowsApi.update(accountId, workflowId, name = trimmed)
        }
    }

    /** Bind the runner machine (a draft's pick). */
    fun setDevice(deviceId: String) {
        mutate("The runner could not be set") { accountId ->
            workflowsApi.update(accountId, workflowId, deviceId = deviceId)
        }
    }

    fun start() = mutate("The workflow could not be started") { workflowsApi.start(it, workflowId) }

    fun pause() = mutate("The workflow could not be paused") { workflowsApi.pause(it, workflowId) }

    fun resume() = mutate("The workflow could not be resumed") { workflowsApi.resume(it, workflowId) }

    fun cancel() = mutate("The workflow could not be cancelled") { workflowsApi.cancel(it, workflowId) }

    /** The ONE human review of the whole run — squash-merge the final PR. */
    fun mergeFinalPr() = mutate("The final pull request could not be merged") {
        workflowsApi.mergeFinalPr(it, workflowId)
    }

    /** [WorkflowsApi.NODE_RETRY] or [WorkflowsApi.NODE_SKIP] on a failed node. */
    fun resolveNode(nodeId: String, action: String) = mutate("The node could not be resolved") {
        workflowsApi.resolveNode(it, nodeId, action)
    }

    /** EXP-984: take a mid-run proposal into the graph, or throw it away. */
    fun admitNode(nodeId: String, admit: Boolean) = mutate(
        if (admit) "The node could not be admitted" else "The node could not be dismissed",
    ) { workflowsApi.admitNode(it, nodeId, admit) }

    /** Permanent, and refused by the server while the workflow is live. */
    fun delete() = mutate("The workflow could not be deleted") { accountId ->
        workflowsApi.delete(accountId, workflowId)
        _deleted.value = true
    }

    // One mutation at a time, with the server's own refusal surfaced.
    private fun mutate(fallback: String, block: suspend (String) -> Unit) {
        if (_busy.value) return
        _busy.value = true
        _error.value = null
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _busy.value = false
                return@launch
            }
            try {
                block(accountId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _error.value = trpcErrorMessage(t, fallback)
            }
            _busy.value = false
        }
    }
}

/**
 * The strip off the joined graph: each node's chip, captioned by its note
 * while it has one (a holding node says why), else its display state.
 */
internal fun workflowStrip(graph: WorkflowGraph, questions: List<WorkflowOpenQuestion>): List<StripWave> {
    val asking = questions.mapTo(HashSet()) { it.nodeId }
    return WorkflowView.nodeStrip(
        nodes = graph.nodes.map { node ->
            StripNodeInput(
                id = node.id,
                identifier = graph.issuesById[node.issueId]?.identifier ?: node.issueId.take(8),
                state = node.state,
                wave = node.wave ?: 0,
                lane = node.lane ?: 0,
                members = node.memberIssueIds.size,
                live = graph.runsByNodeId[node.id]?.busy == true,
                needsYou = node.id in asking,
                note = node.note,
            )
        },
        edges = graph.edges.map { it.from to it.to },
    )
}

/**
 * The Start blocker as the page shows it, or null: only while Start or Pick
 * device is the primary action, and never "Pick the device…" when the
 * primary action already IS Pick device.
 */
internal fun workflowStartNotice(row: WorkflowEntity, deviceLabel: String?): String? {
    val primary = WorkflowView.primaryAction(row.status, deviceLabel, row.finalPrState)
    if (primary != WorkflowPrimaryAction.START && primary != WorkflowPrimaryAction.PICK_DEVICE) return null
    // Pick device already says "pick a device": judge the rest as if one were
    // bound, so only the OTHER reasons surface.
    val deviceId = if (primary == WorkflowPrimaryAction.PICK_DEVICE) row.deviceId ?: "picking" else row.deviceId
    return WorkflowView.startBlocker(
        WorkflowView.Startable(row.status, deviceId, row.repositoryId),
        row.shape,
    )
}
