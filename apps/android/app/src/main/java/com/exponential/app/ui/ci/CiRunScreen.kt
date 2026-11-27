package com.exponential.app.ui.ci

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import com.exponential.app.domain.CiRun
import com.exponential.app.domain.CiRunView
import com.exponential.app.ui.components.ClaimedFailedActionToast
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.rememberTickingNow
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
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
    viewModel: CiRunViewModel = hiltViewModel(),
) {
    val run by viewModel.run.collectAsStateWithLifecycle()
    val jobs by viewModel.jobs.collectAsStateWithLifecycle()
    val selected by viewModel.selectedJobId.collectAsStateWithLifecycle()
    val log by viewModel.log.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val repository by viewModel.repository.collectAsStateWithLifecycle()
    val pullRequestNumber by viewModel.pullRequestNumber.collectAsStateWithLifecycle()
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
        pullRequestNumber = pullRequestNumber,
        onSelectJob = viewModel::selectJob,
        onRerunFailed = viewModel::rerunFailed,
        onRerunAll = viewModel::rerunAll,
        onCancel = viewModel::cancel,
        onFix = { viewModel.fixSeed()?.let(onFix) },
        onOpenExternal = { run?.externalUrl?.takeIf { it.isNotEmpty() }?.let { runCatching { uriHandler.openUri(it) } } },
        onBack = onBack,
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
    pullRequestNumber: Int? = null,
    onRerunAll: () -> Unit = {},
    onFix: () -> Unit = {},
    onOpenExternal: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize().statusBarsPadding()) {
        PageHeader(title = "", onBack = onBack, buttons = false)
        if (run == null) {
            SkeletonRows(rows = 4)
            return@Column
        }
        val tree = remember(run, jobs, selectedJobId, log, nowMs, busy, repository, pullRequestNumber) {
            CiRunView.ciRunTree(
                run, jobs, selectedJobId, log, nowMs, phone = true, busy = busy,
                repository = repository, pullRequestNumber = pullRequestNumber,
            )
        }
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 24.dp)
                .testTag(CiRun.TestIds.PAGE),
        ) {
            CiSurface(surfaceId = "ci-run-${run.id}", tree = tree) { name, context, payload ->
                when (name) {
                    CiRunView.Events.SELECT -> {
                        val job = (context?.get("job") as? JsonPrimitive)?.contentOrNull
                        val open = (payload?.get("open") as? JsonPrimitive)?.booleanOrNull
                        // A Collapsible: closing the open one keeps the selection.
                        if (job != null && job != selectedJobId && open != false) onSelectJob(job)
                    }
                    CiRunView.Events.RERUN -> onRerunFailed()
                    CiRunView.Events.RERUN_ALL -> onRerunAll()
                    CiRunView.Events.CANCEL -> onCancel()
                    CiRunView.Events.FIX -> onFix()
                    CiRunView.Events.OPEN_EXTERNAL -> onOpenExternal()
                }
            }
        }
    }
}
