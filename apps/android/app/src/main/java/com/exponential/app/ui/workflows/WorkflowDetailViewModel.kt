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
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.NormalizedWorkflowLaunch
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.domain.WorkflowLaunch
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.domain.edgeNode
import com.exponential.app.domain.launchOptions
import com.exponential.app.domain.metricCounters
import com.exponential.app.domain.normalizedLaunch
import com.exponential.app.domain.shape
import com.exponential.app.domain.trainNode
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// EXP-981: ONE draft workflow — its graph, the node plan and the start
// configuration. Reads are the two synced shapes (`workflows`,
// `workflow_nodes`) joined against the issues and `blocks` relations the
// client already holds; writes are the member-gated `workflows` router, whose
// refusals are human sentences shown verbatim ([error]).
//
// No client lays the graph out: `wave`/`lane`/`on_cycle` on the nodes ARE the
// server's layout, and the edges come from the shared rule
// ([WorkflowView.edges]) over the synced relations.

/**
 * EXP-982 — one node's coding run as the graph and the sheet read it: where a
 * tap goes, and what the dot says (the ONE session tone table every session
 * list paints with). `live` is the row's status alone — a run the engine lost
 * is reported by the NODE's own state, never by a stale dot.
 */
data class WorkflowNodeRun(
    val sessionId: String,
    val live: Boolean,
    val tone: SessionDotTone,
    /** The agent is mid-turn: the dot pulses (EXP-848). */
    val busy: Boolean,
)

