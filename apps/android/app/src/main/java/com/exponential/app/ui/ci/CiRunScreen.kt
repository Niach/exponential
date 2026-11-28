package com.exponential.app.ui.ci

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.CiPage
import com.exponential.app.domain.CiRun
import com.exponential.app.domain.CiRunView
import com.exponential.app.ui.components.ClaimedFailedActionToast
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PinnedScaffold
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.rememberTickingNow
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, view `ci-run`): THE run page
// of our own CI, the PHONE layout, as the genUI tree web builds
// ([CiRunView.ciRunTree]) painted by the core catalog. The page only adds
// the platform's back control above the surface.

/** The route's screen: the synced run + jobs, the tailed log. */
@Composable
fun CiRunScreen(
    onBack: () -> Unit,
    onOpenRun: (runId: String) -> Unit,
    onFix: (AgentComposerSeed) -> Unit = {},
    onOpenPullRequest: (CiPageTarget.PullRequest) -> Unit = {},
    viewModel: CiRunViewModel = hiltViewModel(),
) {
    val run by viewModel.run.collectAsStateWithLifecycle()
    val jobs by viewModel.jobs.collectAsStateWithLifecycle()
    val selected by viewModel.selectedJobId.collectAsStateWithLifecycle()
    val log by viewModel.log.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val repository by viewModel.repository.collectAsStateWithLifecycle()
    val pullRequest by viewModel.pullRequest.collectAsStateWithLifecycle()
    val author by viewModel.author.collectAsStateWithLifecycle()
    val jobOpen by viewModel.jobOpen.collectAsStateWithLifecycle()
    val context by viewModel.context.collectAsStateWithLifecycle()
    // Phones drill into a job; Back returns to the run first.
    BackHandler(enabled = jobOpen) { viewModel.closeJob() }
    val uriHandler = LocalUriHandler.current
    ClaimedFailedActionToast(failure, viewModel::claimFailure)
    LaunchedEffect(viewModel) { viewModel.opened.collect(onOpenRun) }
    val running = run?.state == "pending" || jobs.any { it.status == "in_progress" }
    val now = rememberTickingNow(running)
    CiRunPage(
        run = run?.toRun(),
        jobs = jobs,
        selectedJobId = selected,
        log = log.takeIf { it.jobId == selected && it.loaded }?.text,
        nowMs = now,
        busy = busy,
        repository = repository,
        pullRequest = pullRequest,
        author = author,
        jobOpen = jobOpen,
        context = context,
        onSelectJob = viewModel::openJob,
        onOpenPullRequest = { id -> viewModel.pullRequestTarget(id)?.let(onOpenPullRequest) },
        onRerunFailed = viewModel::rerunFailed,
        onRerunAll = viewModel::rerunAll,
        onCancel = viewModel::cancel,
        onFix = { viewModel.fixSeed()?.let(onFix) },
        onOpenExternal = { run?.externalUrl?.takeIf { it.isNotEmpty() }?.let { runCatching { uriHandler.openUri(it) } } },
        onBack = { if (jobOpen) viewModel.closeJob() else onBack() },
    )
}

/** The stateless page ([run] null = not synced yet: skeleton rows). [log] =
 *  the selected job's log as read so far (null = none yet). */
@Composable
fun CiRunPage(
    run: CiRun.Run?,
    jobs: List<CiRun.Job>,
    selectedJobId: String?,
    log: String?,
    nowMs: Long,
    busy: CiRunView.Busy?,
    onSelectJob: (String) -> Unit,
    onRerunFailed: () -> Unit,
    onCancel: () -> Unit,
    onBack: (() -> Unit)?,
    repository: String? = null,
    pullRequest: CiRun.PullRequest? = null,
    author: String? = null,
    jobOpen: Boolean = false,
    onRerunAll: () -> Unit = {},
    onFix: () -> Unit = {},
    onOpenExternal: () -> Unit = {},
    context: CiPage.RunContext? = null,
    onOpenPullRequest: (String) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    // #EXP-2169 ("one look"): the pushed page's frame (the repo page's, the
    // settings pages'): the pinned header over the scrolling body.
    // ci-run-view.json: phones put `name #n` over the repository in the header.
    val header = run?.let { CiRunView.ciRunHeader(it, repository) }
    PinnedScaffold(
        modifier = modifier,
        topBar = {
            PageHeader(
                title = header?.first.orEmpty(),
                subtitle = header?.second,
                onBack = onBack,
                buttons = false,
                modifier = Modifier.statusBarsPadding(),
            )
        },
    ) { padding ->
        if (run == null) {
            Column(Modifier.padding(padding)) { SkeletonRows(rows = 4) }
            return@PinnedScaffold
        }
        val tree = remember(run, jobs, selectedJobId, log, nowMs, busy, repository, pullRequest, author, jobOpen, context) {
            CiRunView.ciRunTree(
                run, jobs, selectedJobId, log, nowMs, phone = true, busy = busy,
                repository = repository, pullRequest = pullRequest, author = author, jobOpen = jobOpen, context = context,
            )
        }
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 24.dp)
                .testTag(CiRun.TestIds.PAGE),
        ) {
            CiSurface(surfaceId = "ci-run-${run.id}", tree = tree) { name, context, payload ->
                when (name) {
                    CiRunView.Events.SELECT -> {
                        // A press DRILLS into the job (its Card alone; Back returns).
                        val job = (context?.get("job") as? JsonPrimitive)?.contentOrNull
                        if (job != null) onSelectJob(job)
                    }
                    CiRunView.Events.MENU -> when ((payload?.get("value") as? JsonPrimitive)?.contentOrNull) {
                        "rerunAll" -> onRerunAll()
                        "openExternal" -> onOpenExternal()
                    }
                    CiRunView.Events.RERUN -> onRerunFailed()
                    CiRunView.Events.RERUN_ALL -> onRerunAll()
                    CiRunView.Events.CANCEL -> onCancel()
                    CiRunView.Events.FIX -> onFix()
                    CiRunView.Events.OPEN_EXTERNAL -> onOpenExternal()
                    // #EXP-2214: a What happened row opens its pull request.
                    CiRunView.Events.OPEN_PR -> (context?.get("pr") as? JsonPrimitive)?.contentOrNull?.let(onOpenPullRequest)
                }
            }
        }
    }
}
