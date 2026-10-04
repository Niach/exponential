package com.exponential.app.ui.work

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.changesFaceCounts
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DetailChrome
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.domain.activeQuestionIds
import com.exponential.app.domain.availableFaces
import com.exponential.app.domain.canOfferFixConflicts
import com.exponential.app.domain.codingTarget
import com.exponential.app.domain.faceShowsContextMenu
import com.exponential.app.domain.fallbackFace
import com.exponential.app.domain.isSessionLive
import com.exponential.app.domain.issueResultsRun
import com.exponential.app.domain.parseSessionResultGroups
import com.exponential.app.domain.runHasEnded
import com.exponential.app.domain.shouldAutoBack
import com.exponential.app.domain.CodingReadiness
import com.exponential.app.ui.issue.ChangesLoadState
import com.exponential.app.ui.issue.CodingReadinessSheet
import com.exponential.app.ui.issue.CodingReadinessViewModel
import com.exponential.app.ui.issue.ChangesViewModel
import com.exponential.app.ui.issue.CommentThreadViewModel
import com.exponential.app.ui.issue.IssueDetailViewModel
import com.exponential.app.ui.issue.IssueFace
import com.exponential.app.ui.issue.IssueMenuActions
import com.exponential.app.ui.issue.StartButtonUi
import com.exponential.app.ui.issue.StartCircle
import com.exponential.app.ui.issue.rememberIssueFaceController
import com.exponential.app.ui.issue.toDiffFile
import com.exponential.app.ui.components.GlassSegmentedControlDefaults
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.IssueChipSize
import com.exponential.app.ui.components.IssueChipStack
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.session.AgentSessionViewModel
import com.exponential.app.ui.session.RunFace
import com.exponential.app.ui.session.sessionRowTitle
import com.exponential.app.ui.steer.ActionRunState
import dev.chrisbanes.haze.rememberHazeState
import kotlinx.coroutines.flow.MutableStateFlow

