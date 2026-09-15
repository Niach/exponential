package com.exponential.app.ui.work

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.SwitcherMode
import com.exponential.app.domain.SwitcherTarget
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.domain.activeQuestionIds
import com.exponential.app.domain.availableFaces
import com.exponential.app.domain.canOfferFixConflicts
import com.exponential.app.domain.codingTarget
import com.exponential.app.domain.fallbackFace
import com.exponential.app.domain.isSessionLive
import com.exponential.app.domain.switcherBadge
import com.exponential.app.domain.switcherMode
import com.exponential.app.domain.switcherTargets
import com.exponential.app.ui.issue.ChangesViewModel
import com.exponential.app.ui.issue.CommentThreadViewModel
import com.exponential.app.ui.issue.IssueDetailViewModel
import com.exponential.app.ui.issue.IssueFace
import com.exponential.app.ui.issue.IssueMenuActions
import com.exponential.app.ui.issue.StartButtonUi
import com.exponential.app.ui.issue.StartCircle
import com.exponential.app.ui.issue.rememberIssueFaceController
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.session.AgentSessionViewModel
import com.exponential.app.ui.session.RunFace
import com.exponential.app.ui.session.sessionDotTone
import com.exponential.app.ui.session.sessionRowTitle
import com.exponential.app.ui.steer.ActionRunState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch

// EXP-893: the phone WORK SCREEN — one screen per subject (an issue, or a
// session) with up to three FACES held as screen state, never as navigation:
// Issue, Run and Changes (`domain/WorkFaces.kt`, the ×3 rules). System Back
// pops the whole screen. Opening a run from any list opens it on the Run
// face; opening an issue lands on the Issue face with its run one switcher
// tap away. The host owns the Scaffold, the top bar (title dot + verbs +
// the issue menu), the snackbar, the kill / resume confirms, the ended edge
// and the bar's trailing circle; each face renders inside the content slot.

/** What a Work screen is about — the route decides, the screen resolves. */
sealed interface WorkSubject {
    data class Issue(val id: String) : WorkSubject
    data class Session(val id: String) : WorkSubject
}

