package com.exponential.app.ui.issue

import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.PaddingValues
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
import com.exponential.app.domain.ClosePr
import com.exponential.app.domain.CodingReadiness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.work.ChangesMergeControl
import com.exponential.app.ui.work.PullRequestStack
import com.exponential.app.ui.work.STACK_CONFIRM_CANCEL_LABEL
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
 *
 * EXP-1316 [embedded]: the thread's issue tab = the issue body alone: no own
 * top bar (the thread's header stays), no `Threads` row, no Start (the bar
 * keeps properties + Comment); the thread host provides the haze, the title
 * collapse and the markdown toolbar, and hands its content [padding].
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
    /** #EXP-1381: [section] = the settings category the fix lives on. */
    onOpenTeamSettings: ((teamId: String, section: String) -> Unit)? = null,
    onOpenDevices: (() -> Unit)? = null,
    /** EXP-1097: the Sub-issues `+`; null hides it. */
    onCreateSubIssue: ((boardId: String, parentId: String) -> Unit)? = null,
    /** EXP-1241: scroll to this comment once it renders, then highlight it. */
    focusCommentId: String? = null,
    embedded: Boolean = false,
    /** [embedded]: the host's content padding. */
    padding: PaddingValues = PaddingValues(0.dp),
) {
    val issueVm = hiltViewModel<IssueDetailViewModel, IssueDetailViewModel.Factory>(
        key = "issue:$issueId",
    ) { factory -> factory.create(issueId) }
    val commentVm: CommentThreadViewModel = hiltViewModel(key = "comments:$issueId")
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
    LaunchedEffect(readinessVm, readinessBoard, readinessMember, steerEnabled, embedded) {
        if (embedded) return@LaunchedEffect
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

    // ── EXP-1291: the PR rows' Merge / the menu's Close PR ──────────────────
    val teamPullRequests by issueVm.teamPullRequests.collectAsStateWithLifecycle()
    val prIssueCounts by issueVm.pullRequestIssueCounts.collectAsStateWithLifecycle()
    val prBusyId by issueVm.prBusyId.collectAsStateWithLifecycle()
    val issuePullRequests by issueVm.pullRequests.collectAsStateWithLifecycle()
    val prMergeControl: (PullRequestEntity) -> ChangesMergeControl? = { pr ->
        if (!permissions.isMember) {
            null
        } else {
            val stack = PullRequestStack.stackConfirm(pr, teamPullRequests)
            ChangesMergeControl(
                label = if (stack != null) DomainContract.diffUiMergeStack else DomainContract.diffUiMergePr,
                loading = prBusyId == pr.id,
                // Refusals toast through the face's mutation-error slot.
                error = null,
                confirmPrompt = Prompts.MergeIssuePr.prompt(pr.number, prIssueCounts[pr.id] ?: 1),
                onConfirm = { issueVm.mergePullRequest(pr) },
                stackConfirm = stack,
                onMergeStack = {
                    val through = stack?.throughId?.let { id -> teamPullRequests.firstOrNull { it.id == id } } ?: pr
                    issueVm.mergePullRequest(through, stack = true)
                },
            )
        }
    }
    // EXP-1233: a REAL conflict opens the composer seeded to fix it.
    LaunchedEffect(issueVm) {
        issueVm.mergeConflicts.collect { pr ->
            onOpenAgent(
                AgentComposerSeed(
                    issueIds = listOf(issueId),
                    prIssueId = issueId,
                    conflict = true,
                    text = fixConflictsPrompt(pr),
                ),
            )
        }
    }
    // EXP-1154: Close PR (the `…` menu) acts on the newest OPEN pull request.
    val closablePr = issuePullRequests.firstOrNull { it.state == DomainContract.prStateOpen }
        ?.takeIf { permissions.isMember }
    var closePrConfirm by remember { mutableStateOf<PullRequestEntity?>(null) }

    val face: @Composable (PaddingValues) -> Unit = { contentPadding ->
        IssueFace(
            viewModel = issueVm,
            commentViewModel = commentVm,
            controller = controller,
            padding = contentPadding,
            onBack = onBack,
            onOpenIssue = onOpenIssue,
            onOpenPullRequest = openPullRequest,
            prMergeControl = prMergeControl,
            trailingBarSlot = {
                val ui = startUi.takeUnless { embedded }
                if (ui != null) StartCircle(ui = ui, onClick = { startCoding() }) else Spacer(Modifier.size(0.dp))
            },
            focusCommentId = focusCommentId,
            onAddSubIssue = onCreateSubIssue?.let { create ->
                issue?.let { parent -> { create(parent.boardId, parent.id) } }
            },
            onOpenThread = if (embedded) null else onOpenThread,
        )
    }
    val hazeState = rememberHazeState()
    val titleCollapse = remember { TitleCollapseState() }
    if (embedded) {
        face(padding)
    } else ProvideMarkdownToolbar {
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
                        menu = {
                            IssueMenuActions(
                                viewModel = issueVm,
                                controller = controller,
                                onClosePr = closablePr?.let { pr -> { closePrConfirm = pr } },
                            )
                        },
                    )
                },
            ) { scaffoldPadding ->
                face(scaffoldPadding)
            }
        }
    }

    closePrConfirm?.let { pr ->
        val others = ((prIssueCounts[pr.id] ?: 1) - 1).coerceAtLeast(0)
        GlassAlert(
            title = ClosePr.TITLE,
            body = ClosePr.body(others),
            onDismiss = { closePrConfirm = null },
            trailing = listOf(
                GlassAlertAction(STACK_CONFIRM_CANCEL_LABEL, onClick = { closePrConfirm = null }),
                GlassAlertAction(
                    ClosePr.CONFIRM,
                    destructive = true,
                    testTag = "close-pr-confirm",
                    onClick = {
                        closePrConfirm = null
                        issueVm.closePullRequest(pr)
                    },
                ),
            ),
            defaultAction = 0,
        )
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
                    CodingReadiness.Fix.CONNECT_GITHUB ->
                        teamId?.let { openTeamSettings?.invoke(it, com.exponential.app.domain.SettingsNav.REPOSITORIES) }
                    CodingReadiness.Fix.BOARD_SETTINGS ->
                        teamId?.let { openTeamSettings?.invoke(it, com.exponential.app.domain.SettingsNav.BOARDS) }
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

/**
 * EXP-1233/EXP-1291: the composer text a conflict-refused merge seeds — the
 * agent rebases the PR's branch onto its base, resolves, force-pushes and
 * merges (the retired Fix merge conflicts builtin's job, now plain words).
 */
internal fun fixConflictsPrompt(pr: PullRequestEntity): String {
    val name = pr.number?.let { "PR #$it" } ?: "the pull request"
    return "Fix the merge conflicts on $name (`${pr.head}` into `${pr.base}`): rebase onto its base, " +
        "resolve the conflicts, force-push, then merge it."
}