/** The graph as the screen draws it: nodes in layout order plus their edges. */
data class WorkflowGraph(
    val nodes: List<WorkflowNodeEntity> = emptyList(),
    val edges: List<WorkflowView.Edge> = emptyList(),
    val issuesById: Map<String, IssueEntity> = emptyMap(),
    /** `node id → its run`, for every node whose session row has synced. */
    val runsByNodeId: Map<String, WorkflowNodeRun> = emptyMap(),
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

    val workflow: StateFlow<WorkflowEntity?> = dbFlow
        .flatMapLatest { db ->
            if (db == null || workflowId.isEmpty()) {
                flowOf(null)
            } else {
                db.workflowDao().observeById(workflowId)
            }
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

    // Every synced issue and relation: a node's issue can live on any board of
    // the team, so neither pool may be scoped to one.
    private val allIssues: StateFlow<List<IssueEntity>> =
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val relations: StateFlow<List<IssueRelationEntity>> =
        dbFlow.scopedQuery(emptyList<IssueRelationEntity>()) { it.issueRelationDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // EXP-982: the run each started node is in — a node that is up wears its
    // session's own dot, and the Running strip opens it. Every synced row: a
    // node's run can be a batch session, which is scoped to no single issue.
    private val sessions: StateFlow<List<CodingSessionEntity>> =
        dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val graph: StateFlow<WorkflowGraph> = combine(
        nodes,
        relations,
        allIssues,
        workflow,
        sessions,
    ) { nodeRows, relationRows, issues, row, sessionRows ->
        // The covered issues of THIS workflow; a relation with an end outside
        // them is somebody else's edge.
        val covered = HashSet<String>()
        nodeRows.forEach { node ->
            covered.add(node.issueId)
            covered.addAll(node.memberIssueIds)
        }
        val issuesById = issues.filter { it.id in covered }.associateBy { it.id }
        val sessionsById = sessionRows.associateBy { it.id }
        val runs = HashMap<String, WorkflowNodeRun>()
        nodeRows.forEach { node ->
            val session = node.sessionId?.takeIf { it.isNotBlank() }?.let { sessionsById[it] }
                ?: return@forEach
            val live = session.status == DomainContract.codingSessionStatusRunning ||
                session.status == DomainContract.codingSessionStatusInReview
            val state = codingSessionDisplayState(session, issuesById[node.issueId]?.prState)
            runs[node.id] = WorkflowNodeRun(
                sessionId = session.id,
                live = live,
                // An ended run keeps its row (Open run still works) but never a
                // live tone: the strip lists what is up, nothing else.
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
                busy = live && state == CodingSessionDisplayState.Running && session.agentBusy,
            )
        }
        WorkflowGraph(
            nodes = nodeRows,
            edges = WorkflowView.edges(
                nodes = nodeRows.map { it.edgeNode },
                relations = relationRows
                    .filter { it.issueId in covered && it.relatedIssueId in covered }
                    .map {
                        WorkflowView.EdgeRelation(
                            type = it.type,
                            issueId = it.issueId,
                            relatedIssueId = it.relatedIssueId,
                        )
                    },
                cycleEdges = row?.shape?.cycleEdges.orEmpty(),
            ),
            issuesById = issuesById,
            runsByNodeId = runs,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), WorkflowGraph())

    /**
     * The machines the workflow can be bound to: every synced device — the
     * caller's own plus the team's shared servers — ONLINE OR NOT, because a
     * workflow outlives a machine's uptime. The server refuses a device that
     * cannot run workflows with its own sentence.
     */
    val devices: StateFlow<List<SteerDevice>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
        DeviceLiveness.ticker(),
        auth.userId,
    ) { rows, nowMs, userId ->
        rows.sortedWith(stableDeviceOrder(nowMs)).map { it.toSteerDevice(nowMs, userId) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The bound machine's row, when it is still in the registry. */
    val device: StateFlow<SteerDevice?> = combine(devices, workflow) { rows, row ->
        row?.deviceId?.let { id -> rows.firstOrNull { it.deviceId == id } }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-1029: the workflow's launch as a RUN reads it — the agent and the
     * two models, folded out of whatever vintage the row stores. Nothing on
     * this screen edits it (EXP-1014): the phone carries the stored launch and
     * the node sheet names the model each node runs on.
     */
    val launch: StateFlow<NormalizedWorkflowLaunch> = workflow
        .map { normalizedLaunch(it?.launchOptions ?: WorkflowLaunch()) }
        .stateIn(
            viewModelScope,
            SharingStarted.WhileSubscribed(5_000),
            normalizedLaunch(WorkflowLaunch()),
        )

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

    /** The name is a label: editable at any status, saved on blur. */
    fun rename(name: String) {
        val trimmed = name.trim()
        if (trimmed.isEmpty() || trimmed == workflow.value?.name) return
        mutate("The workflow could not be renamed") { accountId ->
            workflowsApi.update(accountId, workflowId, name = trimmed)
        }
    }

    /** Bind (or, with a blank id, unbind) the runner machine. */
    fun setDevice(deviceId: String) {
        mutate("The runner could not be set") { accountId ->
            workflowsApi.update(
                accountId,
                workflowId,
                deviceId = deviceId.takeIf { it.isNotEmpty() },
                clearDevice = deviceId.isEmpty(),
            )
        }
    }

    /** What the plan declares for ONE node — addressed by its issue. */
    fun updateNode(issueId: String, kind: String? = null, risk: String? = null) {
        mutate("The node could not be updated") { accountId ->
            workflowsApi.updateNode(accountId, workflowId, issueId, kind = kind, risk = risk)
        }
    }

    /** EXP-984: take a mid-run proposal into the graph, or throw it away. */
    fun admitNode(nodeId: String, admit: Boolean) {
        mutate(
            if (admit) "The node could not be admitted" else "The node could not be dismissed",
        ) { accountId ->
            workflowsApi.admitNode(accountId, nodeId, admit)
        }
    }

    // ── Running a workflow (EXP-982) ────────────────────────────────────────
    // The server flips INTENT only; the bound device's engine does the work off
    // these same synced rows, so every button below is one mutation and then
    // Electric.

    /**
     * Why Start is disabled, or null when the draft can start. A row that has
     * not synced blocks too — the screen shows "Syncing…" rather than a button
     * whose refusal nobody can predict.
     */
    val startBlocker: StateFlow<String?> = workflow
        .map { row ->
            row ?: return@map "The workflow has not synced yet."
            WorkflowView.startBlocker(
                WorkflowView.Startable(
                    status = row.status,
                    deviceId = row.deviceId,
                    repositoryId = row.repositoryId,
                    startOn = row.startOn,
                ),
                row.shape,
            )
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The nodes whose PR is up, in landing order, each with its step. */
    val mergeTrain: StateFlow<List<WorkflowView.TrainEntry>> = combine(
        nodes,
        workflow,
    ) { nodeRows, row ->
        WorkflowView.mergeTrain(nodeRows.map { it.trainNode })
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The final-PR node's caption, or null while that node is not drawn. */
    val finalPrCaption: StateFlow<String?> = combine(nodes, workflow) { nodeRows, row ->
        if (row == null) {
            null
        } else {
            WorkflowView.finalPrCaption(
                nodeStates = nodeRows.map { it.state },
                finalPrState = row.finalPrState,
                finalPrNumber = row.finalPrNumber,
            )
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-984: the run's counters, as the Metrics section's rows. A DRAFT has
     * run nothing, so it has no metrics to read — the section is hidden there
     * rather than showing a critical path with nothing behind it.
     */
    val metricRows: StateFlow<List<WorkflowView.MetricRow>> = workflow
        .map { row ->
            if (row == null || row.status == DomainContract.wfStatusDraft) {
                emptyList()
            } else {
                WorkflowView.metricRows(row.metricCounters)
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun start() {
        mutate("The workflow could not be started") { accountId ->
            workflowsApi.start(accountId, workflowId)
        }
    }

    fun pause() {
        mutate("The workflow could not be paused") { accountId ->
            workflowsApi.pause(accountId, workflowId)
        }
    }

    fun resume() {
        mutate("The workflow could not be resumed") { accountId ->
            workflowsApi.resume(accountId, workflowId)
        }
    }

    fun cancel() {
        mutate("The workflow could not be cancelled") { accountId ->
            workflowsApi.cancel(accountId, workflowId)
        }
    }

    /** The gate: clear a node's PR for the train, or take the approval back. */
    fun approveNode(nodeId: String, approved: Boolean) {
        mutate("The node could not be approved") { accountId ->
            workflowsApi.approveNode(accountId, nodeId, approved = approved)
        }
    }

    /** [WorkflowsApi.NODE_RETRY] or [WorkflowsApi.NODE_SKIP] on a failed node. */
    fun resolveNode(nodeId: String, action: String) {
        mutate("The node could not be resolved") { accountId ->
            workflowsApi.resolveNode(accountId, nodeId, action)
        }
    }

    /** Permanent, and refused by the server while the workflow is live. */
    fun delete() {
        mutate("The workflow could not be deleted") { accountId ->
            workflowsApi.delete(accountId, workflowId)
            _deleted.value = true
        }
    }

    // One mutation at a time, with the server's own refusal surfaced: its copy
    // names the actual reason (a started issue, two repositories, a device
    // that cannot run workflows).
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
