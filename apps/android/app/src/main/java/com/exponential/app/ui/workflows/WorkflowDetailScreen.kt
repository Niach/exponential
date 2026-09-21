package com.exponential.app.ui.workflows

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.WorkflowsApi
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.WorkflowLaunch
import com.exponential.app.domain.WorkflowNodeReview
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.captionNode
import com.exponential.app.domain.line
import com.exponential.app.domain.shape
import com.exponential.app.domain.workflowNodeReview
import com.exponential.app.ui.components.AccountPickerPill
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GlassNotice
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.SUBAGENT_MODEL_LABEL
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.accountOptionsFor
import com.exponential.app.ui.components.ambientAccountOptions
import com.exponential.app.ui.components.deviceOptionLabel
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelOptionsFor
import com.exponential.app.ui.components.subagentModelLabel
import com.exponential.app.ui.components.subagentModelOptions
import com.exponential.app.ui.components.supportsSubagentModel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-981/982: ONE workflow. A DRAFT is looked at, configured, planned by an
 * agent or deleted; from P3 it is also STARTED, and then the bound device's
 * engine runs every node once, lands the reviewed PRs into the integration
 * branch in order, and opens the one final pull request.
 *
 * The page reads top to bottom: the name (editable inline) with the shape line
 * and, when the plan holds one, the cycle note in the destructive tone; the
 * GRAPH as this phone's wave-grouped list ([WorkflowGraphList]), which carries
 * the final-PR section at its end; the merge train; "How it runs" (read-only
 * once the workflow left draft); then the status's own actions.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun WorkflowDetailScreen(
    onBack: () -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    onOpenAgent: (AgentComposerSeed) -> Unit,
    /** The node's coding run, opened the usual way (the Work screen's Run face). */
    onOpenSession: (sessionId: String) -> Unit,
    /** The node issue's pull request, on the issue detail's own Changes page. */
    onOpenChanges: (issueId: String) -> Unit,
    viewModel: WorkflowDetailViewModel = hiltViewModel(),
) {
    val workflow by viewModel.workflow.collectAsStateWithLifecycle()
    val graph by viewModel.graph.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val device by viewModel.device.collectAsStateWithLifecycle()
    val launch by viewModel.launch.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val error by viewModel.error.collectAsStateWithLifecycle()
    val deleted by viewModel.deleted.collectAsStateWithLifecycle()
    val startBlocker by viewModel.startBlocker.collectAsStateWithLifecycle()
    val mergeTrain by viewModel.mergeTrain.collectAsStateWithLifecycle()
    val finalPrCaption by viewModel.finalPrCaption.collectAsStateWithLifecycle()
    val metricRows by viewModel.metricRows.collectAsStateWithLifecycle()

    // A delete pops back to the list; so does a workflow that stopped syncing
    // (someone else deleted it) once its row has actually been seen.
    LaunchedEffect(deleted) { if (deleted) onBack() }

    var selectedNode by remember { mutableStateOf<WorkflowNodeEntity?>(null) }
    var confirmDelete by remember { mutableStateOf(false) }
    var confirmCancel by remember { mutableStateOf(false) }
    // The name field is LOCAL while it is being typed; a blur writes it.
    var nameDraft by remember { mutableStateOf<String?>(null) }

    val row = workflow
    val shape = remember(row?.metrics) { row?.shape ?: WorkflowView.Shape() }
    val cycleNote = remember(shape) { WorkflowView.cycleNote(shape) }
    val isDraft = row?.status == DomainContract.wfStatusDraft
    // The node the sheet is showing, kept LIVE: an approval or a retry lands
    // through Electric, and the sheet has to move with it.
    val sheetNode = selectedNode?.let { picked ->
        graph.nodes.firstOrNull { it.id == picked.id } ?: picked
    }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(WorkflowView.WORKFLOWS_TITLE) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                    containerColor = Color.Transparent,
                ),
            )
        },
    ) { padding ->
        if (row == null) {
            // The row has not synced (yet, or at all) — never a crash, and
            // never an empty page that looks like a broken workflow.
            Text(
                "Syncing…",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(padding).padding(16.dp),
            )
            return@Scaffold
        }
        LazyColumn(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .testTag("workflow-detail"),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            item(key = "__header__") {
                Column(modifier = Modifier.fillMaxWidth()) {
                    GlassTextField(
                        value = nameDraft ?: row.name,
                        onValueChange = { nameDraft = it },
                        placeholder = "Workflow name",
                        singleLine = true,
                        bordered = false,
                        containerColor = Color.Transparent,
                        textStyle = MaterialTheme.typography.titleMedium,
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("workflow-name")
                            .onFocusChanged { state ->
                                if (!state.isFocused) {
                                    nameDraft?.let(viewModel::rename)
                                    nameDraft = null
                                }
                            },
                    )
                    Text(
                        WorkflowView.shapeLine(shape),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Tertiary,
                        ),
                        modifier = Modifier.padding(horizontal = 16.dp),
                    )
                    cycleNote?.let { note ->
                        Text(
                            note,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier
                                .padding(horizontal = 16.dp, vertical = 4.dp)
                                .testTag("workflow-header-cycle-note"),
                        )
                    }
                }
            }
            error?.let { message ->
                item(key = "__error__") {
                    GlassNotice(
                        text = message,
                        contentColor = MaterialTheme.colorScheme.error,
                        onClick = viewModel::clearError,
                        modifier = Modifier.fillMaxWidth().testTag("workflow-error"),
                        leading = {
                            Icon(
                                ExpIcons.uiWarning,
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                            )
                        },
                    )
                }
            }
            item(key = "__graph__") {
                // The cycle note already sits in the header; the graph paints
                // the nodes and edges it names.
                WorkflowGraphList(
                    graph = graph,
                    workflowStatus = row.status,
                    cycleNote = null,
                    onSelectNode = { selectedNode = it },
                    onOpenRun = onOpenSession,
                    finalPrCaption = finalPrCaption,
                    finalPrUrl = row.finalPrUrl,
                )
            }
            // EXP-982: what is queued to land on the integration branch, in
            // landing order. A draft lands nothing, so it has no train.
            if (!isDraft) {
                item(key = "__merge_train__") {
                    MergeTrainSection(
                        entries = mergeTrain,
                        graph = graph,
                        onSelectNode = { selectedNode = it },
                    )
                }
            }
            item(key = "__how_it_runs__") {
                Column(modifier = Modifier.fillMaxWidth().padding(top = 8.dp)) {
                    SectionHeader("How it runs", modifier = Modifier.padding(horizontal = 16.dp))
                    HowItRunsSection(
                        devices = devices,
                        device = device,
                        launch = launch,
                        gate = row.gate,
                        startOn = row.startOn,
                        enabled = isDraft && !busy,
                        onDeviceChange = viewModel::setDevice,
                        onLaunchChange = viewModel::setLaunch,
                        onGateChange = viewModel::setGate,
                        onStartOnChange = viewModel::setStartOn,
                    )
                }
            }
            // EXP-984: what the run actually cost, once there IS a run.
            if (metricRows.isNotEmpty()) {
                item(key = "__metrics__") { WorkflowMetricsSection(metricRows) }
            }
            item(key = "__actions__") {
                // EXP-982: what a workflow offers is its STATUS. A draft is
                // started, planned or thrown away; a live one is held or
                // abandoned; one that is over can only be deleted.
                Column(modifier = Modifier.fillMaxWidth().padding(top = 12.dp)) {
                    FlowRow(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        when (row.status) {
                            DomainContract.wfStatusDraft -> {
                                GlassPill(
                                    WorkflowView.START_WORKFLOW_LABEL,
                                    primary = true,
                                    icon = ExpIcons.actionRun,
                                    enabled = startBlocker == null && !busy,
                                    onClick = viewModel::start,
                                    modifier = Modifier.testTag("workflow-start"),
                                )
                                // EXP-981: Plan opens the Agent composer seeded
                                // with the hidden plan-workflow builtin AND this
                                // workflow's id; its free text ("Anything the
                                // plan should respect") stays optional.
                                GlassPill(
                                    WorkflowView.PLAN_WORKFLOW_LABEL,
                                    icon = ExpIcons.uiChecklist,
                                    onClick = {
                                        onOpenAgent(
                                            AgentComposerSeed(
                                                actionId = DomainContract.builtinPlanWorkflowId,
                                                workflowId = row.id,
                                                deviceId = row.deviceId,
                                            ),
                                        )
                                    },
                                    modifier = Modifier.testTag("workflow-plan"),
                                )
                                WorkflowDeletePill(onClick = { confirmDelete = true })
                            }
                            DomainContract.wfStatusRunning -> {
                                GlassPill(
                                    WorkflowView.PAUSE_WORKFLOW_LABEL,
                                    icon = ExpIcons.uiStop,
                                    enabled = !busy,
                                    onClick = viewModel::pause,
                                    modifier = Modifier.testTag("workflow-pause"),
                                )
                                WorkflowCancelPill(onClick = { confirmCancel = true })
                            }
                            DomainContract.wfStatusPaused -> {
                                GlassPill(
                                    WorkflowView.RESUME_WORKFLOW_LABEL,
                                    primary = true,
                                    icon = ExpIcons.actionRun,
                                    enabled = !busy,
                                    onClick = viewModel::resume,
                                    modifier = Modifier.testTag("workflow-resume"),
                                )
                                WorkflowCancelPill(onClick = { confirmCancel = true })
                            }
                            else -> WorkflowDeletePill(onClick = { confirmDelete = true })
                        }
                    }
                    // Why Start cannot be pressed, in the server's own words.
                    if (isDraft) {
                        startBlocker?.let { blocker ->
                            Text(
                                blocker,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(
                                    alpha = TextEmphasis.Tertiary,
                                ),
                                modifier = Modifier
                                    .padding(top = 6.dp, start = 4.dp)
                                    .testTag("workflow-start-blocker"),
                            )
                        }
                    }
                }
            }
            item(key = "__bottom__") { Spacer(Modifier.height(24.dp)) }
        }
    }

    sheetNode?.let { node ->
        WorkflowNodeSheet(
            node = node,
            graph = graph,
            workflowStatus = row?.status.orEmpty(),
            gate = row?.gate.orEmpty(),
            editable = isDraft && !busy,
            busy = busy,
            onKindChange = { viewModel.updateNode(node.issueId, kind = it) },
            onRiskChange = { viewModel.updateNode(node.issueId, risk = it) },
            onApprove = { approved -> viewModel.approveNode(node.id, approved) },
            onResolve = { action -> viewModel.resolveNode(node.id, action) },
            onAdmit = { admit -> viewModel.admitNode(node.id, admit) },
            onOpenIssue = { issueId ->
                selectedNode = null
                onOpenIssue(issueId)
            },
            onOpenSession = { sessionId ->
                selectedNode = null
                onOpenSession(sessionId)
            },
            onOpenChanges = { issueId ->
                selectedNode = null
                onOpenChanges(issueId)
            },
            onDismiss = { selectedNode = null },
        )
    }

    if (confirmCancel) {
        AlertDialog(
            onDismissRequest = { confirmCancel = false },
            title = { Text(WorkflowView.CANCEL_WORKFLOW_LABEL) },
            text = { Text(WorkflowView.CANCEL_WORKFLOW_CONFIRM) },
            confirmButton = {
                TextButton(onClick = {
                    confirmCancel = false
                    viewModel.cancel()
                }) {
                    Text("Cancel workflow", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmCancel = false }) { Text("Keep running") }
            },
        )
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text(WorkflowView.DELETE_WORKFLOW_LABEL) },
            text = { Text("This action cannot be undone. The issues stay where they are.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    viewModel.delete()
                }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmDelete = false }) { Text("Cancel") }
            },
        )
    }
}

