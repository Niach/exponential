package com.exponential.app.ui.workflows

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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.WorkflowLaunch
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.captionNode
import com.exponential.app.domain.shape
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
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.components.deviceOptionLabel
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelOptionsFor
import com.exponential.app.ui.components.subagentModelLabel
import com.exponential.app.ui.components.subagentModelOptions
import com.exponential.app.ui.components.supportsSubagentModel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-981: ONE workflow. P2 ships DRAFTS: look at the graph, configure how it
 * runs, hand it to an agent to plan, or delete it. There is no Start button —
 * the engine arrives with the next PR.
 *
 * The page reads top to bottom: the name (editable inline) with the shape line
 * and, when the plan holds one, the cycle note in the destructive tone; the
 * GRAPH as this phone's wave-grouped list ([WorkflowGraphList]); "How it runs";
 * then Plan and Delete workflow.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkflowDetailScreen(
    onBack: () -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    onOpenAgent: (AgentComposerSeed) -> Unit,
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

    // A delete pops back to the list; so does a workflow that stopped syncing
    // (someone else deleted it) once its row has actually been seen.
    LaunchedEffect(deleted) { if (deleted) onBack() }

    var selectedNode by remember { mutableStateOf<WorkflowNodeEntity?>(null) }
    var confirmDelete by remember { mutableStateOf(false) }
    // The name field is LOCAL while it is being typed; a blur writes it.
    var nameDraft by remember { mutableStateOf<String?>(null) }

    val row = workflow
    val shape = remember(row?.metrics) { row?.shape ?: WorkflowView.Shape() }
    val cycleNote = remember(shape) { WorkflowView.cycleNote(shape) }
    val isDraft = row?.status == DomainContract.wfStatusDraft

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
                )
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
            item(key = "__actions__") {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // EXP-981: Plan opens the Agent composer seeded with the
                    // hidden plan-workflow builtin AND this workflow's id; its
                    // free text ("Anything the plan should respect") stays
                    // optional.
                    GlassPill(
                        WorkflowView.PLAN_WORKFLOW_LABEL,
                        primary = true,
                        icon = ExpIcons.actionRun,
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
                    GlassPill(
                        WorkflowView.DELETE_WORKFLOW_LABEL,
                        icon = ExpIcons.uiDelete,
                        // Destructive, so it takes the error tone rather than
                        // the page's one primary fill.
                        tint = MaterialTheme.colorScheme.error,
                        onClick = { confirmDelete = true },
                        modifier = Modifier.testTag("workflow-delete"),
                    )
                }
            }
            item(key = "__bottom__") { Spacer(Modifier.height(24.dp)) }
        }
    }

    selectedNode?.let { node ->
        WorkflowNodeSheet(
            node = node,
            graph = graph,
            workflowStatus = row?.status.orEmpty(),
            editable = isDraft && !busy,
            onKindChange = { viewModel.updateNode(node.issueId, kind = it) },
            onRiskChange = { viewModel.updateNode(node.issueId, risk = it) },
            onOpenIssue = { issueId ->
                selectedNode = null
                onOpenIssue(issueId)
            },
            onDismiss = { selectedNode = null },
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
        PickerRow(
            label = "Agent",
            value = agentLabel(agent),
            options = DomainContract.codingAgentValues,
            selected = agent,
            optionLabel = ::agentLabel,
            enabled = enabled,
            onSelect = { next -> onLaunchChange { it.copy(agent = next, model = "", effort = "") } },
        )
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
        // The composer offers accounts only when the bound machine reports two
        // or more logins for the picked agent; a workflow follows that rule.
        val profiles = device?.agentAccounts?.get(agent)?.profiles.orEmpty()
        if (profiles.size >= 2) {
            GroupDivider()
            PickerRow(
                label = "Account",
                value = workflowAccountLabel(profiles.map { it.id to accountName(it) }, launch.account),
                // "" is the machine's ACTIVE login, never an id on the wire.
                options = listOf("") + profiles.map { it.id },
                selected = launch.account,
                optionLabel = { id ->
                    workflowAccountLabel(profiles.map { it.id to accountName(it) }, id)
                },
                enabled = enabled,
                onSelect = { next -> onLaunchChange { it.copy(account = next) } },
            )
        }
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

private fun accountName(profile: com.exponential.app.data.api.AgentAccountProfile): String =
    profile.email ?: profile.label?.trim()?.takeIf { it.isNotEmpty() } ?: profile.id

/** The active login reads as itself, never as an id (composer parity). */
private fun workflowAccountLabel(profiles: List<Pair<String, String>>, id: String): String =
    if (id.isEmpty()) {
        "Active login"
    } else {
        profiles.firstOrNull { it.first == id }?.second ?: id
    }

/**
 * One node, opened from the graph: what it is, what it covers, the two picks
 * the plan carries (Kind, Risk), the `touches` globs it declared, and the way
 * out to the issue itself.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun WorkflowNodeSheet(
    node: WorkflowNodeEntity,
    graph: WorkflowGraph,
    workflowStatus: String,
    editable: Boolean,
    onKindChange: (String) -> Unit,
    onRiskChange: (String) -> Unit,
    onOpenIssue: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val issue = graph.issuesById[node.issueId]
    val caption = remember(node, workflowStatus) {
        WorkflowView.nodeCaption(node.captionNode, workflowStatus)
    }
    GlassSheet(title = issue?.identifier ?: "Node", onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .testTag("workflow-node-sheet"),
        ) {
            if (issue != null) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
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
            Text(
                caption,
                style = MaterialTheme.typography.labelSmall,
                color = workflowToneColor(WorkflowView.nodeTone(node.state)),
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
            )
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
            if (issue != null) {
                Spacer(Modifier.height(12.dp))
                Row(modifier = Modifier.padding(horizontal = 16.dp)) {
                    GlassPill(
                        "Open issue",
                        icon = ExpIcons.navIssues,
                        onClick = { onOpenIssue(issue.id) },
                    )
                }
            }
            Spacer(Modifier.height(16.dp))
        }
    }
}
