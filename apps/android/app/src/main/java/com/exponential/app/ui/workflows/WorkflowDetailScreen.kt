package com.exponential.app.ui.workflows

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.WorkflowsApi
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssueNesting
import com.exponential.app.domain.NodeChip
import com.exponential.app.domain.NodeChipAction
import com.exponential.app.domain.SessionTreeContext
import com.exponential.app.domain.SessionTreeNode
import com.exponential.app.domain.SwitcherMode
import com.exponential.app.domain.SwitcherTarget
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.domain.WorkflowNodeDisplayState
import com.exponential.app.domain.WorkflowOpenQuestion
import com.exponential.app.domain.WorkflowPrimaryAction
import com.exponential.app.domain.WorkflowSelection
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.availableFaces
import com.exponential.app.domain.coveredIssueIds
import com.exponential.app.domain.fallbackFace
import com.exponential.app.domain.groupSessionResults
import com.exponential.app.domain.parseSessionResults
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.domain.sessionTree
import com.exponential.app.domain.switcherMode
import com.exponential.app.domain.switcherTargets
import com.exponential.app.domain.visibleSessionTreeRows
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.FloatingBarCluster
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassNotice
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.IssueChipStack
import com.exponential.app.ui.components.IssueGraphPopover
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.components.WorkflowEventList
import com.exponential.app.ui.components.deviceOptionLabel
import com.exponential.app.ui.components.picker.DevicePicker
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.ChangesLoadState
import com.exponential.app.ui.issue.ChangesViewModel
import com.exponential.app.ui.issue.CommentThreadViewModel
import com.exponential.app.ui.issue.IssueDetailViewModel
import com.exponential.app.ui.issue.IssueFace
import com.exponential.app.ui.issue.IssueRow
import com.exponential.app.ui.issue.LiveDot
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.rememberIssueFaceController
import com.exponential.app.ui.issue.toDiffFile
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.session.AgentSessionViewModel
import com.exponential.app.ui.session.RunFace
import com.exponential.app.ui.session.RunningSessionRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.work.ChangesFace
import com.exponential.app.ui.work.ChangesMergeControl
import com.exponential.app.ui.work.FaceSwitcher
import com.exponential.app.ui.work.GithubHeaderAction
import com.exponential.app.ui.work.ResultsFace
import kotlinx.coroutines.flow.MutableStateFlow