/** Destructive, so it takes the error tone rather than the page's primary fill. */
@Composable
private fun WorkflowDeletePill(onClick: () -> Unit) {
    GlassPill(
        WorkflowView.DELETE_WORKFLOW_LABEL,
        icon = ExpIcons.uiDelete,
        tint = MaterialTheme.colorScheme.error,
        onClick = onClick,
        modifier = Modifier.testTag("workflow-delete"),
    )
}

/** Abandoning a started workflow — confirmed, like every destructive native
 *  action. */
@Composable
private fun WorkflowCancelPill(onClick: () -> Unit) {
    GlassPill(
        WorkflowView.CANCEL_WORKFLOW_LABEL,
        icon = ExpIcons.uiClose,
        tint = MaterialTheme.colorScheme.error,
        onClick = onClick,
        modifier = Modifier.testTag("workflow-cancel"),
    )
}

/**
 * EXP-982: the merge train — every node whose PR is up, in the order it lands
 * on the integration branch. Web and the desktop draw a horizontal strip; a
 * phone draws the same thing as a SHORT list, one row per node with its step.
 */
@Composable
private fun MergeTrainSection(
    entries: List<WorkflowView.TrainEntry>,
    graph: WorkflowGraph,
    onSelectNode: (WorkflowNodeEntity) -> Unit,
) {
    val nodesById = remember(graph.nodes) { graph.nodes.associateBy { it.id } }
    Column(modifier = Modifier.fillMaxWidth().padding(top = 8.dp)) {
        SectionHeader(WorkflowView.MERGE_TRAIN_TITLE)
        if (entries.isEmpty()) {
            Text(
                WorkflowView.MERGE_TRAIN_EMPTY,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier
                    .padding(horizontal = 12.dp, vertical = 6.dp)
                    .testTag("workflow-train-empty"),
            )
            return@Column
        }
        entries.forEach { entry ->
            val node = nodesById[entry.id] ?: return@forEach
            val issue = graph.issuesById[node.issueId]
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .flatRow()
                    .clickable { onSelectNode(node) }
                    .padding(
                        horizontal = GlassTokens.RowPaddingH,
                        vertical = GlassTokens.RowPaddingV,
                    )
                    .testTag("workflow-train-row"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                WorkflowStateGlyph(node.state)
                Spacer(Modifier.width(8.dp))
                Text(
                    issue?.let { WorkflowView.nodeTitle(it.identifier, node.memberIssueIds.size) }
                        ?: WorkflowView.nodeKindLabel(node.kind),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    WorkflowView.trainStepLabel(entry.step),
                    style = MaterialTheme.typography.labelSmall,
                    // Amber means "a person is needed" here too.
                    color = if (entry.step == WorkflowView.TrainStep.NeedsApproval) {
                        workflowToneColor(WorkflowView.Tone.Amber)
                    } else {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                    },
                )
            }
        }
    }
}

