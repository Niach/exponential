package com.exponential.app.ui.workflows

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.border
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.foundation.lazy.items
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.HiltViewModelFactory
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.HasDefaultViewModelProviderFactory
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.viewmodel.CreationExtras
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.WorkflowsApi
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentComposerSeed
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
import com.exponential.app.domain.WorkflowOverflowItem
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
import com.exponential.app.ui.components.BarSolidPill
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
 * primary action ([WorkflowView.primaryAction]) and the overflow
 * ([WorkflowView.overflowMenu]: Plan / Runs on / Delete, or Stop), the caption line ([WorkflowView.headerCaption]), the open question
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
    /** Plan: the Agent composer, seeded with the plan-workflow builtin. */
    onOpenAgent: (AgentComposerSeed) -> Unit,
    /** A run that belongs to no node (the planner), opened the usual way. */
    onOpenSession: (sessionId: String) -> Unit,
    /** The standalone Changes route — for a PR the face cannot show. */
    onOpenChanges: (issueId: String) -> Unit,
    viewModel: WorkflowDetailViewModel = hiltViewModel(),
) {
    val workflow by viewModel.workflow.collectAsStateWithLifecycle()
    val graph by viewModel.graph.collectAsStateWithLifecycle()
    val strip by viewModel.strip.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val device by viewModel.device.collectAsStateWithLifecycle()
    val headerNodes by viewModel.headerNodes.collectAsStateWithLifecycle()
    val questions by viewModel.openQuestions.collectAsStateWithLifecycle()
    val ownSessionIds by viewModel.ownSessionIds.collectAsStateWithLifecycle()
    val startNotice by viewModel.startNotice.collectAsStateWithLifecycle()
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
    // The phone picks ONE node or All; the rules are the ×4 picker model.
    val current = WorkflowSelection(listOfNotNull(selectedId), selectedId, selectedId)
    val selection = if (order.isEmpty()) current else current.prune(order)
    // A node that left the workflow falls back to All — once the strip exists.
    LaunchedEffect(order) {
        if (order.isNotEmpty() && selection.single != selectedId) selectedId = selection.single
    }
    // The face is the PAGE's: a pick or a step never resets it (EXP-1084).
    val selectNode: (String?) -> Unit = { id -> selectedId = id }
    val focusManager = LocalFocusManager.current
    var nameDraft by remember { mutableStateOf<String?>(null) }
    val nodeOfIssue: (String) -> WorkflowNodeEntity? = { issueId ->
        graph.nodes.firstOrNull { issueId in it.coveredIssueIds }
    }

    val row = workflow
    val deviceLabel = device?.let(::deviceOptionLabel) ?: row?.deviceId?.take(8)
    val primary = row?.let { WorkflowView.primaryAction(it.status, deviceLabel, it.finalPrState) }
    val finalPrUrl = row?.finalPrUrl?.takeIf { it.isNotBlank() }
    val selectedNode = selection.single?.let { id -> graph.nodes.firstOrNull { it.id == id } }
    val allFace = WorkFaceKind.entries.firstOrNull { it.name == faceName }

    ProvideMarkdownToolbar {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                Column(modifier = Modifier.fillMaxWidth()) {
                    CenterAlignedTopAppBar(
                        title = {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                // The workflow's status in the chip vocabulary
                                // (web `workflowStatusGlyph`, iOS `display(status:)`).
                                row?.let { wf ->
                                    Box(Modifier.testTag("workflow-status-glyph")) {
                                        DisplayGlyph(WorkflowView.statusGlyph(wf.status), live = false, status = null)
                                    }
                                }
                                // The name saves on Done or blur, like every
                                // other title field.
                                BasicTextField(
                                    value = nameDraft ?: row?.name.orEmpty(),
                                    onValueChange = { nameDraft = it },
                                    enabled = row != null,
                                    singleLine = true,
                                    textStyle = MaterialTheme.typography.titleMedium.copy(
                                        color = MaterialTheme.colorScheme.onSurface,
                                    ),
                                    cursorBrush = SolidColor(MaterialTheme.colorScheme.onSurface),
                                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                                    keyboardActions = KeyboardActions(onDone = { focusManager.clearFocus() }),
                                    modifier = Modifier
                                        .weight(1f, fill = false)
                                        .testTag("workflow-name")
                                        .onFocusChanged { state ->
                                            if (!state.isFocused) {
                                                nameDraft?.let(viewModel::rename)
                                                nameDraft = null
                                            }
                                        },
                                )
                            }
                        },
                        navigationIcon = { TopBarBackButton(onClick = onBack) },
                        actions = {
                            if (selectedNode == null && allFace == WorkFaceKind.Changes && finalPrUrl != null) {
                                GithubHeaderAction(finalPrUrl)
                            }
                            primary?.let { action ->
                                PrimaryActionPill(
                                    action = action,
                                    // Start stays off while the blocker notice
                                    // under the header says why.
                                    enabled = !busy &&
                                        !(action == WorkflowPrimaryAction.START && startNotice != null),
                                    onClick = {
                                        when (action) {
                                            WorkflowPrimaryAction.PICK_DEVICE -> pickerOpen = true
                                            WorkflowPrimaryAction.START -> viewModel.start()
                                            WorkflowPrimaryAction.PAUSE -> viewModel.pause()
                                            WorkflowPrimaryAction.RESUME -> viewModel.resume()
                                            // The final PR row (with Merge) lives on
                                            // All × Changes, on this page.
                                            WorkflowPrimaryAction.REVIEW_FINAL_PR -> {
                                                selectNode(null)
                                                faceName = WorkFaceKind.Changes.name
                                            }
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
                                        WorkflowView.overflowMenu(row.status).forEach { item ->
                                            val (label, icon) = when (item) {
                                                WorkflowOverflowItem.PLAN ->
                                                    WorkflowView.PLAN_WORKFLOW_LABEL to ExpIcons.uiChecklist
                                                WorkflowOverflowItem.RUNS_ON ->
                                                    WorkflowView.RUNS_ON_LABEL to ExpIcons.uiDevice
                                                WorkflowOverflowItem.STOP ->
                                                    WorkflowView.STOP_WORKFLOW_LABEL to ExpIcons.uiStop
                                                WorkflowOverflowItem.DELETE ->
                                                    WorkflowView.DELETE_WORKFLOW_LABEL to ExpIcons.uiDelete
                                            }
                                            GlassMenuItem(
                                                leadingIcon = { Icon(icon, contentDescription = null) },
                                                text = { Text(label) },
                                                destructive = item == WorkflowOverflowItem.STOP ||
                                                    item == WorkflowOverflowItem.DELETE,
                                                modifier = Modifier.testTag("workflow-overflow-${item.wire}"),
                                                onClick = {
                                                    menuOpen = false
                                                    when (item) {
                                                        // The planner run: the Agent composer seeded
                                                        // with the plan-workflow builtin + this workflow.
                                                        WorkflowOverflowItem.PLAN -> onOpenAgent(
                                                            AgentComposerSeed(
                                                                actionId = DomainContract.builtinPlanWorkflowId,
                                                                workflowId = row.id,
                                                                deviceId = row.deviceId,
                                                            ),
                                                        )
                                                        WorkflowOverflowItem.RUNS_ON -> pickerOpen = true
                                                        WorkflowOverflowItem.STOP -> confirmStop = true
                                                        WorkflowOverflowItem.DELETE -> confirmDelete = true
                                                    }
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
                    // EVERY open question; each one's answer field only on
                    // the caller's own run (EXP-312).
                    questions.forEach { question ->
                        key(question.sessionId, question.askedAt) {
                            QuestionBanner(
                                question = question,
                                own = question.sessionId in ownSessionIds,
                                title = graph.nodes.firstOrNull { it.id == question.nodeId }
                                    ?.let { node -> graph.issuesById[node.issueId]?.identifier },
                                onSelect = { selectNode(question.nodeId) },
                            )
                        }
                    }
                    startNotice?.let { notice ->
                        GlassNotice(
                            text = notice,
                            contentColor = MaterialTheme.colorScheme.error,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 16.dp, vertical = 4.dp)
                                .testTag("workflow-start-blocker"),
                        )
                    }
                    NodeStrip(
                        strip = strip,
                        graph = graph,
                        selectedId = selection.single,
                        onSelectAll = { selectNode(null) },
                        // A tap picks exactly that node; the picked chip stays.
                        onSelect = { id -> selectNode(selection.click(id, order).single) },
                        onLongPress = { id -> sheetNodeId = id },
                    )
                    // Stepping: one node selected, the chevrons walk the strip
                    // in its own order; back past the first node is All.
                    if (!selection.isAll && order.isNotEmpty()) {
                        val position = selection.position(order)
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 8.dp)
                                .testTag("workflow-step"),
                            horizontalArrangement = Arrangement.End,
                        ) {
                            CircleIconButton(
                                ExpIcons.uiChevronLeft,
                                "Previous node",
                                onClick = { selectNode(selection.step(-1, order).single) },
                                borderless = true,
                                modifier = Modifier.testTag("workflow-step-previous"),
                            )
                            CircleIconButton(
                                ExpIcons.uiChevronRight,
                                "Next node",
                                enabled = position < order.size,
                                onClick = { selectNode(selection.step(1, order).single) },
                                borderless = true,
                                modifier = Modifier.testTag("workflow-step-next"),
                            )
                        }
                    }
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
                    val runId = graph.runsByNodeId[selectedNode.id]?.sessionId
                    NodeFaces(
                        issueId = selectedNode.issueId,
                        sessionId = runId,
                        ownRun = runId != null && runId in ownSessionIds,
                        viewModel = viewModel,
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
                    onOpenNodeChanges = { nodeId ->
                        selectNode(nodeId)
                        faceName = WorkFaceKind.Changes.name
                    },
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
            title = { Text(WorkflowView.STOP_WORKFLOW_LABEL) },
            text = { Text(WorkflowView.CANCEL_WORKFLOW_CONFIRM) },
            confirmButton = {
                TextButton(onClick = {
                    confirmStop = false
                    viewModel.cancel()
                }) { Text(WorkflowView.STOP_WORKFLOW_LABEL, color = MaterialTheme.colorScheme.error) }
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

@Composable
private fun PrimaryActionPill(action: WorkflowPrimaryAction, enabled: Boolean, onClick: () -> Unit) {
    val (label, icon) = when (action) {
        WorkflowPrimaryAction.PICK_DEVICE -> WorkflowView.PICK_DEVICE_LABEL to ExpIcons.uiDevice
        WorkflowPrimaryAction.START -> WorkflowView.START_WORKFLOW_LABEL to ExpIcons.actionRun
        WorkflowPrimaryAction.PAUSE -> WorkflowView.PAUSE_WORKFLOW_LABEL to ExpIcons.runPause
        WorkflowPrimaryAction.RESUME -> WorkflowView.RESUME_WORKFLOW_LABEL to ExpIcons.actionRun
        WorkflowPrimaryAction.REVIEW_FINAL_PR -> WorkflowView.REVIEW_FINAL_PR_LABEL to ExpIcons.navReviews
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
 * The open question, ONLY while one is open: the asking node and the
 * question. EXP-312: only the run's OWNER steers it, so only then does an
 * inline answer follow — it goes to the run as a message, the same path a
 * `needs_input` run is answered through from its Run face. A teammate's
 * question reads as the question alone.
 */
@Composable
private fun QuestionBanner(question: WorkflowOpenQuestion, own: Boolean, title: String?, onSelect: () -> Unit) {
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
        if (own) QuestionAnswer(question)
    }
}

/** The owner's answer field — the only thing here that connects to the run. */
@Composable
private fun QuestionAnswer(question: WorkflowOpenQuestion) {
    // Scoped to the banner row: the relay connection goes when the question does.
    val sessionVm = hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
        viewModelStoreOwner = rememberScopedViewModelStoreOwner("question:${question.sessionId}"),
        key = "session:${question.sessionId}",
    ) { factory -> factory.create(question.sessionId) }
    var answer by rememberSaveable(question.sessionId, question.askedAt) { mutableStateOf("") }
    var failed by remember { mutableStateOf(false) }
    LaunchedEffect(sessionVm) { sessionVm.ensureConnected() }
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
            WorkflowView.ALL_NODES_LABEL,
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
    DisplayGlyph(chip.display, live = chip.live, status = status)
}

/** One display state as a 14dp glyph — the chips' and the header's. */
@Composable
private fun DisplayGlyph(
    display: WorkflowNodeDisplayState,
    live: Boolean,
    status: com.exponential.app.domain.ResolvedIssueStatus?,
) {
    val icon = when (display) {
        WorkflowNodeDisplayState.RUNNING -> {
            Box(Modifier.size(14.dp), contentAlignment = Alignment.Center) { LiveDot(busy = live) }
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
        contentDescription = display.label,
        modifier = Modifier.size(14.dp),
        tint = when (display) {
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
    var confirmDismiss by remember { mutableStateOf(false) }
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
                        NodeChipAction.DISMISS -> confirmDismiss = true
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
    if (confirmDismiss) {
        AlertDialog(
            onDismissRequest = { confirmDismiss = false },
            title = { Text(WorkflowView.DISMISS_NODE_LABEL) },
            text = { Text(WorkflowView.DISMISS_NODE_CONFIRM) },
            confirmButton = {
                TextButton(onClick = {
                    confirmDismiss = false
                    viewModel.admitNode(node.id, admit = false)
                    onDismiss()
                }) { Text(WorkflowView.DISMISS_NODE_LABEL, color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirmDismiss = false }) { Text("Cancel") } },
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

/**
 * ONE node: its issue's Work screen faces, in place. The run's live
 * connection ([AgentSessionViewModel] = `SteerConnectionStore.acquire`) is
 * held ONLY while this node is picked, its run is the caller's own (EXP-312)
 * and the Run or Changes face (the live diff) wants it — scoped to this
 * composition, so a pick elsewhere or another face releases it.
 */
@Composable
private fun NodeFaces(
    issueId: String,
    sessionId: String?,
    ownRun: Boolean,
    viewModel: WorkflowDetailViewModel,
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

    val wanted = WorkFaceKind.entries.firstOrNull { it.name == faceName } ?: WorkFaceKind.Issue
    val wantsLive = wanted == WorkFaceKind.Run || wanted == WorkFaceKind.Changes
    val sessionVm: AgentSessionViewModel? = if (sessionId != null && ownRun && wantsLive) {
        hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
            viewModelStoreOwner = rememberScopedViewModelStoreOwner("node-run:$sessionId"),
            key = "session:$sessionId",
        ) { factory -> factory.create(sessionId) }
    } else {
        null
    }
    // Results read the SYNCED row, so they need no connection.
    val sessionRow by viewModel.workflowSessions.collectAsStateWithLifecycle()
    val session by (sessionVm?.session ?: remember { MutableStateFlow(null) }).collectAsStateWithLifecycle()
    val activity by (sessionVm?.activity ?: remember { MutableStateFlow(ActivityFeedState()) })
        .collectAsStateWithLifecycle()
    val parsedDiff = remember(activity.latestDiff) { activity.latestDiff?.let { Diff.parse(it) } }
    // Any PR (open, merged, closed): its files load off `issues.prFiles`.
    val hasPr = issue != null && !issue.prUrl.isNullOrBlank()
    val hasChanges = parsedDiff != null || hasPr
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
    val runResults = session?.results ?: sessionRow.firstOrNull { it.id == sessionId }?.results
    val results = remember(runResults) { groupSessionResults(parseSessionResults(runResults)) }
    val faces = availableFaces(
        hasIssue = true,
        hasRun = sessionId != null && ownRun,
        hasChanges = hasChanges,
        hasResults = results.isNotEmpty(),
    )
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
    /** A node row with a PR on All × Changes: that node's Changes face. */
    onOpenNodeChanges: (nodeId: String) -> Unit,
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
    var decisionsOpen by rememberSaveable { mutableStateOf(false) }
    val hasFinalPr = !workflow?.finalPrUrl.isNullOrBlank()

    // Changes exists before the final PR: every node keeps its row there.
    val faces = availableFaces(
        hasIssue = true,
        hasRun = sessions.isNotEmpty(),
        hasChanges = hasFinalPr || graph.nodes.isNotEmpty(),
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
                // The decisions log, collapsed under the issues; empty = hidden.
                val decisions = workflow?.decisions?.trim().orEmpty()
                if (decisions.isNotEmpty()) {
                    item(key = "__decisions__") {
                        DecisionsSection(
                            decisions = decisions,
                            open = decisionsOpen,
                            onToggle = { decisionsOpen = !decisionsOpen },
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
        WorkFaceKind.Changes -> AllChangesFace(
            graph = graph,
            finalPrCaption = workflow?.let { row ->
                WorkflowView.finalPrCaption(graph.nodes.map { it.state }, row.finalPrState, row.finalPrNumber)
            },
            finalPrOpen = finalPrOpen,
            busy = busy,
            onMerge = viewModel::mergeFinalPr,
            onOpenNodeChanges = onOpenNodeChanges,
            padding = padding,
            trailing = trailing,
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

/** The decisions log under All × Issue: a notice that opens to the text. */
@Composable
private fun DecisionsSection(decisions: String, open: Boolean, onToggle: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(top = 8.dp).testTag("workflow-decisions"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        GlassNotice(
            text = WorkflowView.DECISIONS_LABEL,
            onClick = onToggle,
            leading = {
                Icon(
                    if (open) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                )
            },
            modifier = Modifier.fillMaxWidth(),
        )
        if (open) {
            Text(
                decisions,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.padding(horizontal = 4.dp).testTag("workflow-decisions-text"),
            )
        }
    }
}

/**
 * All × Changes: the final pull request (Merge in the bar while it is open)
 * over EVERY node's row — its chip and its PR state; a node with a PR opens
 * its own Changes face in place, one without reads [WorkflowView.NO_CHANGES_LABEL].
 */
@Composable
private fun AllChangesFace(
    graph: WorkflowGraph,
    finalPrCaption: String?,
    finalPrOpen: Boolean,
    busy: Boolean,
    onMerge: () -> Unit,
    onOpenNodeChanges: (String) -> Unit,
    padding: PaddingValues,
    trailing: @Composable () -> Unit,
) {
    var confirmMerge by remember { mutableStateOf(false) }
    Box(modifier = Modifier.padding(padding).fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().testTag("workflow-all-changes"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            finalPrCaption?.let { caption ->
                item(key = "__final_pr__") {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp).testTag("workflow-final-pr"),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Icon(ExpIcons.prOpen, contentDescription = null, modifier = Modifier.size(14.dp))
                        Text(
                            WorkflowView.FINAL_PR_TITLE,
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.weight(1f),
                        )
                        Text(
                            caption,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            }
            items(graph.nodes, key = { it.id }) { node ->
                val issue = graph.issuesById[node.issueId]
                val hasPr = issue != null && !issue.prUrl.isNullOrBlank()
                val prLabel = if (hasPr) {
                    val state = when (issue?.prState) {
                        DomainContract.prStateMerged -> "Merged"
                        DomainContract.prStateClosed -> "Closed"
                        else -> "Open"
                    }
                    issue?.prNumber?.let { "#$it · $state" } ?: state
                } else {
                    WorkflowView.NO_CHANGES_LABEL
                }
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .then(if (hasPr) Modifier.clickable { onOpenNodeChanges(node.id) } else Modifier)
                        .padding(vertical = 4.dp)
                        .testTag("workflow-changes-${node.id}"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    val status = graph.statusByIssueId[node.issueId]
                    Box(Modifier.weight(1f, fill = false)) {
                        IssueChip(
                            identifier = WorkflowView.nodeTitle(
                                issue?.identifier ?: node.issueId.take(8),
                                node.memberIssueIds.size,
                            ),
                            title = issue?.title ?: WorkflowView.NODE_UNSYNCED_TITLE,
                            status = status,
                            onClick = if (hasPr) {
                                { onOpenNodeChanges(node.id) }
                            } else {
                                null
                            },
                            leading = {
                                DisplayGlyph(WorkflowView.nodeDisplayState(node.state), live = false, status = status)
                            },
                        )
                    }
                    Text(
                        prLabel,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.testTag(if (hasPr) "workflow-changes-pr" else "workflow-changes-empty"),
                    )
                }
            }
        }
        FloatingBarCluster(
            modifier = Modifier.align(Alignment.BottomCenter),
            centre = if (finalPrOpen) {
                {
                    BarSolidPill(
                        label = WorkflowView.MERGE_FINAL_PR_LABEL,
                        icon = ExpIcons.prMerged,
                        loading = busy,
                        onClick = { confirmMerge = true },
                        modifier = Modifier.testTag("pr-merge-bar"),
                    )
                }
            } else {
                null
            },
            right = trailing,
        )
    }
    if (confirmMerge) {
        AlertDialog(
            onDismissRequest = { confirmMerge = false },
            title = { Text("Merge pull request?") },
            text = { Text(WorkflowView.MERGE_FINAL_PR_CONFIRM) },
            confirmButton = {
                TextButton(onClick = {
                    confirmMerge = false
                    onMerge()
                }) { Text(WorkflowView.MERGE_FINAL_PR_LABEL) }
            },
            dismissButton = { TextButton(onClick = { confirmMerge = false }) { Text("Cancel") } },
        )
    }
}

/**
 * A [ViewModelStoreOwner] that lives exactly as long as the calling
 * composition: its view models are cleared on dispose (a pick elsewhere, a
 * face change, the question closing), not when the whole page leaves. Hilt
 * view models resolve through the host's factory and creation extras.
 */
@Composable
private fun rememberScopedViewModelStoreOwner(key: String): ViewModelStoreOwner {
    val parent = checkNotNull(LocalViewModelStoreOwner.current) { "No ViewModelStoreOwner" }
    val context = LocalContext.current
    val owner = remember(key, parent) {
        val host = parent as? HasDefaultViewModelProviderFactory
        object : ViewModelStoreOwner, HasDefaultViewModelProviderFactory {
            override val viewModelStore = ViewModelStore()
            override val defaultViewModelProviderFactory: ViewModelProvider.Factory =
                HiltViewModelFactory(
                    context,
                    host?.defaultViewModelProviderFactory ?: ViewModelProvider.NewInstanceFactory(),
                )
            override val defaultViewModelCreationExtras: CreationExtras =
                host?.defaultViewModelCreationExtras ?: CreationExtras.Empty
        }
    }
    DisposableEffect(owner) { onDispose { owner.viewModelStore.clear() } }
    return owner
}