/**
 * EXP-1087: ONE workflow on the phone. The header carries the name, the ONE
 * primary action ([WorkflowView.primaryAction]) and an overflow (Stop,
 * Delete), the caption line ([WorkflowView.headerCaption]), the open question
 * while there is one, and the node STRIP — `All` first, then one row per
 * wave. Under it the Work screen's faces behind the existing switcher: one
 * node = that issue's Issue / Run / Changes / Results in place; All = the
 * workflow's issues nested, its runs as a tree, the final pull request and
 * every run's screenshots. Tap selects a chip; long-press opens the
 * mini-graph and, on a failed or proposed node, its verdicts.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkflowDetailScreen(
    onBack: () -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    /** A run that belongs to no node (the planner), opened the usual way. */
    onOpenSession: (sessionId: String) -> Unit,
    /** The standalone Changes route — for a PR the face cannot show. */
    onOpenChanges: (issueId: String) -> Unit,
    /** `review_final_pr`: the Reviews screen. */
    onOpenReviews: () -> Unit,
    viewModel: WorkflowDetailViewModel = hiltViewModel(),
) {
    val workflow by viewModel.workflow.collectAsStateWithLifecycle()
    val graph by viewModel.graph.collectAsStateWithLifecycle()
    val strip by viewModel.strip.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val device by viewModel.device.collectAsStateWithLifecycle()
    val headerNodes by viewModel.headerNodes.collectAsStateWithLifecycle()
    val questions by viewModel.openQuestions.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val error by viewModel.error.collectAsStateWithLifecycle()
    val deleted by viewModel.deleted.collectAsStateWithLifecycle()

    LaunchedEffect(deleted) { if (deleted) onBack() }

    var selectedId by rememberSaveable { mutableStateOf<String?>(null) }
    var faceName by rememberSaveable { mutableStateOf<String?>(null) }
    var menuOpen by remember { mutableStateOf(false) }
    var pickerOpen by remember { mutableStateOf(false) }
    var confirmStop by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var sheetNodeId by remember { mutableStateOf<String?>(null) }
    val snackbarHostState = remember { SnackbarHostState() }

    val order = remember(strip) { WorkflowSelection.order(strip) }
    val selection = WorkflowSelection(selectedId).reconcile(order)
    // A node that left the workflow falls back to All — once the strip exists.
    LaunchedEffect(order) {
        if (order.isNotEmpty() && selection.nodeId != selectedId) selectedId = selection.nodeId
    }
    val selectNode: (String?) -> Unit = { id ->
        if (id != selectedId) faceName = null
        selectedId = id
    }
    val nodeOfIssue: (String) -> WorkflowNodeEntity? = { issueId ->
        graph.nodes.firstOrNull { issueId in it.coveredIssueIds }
    }

    val row = workflow
    val deviceLabel = device?.let(::deviceOptionLabel) ?: row?.deviceId?.take(8)
    val primary = row?.let { WorkflowView.primaryAction(it.status, deviceLabel) }
    val finalPrUrl = row?.finalPrUrl?.takeIf { it.isNotBlank() }
    val selectedNode = selection.nodeId?.let { id -> graph.nodes.firstOrNull { it.id == id } }
    val allFace = WorkFaceKind.entries.firstOrNull { it.name == faceName }

    ProvideMarkdownToolbar {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                Column(modifier = Modifier.fillMaxWidth()) {
                    CenterAlignedTopAppBar(
                        title = {
                            Text(
                                row?.name.orEmpty(),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        },
                        navigationIcon = { TopBarBackButton(onClick = onBack) },
                        actions = {
                            if (selectedNode == null && allFace == WorkFaceKind.Changes && finalPrUrl != null) {
                                GithubHeaderAction(finalPrUrl)
                            }
                            primary?.let { action ->
                                PrimaryActionPill(
                                    action = action,
                                    enabled = !busy,
                                    onClick = {
                                        when (action) {
                                            WorkflowPrimaryAction.PICK_DEVICE -> pickerOpen = true
                                            WorkflowPrimaryAction.START -> viewModel.start()
                                            WorkflowPrimaryAction.PAUSE -> viewModel.pause()
                                            WorkflowPrimaryAction.RESUME -> viewModel.resume()
                                            WorkflowPrimaryAction.REVIEW_FINAL_PR -> onOpenReviews()
                                        }
                                    },
                                )
                            }
                            if (row != null) {
                                Box {
                                    CircleIconButton(
                                        ExpIcons.uiMore,
                                        "Workflow actions",
                                        onClick = { menuOpen = true },
                                        modifier = Modifier.padding(end = 8.dp).testTag("workflow-overflow"),
                                        borderless = true,
                                    )
                                    GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                                        if (row.status == DomainContract.wfStatusRunning ||
                                            row.status == DomainContract.wfStatusPaused
                                        ) {
                                            GlassMenuItem(
                                                leadingIcon = { Icon(ExpIcons.uiStop, contentDescription = null) },
                                                text = { Text(STOP_WORKFLOW_LABEL) },
                                                destructive = true,
                                                onClick = {
                                                    menuOpen = false
                                                    confirmStop = true
                                                },
                                            )
                                        } else {
                                            GlassMenuItem(
                                                leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                                                text = { Text(WorkflowView.DELETE_WORKFLOW_LABEL) },
                                                destructive = true,
                                                onClick = {
                                                    menuOpen = false
                                                    confirmDelete = true
                                                },
                                            )
                                        }
                                    }
                                }
                            }
                        },
                        colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
                    )
                    if (row != null) {
                        Text(
                            WorkflowView.headerCaption(row.status, headerNodes, deviceLabel),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            modifier = Modifier.padding(horizontal = 16.dp).testTag("workflow-caption"),
                        )
                    }
                    error?.let { message ->
                        GlassNotice(
                            text = message,
                            contentColor = MaterialTheme.colorScheme.error,
                            onClick = viewModel::clearError,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 16.dp, vertical = 4.dp)
                                .testTag("workflow-error"),
                        )
                    }
                    questions.firstOrNull()?.let { question ->
                        key(question.sessionId) {
                            QuestionBanner(
                                question = question,
                                title = graph.nodes.firstOrNull { it.id == question.nodeId }
                                    ?.let { node -> graph.issuesById[node.issueId]?.identifier },
                                onSelect = { selectNode(question.nodeId) },
                            )
                        }
                    }
                    NodeStrip(
                        strip = strip,
                        graph = graph,
                        selectedId = selection.nodeId,
                        onSelectAll = { selectNode(null) },
                        onSelect = { id -> selectNode(WorkflowSelection(selectedId).toggle(id).nodeId) },
                        onLongPress = { id -> sheetNodeId = id },
                    )
                }
            },
            snackbarHost = { SnackbarHost(snackbarHostState) },
        ) { padding ->
            if (row == null) {
                Text(
                    "Syncing…",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier.padding(padding).padding(16.dp),
                )
                return@Scaffold
            }
            if (selectedNode != null) {
                key(selectedNode.id) {
                    NodeFaces(
                        issueId = selectedNode.issueId,
                        sessionId = graph.runsByNodeId[selectedNode.id]?.sessionId,
                        faceName = faceName,
                        onFace = { faceName = it.name },
                        padding = padding,
                        snackbarHostState = snackbarHostState,
                        onClose = { selectNode(null) },
                        onOpenIssue = onOpenIssue,
                        onOpenChanges = onOpenChanges,
                    )
                }
            } else {
                AllFaces(
                    viewModel = viewModel,
                    graph = graph,
                    finalPrOpen = finalPrUrl != null && row.finalPrState == DomainContract.prStateOpen,
                    faceName = faceName,
                    onFace = { faceName = it.name },
                    padding = padding,
                    busy = busy,
                    onSelectIssue = { issueId ->
                        val node = nodeOfIssue(issueId)
                        if (node != null) selectNode(node.id) else onOpenIssue(issueId)
                    },
                    onSelectRun = { session ->
                        val node = graph.nodes.firstOrNull {
                            it.id == session.workflowNodeId || it.sessionId == session.id
                        }
                        if (node != null) {
                            selectNode(node.id)
                            faceName = WorkFaceKind.Run.name
                        } else {
                            onOpenSession(session.id)
                        }
                    },
                )
            }
        }
    }

    // `pick_device`: the shared picker over the caller's own and the team's
    // shared runners that are online right now.
    DevicePicker(
        devices = devices.filter { it.online }.map { row ->
            DevicePickerDevice(
                id = row.deviceId,
                name = deviceOptionLabel(row),
                icon = row.icon,
                isServer = row.isServer,
            )
        },
        value = device?.deviceId,
        onChange = { id ->
            pickerOpen = false
            viewModel.setDevice(id)
        },
        open = pickerOpen,
        onOpenChange = { pickerOpen = it },
    )

    sheetNodeId?.let { id ->
        val node = graph.nodes.firstOrNull { it.id == id }
        val chip = strip.flatMap { it.nodes }.firstOrNull { it.id == id }
        if (node == null || chip == null) {
            sheetNodeId = null
        } else {
            NodeSheet(
                node = node,
                chip = chip,
                viewModel = viewModel,
                graph = graph,
                busy = busy,
                onOpenIssue = { issueId ->
                    sheetNodeId = null
                    val target = nodeOfIssue(issueId)
                    if (target != null) selectNode(target.id) else onOpenIssue(issueId)
                },
                onDismiss = { sheetNodeId = null },
            )
        }
    }

    if (confirmStop) {
        AlertDialog(
            onDismissRequest = { confirmStop = false },
            title = { Text(STOP_WORKFLOW_LABEL) },
            text = { Text(WorkflowView.CANCEL_WORKFLOW_CONFIRM) },
            confirmButton = {
                TextButton(onClick = {
                    confirmStop = false
                    viewModel.cancel()
                }) { Text(STOP_WORKFLOW_LABEL, color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirmStop = false }) { Text("Cancel") } },
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
                }) { Text("Delete", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
}

/** The overflow's word for ending a live workflow — the ONE ending verb ×4. */
private const val STOP_WORKFLOW_LABEL = "Stop"
private const val ALL_CHIP_LABEL = "All"
private const val PICK_DEVICE_LABEL = "Pick device"
private const val REVIEW_FINAL_PR_LABEL = "Review"

@Composable
private fun PrimaryActionPill(action: WorkflowPrimaryAction, enabled: Boolean, onClick: () -> Unit) {
    val (label, icon) = when (action) {
        WorkflowPrimaryAction.PICK_DEVICE -> PICK_DEVICE_LABEL to ExpIcons.uiDevice
        WorkflowPrimaryAction.START -> WorkflowView.START_WORKFLOW_LABEL to ExpIcons.actionRun
        WorkflowPrimaryAction.PAUSE -> WorkflowView.PAUSE_WORKFLOW_LABEL to ExpIcons.uiStop
        WorkflowPrimaryAction.RESUME -> WorkflowView.RESUME_WORKFLOW_LABEL to ExpIcons.actionRun
        WorkflowPrimaryAction.REVIEW_FINAL_PR -> REVIEW_FINAL_PR_LABEL to ExpIcons.navReviews
    }
    GlassPill(
        label,
        size = PillSize.Sm,
        primary = true,
        icon = icon,
        enabled = enabled,
        onClick = onClick,
        modifier = Modifier.testTag("workflow-primary-${action.wire}"),
    )
}

/**
 * The open question, ONLY while one is open: the asking node, the question,
 * and an inline answer that goes to the run as a message — the same path a
 * `needs_input` run is answered through from its Run face.
 */
@Composable
private fun QuestionBanner(question: WorkflowOpenQuestion, title: String?, onSelect: () -> Unit) {
    val sessionVm = hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
        key = "session:${question.sessionId}",
    ) { factory -> factory.create(question.sessionId) }
    var answer by rememberSaveable(question.sessionId, question.askedAt) { mutableStateOf("") }
    var failed by remember { mutableStateOf(false) }
    LaunchedEffect(sessionVm) { sessionVm.ensureConnected() }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 6.dp)
            .testTag("workflow-question"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        GlassNotice(
            text = listOfNotNull(title, question.question).joinToString(" · "),
            contentColor = NeedsInputAmber,
            onClick = onSelect,
            modifier = Modifier.fillMaxWidth(),
            leading = { StaticDot(NeedsInputAmber) },
        )
        GlassTextField(
            value = answer,
            onValueChange = {
                answer = it
                failed = false
            },
            placeholder = if (failed) "Not sent. The run is not reachable." else "Answer",
            singleLine = true,
            modifier = Modifier.fillMaxWidth().testTag("workflow-question-answer"),
            trailingIcon = {
                CircleIconButton(
                    ExpIcons.uiSend,
                    "Send",
                    enabled = answer.isNotBlank(),
                    borderless = true,
                    onClick = {
                        if (sessionVm.sendCommand(answer.trim())) answer = "" else failed = true
                    },
                )
            },
        )
    }
}

