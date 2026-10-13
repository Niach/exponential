package com.exponential.app.ui.issue

import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.CodingReadiness
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.work.LocalTitleCollapse
import com.exponential.app.ui.work.TitleCollapseState
import com.exponential.app.ui.work.WorkTopBar
import dev.chrisbanes.haze.rememberHazeState

/**
 * EXP-1291: the ISSUE screen (`issue/{issueId}?comment=`) — the issue face
 * alone (EXP-1291 retired the Issue | Session | Guide faces: sessions live in
 * threads now). The host owns the Scaffold, the top bar (identifier, the
 * collapsed title, the issue `…` menu), the "Ready to code?" checklist and
 * the bar's Start circle (lane SESSION owns Start: today it opens the home
 * composer seeded with this issue). A pull request row opens the thread that
 * opened it ([onOpenThread]), else the PR on GitHub.
 */
@Composable
fun IssueScreen(
    issueId: String,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit,
    /** Start: the home composer with this issue preselected. */
    onOpenAgent: (AgentComposerSeed) -> Unit,
    /** The issue's `Threads` row and a PR row's thread. */
    onOpenThread: (rootSessionId: String) -> Unit,
    /** EXP-1121: the "Ready to code?" fixes that leave the screen; null hides. */
    onOpenTeamSettings: ((teamId: String) -> Unit)? = null,
    onOpenDevices: (() -> Unit)? = null,
    /** EXP-1097: the Sub-issues `+`; null hides it. */
    onCreateSubIssue: ((boardId: String, parentId: String) -> Unit)? = null,
    /** EXP-1241: scroll to this comment once it renders, then highlight it. */
    focusCommentId: String? = null,
) {
    val issueVm = hiltViewModel<IssueDetailViewModel, IssueDetailViewModel.Factory>(
        key = "issue:$issueId",
    ) { factory -> factory.create(issueId) }
    val commentVm: CommentThreadViewModel = hiltViewModel()
    val controller = rememberIssueFaceController()
    val state by issueVm.state.collectAsStateWithLifecycle()
    val issue = state.issue
    val permissions by issueVm.permissions.collectAsStateWithLifecycle()
    val steerEnabled by issueVm.steerEnabled.collectAsStateWithLifecycle()
    val context = LocalContext.current

    // EXP-1121: whether this issue can Start right now (the readiness steps).
    val readinessVm: CodingReadinessViewModel = hiltViewModel(key = "readiness:$issueId")
    val readinessBoard = state.board
    val readinessMember = permissions.isMember
    LaunchedEffect(readinessVm, readinessBoard, readinessMember, steerEnabled) {
        readinessVm.bind(
            CodingReadinessViewModel.Host(
                board = readinessBoard,
                isMember = readinessMember,
                remoteStartEnabled = steerEnabled,
            ),
        )
    }
    val readinessState by readinessVm.state.collectAsStateWithLifecycle()
    val readiness = readinessState.readiness
    var readinessSheetOpen by rememberSaveable { mutableStateOf(false) }
    var readinessResumed by remember { mutableStateOf(false) }
    LifecycleResumeEffect(readinessVm) {
        if (readinessResumed) readinessVm.reload()
        readinessResumed = true
        onPauseOrDispose {}
    }
    val startUi: StartButtonUi? = readiness?.let(StartButtonUi::from)
    fun startCoding() {
        val r = readiness ?: return
        when {
            !r.visible || r.loading -> Unit
            r.ready -> onOpenAgent(AgentComposerSeed(issueIds = listOf(issueId)))
            else -> readinessSheetOpen = true
        }
    }
    val openPullRequest: (PullRequestEntity) -> Unit = { pr ->
        val root = pr.rootSessionId
        if (root != null) {
            onOpenThread(root)
        } else if (pr.url.isNotBlank()) {
            CustomTabsIntent.Builder().build().launchUrl(context, pr.url.toUri())
        }
    }

    val hazeState = rememberHazeState()
    val titleCollapse = remember { TitleCollapseState() }
    ProvideMarkdownToolbar {
        CompositionLocalProvider(
            LocalDetailHaze provides hazeState,
            LocalTitleCollapse provides titleCollapse,
        ) {
            Scaffold(
                containerColor = Color.Transparent,
                topBar = {
                    WorkTopBar(
                        title = issue?.identifier.orEmpty(),
                        collapsedTitle = issue?.title,
                        collapsed = titleCollapse.issueTitleCollapsed,
                        onHeaderBottom = titleCollapse::reportHeaderBottom,
                        onBack = onBack,
                        verb = null,
                        verbEnabled = false,
                        onVerb = {},
                        menu = { IssueMenuActions(viewModel = issueVm, controller = controller) },
                    )
                },
            ) { padding ->
                IssueFace(
                    viewModel = issueVm,
                    commentViewModel = commentVm,
                    controller = controller,
                    padding = padding,
                    onBack = onBack,
                    onOpenIssue = onOpenIssue,
                    onOpenPullRequest = openPullRequest,
                    trailingBarSlot = {
                        val ui = startUi
                        if (ui != null) StartCircle(ui = ui, onClick = { startCoding() }) else Spacer(Modifier.size(0.dp))
                    },
                    focusCommentId = focusCommentId,
                    onAddSubIssue = onCreateSubIssue?.let { create ->
                        issue?.let { parent -> { create(parent.boardId, parent.id) } }
                    },
                    onOpenThread = onOpenThread,
                )
            }
        }
    }

    // EXP-1121: the "Ready to code?" checklist behind a not-ready Start.
    if (readinessSheetOpen && readiness?.visible == true) {
        val teamId = readinessState.board?.teamId
        val openTeamSettings = onOpenTeamSettings?.takeIf { teamId != null }
        val fixes = CodingReadiness.Fix.entries.filterTo(mutableSetOf()) { fix ->
            when (fix) {
                CodingReadiness.Fix.BOARD_SETTINGS -> openTeamSettings != null
                CodingReadiness.Fix.CONNECT_GITHUB -> permissions.isMember || openTeamSettings != null
                CodingReadiness.Fix.OPEN_DEVICES -> onOpenDevices != null
                CodingReadiness.Fix.CHOOSE_REPOSITORY -> permissions.isMember
                CodingReadiness.Fix.GET_DESKTOP_APP, CodingReadiness.Fix.SET_UP_SERVER -> true
            }
        }
        CodingReadinessSheet(
            viewModel = readinessVm,
            availableFixes = fixes,
            onNavigateFix = { fix ->
                readinessSheetOpen = false
                when (fix) {
                    CodingReadiness.Fix.CONNECT_GITHUB, CodingReadiness.Fix.BOARD_SETTINGS ->
                        teamId?.let { openTeamSettings?.invoke(it) }
                    CodingReadiness.Fix.OPEN_DEVICES -> onOpenDevices?.invoke()
                    else -> Unit
                }
            },
            onStart = {
                readinessSheetOpen = false
                onOpenAgent(AgentComposerSeed(issueIds = listOf(issueId)))
            },
            onDismiss = { readinessSheetOpen = false },
        )
    }
}