@Composable
fun WorkScreen(
    subject: WorkSubject,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit,
    /** The standalone Changes route — for a PR the face cannot show. */
    onOpenChanges: (issueId: String) -> Unit,
    // EXP-825: every start / fix-conflicts goes through the Agent page composer.
    onOpenAgent: (AgentComposerSeed) -> Unit,
) {
    // ── Screen state (survives rotation and process death) ─────────────────
    var faceName by rememberSaveable { mutableStateOf<String?>(null) }
    var shownSessionId by rememberSaveable {
        mutableStateOf((subject as? WorkSubject.Session)?.id)
    }
    // A run the reader PICKED (or arrived on) stays; only an auto-chosen one
    // follows `codingTarget` as the issue's runs come and go.
    var pinnedByUser by rememberSaveable { mutableStateOf(subject is WorkSubject.Session) }
    var killDialogOpen by rememberSaveable { mutableStateOf(false) }
    var resumeConfirmOpen by rememberSaveable { mutableStateOf(false) }
    val snackbarHostState = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()

    // ── The shown run's model, one per id ──────────────────────────────────
    val sessionVm: AgentSessionViewModel? = shownSessionId?.let { id ->
        hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
            key = "session:$id",
        ) { factory -> factory.create(id) }
    }
    val shownSession by (sessionVm?.session ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()

    // ── The issue, from the subject or the run's own row ───────────────────
    val issueId: String? = when (subject) {
        is WorkSubject.Issue -> subject.id
        is WorkSubject.Session -> shownSession?.issueId
    }
    val issueVm: IssueDetailViewModel? = issueId?.let { id ->
        hiltViewModel<IssueDetailViewModel, IssueDetailViewModel.Factory>(
            key = "issue:$id",
        ) { factory -> factory.create(id) }
    }
    val commentVm: CommentThreadViewModel? = if (issueId != null) hiltViewModel() else null
    val issueController = rememberIssueFaceController()

    val issueState by (issueVm?.state ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val issue = issueState?.issue
    val issueSessions by (issueVm?.issueSessions ?: remember { MutableStateFlow(emptyList()) })
        .collectAsStateWithLifecycle()
    val issueRuns by (issueVm?.issueRuns ?: remember { MutableStateFlow(emptyList()) })
        .collectAsStateWithLifecycle()
    // The minute clock every liveness cut is taken on (`CodingSessionLiveness`)
    // — the screen's own, so an issue-less run's Stop retires on time too.
    val liveClock by remember { CodingSessionLiveness.minuteTicker() }
        .collectAsStateWithLifecycle(initialValue = System.currentTimeMillis())
    val currentUserId by (issueVm?.currentUserId ?: sessionVm?.currentUserId
        ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val permissions by (issueVm?.permissions ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val steerEnabled by (issueVm?.steerEnabled ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val steerDevices by (issueVm?.steerDevices ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()

    // `codingTarget`: the bound run when it is mine and live, else my newest
    // live run on the issue, else my newest run at all — followed only while
    // the reader has not picked one (desktop `work_header.rs`).
    LaunchedEffect(issueSessions, liveClock, currentUserId, pinnedByUser, issueId) {
        if (pinnedByUser || issueId == null) return@LaunchedEffect
        val target = codingTarget(issueSessions, issueId, shownSessionId, currentUserId, liveClock)
        if (target?.id != shownSessionId) shownSessionId = target?.id
    }

    // ── The shown run's live state, read off its model ─────────────────────
    val phase by (sessionVm?.phase ?: remember { MutableStateFlow(AgentPhase.Idle) })
        .collectAsStateWithLifecycle()
    val activity by (sessionVm?.activity ?: remember { MutableStateFlow(ActivityFeedState()) })
        .collectAsStateWithLifecycle()
    val resumeTarget by (sessionVm?.resumeTarget ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val runState by (sessionVm?.runState ?: remember { MutableStateFlow(ActionRunState.Idle) })
        .collectAsStateWithLifecycle()
    val startedSessionId by (sessionVm?.startedSessionId ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val mergeTarget by (sessionVm?.mergeTarget ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val mergeIssue by (sessionVm?.mergeIssue ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val merging by (sessionVm?.merging ?: remember { MutableStateFlow(false) })
        .collectAsStateWithLifecycle()
    val mergeError by (sessionVm?.mergeError ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val sessionEnded = shownSession?.status == DomainContract.codingSessionStatusEnded
    val ownShown = shownSession != null && currentUserId != null && shownSession?.userId == currentUserId
    val shownLive = shownSession?.let { isSessionLive(it, liveClock) } == true
    val awaitingInput = phase == AgentPhase.Live &&
        remember(activity.feed) { activeQuestionIds(activity.feed) }.isNotEmpty()
    val latestDiff = activity.latestDiff
    // EXP-895: the raw `git diff` is parsed ONCE here — the Changes face draws
    // the files, the switcher row shows their totals.
    val parsedDiff = remember(latestDiff) { latestDiff?.let { Diff.parse(it) } }
    val diffStats = remember(parsedDiff) { parsedDiff?.let { Diff.totals(it.files) } }

    // EXP-773/849: a Resume or an account switch lands a NEW row — the
    // continuation swaps into the same screen in place.
    LaunchedEffect(startedSessionId) {
        val id = startedSessionId ?: return@LaunchedEffect
        sessionVm?.consumeStartedSession()
        shownSessionId = id
        pinnedByUser = true
        faceName = WorkFaceKind.Run.name
    }

    // ── Faces ───────────────────────────────────────────────────────────────
    val prOpen = issue != null && issue.prState == DomainContract.prStateOpen && !issue.prUrl.isNullOrBlank()
    val hasChanges = latestDiff != null || prOpen
    // EXP-895: the PR the Changes face links out to — the issue's, or the
    // shown run's OWN issue-less one (EXP-734). It wears the header's action
    // slot now, because the bar's leading slot opens the changed-files sheet.
    val changesPrUrl = (issue?.prUrl ?: shownSession?.prUrl)?.takeIf { it.isNotBlank() }
    val faces = availableFaces(
        hasIssue = issueId != null,
        hasRun = shownSessionId != null,
        hasChanges = hasChanges,
    )
    val wantedFace = faceName?.let { name -> WorkFaceKind.entries.firstOrNull { it.name == name } }
        ?: if (subject is WorkSubject.Session) WorkFaceKind.Run else WorkFaceKind.Issue
    val face = fallbackFace(wantedFace, faces) ?: wantedFace
    // Where a face lands when it vanishes under the reader (the diff cleared,
    // the run row went): changes → run → issue.
    LaunchedEffect(face, wantedFace) { if (face != wantedFace) faceName = face.name }

    // Start: only for an issue subject that can be coded on (steer on, a
    // member, a repo-backed board); dimmed without an online desktop.
    val canStart = issueId != null && steerEnabled == true && permissions?.isMember == true &&
        issueState?.board?.repositoryId != null
    val startUi: StartButtonUi? = when {
        !canStart -> null
        steerDevices == null -> null
        else -> StartButtonUi.Start(enabled = steerDevices.orEmpty().isNotEmpty())
    }
    fun startCoding() {
        val id = issueId ?: return
        if (steerDevices.isNullOrEmpty()) {
            scope.launch {
                snackbarHostState.showSnackbar(
                    "No desktop online. Open the Exponential desktop app to run here.",
                )
            }
        } else {
            // EXP-825: the composer IS the launcher.
            onOpenAgent(AgentComposerSeed(issueIds = listOf(id)))
        }
    }

    // The switcher: the other faces, one row per own run with two or more,
    // and Start coding once the shown run ended for good.
    val offerStart = sessionEnded && ownShown && resumeTarget == null && canStart
    val targets = switcherTargets(
        faces = faces,
        shown = face,
        runIds = issueRuns.map { it.session.id },
        shownRunId = shownSessionId,
        offerStart = offerStart,
    )
    val mode = switcherMode(targets)
    val dotTone = sessionDotTone(shownSession, issue?.prState, liveClock, awaitingInput)
    val badge = switcherBadge(face, dotTone, hasChanges)
    val trailingSlot: @Composable () -> Unit = {
        if (mode !is SwitcherMode.Hidden) {
            FaceSwitcher(
                mode = mode,
                badge = badge,
                badgeBusy = shownSession?.agentBusy == true,
                runs = issueRuns,
                shownRunId = shownSessionId,
                diffStats = diffStats,
                onPick = { target ->
                    when (target) {
                        is SwitcherTarget.Face -> faceName = target.face.name
                        is SwitcherTarget.Run -> {
                            shownSessionId = target.id
                            pinnedByUser = true
                            faceName = WorkFaceKind.Run.name
                        }
                        SwitcherTarget.StartCoding -> startCoding()
                    }
                },
            )
        } else if (startUi != null && shownSessionId == null) {
            StartCircle(ui = startUi, onClick = { startCoding() })
        } else {
            Spacer(Modifier.size(0.dp))
        }
    }

    // ── The ended edge (EXP-696) ───────────────────────────────────────────
    // An issue-bound subject stays on screen (the pill flips Stop → Resume,
    // the composer retires, Changes falls back to Run when the diff goes);
    // an issue-less run keeps the auto-back. Edge-triggered on a REAL live
    // row, so a screen opened onto an already-ended run stays put.
    var wasLive by remember(shownSessionId) { mutableStateOf(false) }
    LaunchedEffect(shownSession?.status, issueId) {
        val status = shownSession?.status ?: return@LaunchedEffect
        if (status != DomainContract.codingSessionStatusEnded) {
            wasLive = true
        } else if (wasLive && issueId == null) {
            onBack()
        }
    }

    // ── Top bar inputs ──────────────────────────────────────────────────────
    val title = when {
        issue != null -> issue.identifier
        issueId != null -> ""
        shownSession != null -> sessionRowTitle(shownSession!!, null)
        else -> "Coding session"
    }
    val canKill = ownShown && !sessionEnded && shownLive
    val verb = when {
        face != WorkFaceKind.Run -> null
        canKill -> WorkBarVerb.Stop
        sessionEnded && resumeTarget != null -> WorkBarVerb.Resume
        else -> null
    }

    ProvideMarkdownToolbar {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                WorkTopBar(
                    title = title,
                    dotTone = if (issueId != null) dotTone else null,
                    dotBusy = shownSession?.agentBusy == true,
                    onBack = onBack,
                    verb = verb,
                    verbEnabled = runState !is ActionRunState.Sending,
                    onVerb = {
                        when (verb) {
                            WorkBarVerb.Stop -> killDialogOpen = true
                            WorkBarVerb.Resume -> resumeConfirmOpen = true
                            null -> Unit
                        }
                    },
                    // EXP-895: the Changes face's GitHub circle lives in the
                    // header's action slot — the bar's leading slot now opens
                    // the changed-files sheet.
                    action = changesPrUrl?.takeIf { face == WorkFaceKind.Changes }?.let { url ->
                        { GithubHeaderAction(url) }
                    },
                    menu = if (issueVm != null) {
                        { IssueMenuActions(viewModel = issueVm, controller = issueController) }
                    } else {
                        null
                    },
                )
            },
            snackbarHost = { SnackbarHost(snackbarHostState) },
        ) { padding ->
            when (face) {
                WorkFaceKind.Issue -> if (issueVm != null && commentVm != null) {
                    IssueFace(
                        viewModel = issueVm,
                        commentViewModel = commentVm,
                        controller = issueController,
                        padding = padding,
                        snackbarHostState = snackbarHostState,
                        onBack = onBack,
                        onOpenIssue = onOpenIssue,
                        onOpenChanges = {
                            if (hasChanges) faceName = WorkFaceKind.Changes.name else issueId?.let(onOpenChanges)
                        },
                        trailingBarSlot = trailingSlot,
                    )
                }
                WorkFaceKind.Run -> key(shownSessionId) {
                    if (sessionVm != null) {
                        RunFace(
                            viewModel = sessionVm,
                            padding = padding,
                            onOpenIssue = onOpenIssue,
                            trailingBarSlot = trailingSlot,
                        )
                    }
                }
                WorkFaceKind.Changes -> key(shownSessionId) {
                    // Source B only when there is no live diff to show.
                    val changesVm: ChangesViewModel? = if (latestDiff == null && issueId != null) {
                        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(
                            key = "changes:$issueId",
                        ) { factory -> factory.create(issueId) }
                    } else {
                        null
                    }
                    val changesMerging by (changesVm?.merging ?: remember { MutableStateFlow(false) })
                        .collectAsStateWithLifecycle()
                    val changesError by (changesVm?.actionError ?: remember { MutableStateFlow(null) })
                        .collectAsStateWithLifecycle()
                    val changesErrorFrom by (changesVm?.actionErrorFrom ?: remember { MutableStateFlow(null) })
                        .collectAsStateWithLifecycle()
                    val changesConflict by (changesVm?.actionErrorIsConflict ?: remember { MutableStateFlow(false) })
                        .collectAsStateWithLifecycle()
                    // The live run merges its own target (EXP-678/734); a
                    // PR-only face merges through the issue (EXP-156).
                    val sessionCanMerge = sessionVm != null && mergeTarget != null &&
                        !sessionEnded && phase !is AgentPhase.Ended
                    val merge = when {
                        sessionCanMerge -> {
                            // EXP-706/734: a REAL conflict on an ISSUE target
                            // swaps the verb for the recovery run.
                            val fix = mergeTarget is MergeTarget.Issue &&
                                canOfferFixConflicts(mergeError, mergeIssue?.branch, steerEnabled = steerEnabled == true)
                            ChangesMergeControl(
                                label = if (fix) "Fix conflicts" else "Merge PR",
                                fixConflicts = fix,
                                loading = merging,
                                error = mergeError?.message,
                                confirmText = when (mergeTarget) {
                                    is MergeTarget.Session ->
                                        "Merges this run's pull request and closes the coding session."
                                    else ->
                                        "Merges the pull request, completes every linked issue, " +
                                            "and closes the coding session."
                                },
                                onConfirm = { sessionVm?.merge() },
                                onFixConflicts = {
                                    onOpenAgent(
                                        AgentComposerSeed(
                                            actionId = DomainContract.builtinFixConflictsId,
                                            prIssueId = mergeIssue?.id,
                                        ),
                                    )
                                },
                            )
                        }
                        changesVm != null && prOpen && permissions?.isMember == true -> {
                            val fix = changesConflict &&
                                changesErrorFrom == ChangesViewModel.PrAction.Merge &&
                                steerEnabled == true && !issue?.branch.isNullOrBlank()
                            ChangesMergeControl(
                                label = if (fix) "Fix conflicts" else "Merge PR",
                                fixConflicts = fix,
                                loading = changesMerging,
                                error = changesError,
                                confirmText = "Squash-merges PR #${issue?.prNumber ?: ""} via the GitHub App. " +
                                    "Any live coding session for it closes.",
                                onConfirm = { changesVm.mergePr() },
                                onFixConflicts = {
                                    onOpenAgent(
                                        AgentComposerSeed(
                                            actionId = DomainContract.builtinFixConflictsId,
                                            prIssueId = issueId,
                                        ),
                                    )
                                },
                            )
                        }
                        else -> null
                    }
                    ChangesFace(
                        padding = padding,
                        diff = parsedDiff,
                        changesViewModel = changesVm,
                        merge = merge,
                        trailingBarSlot = trailingSlot,
                    )
                }
            }
        }
    }

    // ── Confirms ────────────────────────────────────────────────────────────
    if (killDialogOpen) {
        AlertDialog(
            onDismissRequest = { killDialogOpen = false },
            // EXP-818: ONE word for ending a run, wherever it is watched from.
            title = { Text("Stop this coding session?") },
            text = {
                Text(
                    "This stops the agent on the desktop " +
                        "and ends the session. It cannot be undone.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    killDialogOpen = false
                    sessionVm?.killSession()
                }) {
                    Text("Stop session", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { killDialogOpen = false }) { Text("Cancel") }
            },
        )
    }

    // A resume relaunches the agent on that machine — cheap, but not silent:
    // the same confirm shape as the other remote commands.
    val resume = resumeTarget
    if (resumeConfirmOpen && resume != null) {
        AlertDialog(
            onDismissRequest = { resumeConfirmOpen = false },
            title = { Text("Resume this run?") },
            text = {
                Text(
                    "Starts the agent again on ${resume.deviceLabel}, in the same " +
                        "workspace, picking up where the run stopped.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        resumeConfirmOpen = false
                        sessionVm?.resumeRun(resume)
                    },
                ) { Text("Resume") }
            },
            dismissButton = {
                TextButton(onClick = { resumeConfirmOpen = false }) { Text("Cancel") }
            },
        )
    }
}