/** The strip: `All`, then one row per wave, scrolling sideways. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun NodeStrip(
    strip: List<com.exponential.app.domain.StripWave>,
    graph: WorkflowGraph,
    selectedId: String?,
    onSelectAll: () -> Unit,
    onSelect: (String) -> Unit,
    onLongPress: (String) -> Unit,
) {
    val nodesById = remember(graph.nodes) { graph.nodes.associateBy { it.id } }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(max = 180.dp)
            .verticalScroll(rememberScrollState())
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .testTag("workflow-strip"),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        GlassPill(
            ALL_CHIP_LABEL,
            size = PillSize.Sm,
            selected = selectedId == null,
            onClick = onSelectAll,
            modifier = Modifier.testTag("workflow-chip-all"),
        )
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            strip.forEach { wave ->
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    wave.nodes.forEach { chip ->
                        val node = nodesById[chip.id]
                        StripChip(
                            chip = chip,
                            status = node?.let { graph.statusByIssueId[it.issueId] },
                            selected = chip.id == selectedId,
                            modifier = Modifier.combinedClickable(
                                onClick = { onSelect(chip.id) },
                                onLongClick = { onLongPress(chip.id) },
                            ),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun StripChip(
    chip: NodeChip,
    status: com.exponential.app.domain.ResolvedIssueStatus?,
    selected: Boolean,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(6.dp)
    Row(
        modifier = Modifier.testTag("workflow-chip-${chip.id}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        val body: @Composable () -> Unit = {
            IssueChip(
                identifier = chip.title,
                title = chip.caption,
                status = status,
                modifier = modifier.then(
                    if (selected) {
                        Modifier.border(1.dp, MaterialTheme.colorScheme.onSurface, shape)
                    } else {
                        Modifier
                    },
                ),
                leading = { NodeGlyph(chip, status) },
            )
        }
        if (chip.stacked) IssueChipStack(ghosts = chip.members.coerceAtMost(2)) { body() } else body()
        if (chip.needsYou) {
            Box(Modifier.testTag("workflow-chip-needs-you")) { StaticDot(NeedsInputAmber) }
        }
    }
}

/** The node's display state by SHAPE: a live dot (pulsing while the agent is
 *  mid-turn), a merged glyph, an error, a skip — a queued node wears its
 *  issue's own status glyph. */