/**
 * EXP-984: the reviewer agent's latest verdict on one node — the shared line
 * ([WorkflowView.reviewLine]) in the verdict's tone, the findings as plain
 * text folded behind Show more when they run long, and the oracle command in
 * mono: the check the reviewer actually RAN is the evidence its prose is not.
 */
@Composable
private fun WorkflowReviewBlock(review: WorkflowNodeReview) {
    var expanded by remember(review) { mutableStateOf(false) }
    val findings = review.findings.trim()
    val folded = findings.length > REVIEW_FINDINGS_CHARS ||
        findings.count { it == '\n' } >= REVIEW_FINDINGS_LINES
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .testTag("workflow-node-review"),
    ) {
        Text(
            WorkflowView.AGENT_REVIEW_TITLE,
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Text(
            WorkflowView.reviewLine(review.line),
            style = MaterialTheme.typography.labelSmall,
            color = if (review.verdict == DomainContract.wfReviewVerdictApprove) {
                DesignTokens.Semantic.Green
            } else {
                MaterialTheme.colorScheme.error
            },
            modifier = Modifier.padding(top = 2.dp).testTag("workflow-node-review-line"),
        )
        if (findings.isNotEmpty()) {
            Text(
                findings,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = if (folded && !expanded) REVIEW_FINDINGS_LINES else Int.MAX_VALUE,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 4.dp),
            )
            if (folded) {
                Text(
                    if (expanded) "Show less" else "Show more",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(
                        alpha = TextEmphasis.Tertiary,
                    ),
                    modifier = Modifier
                        .clickable { expanded = !expanded }
                        .padding(top = 2.dp),
                )
            }
        }
        review.oracle?.let { oracle ->
            Text(
                oracle.command,
                style = MaterialTheme.typography.bodySmall.copy(
                    fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                ),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(top = 4.dp).testTag("workflow-node-review-oracle"),
            )
        }
    }
}

