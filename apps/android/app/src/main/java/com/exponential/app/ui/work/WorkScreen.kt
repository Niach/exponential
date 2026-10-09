package com.exponential.app.ui.work

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.codingSessionDisplayState
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
import com.exponential.app.domain.GuideSectionKey
import com.exponential.app.domain.PrStack
import com.exponential.app.domain.guideSectionPage
import com.exponential.app.domain.sessionResultsForPr
import com.exponential.app.domain.runHasEnded
import com.exponential.app.domain.shouldAutoBack
import com.exponential.app.domain.CodingReadiness
import com.exponential.app.domain.ClosePr
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.issue.CodingReadinessSheet
import com.exponential.app.ui.issue.CodingReadinessViewModel
import com.exponential.app.ui.issue.CommentThreadViewModel
import com.exponential.app.ui.issue.IssueDetailViewModel
import com.exponential.app.ui.issue.IssueFace
import com.exponential.app.ui.issue.IssueMenuActions
import com.exponential.app.ui.issue.StartButtonUi
import com.exponential.app.ui.issue.StartCircle
import com.exponential.app.ui.issue.rememberIssueFaceController
import com.exponential.app.ui.issue.toDiffFile
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.LocalToaster
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
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.domain.Prompts

// EXP-893: the phone WORK SCREEN — one screen per subject (an issue, or a
// session) with up to three FACES held as screen state, never as navigation:
// Issue, Run and Guide (`domain/WorkFaces.kt`, the ×4 rules; EXP-1251 merged
// Changes + Results into the Guide, its section pages under it).
// System Back pops the whole screen. Opening a run from any list opens it on
// the Run face; opening an issue lands on the Issue face. EXP-1150: the faces
// are TABS — [WorkFaceTabs] in the header under the title row, the same on
// every face, plus a horizontal swipe on the body (`swipeTarget`); `Runs`
// tapped again opens the run menu. The host owns the Scaffold, the top bar
// (title dot + verbs + the issue menu, Close PR included), the kill / resume /
// close confirms, the ended edge, the ONE Merge PR control and each face's
// trailing circle. EXP-1154: the Merge rides the floating bar of EVERY face —
// the white [MergeCapsule] in the Guide bar's centred cluster, EXP-1191: the
// glyph-only [MergeCircle] right of the Issue / Run composer capsule — and the
// review of a PR IS this screen on its Guide face. EXP-1248: `Merge stack`
// while the PR sits in an open stack, ONE confirm; the Guide's Stack card
// swaps the subject to another member IN PLACE.

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
    /** EXP-1154: ANOTHER issue's Guide face (a related-work PR row). */
    onOpenIssueChanges: (issueId: String) -> Unit,
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
    // EXP-1251: the issue the screen shows — the subject's, until the Guide's
    // Stack card swaps it to another member IN PLACE (no new screen).
    var subjectIssueId by rememberSaveable { mutableStateOf((subject as? WorkSubject.Issue)?.id) }
    // EXP-1251: the Guide's open section page (`lead` | `other` | `all` | N);
    // null = the Guide itself.
    var guideSectionName by rememberSaveable { mutableStateOf<String?>(null) }
    // A run the reader PICKED (or arrived on) stays; only an auto-chosen one
    // follows `codingTarget` as the issue's runs come and go.
    var pinnedByUser by rememberSaveable { mutableStateOf(subject is WorkSubject.Session) }
    var killDialogOpen by rememberSaveable { mutableStateOf(false) }
    // EXP-876: the covered-issues sheet behind a batch run's title.
    var coveredSheetOpen by remember { mutableStateOf(false) }
    // EXP-897/SLOP-3: the "Related work" sheet behind the top bar's badge.
    var graphSheetOpen by remember { mutableStateOf(false) }
    var resumeConfirmOpen by rememberSaveable { mutableStateOf(false) }
    // EXP-1154: the issue menu's Close PR confirm.
    var closePrConfirmOpen by rememberSaveable { mutableStateOf(false) }
    // EXP-1154: an EXPLICIT ask for the Guide (the PR row's tap, a
    // `?face=guide` arrival) is honoured for a merged or closed PR too.
    var changesRequested by rememberSaveable { mutableStateOf(initialFace == WorkFaceKind.Guide) }

    // ── The shown run's model, one per id ──────────────────────────────────
    val sessionVm: AgentSessionViewModel? = shownSessionId?.let { id ->
        hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
            key = "session:$id",
        ) { factory -> factory.create(id) }
    }
    val shownSession by (sessionVm?.session ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()

    // ── The issue, from the subject or the run's own row ───────────────────
    val issueId: String? = subjectIssueId ?: shownSession?.issueId
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
    // EXP-1154: a pushed branch with no PR yet has Changes too (its
    // `repositories.branchDiff`), web / iOS / desktop parity.
    val pushedBranch = issue != null && issue.prUrl.isNullOrBlank() && !issue.branch.isNullOrBlank()
    val requestedWork = changesRequested && issue != null &&
        (!issue.prUrl.isNullOrBlank() || !issue.branch.isNullOrBlank())
    val issueChanges = prOpen || pushedBranch || requestedWork
    val hasChanges = latestDiff != null || issueChanges
    // EXP-895: the PR the Guide links out to — the issue's, or the
    // shown run's OWN issue-less one (EXP-734). It wears the header's action
    // slot now, because the bar's leading slot opens the changed-files sheet.
    val changesPrUrl = (issue?.prUrl ?: shownSession?.prUrl)?.takeIf { it.isNotBlank() }
    // EXP-932: source B — the issue's open PR files, drawn only when there is
    // no live diff. It is resolved HERE, not inside the Changes face, so the
    // screen reads ONE file list for the run (web's `changesFiles`). The model
    // exists whenever the issue's PR is open — a replayed `latestDiff` of an
    // ENDED run must not take the Merge PR with it (the capsule merges
    // through this model once the run's own target is gone). EXP-1154: a
    // pushed branch's files load through it too.
    val changesVm: ChangesViewModel? = if (issueChanges && issueId != null) {
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(
            key = "changes:$issueId",
        ) { factory -> factory.create(ChangesSource.Issue(issueId)) }
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
    // EXP-1251: the diff the Guide COUNTS — null while it is not known yet
    // (no Changes rows rather than wrong ones).
    val guideFiles: List<Diff.File>? = when {
        parsedDiff != null -> parsedDiff.files
        prLoad is ChangesLoadState.Loaded -> changesFiles
        else -> null
    }
    // EXP-879: the run's published screenshots, parsed off the synced blob.
    // Results is a SUB-FACE of Run — no shown run, no results — which the
    // `shownSession` read gives for free.
    // EXP-933: an ISSUE subject shows the issue's results for EVERYONE —
    // `issueResultsRun` over every member's runs on it (my target run when it
    // has any, else the newest run with results) — so a teammate reading the
    // issue sees the report too. An issue-less run keeps its own results.
    // EXP-1251: an issue reads only its OWN PR's topics (plus the untagged
    // ones) — a run that stacked a second PR keeps each issue to its sections.
    val resultsSource: String? = if (issueId != null) {
        issueResultsRun(issueSessions, issueId, shownSessionId, currentUserId, liveClock, issue?.prUrl)
            ?.results
            ?.let { raw -> sessionResultsForPr(raw, issue?.prUrl) }
    } else {
        shownSession?.results
    }
    val resultGroups = remember(resultsSource) { parseSessionResultGroups(resultsSource) }
    // EXP-1154: an issue with an OPEN PR always has Results — the run's
    // report when one exists (the Guide), else the PR's GitHub body.
    val hasResults = resultGroups.isNotEmpty() || (issueId != null && prOpen)
    val prDescription by (changesVm?.prDescription ?: remember { MutableStateFlow(null) })
        .collectAsStateWithLifecycle()
    val faces = availableFaces(
        hasIssue = issueId != null,
        hasRun = shownSessionId != null,
        hasResults = hasResults,
        hasDiff = hasChanges,
    )
    val wantedFace = faceName?.let { name -> WorkFaceKind.entries.firstOrNull { it.name == name } }
        ?: if (subject is WorkSubject.Session) WorkFaceKind.Run else WorkFaceKind.Issue
    val face = fallbackFace(wantedFace, faces) ?: wantedFace
    // Where a face lands when it vanishes under the reader (the diff cleared,
    // the run row went): guide → run → issue.
    // EXP-933: an ARRIVAL face (`?face=guide`) waits for its data to sync
    // instead of being overwritten by the fallback on the first empty frame;
    // it settles once it shows, or once the reader picks another face.
    LaunchedEffect(face, wantedFace) {
        if (initialFacePending) {
            if (face == wantedFace || wantedFace != initialFace) initialFacePending = false
            return@LaunchedEffect
        }
        if (face != wantedFace) faceName = face.name
    }
    // The Guide's section page closes once the Guide is not on show.
    LaunchedEffect(face) { if (face != WorkFaceKind.Guide) guideSectionName = null }
    // EXP-1154: the PR body is fetched only once the Guide shows it
    // (the view model keeps it cached per issue, so a swipe back is free).
    val wantsPrBody = changesVm != null && prOpen && resultGroups.isEmpty() &&
        face == WorkFaceKind.Guide
    LaunchedEffect(changesVm, wantsPrBody) {
        if (wantsPrBody) changesVm?.loadDescription()
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
    // open pull request on the Guide.
    val faceDots = DetailChrome.faceDots(
        faces = faces,
        runLive = shownLive && !sessionEnded,
        needsInput = shownSession?.needsInput == true || awaitingInput,
        prOpen = prOpen,
    )
    // EXP-1184: the Run tab's mark reads the ×4 display rule, the viewer's
    // own "waiting on a person" (an open question in the feed) overlaid on the
    // synced flag.
    val runMarkState = shownSession?.takeIf { shownLive && !sessionEnded }?.let {
        codingSessionDisplayState(
            status = it.status,
            needsInput = it.needsInput || awaitingInput,
            agentBusy = it.agentBusy,
            prState = issue?.prState ?: it.prState,
        )
    }
    // EXP-1175: the Run face status row's display state — the same rule,
    // ungated (the row itself tells paused and ended apart).
    val runDisplayState = shownSession?.let {
        codingSessionDisplayState(
            status = it.status,
            needsInput = it.needsInput || awaitingInput,
            agentBusy = it.agentBusy,
            prState = issue?.prState ?: it.prState,
        )
    }
    // EXP-1150: Start coding once the shown run ended for good.
    val offerStart = sessionEnded && ownShown && resumeTarget == null && issueId != null &&
        readiness?.visible == true
    val graphVm: PrGraphViewModel = hiltViewModel()
    LaunchedEffect(issueId, shownSessionId) { graphVm.bind(issueId, shownSessionId) }
    val graph by graphVm.graph.collectAsStateWithLifecycle()
    // EXP-876: what names an issue-less BATCH run in the bar below.
    val batchIssues by (sessionVm?.batchIssues ?: remember { MutableStateFlow(emptyList()) })
        .collectAsStateWithLifecycle()
    // EXP-1215: how many issues the subject's ONE pull request covers.
    val prIssueCount = graph.batch?.issues?.size ?: 1
    // EXP-1248: the team's issues — the Stack card and the ONE stack confirm.
    val teamIssues by graphVm.teamIssues.collectAsStateWithLifecycle()
    // Non-null = Merge lands the whole open stack (labelled `Merge stack`).
    fun stackConfirmFor(row: com.exponential.app.data.db.IssueEntity?): PrStack.StackMergeConfirm? =
        row?.let { PrStack.stackMergeConfirm(it, teamIssues, PrStack.StackConfirmMode.Stack) }
    val sessionStackConfirm = (mergeTarget as? MergeTarget.Issue)?.let { stackConfirmFor(mergeIssue) }
    // The live run merges its own target (EXP-678/734).
    val sessionCanMerge = sessionVm != null && mergeTarget != null &&
        !sessionEnded && phase !is AgentPhase.Ended
    // EXP-1233: a REAL conflict on an ISSUE target (with a recorded branch and
    // remote start) OPENS the Fix merge conflicts composer — the ViewModel
    // reports it once ([AgentSessionViewModel.conflictRefusals], collected
    // below) and that refusal never toasts. Every other refusal still does.
    val sessionConflictOpens = mergeTarget is MergeTarget.Issue &&
        canOfferFixConflicts(mergeError, mergeIssue?.branch, steerEnabled = steerEnabled == true)
    val sessionMerge: ChangesMergeControl? = if (sessionCanMerge) {
        ChangesMergeControl(
            label = if (sessionStackConfirm != null) DomainContract.diffUiMergeStack else DomainContract.diffUiMergePr,
            loading = merging,
            error = mergeError?.takeUnless { sessionConflictOpens }?.message,
            confirmPrompt = when (mergeTarget) {
                is MergeTarget.Session -> Prompts.MergeRunPr.prompt(shownSession?.prNumber)
                else -> Prompts.MergeIssuePr.prompt(mergeIssue?.prNumber, prIssueCount)
            },
            onConfirm = { sessionVm.merge() },
            stackConfirm = sessionStackConfirm,
            onMergeStack = { through -> sessionVm.mergeStack(through) },
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
    val changesStackConfirm = if (changesVm != null) stackConfirmFor(issue) else null
    // EXP-1150/EXP-1154: the ONE Merge PR, on every face's floating bar — the
    // live run's own target first, else the issue's open PR.
    val mergeControl: ChangesMergeControl? = when {
        sessionMerge != null -> sessionMerge
        changesVm != null && prOpen && permissions?.isMember == true -> {
            // EXP-1233: the conflict that opened the composer never toasts.
            val conflictOpens = changesConflict &&
                changesErrorFrom == ChangesViewModel.PrAction.Merge &&
                steerEnabled == true && !issue.branch.isNullOrBlank()
            ChangesMergeControl(
                label = if (changesStackConfirm != null) DomainContract.diffUiMergeStack else DomainContract.diffUiMergePr,
                loading = changesMerging,
                // EXP-1154: a Close PR refusal toasts on its own (below).
                error = changesError.takeIf {
                    changesErrorFrom == ChangesViewModel.PrAction.Merge && !conflictOpens
                },
                confirmPrompt = Prompts.MergeIssuePr.prompt(issue.prNumber, prIssueCount),
                onConfirm = { changesVm.mergePr() },
                stackConfirm = changesStackConfirm,
                onMergeStack = { through -> changesVm.mergeStack(through) },
            )
        }
        else -> null
    }
    // EXP-1233: both merge sources report a conflict-refused plain merge ONCE
    // (a one-shot, so a recomposition never re-navigates); the screen applies
    // the gate it alone can (the PR's branch, remote start) and opens the
    // composer on the Fix merge conflicts builtin, this PR picked and the
    // refusal flagged. The gate reads the LATEST values, not the first
    // composition's.
    val openAgent by rememberUpdatedState(onOpenAgent)
    val latestSteerEnabled by rememberUpdatedState(steerEnabled == true)
    val latestSessionBranch by rememberUpdatedState(mergeIssue?.branch)
    val latestIssueBranch by rememberUpdatedState(issue?.branch)
    val openFixConflicts: (String) -> Unit = { prIssueId ->
        openAgent(
            AgentComposerSeed(
                actionId = DomainContract.builtinFixConflictsId,
                prIssueId = prIssueId,
                conflict = true,
            ),
        )
    }
    LaunchedEffect(sessionVm) {
        sessionVm?.conflictRefusals?.collect { refusal ->
            if (canOfferFixConflicts(refusal.failure, latestSessionBranch, latestSteerEnabled)) {
                openFixConflicts(refusal.issueId)
            }
        }
    }
    LaunchedEffect(changesVm) {
        changesVm?.conflictRefusals?.collect { refusal ->
            if (canOfferFixConflicts(refusal.failure, latestIssueBranch, latestSteerEnabled)) {
                openFixConflicts(refusal.issueId)
            }
        }
    }
    // EXP-1154: a refused merge toasts ONCE here, keyed on the one control's
    // error: every pager page draws its own capsule, so a per-capsule toast
    // repeated per composed neighbour and again on each swipe.
    val toaster = LocalToaster.current
    val mergeRefusal = mergeControl?.error
    var toastedMergeError by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(mergeRefusal) {
        if (mergeRefusal == null) {
            toastedMergeError = null
        } else if (mergeRefusal != toastedMergeError) {
            toastedMergeError = mergeRefusal
            toaster.error(mergeRefusal)
        }
    }
    // A refused Close PR toasts once too (the merge control carries only
    // merge refusals).
    var toastedCloseError by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(changesError, changesErrorFrom) {
        val error = changesError?.takeIf { changesErrorFrom == ChangesViewModel.PrAction.Close }
        if (error == null) {
            toastedCloseError = null
        } else if (error != toastedCloseError) {
            toastedCloseError = error
            toaster.error(error)
        }
    }
    // EXP-1251: the Guide's Stack card — the subject's open stack, top first.
    // A member's tap swaps the subject IN PLACE (the Guide stays); its
    // long-press offers Merge through here (ONE confirm, then a merge through
    // that member) for whoever may merge.
    var mergeThroughConfirm by remember { mutableStateOf<PrStack.StackMergeConfirm?>(null) }
    val stackView = remember(issue, teamIssues) { issue?.let { PrStack.stackView(it, teamIssues) } }
    val guideStack = stackView?.let { view ->
        GuideStack(
            view = view,
            onOpen = { memberId ->
                subjectIssueId = memberId
                shownSessionId = null
                pinnedByUser = false
                guideSectionName = null
                faceName = WorkFaceKind.Guide.name
            },
            onMergeThrough = if (mergeControl != null) {
                { memberId ->
                    teamIssues.firstOrNull { it.id == memberId }?.let { member ->
                        mergeThroughConfirm = PrStack.stackMergeConfirm(member, teamIssues, PrStack.StackConfirmMode.Through)
                    }
                }
            } else {
                null
            },
        )
    }
    // The Run face's bar: Start coding once the shown run ended for good.
    val runTrailing: (@Composable () -> Unit)? = startUi?.takeIf { offerStart }?.let { ui ->
        { StartCircle(ui = ui, onClick = { startCoding() }) }
    }
    // EXP-1191: the Issue and Run faces carry Merge as the bar's glyph-only
    // circle right of the composer capsule (the Guide keeps the white capsule
    // in its cluster).
    val issueMergeSlot: (@Composable () -> Unit)? = mergeControl?.let { merge ->
        { MergeCircle(merge, tag = "$MergeCapsuleTag-issue") }
    }
    val runMergeSlot: (@Composable () -> Unit)? = mergeControl?.let { merge ->
        { MergeCircle(merge, tag = "$MergeCapsuleTag-run") }
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
                    // EXP-895: the Guide's GitHub circle lives in the header's
                    // action slot — the section page's bar leading slot opens
                    // the changed-files sheet.
                    action = changesPrUrl?.takeIf { face == WorkFaceKind.Guide }?.let { url ->
                        { GithubHeaderAction(url) }
                    },
                    // SLOP-16: a quiet icon button beside the `…` whose glyph
                    // names the shape (stack, batch, blockers).
                    badge = {
                        PrGraphBadge(graph = graph) { graphSheetOpen = true }
                    },
                    // EXP-934: the `…` belongs to the ISSUE, so it shows on the
                    // Issue face alone (`faceShowsContextMenu`) — Run and the
                    // Guide keep only the run's own verb. A Delete issue
                    // beside a running agent acts on a subject that face is not
                    // even showing.
                    // EXP-1154: Close PR rides it for a member while the PR is open.
                    menu = if (issueVm != null && faceShowsContextMenu(face)) {
                        {
                            IssueMenuActions(
                                viewModel = issueVm,
                                controller = issueController,
                                onClosePr = if (changesVm != null && prOpen && permissions?.isMember == true) {
                                    { closePrConfirmOpen = true }
                                } else {
                                    null
                                },
                            )
                        }
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
                            dots = faceDots,
                            runAgent = shownSession?.agent,
                            runState = runMarkState,
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
                            // EXP-1154: the PR / branch row opens this screen's
                            // own complete diff under the Guide, for a merged PR too.
                            onOpenChanges = {
                                changesRequested = true
                                guideSectionName = guideSectionParam(GuideSectionKey.All)
                                faceName = WorkFaceKind.Guide.name
                            },
                            trailingBarSlot = issueTrailing,
                            mergeBarSlot = issueMergeSlot,
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
                                mergeBarSlot = runMergeSlot,
                                runState = runDisplayState,
                                // EXP-1154: the transcript card's Open Guide
                                // means the run's REPORT, never the PR body.
                                onOpenResults = if (WorkFaceKind.Guide in faces && resultGroups.isNotEmpty()) {
                                    {
                                        guideSectionName = null
                                        faceName = WorkFaceKind.Guide.name
                                    }
                                } else {
                                    null
                                },
                            )
                        }
                    }
                    WorkFaceKind.Guide -> key(shownSessionId, issueId) {
                        val section = parseGuideSectionParam(guideSectionName)
                        if (section != null) {
                            // EXP-1251: a Changes row's section page.
                            GuideSectionDiff(
                                padding = padding,
                                page = remember(resultGroups, guideFiles, section) {
                                    guideSectionPage(resultGroups, guideFiles, section)
                                },
                                load = prLoad,
                                merge = mergeControl,
                                onBack = { guideSectionName = null },
                            )
                        } else {
                            GuideFace(
                                padding = padding,
                                groups = resultGroups,
                                files = guideFiles,
                                prFallback = prDescription.takeIf { resultGroups.isEmpty() && prOpen },
                                stack = guideStack,
                                merge = mergeControl,
                                onOpenSection = if (hasChanges) {
                                    { key -> guideSectionName = guideSectionParam(key) }
                                } else {
                                    null
                                },
                            )
                        }
                    }
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
            // SLOP-16 r3 / EXP-1154: a pull request row opens its Guide:
            // this screen's own face for the subject's PR, else that issue's
            // Work screen on its Guide.
            onOpenPr = { id ->
                if (id == issueId) {
                    changesRequested = true
                    guideSectionName = null
                    faceName = WorkFaceKind.Guide.name
                } else {
                    onOpenIssueChanges(id)
                }
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
        // EXP-818: ONE word for ending a run, wherever it is watched from.
        PromptAlert(
            prompt = Prompts.StopRun.prompt(),
            onDismiss = { killDialogOpen = false },
            handlers = mapOf(
                "stop" to {
                    killDialogOpen = false
                    sessionVm?.killSession()
                },
            ),
        )
    }

    // EXP-1248: Merge through here (a Stack card member's long-press).
    mergeThroughConfirm?.let { confirm ->
        StackMergeDialog(
            confirm = confirm,
            onConfirm = { through ->
                when {
                    changesVm != null -> changesVm.mergeStack(through)
                    else -> sessionVm?.mergeStack(through)
                }
            },
            onDismiss = { mergeThroughConfirm = null },
        )
    }

    // EXP-1154: Close PR without merging (the issue menu's destructive item),
    // the ×4 copy of `close-pr.json`; a refusal toasts via the Merge capsule's
    // model error (`actionError`).
    if (closePrConfirmOpen && changesVm != null) {
        val linked by changesVm.linkedIssueCount.collectAsStateWithLifecycle()
        GlassAlert(
            title = ClosePr.TITLE,
            body = ClosePr.body(linked),
            onDismiss = { closePrConfirmOpen = false },
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { closePrConfirmOpen = false }),
                GlassAlertAction(
                    ClosePr.CONFIRM,
                    destructive = true,
                    testTag = "close-pr-confirm",
                    onClick = {
                        closePrConfirmOpen = false
                        changesVm.closePr()
                    },
                ),
            ),
            defaultAction = 0,
        )
    }

    // A resume relaunches the agent on that machine — cheap, but not silent:
    // the same confirm shape as the other remote commands.
    val resume = resumeTarget
    if (resumeConfirmOpen && resume != null) {
        PromptAlert(
            prompt = Prompts.ResumeRun.prompt(resume.deviceLabel),
            onDismiss = { resumeConfirmOpen = false },
            handlers = mapOf(
                "resume" to {
                    resumeConfirmOpen = false
                    sessionVm?.resumeRun(resume)
                },
            ),
        )
    }
}

/** EXP-876: the covered-issues sheet's title behind a batch run's title. */
internal const val COVERED_ISSUES_TITLE = "Issues in this run"

/** EXP-1251: a section page key as the saveable screen state. */
internal fun guideSectionParam(key: GuideSectionKey): String = when (key) {
    GuideSectionKey.Lead -> "lead"
    GuideSectionKey.Other -> "other"
    GuideSectionKey.All -> "all"
    is GuideSectionKey.Numbered -> key.index.toString()
}

internal fun parseGuideSectionParam(raw: String?): GuideSectionKey? = when (raw) {
    null -> null
    "lead" -> GuideSectionKey.Lead
    "other" -> GuideSectionKey.Other
    "all" -> GuideSectionKey.All
    else -> raw.toIntOrNull()?.takeIf { it >= 1 }?.let { GuideSectionKey.Numbered(it) }
}