@Composable
private fun NodeGlyph(chip: NodeChip, status: com.exponential.app.domain.ResolvedIssueStatus?) {
    val icon = when (chip.display) {
        WorkflowNodeDisplayState.RUNNING -> {
            Box(Modifier.size(14.dp), contentAlignment = Alignment.Center) { LiveDot(busy = chip.live) }
            return
        }
        WorkflowNodeDisplayState.QUEUED -> {
            if (status != null) {
                StatusIcon(status, size = 14.dp)
                return
            }
            ExpIcons.uiQueued
        }
        WorkflowNodeDisplayState.DONE -> ExpIcons.notificationPrMerged
        WorkflowNodeDisplayState.FAILED -> ExpIcons.uiError
        WorkflowNodeDisplayState.SKIPPED -> ExpIcons.uiClose
    }
    Icon(
        icon,
        contentDescription = chip.display.label,
        modifier = Modifier.size(14.dp),
        tint = when (chip.display) {
            WorkflowNodeDisplayState.DONE -> DesignTokens.Semantic.Green
            WorkflowNodeDisplayState.FAILED -> MaterialTheme.colorScheme.error
            else -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        },
    )
}

/** Long-press: THE mini-graph around the node, and its verdicts when the
 *  state offers any ([WorkflowView.nodeChipMenu]). */