/** The findings fold, the same shape the agent feed's cards use (EXP-698). */
private const val REVIEW_FINDINGS_CHARS = 600
private const val REVIEW_FINDINGS_LINES = 6

/**
 * EXP-984: the run's counters ([WorkflowView.metricRows]) as flat label/value
 * rows — the LAST section of a started workflow's detail. A draft has run
 * nothing, so the caller hands over an empty list and nothing is drawn.
 */
@Composable
private fun WorkflowMetricsSection(rows: List<WorkflowView.MetricRow>) {
    if (rows.isEmpty()) return
    Column(modifier = Modifier.fillMaxWidth().padding(top = 8.dp)) {
        SectionHeader(WorkflowView.METRICS_TITLE)
        rows.forEach { metric ->
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .flatRow()
                    .padding(
                        horizontal = GlassTokens.RowPaddingH,
                        vertical = GlassTokens.RowPaddingV,
                    )
                    .testTag("workflow-metric-row"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    metric.label,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    metric.value,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

/**
 * The start configuration, persisted with `workflows.update` a row at a time.
 * The same launch vocabulary every composer speaks (agent, model, subagent
 * model, effort, account), plus the two rules that belong to a workflow: how
 * many nodes may run at once, what review each node passes, and when a
 * dependent may start. Locked once the workflow leaves draft — the server
 * refuses the write, so the rows disable rather than bounce.
 */
@Composable
private fun HowItRunsSection(
    devices: List<SteerDevice>,
    device: SteerDevice?,
    launch: WorkflowLaunch,
    gate: String,
    startOn: String,
    enabled: Boolean,
    onDeviceChange: (String) -> Unit,
    onLaunchChange: ((WorkflowLaunch) -> WorkflowLaunch) -> Unit,
    onGateChange: (String) -> Unit,
    onStartOnChange: (String) -> Unit,
) {
    // An unset agent means "the runner's own default", which is claude
    // everywhere the vocabularies are checked.
    val agent = launch.agent.ifEmpty { DEFAULT_AGENT }
    OptionGroup {
        PickerRow(
            label = "Runner",
            value = device?.let(::deviceOptionLabel) ?: "Select",
            options = listOf("") + devices.map { it.deviceId },
            selected = device?.deviceId ?: "",
            optionLabel = { id ->
                if (id.isEmpty()) {
                    "No machine"
                } else {
                    devices.firstOrNull { it.deviceId == id }?.let(::deviceOptionLabel) ?: id
                }
            },
            enabled = enabled,
            onSelect = onDeviceChange,
        )
        GroupDivider()
        // EXP-872: ONE account row, not an Agent row plus an Account row — the
        // agent RIDES the picked login, so a workflow configures which login
        // its nodes spend and the agent follows. A machine with no login
        // reported (and the state before a runner is even picked) still offers
        // the ambient one per agent, so the block stays editable in draft.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "Account",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Quaternary,
                ),
            )
            Spacer(Modifier.weight(1f))
            AccountPickerPill(
                options = if (device == null) {
                    ambientAccountOptions(DomainContract.codingAgentValues, agent)
                } else {
                    accountOptionsFor(device, DomainContract.codingAgentValues)
                },
                selectedKey = "$agent:${launch.account.ifEmpty { SYSTEM_PROFILE_ID }}",
                // Locked once the workflow leaves draft, like every row here:
                // the server refuses the write, so the pill states the pick
                // instead of bouncing it.
                enabled = enabled,
                onSelect = { option ->
                    onLaunchChange { current ->
                        // Only an AGENT change invalidates the vocabularies
                        // below it; another login of the same agent leaves
                        // model, subagent model and effort exactly as picked.
                        val agentChanged =
                            option.agent != current.agent.ifEmpty { DEFAULT_AGENT }
                        // EXP-1002: the phase pins are models of the OLD
                        // agent too.
                        (if (agentChanged) current.withoutPhaseModels() else current).copy(
                            agent = option.agent,
                            // "" is the machine's ACTIVE login, never an id on
                            // the wire (`workflows.update` omits the key).
                            account = option.id.takeIf { it != SYSTEM_PROFILE_ID }.orEmpty(),
                            model = if (agentChanged) "" else current.model,
                            subagentModel = if (agentChanged) "" else current.subagentModel,
                            effort = if (agentChanged) "" else current.effort,
                        )
                    }
                },
            )
        }
        GroupDivider()
        PickerRow(
            label = "Model",
            value = modelLabel(launch.model),
            options = modelOptionsFor(agent),
            selected = launch.model,
            optionLabel = ::modelLabel,
            enabled = enabled,
            onSelect = { next -> onLaunchChange { it.copy(model = next) } },
        )
        if (supportsSubagentModel(agent)) {
            GroupDivider()
            PickerRow(
                label = SUBAGENT_MODEL_LABEL,
                value = subagentModelLabel(launch.subagentModel),
                options = subagentModelOptions(),
                selected = launch.subagentModel,
                optionLabel = ::subagentModelLabel,
                enabled = enabled,
                onSelect = { next -> onLaunchChange { it.copy(subagentModel = next) } },
            )
        }
        GroupDivider()
        PickerRow(
            label = if (agent == "codex") "Reasoning" else "Effort",
            value = effortLabel(launch.effort),
            options = listOf(CLI_DEFAULT_EFFORT) + effortValuesFor(agent),
            selected = launch.effort,
            optionLabel = ::effortLabel,
            enabled = enabled,
            onSelect = { next -> onLaunchChange { it.copy(effort = next) } },
        )
        GroupDivider()
        PickerRow(
            label = "Max parallel",
            value = launch.maxParallel.toString(),
            options = MAX_PARALLEL_OPTIONS,
            selected = launch.maxParallel.toString(),
            optionLabel = { it },
            enabled = enabled,
            onSelect = { next ->
                next.toIntOrNull()?.let { value -> onLaunchChange { it.copy(maxParallel = value) } }
            },
        )
        GroupDivider()
        PickerRow(
            label = "Gate",
            value = WorkflowView.gateLabel(gate),
            options = DomainContract.wfGateValues,
            selected = gate,
            optionLabel = WorkflowView::gateLabel,
            enabled = enabled,
            onSelect = onGateChange,
        )
        // EXP-984: only an AGENT gate reviews anything, so the model it
        // reviews on is only a question there. "" = the engine picks one.
        if (gate == DomainContract.wfGateAgent) {
            GroupDivider()
            PickerRow(
                label = WorkflowView.REVIEW_MODEL_LABEL,
                value = subagentModelLabel(launch.reviewModel),
                options = subagentModelOptions(),
                selected = launch.reviewModel,
                optionLabel = ::subagentModelLabel,
                enabled = enabled,
                onSelect = { next -> onLaunchChange { it.copy(reviewModel = next) } },
            )
        }
        GroupDivider()
        PickerRow(
            label = "Start",
            value = WorkflowView.startOnLabel(startOn),
            options = DomainContract.wfStartOnValues,
            selected = startOn,
            optionLabel = WorkflowView::startOnLabel,
            enabled = enabled,
            onSelect = onStartOnChange,
        )
    }
}

/** 1 to 8 — the server's `WORKFLOW_MAX_PARALLEL_CAP`, default 3. */
private val MAX_PARALLEL_OPTIONS: List<String> = (1..8).map { it.toString() }

/**
 * One node, opened from the graph: what it is, what it covers, the two picks
 * the plan carries (Kind, Risk), the `touches` globs it declared, and the way
 * out to the issue itself. Once the workflow is running (EXP-982) it is also
 * where a person gives their verdict: approve the node's PR for the merge
 * train, or unstick a failed one with Retry / Skip.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun WorkflowNodeSheet(
    node: WorkflowNodeEntity,
    graph: WorkflowGraph,
    workflowStatus: String,
    /** The workflow's review gate — what [WorkflowView.nodeNeedsApproval] reads. */
    gate: String,
    editable: Boolean,
    busy: Boolean,
    onKindChange: (String) -> Unit,
    onRiskChange: (String) -> Unit,
    onApprove: (Boolean) -> Unit,
    onResolve: (String) -> Unit,
    /** EXP-984: `workflows.admitNode` — take the proposal, or throw it away. */
    onAdmit: (Boolean) -> Unit,
    onOpenIssue: (String) -> Unit,
    onOpenSession: (String) -> Unit,
    onOpenChanges: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var confirmSkip by remember { mutableStateOf(false) }
    val issue = graph.issuesById[node.issueId]
    val caption = remember(node, workflowStatus) {
        WorkflowView.nodeCaption(node.captionNode, workflowStatus)
    }
    // EXP-984: a proposal is not part of the run, so the run's own verdicts
    // (approve, retry, skip) make no sense on it — Admit / Dismiss do.
    val isProposed = node.state == DomainContract.wfNodeStateProposed
    val review = remember(node.review) { workflowNodeReview(node.review) }
    GlassSheet(title = issue?.identifier ?: "Node", onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .testTag("workflow-node-sheet"),
        ) {
            if (issue != null) {
                // The BADGE is the way into the issue — tapping the subject
                // opens it, so the sheet needs no "Open issue" pill.
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onOpenIssue(issue.id) }
                        .padding(horizontal = 16.dp)
                        .testTag("workflow-node-issue"),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    StatusIcon(IssueStatus.fromWire(issue.status), size = 14.dp)
                    Spacer(Modifier.width(8.dp))
                    IssueChip(
                        identifier = WorkflowView.nodeTitle(
                            issue.identifier,
                            node.memberIssueIds.size,
                        ),
                        title = null,
                        status = null,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        issue.title,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Row(
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (workflowStatus != DomainContract.wfStatusDraft &&
                    workflowStateHasGlyph(node.state)
                ) {
                    WorkflowStateGlyph(node.state)
                    Spacer(Modifier.width(6.dp))
                }
                Text(
                    caption,
                    style = MaterialTheme.typography.labelSmall,
                    color = workflowToneColor(WorkflowView.nodeTone(node.state)),
                )
            }
            // EXP-984: a follow-up somebody filed DURING the run that was not
            // plainly additive. It is drawn in the graph but is not part of
            // the run until a member admits it.
            if (isProposed) {
                Text(
                    WorkflowView.PROPOSED_NODE_NOTE,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier
                        .padding(horizontal = 16.dp)
                        .testTag("workflow-node-proposed-note"),
                )
            }
            // EXP-982: why the engine stopped here — its own sentence, shown
            // verbatim rather than translated into a state word.
            node.note?.takeIf { it.isNotBlank() }?.let { note ->
                Text(
                    note,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier
                        .padding(horizontal = 16.dp)
                        .testTag("workflow-node-sheet-note"),
                )
            }
            // EXP-983: the stamp a dependent of this node starts on when the
            // workflow starts `on contract` — the node has said what it will
            // build, long before its PR is up.
            node.checkpointAt?.takeIf { it.isNotBlank() }?.let { stamp ->
                Text(
                    "${WorkflowView.CONTRACT_PUBLISHED_LABEL} · ${relativeTime(stamp)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier
                        .padding(horizontal = 16.dp)
                        .testTag("workflow-node-checkpoint"),
                )
            }
            // EXP-983: the engine's serialization edges — two siblings' work
            // collided, so this node merges theirs in before it pushes.
            if (node.afterNodeIds.isNotEmpty()) {
                Text(
                    "Merges in first",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .padding(start = 16.dp, end = 16.dp, top = 6.dp)
                        .testTag("workflow-node-serial"),
                )
                FlowRow(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    node.afterNodeIds.forEach { nodeId ->
                        val from = graph.nodes.firstOrNull { it.id == nodeId } ?: return@forEach
                        val issue = graph.issuesById[from.issueId] ?: return@forEach
                        IssueChip(
                            identifier = WorkflowView.nodeTitle(
                                issue.identifier,
                                from.memberIssueIds.size,
                            ),
                            title = issue.title,
                            status = null,
                            onClick = { onOpenIssue(issue.id) },
                        )
                    }
                }
            }
            // A compound node names the sub-issues it runs as one batch.
            if (node.memberIssueIds.isNotEmpty()) {
                FlowRow(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    node.memberIssueIds.forEach { memberId ->
                        val member = graph.issuesById[memberId] ?: return@forEach
                        IssueChip(
                            identifier = member.identifier,
                            title = member.title,
                            status = null,
                            onClick = { onOpenIssue(member.id) },
                        )
                    }
                }
            }
            // EXP-984: the reviewer agent's latest verdict. An approval counts
            // as THE approval only with a passing oracle (and never on the
            // contract node); otherwise it is advisory and a person still
            // approves — which is exactly what the line says.
            review?.let { WorkflowReviewBlock(it) }
            Spacer(Modifier.height(8.dp))
            OptionGroup {
                PickerRow(
                    label = "Kind",
                    value = WorkflowView.nodeKindLabel(node.kind),
                    options = DomainContract.wfNodeKindValues,
                    selected = node.kind,
                    optionLabel = WorkflowView::nodeKindLabel,
                    enabled = editable,
                    onSelect = onKindChange,
                )
                GroupDivider()
                PickerRow(
                    label = "Risk",
                    value = WorkflowView.riskLabel(node.risk),
                    options = DomainContract.wfRiskValues,
                    selected = node.risk,
                    optionLabel = WorkflowView::riskLabel,
                    // Risk stays adjustable after the draft (server parity).
                    enabled = true,
                    onSelect = onRiskChange,
                )
            }
            // What the plan says this node changes — read-only, mono, one per
            // line: they are globs, not prose.
            if (node.touches.isNotEmpty()) {
                Spacer(Modifier.height(12.dp))
                Text(
                    "Touches",
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.padding(horizontal = 16.dp),
                )
                node.touches.forEach { glob ->
                    Text(
                        glob,
                        style = MaterialTheme.typography.bodySmall.copy(
                            fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                        ),
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Secondary,
                        ),
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 1.dp),
                    )
                }
            }
            Spacer(Modifier.height(12.dp))
            FlowRow(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                // EXP-984: a proposal's only two verdicts — take it into the
                // workflow, or throw the node away (never the issue).
                if (isProposed) {
                    GlassPill(
                        WorkflowView.ADMIT_NODE_LABEL,
                        primary = true,
                        icon = ExpIcons.uiCheck,
                        enabled = !busy,
                        onClick = { onAdmit(true) },
                        modifier = Modifier.testTag("workflow-node-admit"),
                    )
                    GlassPill(
                        WorkflowView.DISMISS_NODE_LABEL,
                        icon = ExpIcons.uiClose,
                        tint = MaterialTheme.colorScheme.error,
                        enabled = !busy,
                        onClick = { onAdmit(false) },
                        modifier = Modifier.testTag("workflow-node-dismiss"),
                    )
                }
                // The gate, while this node's PR is up and nobody cleared it.
                if (!isProposed &&
                    node.state == DomainContract.wfNodeStateInReview &&
                    WorkflowView.nodeNeedsApproval(gate, node.kind) &&
                    node.approvedAt.isNullOrEmpty()
                ) {
                    GlassPill(
                        WorkflowView.APPROVE_NODE_LABEL,
                        primary = true,
                        icon = ExpIcons.uiCheck,
                        enabled = !busy,
                        onClick = { onApprove(true) },
                        modifier = Modifier.testTag("workflow-node-approve"),
                    )
                }
                // Taking it back is possible right up to the moment it lands.
                if (!isProposed &&
                    !node.approvedAt.isNullOrEmpty() &&
                    node.state != DomainContract.wfNodeStateLanded
                ) {
                    GlassPill(
                        WorkflowView.WITHDRAW_APPROVAL_LABEL,
                        icon = ExpIcons.uiUndo,
                        enabled = !busy,
                        onClick = { onApprove(false) },
                        modifier = Modifier.testTag("workflow-node-withdraw"),
                    )
                }
                // A failed node's two ways out.
                if (!isProposed && node.state == DomainContract.wfNodeStateFailed) {
                    GlassPill(
                        WorkflowView.RETRY_NODE_LABEL,
                        icon = ExpIcons.uiRefresh,
                        enabled = !busy,
                        onClick = { onResolve(WorkflowsApi.NODE_RETRY) },
                        modifier = Modifier.testTag("workflow-node-retry"),
                    )
                    GlassPill(
                        WorkflowView.SKIP_NODE_LABEL,
                        icon = ExpIcons.uiClose,
                        tint = MaterialTheme.colorScheme.error,
                        enabled = !busy,
                        onClick = { confirmSkip = true },
                        modifier = Modifier.testTag("workflow-node-skip"),
                    )
                }
                // The node's coding run, opened where every run is steered.
                node.sessionId?.takeIf { it.isNotBlank() }?.let { sessionId ->
                    GlassPill(
                        "Open run",
                        icon = ExpIcons.navActions,
                        onClick = { onOpenSession(sessionId) },
                        modifier = Modifier.testTag("workflow-node-run"),
                    )
                }
                // The node's pull request, on the issue detail's Changes page.
                if (issue != null && !issue.prUrl.isNullOrBlank()) {
                    GlassPill(
                        issue.prNumber?.let { "PR #$it" } ?: "Pull request",
                        icon = ExpIcons.prOpen,
                        onClick = { onOpenChanges(issue.id) },
                        modifier = Modifier.testTag("workflow-node-pr"),
                    )
                }
            }
            Spacer(Modifier.height(16.dp))
        }
    }

    if (confirmSkip) {
        AlertDialog(
            onDismissRequest = { confirmSkip = false },
            title = { Text(WorkflowView.SKIP_NODE_LABEL) },
            text = { Text(WorkflowView.SKIP_NODE_CONFIRM) },
            confirmButton = {
                TextButton(onClick = {
                    confirmSkip = false
                    onResolve(WorkflowsApi.NODE_SKIP)
                }) {
                    Text(WorkflowView.SKIP_NODE_LABEL, color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmSkip = false }) { Text("Cancel") }
            },
        )
    }
}