// EXP-893: the phone WORK SCREEN — one screen per subject (an issue, or a
// session) with up to four FACES held as screen state, never as navigation:
// Issue, Run, Changes and Results (`domain/WorkFaces.kt`, the ×4 rules).
// System Back pops the whole screen. Opening a run from any list opens it on
// the Run face; opening an issue lands on the Issue face. EXP-1150: the faces
// are TABS — [WorkFaceTabs] in the header under the title row, the same on
// every face, plus a horizontal swipe on the body (`swipeTarget`); `Runs`
// tapped again opens the run menu. The host owns the Scaffold, the top bar
// (title dot + verbs + the issue menu), the kill / resume confirms, the ended
// edge, the ONE Merge PR control (the header's pill beside the tabs, every
// face) and each face's trailing circle.

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
    // EXP-1121: the "Ready to code?" fixes that leave the screen — team
    // settings (board repositories + the GitHub connection) for the issue's
    // team, and the Devices tab. Null hides the fix (never a dead button).
    onOpenTeamSettings: ((teamId: String) -> Unit)? = null,
    onOpenDevices: (() -> Unit)? = null,
    // EXP-1097: the Sub-issues `+` — the create screen on the parent's board
    // with the parent preset. Null hides the `+`.
    onCreateSubIssue: ((boardId: String, parentId: String) -> Unit)? = null,
    // EXP-933: the face to open on (`issue/{id}?face=results` from an agent
    // message's inbox row or push); null = the subject's default face. A face
    // not yet available falls back like any vanished face.
    initialFace: WorkFaceKind? = null,
) {
    // ── Screen state (survives rotation and process death) ─────────────────
    var faceName by rememberSaveable { mutableStateOf(initialFace?.name) }
    var initialFacePending by rememberSaveable { mutableStateOf(initialFace != null) }
    var shownSessionId by rememberSaveable {
        mutableStateOf((subject as? WorkSubject.Session)?.id)
    }
    // A run the reader PICKED (or arrived on) stays; only an auto-chosen one
    // follows `codingTarget` as the issue's runs come and go.
    var pinnedByUser by rememberSaveable { mutableStateOf(subject is WorkSubject.Session) }
    var killDialogOpen by rememberSaveable { mutableStateOf(false) }
    // EXP-876: the covered-issues sheet behind a batch run's title.
    var coveredSheetOpen by remember { mutableStateOf(false) }
    // EXP-897/SLOP-3: the "Related work" sheet behind the top bar's badge.
    var graphSheetOpen by remember { mutableStateOf(false) }
    var resumeConfirmOpen by rememberSaveable { mutableStateOf(false) }

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
    // EXP-974: the shown run's resume chain, newest first — the `Runs` menu's
    // rows for an ISSUE-LESS subject (a chat, action or batch run), so a
    // resumed run and its successor share one tab and its menu picks
    // between them. An issue-bound subject keeps the issue's own runs, which
    // already include its resumes.
    val chainRuns by (sessionVm?.chainRuns ?: remember { MutableStateFlow(emptyList()) })
        .collectAsStateWithLifecycle()
    val menuRuns = if (issueId != null) issueRuns else chainRuns
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
    // EXP-1121: whether this issue can Start coding right now (the three
    // readiness steps), fed from the issue's synced board + membership.
    val readinessVm: CodingReadinessViewModel? = issueId?.let { id ->
        hiltViewModel<CodingReadinessViewModel>(key = "readiness:$id")
    }
    val readinessBoard = issueState?.board
    val readinessMember = permissions?.isMember == true
    LaunchedEffect(readinessVm, readinessBoard, readinessMember, steerEnabled) {
        readinessVm?.bind(
            CodingReadinessViewModel.Host(
                board = readinessBoard,
                isMember = readinessMember,
                remoteStartEnabled = steerEnabled,
            ),
        )
    }
    val readinessState by (readinessVm?.state ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val readiness = readinessState?.readiness
    var readinessSheetOpen by rememberSaveable { mutableStateOf(false) }
    // Back from settings, Devices or the GitHub hop: re-probe, the rows tick.
    var readinessResumed by remember { mutableStateOf(false) }
    LifecycleResumeEffect(readinessVm) {
        if (readinessResumed) readinessVm?.reload()
        readinessResumed = true
        onPauseOrDispose {}
    }

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
    // EXP-888: a sweep end (`ended_by = 'stale'`) is NOT an end — the device
    // ignores the flip and its next heartbeat revives the row — so the pill
    // keeps saying Stop and the composer stays open.
    val sessionEnded = shownSession?.let { runHasEnded(it) } == true
    val ownShown = shownSession != null && currentUserId != null && shownSession?.userId == currentUserId
    val shownLive = shownSession?.let { isSessionLive(it, liveClock) } == true
    val awaitingInput = phase == AgentPhase.Live &&
        remember(activity.feed) { activeQuestionIds(activity.feed) }.isNotEmpty()
    val latestDiff = activity.latestDiff
    // EXP-895: the raw `git diff` is parsed ONCE here — the Changes face draws
    // the files.
    val parsedDiff = remember(latestDiff) { latestDiff?.let { Diff.parse(it) } }

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
    // EXP-932: source B — the issue's open PR files, drawn only when there is
    // no live diff. It is resolved HERE, not inside the Changes face, so the
    // screen reads ONE file list for the run (web's `changesFiles`). The model
    // exists whenever the issue's PR is open — a replayed `latestDiff` of an
    // ENDED run must not take the header's Merge PR with it (the pill merges
    // through this model once the run's own target is gone).
    val changesVm: ChangesViewModel? = if (prOpen && issueId != null) {
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(
            key = "changes:$issueId",
        ) { factory -> factory.create(issueId) }
    } else {
        null
    }
    val prLoadState by (changesVm?.load ?: remember { MutableStateFlow<ChangesLoadState?>(null) })
        .collectAsStateWithLifecycle()
    // The Changes face's caption source stays the PR files only while no live
    // diff draws.
    val prLoad = if (latestDiff == null) prLoadState else null
    // The ONE list — both sources land in the shared model, exactly as the
    // Reviews page does it.
    val changesFiles: List<Diff.File> = remember(parsedDiff, prLoad) {
        val load = prLoad
        when {
            parsedDiff != null -> parsedDiff.files
            load is ChangesLoadState.Loaded -> load.files.map { it.toDiffFile() }
            else -> emptyList()
        }
    }
    // EXP-1152: the Changes tab wears these files' `+N −M` (desktop parity).
    val changesCounts = remember(changesFiles) { changesFaceCounts(Diff.totals(changesFiles)) }
    // EXP-879: the run's published screenshots, parsed off the synced blob.
    // Results is a SUB-FACE of Run — no shown run, no results — which the
    // `shownSession` read gives for free.
    // EXP-933: an ISSUE subject shows the issue's results for EVERYONE —
    // `issueResultsRun` over every member's runs on it (my target run when it
    // has any, else the newest run with results) — so a teammate reading the
    // issue sees the report too. An issue-less run keeps its own results.
    val resultsSource: String? = if (issueId != null) {
        issueResultsRun(issueSessions, issueId, shownSessionId, currentUserId, liveClock)?.results
    } else {
        shownSession?.results
    }
    val resultGroups = remember(resultsSource) { parseSessionResultGroups(resultsSource) }
    val faces = availableFaces(
        hasIssue = issueId != null,
        hasRun = shownSessionId != null,
        hasChanges = hasChanges,
        hasResults = resultGroups.isNotEmpty(),
    )
    val wantedFace = faceName?.let { name -> WorkFaceKind.entries.firstOrNull { it.name == name } }
        ?: if (subject is WorkSubject.Session) WorkFaceKind.Run else WorkFaceKind.Issue
    val face = fallbackFace(wantedFace, faces) ?: wantedFace
    // Where a face lands when it vanishes under the reader (the diff cleared,
    // the run row went): changes → run → issue.
    // EXP-933: an ARRIVAL face (`?face=results`) waits for its data to sync
    // instead of being overwritten by the fallback on the first empty frame;
    // it settles once it shows, or once the reader picks another face.
    LaunchedEffect(face, wantedFace) {
        if (initialFacePending) {
            if (face == wantedFace || wantedFace != initialFace) initialFacePending = false
            return@LaunchedEffect
        }
        if (face != wantedFace) faceName = face.name
    }

    // EXP-1121: Start coding ALWAYS renders for a member while remote start
    // is on (`readiness.visible`) — solid and white once every step is met,
    // dashed with an amber dot while one is missing, inert while loading.
    val startUi: StartButtonUi? = if (issueId == null) null else readiness?.let(StartButtonUi::from)
    fun startCoding() {
        val id = issueId ?: return
        val r = readiness ?: return
        when {
            !r.visible || r.loading -> Unit
            // EXP-825: the composer IS the launcher.
            r.ready -> onOpenAgent(AgentComposerSeed(issueIds = listOf(id)))
            else -> readinessSheetOpen = true
        }
    }

    // EXP-1162: the state dots ride the face TABS, never the title — the Run
    // tab while the shown run is live (amber while it waits on a person), an
    // open pull request on Results (or Changes without Results).
    val faceDots = DetailChrome.faceDots(
        faces = faces,
        runLive = shownLive && !sessionEnded,
        needsInput = shownSession?.needsInput == true || awaitingInput,
        prOpen = prOpen,
    )
    // EXP-1150: Start coding once the shown run ended for good.
    val offerStart = sessionEnded && ownShown && resumeTarget == null && issueId != null &&
        readiness?.visible == true
    // EXP-1145: a stack member's Merge asks first, per merge source.
    val sessionStackChoice by (sessionVm?.stackMergeChoice ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    // The live run merges its own target (EXP-678/734).
    val sessionCanMerge = sessionVm != null && mergeTarget != null &&
        !sessionEnded && phase !is AgentPhase.Ended
    val sessionMerge: ChangesMergeControl? = if (sessionCanMerge) {
        // EXP-706/734: a REAL conflict on an ISSUE target swaps the verb for
        // the recovery run.
        val fix = mergeTarget is MergeTarget.Issue &&
            canOfferFixConflicts(mergeError, mergeIssue?.branch, steerEnabled = steerEnabled == true)
        ChangesMergeControl(
            label = if (fix) "Fix conflicts" else DomainContract.diffUiMergePr,
            fixConflicts = fix,
            loading = merging,
            error = mergeError?.message,
            confirmText = when (mergeTarget) {
                is MergeTarget.Session ->
                    "Merges this run's pull request and ends the run."
                else ->
                    "Merges the pull request, completes every linked issue, " +
                        "and ends the run."
            },
            onConfirm = { sessionVm.merge() },
            stackChoice = if (fix) null else sessionStackChoice,
            stackIssueId = (mergeTarget as? MergeTarget.Issue)?.issueId,
            onMergeStack = { through -> sessionVm.mergeStack(through) },
            onFixConflicts = {
                onOpenAgent(
                    AgentComposerSeed(
                        actionId = DomainContract.builtinFixConflictsId,
                        prIssueId = mergeIssue?.id,
                    ),
                )
            },
        )
    } else {
        null
    }
    // The PR-only source merges through the issue (EXP-156).
    val changesMerging by (changesVm?.merging ?: remember { MutableStateFlow(false) })
        .collectAsStateWithLifecycle()
    val changesError by (changesVm?.actionError ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val changesErrorFrom by (changesVm?.actionErrorFrom ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val changesConflict by (changesVm?.actionErrorIsConflict ?: remember { MutableStateFlow(false) })
        .collectAsStateWithLifecycle()
    val changesStackChoice by (changesVm?.stackMergeChoice ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    // EXP-1150: the header's ONE Merge PR, on every face — the live run's own
    // target first, else the issue's open PR.
    val headerMerge: ChangesMergeControl? = when {
        sessionMerge != null -> sessionMerge
        changesVm != null && prOpen && permissions?.isMember == true -> {
            val fix = changesConflict &&
                changesErrorFrom == ChangesViewModel.PrAction.Merge &&
                steerEnabled == true && !issue.branch.isNullOrBlank()
            ChangesMergeControl(
                label = if (fix) "Fix conflicts" else DomainContract.diffUiMergePr,
                fixConflicts = fix,
                loading = changesMerging,
                error = changesError,
                confirmText = "Squash-merges PR #${issue.prNumber ?: ""} via the GitHub App. " +
                    "Any live run for it ends.",
                onConfirm = { changesVm.mergePr() },
                stackChoice = if (fix) null else changesStackChoice,
                stackIssueId = issueId,
                onMergeStack = { through -> changesVm.mergeStack(through) },
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
    // The Run face's bar: Start coding once the shown run ended for good.
    val runTrailing: (@Composable () -> Unit)? = startUi?.takeIf { offerStart }?.let { ui ->
        { StartCircle(ui = ui, onClick = { startCoding() }) }
    }
    // The Issue face's right circle: Start coding whenever the issue can be
    // started — the only trailing candidate now the switcher is gone.
    val issueTrailing: @Composable () -> Unit = {
        if (startUi != null) {
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
    // EXP-935: a Resume / account switch is IN FLIGHT — the machine ends the
    // live run before its continuation row syncs, and the successor lands in
    // `startedSessionId` a moment later to swap into this very screen. Popping
    // in that gap is what sent an account switch on a chat run back to the
    // list. A send that never lands goes Failed, which clears this and lets
    // the auto-back happen then.
    val continuationPending = runState is ActionRunState.Sending || runState is ActionRunState.Sent
    // EXP-888: `runHasEnded`, never the raw status — a swept row reads `ended`
    // while its agent is still running, and backing out of it would strand the
    // run the next heartbeat revives. `endedBy` is a key too: the sweep's
    // `stale` turning into a real end never moves `status`.
    LaunchedEffect(shownSession?.status, shownSession?.endedBy, issueId, continuationPending) {
        val row = shownSession ?: return@LaunchedEffect
        if (!runHasEnded(row)) {
            wasLive = true
        } else if (
            shouldAutoBack(
                ended = true,
                wasLive = wasLive,
                issueId = issueId,
                continuationPending = continuationPending,
            )
        ) {
            onBack()
        }
    }

    // ── The related-work graph (EXP-897/SLOP-3) ─────────────────────────────
    // ONE model for the badge and its sheet: the blockers off the `blocks`
    // relations, the batch off a shared `pr_url`, the stack off `pr_base_branch`.
    val graphVm: PrGraphViewModel = hiltViewModel()
    LaunchedEffect(issueId, shownSessionId) { graphVm.bind(issueId, shownSessionId) }
    val graph by graphVm.graph.collectAsStateWithLifecycle()
    // EXP-876: what names an issue-less BATCH run in the bar below.
    val batchIssues by (sessionVm?.batchIssues ?: remember { MutableStateFlow(emptyList()) })
        .collectAsStateWithLifecycle()

    // ── Top bar inputs ──────────────────────────────────────────────────────
    val title = when {
        issue != null -> issue.identifier
        issueId != null -> ""
        // EXP-968: the joined issue when there is one — never a hard null
        // that would make the bar say "Issue syncing…" about a synced row.
        shownSession != null -> sessionRowTitle(shownSession!!, issue, batchIssues)
        else -> "Run"
    }
    val canKill = ownShown && !sessionEnded && shownLive
    val verb = when {
        face != WorkFaceKind.Run -> null
        canKill -> WorkBarVerb.Stop
        sessionEnded && resumeTarget != null -> WorkBarVerb.Resume
        else -> null
    }

    // EXP-1162: the detail chrome — ONE backdrop every face's scroller feeds
    // (the header band and the bottom strips blur it), and the title-collapse
    // input the Issue face's title row reports into.
    val hazeState = rememberHazeState()
    val titleCollapse = remember { TitleCollapseState() }
    // Faces without a title row of their own are always collapsed.
    val titleCollapsed = face != WorkFaceKind.Issue || titleCollapse.issueTitleCollapsed

    ProvideMarkdownToolbar {
        CompositionLocalProvider(
            LocalDetailHaze provides hazeState,
            LocalTitleCollapse provides titleCollapse,
        ) {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                WorkTopBar(
                    title = title,
                    // EXP-876: a multi-issue run's title (`EXP-874 +2`) is the
                    // stacked issue chip that opens the issues it covers.
                    titleContent = if (issueId == null && batchIssues.size > 1) {
                        {
                            IssueChipStack {
                                IssueChip(
                                    identifier = title,
                                    title = null,
                                    status = null,
                                    size = IssueChipSize.Sm,
                                    onClick = { coveredSheetOpen = true },
                                    modifier = Modifier.testTag("work-covered-issues"),
                                )
                            }
                        }
                    } else {
                        null
                    },
                    // EXP-1162: an issue subject's header breaks into
                    // identifier-over-title; issue-less and batch runs keep
                    // their one title.
                    collapsedTitle = issue?.title,
                    collapsed = titleCollapsed,
                    onHeaderBottom = titleCollapse::reportHeaderBottom,
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
                    // SLOP-16: a quiet icon button beside the `…` whose glyph
                    // names the shape (stack, batch, blockers).
                    badge = {
                        PrGraphBadge(graph = graph) { graphSheetOpen = true }
                    },
                    // EXP-934: the `…` belongs to the ISSUE, so it shows on the
                    // Issue face alone (`faceShowsContextMenu`) — Run, Changes
                    // and Results keep only the run's own verb. A Delete issue
                    // beside a running agent acts on a subject that face is not
                    // even showing.
                    menu = if (issueVm != null && faceShowsContextMenu(face)) {
                        { IssueMenuActions(viewModel = issueVm, controller = issueController) }
                    } else {
                        null
                    },
                    // EXP-1150: the face TABS, part of the header on every face.
                    tabs = {
                        WorkFaceTabs(
                            faces = faces,
                            face = face,
                            onFace = { faceName = it.name },
                            runs = menuRuns,
                            shownRunId = shownSessionId,
                            onPickRun = { id ->
                                shownSessionId = id
                                pinnedByUser = true
                                faceName = WorkFaceKind.Run.name
                            },
                            changesCounts = changesCounts,
                            dots = faceDots,
                            runAgent = shownSession?.agent,
                            runBusy = shownSession?.agentBusy == true,
                            trailing = headerMerge?.let { merge ->
                                {
                                    MergePrHeaderPill(
                                        merge,
                                        modifier = Modifier.height(GlassSegmentedControlDefaults.Height),
                                    )
                                }
                            },
                        )
                    },
                )
            },
        ) { scaffoldPadding ->
            // EXP-1150: the body swipe to the neighbour (the tabs ride the
            // header). EXP-1152: a pager — each face is a PAGE.
            WorkFaceFrame(
                faces = faces,
                face = face,
                padding = scaffoldPadding,
                onFace = { faceName = it.name },
            ) { page, padding ->
                when (page) {
                    WorkFaceKind.Issue -> if (issueVm != null && commentVm != null) {
                        IssueFace(
                            viewModel = issueVm,
                            commentViewModel = commentVm,
                            controller = issueController,
                            padding = padding,
                            onBack = onBack,
                            onOpenIssue = onOpenIssue,
                            onOpenChanges = {
                                if (hasChanges) faceName = WorkFaceKind.Changes.name else issueId?.let(onOpenChanges)
                            },
                            trailingBarSlot = issueTrailing,
                            onAddSubIssue = onCreateSubIssue?.let { create ->
                                issue?.let { parent -> { create(parent.boardId, parent.id) } }
                            },
                        )
                    }
                    WorkFaceKind.Run -> key(shownSessionId) {
                        if (sessionVm != null) {
                            RunFace(
                                viewModel = sessionVm,
                                padding = padding,
                                onOpenIssue = onOpenIssue,
                                trailingBarSlot = runTrailing,
                                onOpenResults = if (WorkFaceKind.Results in faces) {
                                    { faceName = WorkFaceKind.Results.name }
                                } else {
                                    null
                                },
                            )
                        }
                    }
                    WorkFaceKind.Changes -> key(shownSessionId) {
                        ChangesFace(
                            padding = padding,
                            // EXP-932: the files the host resolved.
                            files = changesFiles,
                            prLoad = prLoad,
                        )
                    }
                    WorkFaceKind.Results -> ResultsFace(
                        padding = padding,
                        groups = resultGroups,
                    )
                }
            }
        }
        }
    }

    // EXP-897/SLOP-3: the badge's "Related work" sheet.
    if (graphSheetOpen) {
        val graphStatuses by graphVm.issueStatuses.collectAsStateWithLifecycle()
        val graphUsers by graphVm.users.collectAsStateWithLifecycle()
        PrGraphSheet(
            graph = graph,
            statuses = graphStatuses,
            users = graphUsers,
            onOpenIssue = onOpenIssue,
            // SLOP-16 r3: a pull request row opens its Changes: this screen's
            // own face for the subject's PR, else the review route.
            onOpenPr = { id ->
                if (id == issueId && hasChanges) faceName = WorkFaceKind.Changes.name else onOpenChanges(id)
            },
            onDismiss = { graphSheetOpen = false },
        )
    }

    // EXP-876: the issues a batch run covers, each opening its issue.
    if (coveredSheetOpen && batchIssues.size > 1) {
        val statusTargets by (sessionVm?.issueRefCandidates ?: remember { MutableStateFlow(emptyList()) })
            .collectAsStateWithLifecycle()
        val statusById = remember(statusTargets) {
            statusTargets.associate { it.issueId to it.resolvedStatus }
        }
        GlassSheet(title = COVERED_ISSUES_TITLE, onDismiss = { coveredSheetOpen = false }) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp)
                    .padding(bottom = 8.dp)
                    .testTag("work-covered-issues-sheet"),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                batchIssues.forEach { covered ->
                    IssueChip(
                        identifier = covered.identifier,
                        title = covered.title,
                        status = statusById[covered.id],
                        onClick = {
                            coveredSheetOpen = false
                            onOpenIssue(covered.id)
                        },
                    )
                }
            }
        }
    }

    // EXP-1121: the "Ready to code?" checklist behind a not-ready Start coding.
    if (readinessSheetOpen && readinessVm != null && readiness?.visible == true) {
        val teamId = readinessState?.board?.teamId
        val openTeamSettings = onOpenTeamSettings?.takeIf { teamId != null }
        val fixes = CodingReadiness.Fix.entries.filterTo(mutableSetOf()) { fix ->
            when (fix) {
                // Team settings is where Android edits a board's repository
                // (member-level `boards.setRepository`).
                CodingReadiness.Fix.BOARD_SETTINGS -> openTeamSettings != null
                // SLOP-26: Connect GitHub runs in the sheet's own picker for a
                // member with a board; team settings is the fallback.
                CodingReadiness.Fix.CONNECT_GITHUB -> permissions?.isMember == true || openTeamSettings != null
                CodingReadiness.Fix.OPEN_DEVICES -> onOpenDevices != null
                // `boards.setRepository` = `mutate_resources`: any member.
                CodingReadiness.Fix.CHOOSE_REPOSITORY -> permissions?.isMember == true
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
                issueId?.let { onOpenAgent(AgentComposerSeed(issueIds = listOf(it))) }
            },
            onDismiss = { readinessSheetOpen = false },
        )
    }

    // ── Confirms ────────────────────────────────────────────────────────────
    if (killDialogOpen) {
        AlertDialog(
            onDismissRequest = { killDialogOpen = false },
            // EXP-818: ONE word for ending a run, wherever it is watched from.
            title = { Text("Stop this run?") },
            text = {
                Text(
                    "This stops the agent on the desktop " +
                        "and ends the run. It cannot be undone.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    killDialogOpen = false
                    sessionVm?.killSession()
                }) {
                    Text("Stop run", color = MaterialTheme.colorScheme.error)
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

/** EXP-876: the covered-issues sheet's title behind a batch run's title. */
internal const val COVERED_ISSUES_TITLE = "Issues in this run"