@Composable
private fun NodeSheet(
    node: WorkflowNodeEntity,
    chip: NodeChip,
    viewModel: WorkflowDetailViewModel,
    graph: WorkflowGraph,
    busy: Boolean,
    onOpenIssue: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val relations by viewModel.relations.collectAsStateWithLifecycle()
    var confirmSkip by remember { mutableStateOf(false) }
    val blocks = remember(node, relations, graph.issuesById) {
        IssueGraph.blockGraph(node.coveredIssueIds, relations, graph.issuesById.values.toList())
    }
    GlassSheet(title = chip.title, onDismiss = onDismiss) {
        IssueGraphPopover(
            graph = blocks,
            issuesById = graph.issuesById,
            onOpenIssue = onOpenIssue,
            modifier = Modifier.fillMaxWidth(),
        )
        WorkflowView.nodeChipMenu(node.state).forEach { action ->
            val label = when (action) {
                NodeChipAction.RETRY -> WorkflowView.RETRY_NODE_LABEL
                NodeChipAction.SKIP -> WorkflowView.SKIP_NODE_LABEL
                NodeChipAction.ADMIT -> WorkflowView.ADMIT_NODE_LABEL
                NodeChipAction.DISMISS -> WorkflowView.DISMISS_NODE_LABEL
            }
            GlassSheetRow(
                label = label,
                enabled = !busy,
                onClick = {
                    when (action) {
                        NodeChipAction.RETRY -> {
                            viewModel.resolveNode(node.id, WorkflowsApi.NODE_RETRY)
                            onDismiss()
                        }
                        NodeChipAction.SKIP -> confirmSkip = true
                        NodeChipAction.ADMIT -> {
                            viewModel.admitNode(node.id, admit = true)
                            onDismiss()
                        }
                        NodeChipAction.DISMISS -> {
                            viewModel.admitNode(node.id, admit = false)
                            onDismiss()
                        }
                    }
                },
            )
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
                    viewModel.resolveNode(node.id, WorkflowsApi.NODE_SKIP)
                    onDismiss()
                }) { Text(WorkflowView.SKIP_NODE_LABEL, color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirmSkip = false }) { Text("Cancel") } },
        )
    }
}

/** The switcher the Work screen uses, over [faces] — no run rows, no start. */
@Composable
private fun faceSwitcherSlot(
    faces: List<WorkFaceKind>,
    face: WorkFaceKind,
    diffStats: Diff.Totals?,
    onFace: (WorkFaceKind) -> Unit,
): @Composable () -> Unit {
    val mode = switcherMode(switcherTargets(faces, face, emptyList(), null, offerStart = false))
    return {
        if (mode !is SwitcherMode.Hidden) {
            FaceSwitcher(
                mode = mode,
                badge = null,
                badgeBusy = false,
                runs = emptyList(),
                shownRunId = null,
                diffStats = diffStats,
                onPick = { target -> if (target is SwitcherTarget.Face) onFace(target.face) },
            )
        }
    }
}

/** ONE node: its issue's Work screen faces, in place. */
@Composable
private fun NodeFaces(
    issueId: String,
    sessionId: String?,
    faceName: String?,
    onFace: (WorkFaceKind) -> Unit,
    padding: PaddingValues,
    snackbarHostState: SnackbarHostState,
    onClose: () -> Unit,
    onOpenIssue: (String) -> Unit,
    onOpenChanges: (String) -> Unit,
) {
    val issueVm = hiltViewModel<IssueDetailViewModel, IssueDetailViewModel.Factory>(
        key = "issue:$issueId",
    ) { factory -> factory.create(issueId) }
    val commentVm: CommentThreadViewModel = hiltViewModel(key = "comments:$issueId")
    val controller = rememberIssueFaceController()
    val issueState by issueVm.state.collectAsStateWithLifecycle()
    val issue = issueState?.issue

    val sessionVm: AgentSessionViewModel? = sessionId?.let { id ->
        hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(key = "session:$id") { factory ->
            factory.create(id)
        }
    }
    val session by (sessionVm?.session ?: remember { MutableStateFlow(null) }).collectAsStateWithLifecycle()
    val activity by (sessionVm?.activity ?: remember { MutableStateFlow(ActivityFeedState()) })
        .collectAsStateWithLifecycle()
    val parsedDiff = remember(activity.latestDiff) { activity.latestDiff?.let { Diff.parse(it) } }
    val prOpen = issue != null && issue.prState == DomainContract.prStateOpen && !issue.prUrl.isNullOrBlank()
    val hasChanges = parsedDiff != null || prOpen
    val changesVm: ChangesViewModel? = if (hasChanges && parsedDiff == null) {
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = "changes:$issueId") { factory ->
            factory.create(issueId)
        }
    } else {
        null
    }
    val prLoad by (changesVm?.load ?: remember { MutableStateFlow<ChangesLoadState?>(null) })
        .collectAsStateWithLifecycle()
    val files: List<Diff.File> = remember(parsedDiff, prLoad) {
        val load = prLoad
        when {
            parsedDiff != null -> parsedDiff.files
            load is ChangesLoadState.Loaded -> load.files.map { it.toDiffFile() }
            else -> emptyList()
        }
    }
    val diffStats = remember(files) { files.takeIf { it.isNotEmpty() }?.let { Diff.totals(it) } }
    val results = remember(session?.results) { groupSessionResults(parseSessionResults(session?.results)) }
    val faces = availableFaces(
        hasIssue = true,
        hasRun = sessionId != null,
        hasChanges = hasChanges,
        hasResults = results.isNotEmpty(),
    )
    val wanted = WorkFaceKind.entries.firstOrNull { it.name == faceName } ?: WorkFaceKind.Issue
    val face = fallbackFace(wanted, faces) ?: WorkFaceKind.Issue
    val trailing = faceSwitcherSlot(faces, face, diffStats, onFace)

    when (face) {
        WorkFaceKind.Issue -> IssueFace(
            viewModel = issueVm,
            commentViewModel = commentVm,
            controller = controller,
            padding = padding,
            snackbarHostState = snackbarHostState,
            onBack = onClose,
            onOpenIssue = onOpenIssue,
            onOpenChanges = {
                if (hasChanges) onFace(WorkFaceKind.Changes) else onOpenChanges(issueId)
            },
            trailingBarSlot = trailing,
        )
        WorkFaceKind.Run -> if (sessionVm != null) {
            RunFace(
                viewModel = sessionVm,
                padding = padding,
                onOpenIssue = onOpenIssue,
                trailingBarSlot = trailing,
            )
        }
        // A node's pull request lands through the workflow's engine, so its
        // Changes face shows the files and merges nothing.
        WorkFaceKind.Changes -> ChangesFace(
            padding = padding,
            files = files,
            prLoad = prLoad,
            merge = null,
            trailingBarSlot = trailing,
        )
        WorkFaceKind.Results -> ResultsFace(padding = padding, groups = results, trailingBarSlot = trailing)
    }
}

/** All: the workflow's issues, its runs, its final pull request, its screenshots. */
@Composable
private fun AllFaces(
    viewModel: WorkflowDetailViewModel,
    graph: WorkflowGraph,
    finalPrOpen: Boolean,
    faceName: String?,
    onFace: (WorkFaceKind) -> Unit,
    padding: PaddingValues,
    busy: Boolean,
    onSelectIssue: (String) -> Unit,
    onSelectRun: (CodingSessionEntity) -> Unit,
) {
    val sessions by viewModel.workflowSessions.collectAsStateWithLifecycle()
    val results by viewModel.results.collectAsStateWithLifecycle()
    val relations by viewModel.relations.collectAsStateWithLifecycle()
    val events by viewModel.events.collectAsStateWithLifecycle()
    val deviceRows by viewModel.deviceRows.collectAsStateWithLifecycle()
    val workflow by viewModel.workflow.collectAsStateWithLifecycle()
    var collapsed by rememberSaveable { mutableStateOf(emptySet<String>()) }
    val hasFinalPr = !workflow?.finalPrUrl.isNullOrBlank()

    val faces = availableFaces(
        hasIssue = true,
        hasRun = sessions.isNotEmpty(),
        hasChanges = hasFinalPr,
        hasResults = results.isNotEmpty(),
    )
    val wanted = WorkFaceKind.entries.firstOrNull { it.name == faceName } ?: WorkFaceKind.Issue
    val face = fallbackFace(wanted, faces) ?: WorkFaceKind.Issue
    val trailing = faceSwitcherSlot(faces, face, null, onFace)

    when (face) {
        WorkFaceKind.Issue -> {
            // The covered issues in strip order, sub-issues nested under their
            // parent (the ×4 nesting rule).
            val ids = remember(graph.nodes) { graph.nodes.flatMap { it.coveredIssueIds }.distinct() }
            val rows = remember(ids, relations, graph.issuesById) {
                IssueNesting.nestIssueRows(
                    groups = listOf(ids.filter { it in graph.issuesById }),
                    relations = relations,
                    identifierOf = { graph.issuesById[it]?.identifier ?: it },
                ).firstOrNull().orEmpty()
            }
            val guides = remember(rows) { TreeGuides.compute(rows.map { it.depth }) }
            FaceList(padding = padding, trailing = trailing, tag = "workflow-all-issues") {
                itemsIndexed(rows, key = { _, row -> row.id }) { index, row ->
                    val issue = graph.issuesById[row.id] ?: return@itemsIndexed
                    TreeGuidesRow(depth = row.depth, guide = guides.getOrNull(index)) {
                        IssueRow(
                            issue = issue,
                            labels = emptyList(),
                            assignee = null,
                            onClick = { onSelectIssue(issue.id) },
                            resolvedStatus = graph.statusByIssueId[issue.id],
                        )
                    }
                }
                item(key = "__events__") { WorkflowEventList(events, Modifier.fillMaxWidth()) }
            }
        }
        WorkFaceKind.Run -> {
            val nowMs = System.currentTimeMillis()
            val tree = remember(sessions, collapsed, graph.issuesById) {
                visibleSessionTreeRows(
                    sessionTree(sessions, SessionTreeContext(issues = graph.issuesById.values.toList())),
                    collapsed,
                )
            }
            val guides = remember(tree) { TreeGuides.compute(tree.map { it.depth }) }
            FaceList(padding = padding, trailing = trailing, tag = "workflow-all-runs") {
                itemsIndexed(tree, key = { _, entry -> entry.key }) { index, entry ->
                    val node = entry.node as? SessionTreeNode.Session ?: return@itemsIndexed
                    TreeGuidesRow(depth = entry.depth, guide = guides.getOrNull(index)) {
                        RunningSessionRow(
                            session = node.session,
                            issue = node.session.issueId?.let { graph.issuesById[it] },
                            device = resolveSessionDevice(node.session, deviceRows, nowMs),
                            onClick = { onSelectRun(node.session) },
                            expandable = entry.hasChildren,
                            expanded = entry.key !in collapsed,
                            onToggle = {
                                collapsed = if (entry.key in collapsed) collapsed - entry.key else collapsed + entry.key
                            },
                        )
                    }
                }
            }
        }
        WorkFaceKind.Changes -> ChangesFace(
            padding = padding,
            files = emptyList(),
            prLoad = null,
            merge = if (finalPrOpen) {
                ChangesMergeControl(
                    label = WorkflowView.MERGE_FINAL_PR_LABEL,
                    fixConflicts = false,
                    loading = busy,
                    error = null,
                    confirmText = WorkflowView.MERGE_FINAL_PR_CONFIRM,
                    onConfirm = viewModel::mergeFinalPr,
                    onFixConflicts = {},
                )
            } else {
                null
            },
            trailingBarSlot = trailing,
        )
        WorkFaceKind.Results -> ResultsFace(padding = padding, groups = results, trailingBarSlot = trailing)
    }
}

/** A face that is a plain list: the rows, and the Work screen's floating bar
 *  carrying the switcher. */
@Composable
private fun FaceList(
    padding: PaddingValues,
    trailing: @Composable () -> Unit,
    tag: String,
    content: androidx.compose.foundation.lazy.LazyListScope.() -> Unit,
) {
    Box(modifier = Modifier.padding(padding).fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().testTag(tag),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
            verticalArrangement = Arrangement.spacedBy(6.dp),
            content = content,
        )
        FloatingBarCluster(modifier = Modifier.align(Alignment.BottomCenter), right = trailing)
    }
}
