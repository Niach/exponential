package com.exponential.app.ui.ci

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.PullRequestsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.ci.CiPageSource
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CiPage
import com.exponential.app.domain.CiPageView
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.ClaimedFailedActionToast
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PinnedHeaderColumn
import com.exponential.app.ui.components.rememberTickingNow
import dagger.hilt.android.lifecycle.HiltViewModel
import java.time.Instant
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// #EXP-2172 (fixtures/ci-page.json, view `ci`): THE CI page, the PHONE
// layout, as the genUI tree web builds ([CiPageView.ciPageTree]) painted by
// the core catalog. The screen adds the platform's title and back above it.
// #EXP-2214: four tabs, Merge queue (first) · Pull requests · Releases · Runners.

/** What a press on the page opens. */
sealed interface CiPageTarget {
    data class Run(val runId: String) : CiPageTarget
    /** #EXP-2214: Open log of our job = the run page with `?job=`. */
    data class Job(val runId: String, val jobId: String) : CiPageTarget
    /** #EXP-2214: Open log of a forge's job. */
    data class Url(val url: String) : CiPageTarget
    data class PullRequest(val rootSessionId: String?, val url: String) : CiPageTarget
    data class Runner(val deviceRowId: String) : CiPageTarget
}

private fun str(o: JsonObject?, key: String) = (o?.get(key) as? JsonPrimitive)?.contentOrNull

/** The opening events every CI tree fires (the CI page's and the repo
 *  page's Runs tab): a run, our job's log, a forge's job, a pull request. */
internal fun ciOpenTarget(name: String, context: JsonObject?, payload: JsonObject?, pullRequests: List<PullRequestEntity>): CiPageTarget? =
    when (name) {
        CiPageView.Events.OPEN_RUN -> (str(context, "run") ?: str(payload, "id"))?.let(CiPageTarget::Run)
        CiPageView.Events.OPEN_JOB -> {
            val run = str(context, "run")
            val job = str(context, "job")
            if (run != null && job != null) CiPageTarget.Job(run, job) else null
        }
        CiPageView.Events.OPEN_URL -> str(context, "url")?.takeIf { it.isNotEmpty() }?.let(CiPageTarget::Url)
        CiPageView.Events.OPEN_PR -> str(context, "pr")?.let { id ->
            pullRequests.firstOrNull { it.id == id }?.let { CiPageTarget.PullRequest(it.rootSessionId, it.url) }
        }
        CiPageView.Events.OPEN_RUNNER -> str(context, "device")?.let(CiPageTarget::Runner)
        else -> null
    }

/**
 * #EXP-2214: the merge queue's own controls from a CI tree: the Up next ⋯
 * menu (Merge now = `pullRequests.mergeNow`, Hold = `pullRequests.hold
 * {hold: true}`) and Resume (`hold: false`). Merge now is owners-only: the
 * server refuses others and the toast says why. A failure = [onFailed].
 */
internal fun CoroutineScope.ciQueueCommand(
    name: String,
    context: JsonObject?,
    payload: JsonObject?,
    accountId: String?,
    api: PullRequestsApi,
    onFailed: (FailedAction) -> Unit,
): Boolean {
    val pr = str(context, "pr") ?: return false
    val account = accountId ?: return false
    val value = str(payload, "value")
    val (verb, thing, call) = when {
        name == CiPageView.Events.QUEUE_MENU && value == "merge-now" ->
            Triple("merge", "the pull request now", suspend { api.mergeNow(account, pr) })
        name == CiPageView.Events.QUEUE_MENU && value == "hold" ->
            Triple("hold", "the pull request", suspend { api.hold(account, pr, hold = true) })
        name == CiPageView.Events.RESUME ->
            Triple("resume", "the pull request", suspend { api.hold(account, pr, hold = false) })
        else -> return false
    }
    fun go() {
        launch {
            runCatching { call() }.onFailure { t ->
                if (t is CancellationException) throw t
                onFailed(FailedAction(verb, thing, trpcErrorMessage(t, ""), retry = ::go))
            }
        }
    }
    go()
    return true
}

/**
 * #EXP-2172: THE CI page's state: the synced inputs ([CiPageSource], #EXP-2214:
 * the repositories shape included), the tab (the merge queue first) and the
 * filter.
 */
@HiltViewModel
class CiPageViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    source: CiPageSource,
    private val pullRequestsApi: PullRequestsApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val pullRequests: StateFlow<List<PullRequestEntity>> =
        dbFlow.scopedQuery(emptyList<PullRequestEntity>()) { it.pullRequestDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Null until the first read. */
    val input: StateFlow<CiPage.Input?> = source.input
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _tab = MutableStateFlow(CiPage.TAB_ORDER.first())
    val tab: StateFlow<String> = _tab
    private val _filter = MutableStateFlow(CiPage.RunFilter())
    val filter: StateFlow<CiPage.RunFilter> = _filter

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure
    fun claimFailure(failure: FailedAction): Boolean = _failure.compareAndSet(failure, null)

    /** A tree event → the new state, a queue command, or what it opens. */
    fun onEvent(name: String, context: JsonObject?, payload: JsonObject?): CiPageTarget? {
        val value = str(payload, "value")
        val pick = value?.takeIf { it != "all" }
        when (name) {
            CiPageView.Events.TAB -> if (value in CiPage.TAB_ORDER) _tab.value = value!!
            // The runner chip opens the Runners tab.
            CiPageView.Events.RUNNERS -> _tab.value = "runners"
            CiPageView.Events.STATUS -> if (value != null && value in CiPage.Copy.FILTERS) _filter.value = _filter.value.copy(status = value)
            CiPageView.Events.REPOSITORY -> if (value != null) _filter.value = _filter.value.copy(repositoryId = pick)
            CiPageView.Events.QUEUE_MENU, CiPageView.Events.RESUME ->
                viewModelScope.ciQueueCommand(name, context, payload, auth.activeAccountId.value, pullRequestsApi) { _failure.value = it }
            else -> return ciOpenTarget(name, context, payload, pullRequests.value)
        }
        return null
    }
}

/** The route's screen. */
@Composable
fun CiPageScreen(
    onBack: (() -> Unit)?,
    onOpen: (CiPageTarget) -> Unit,
    viewModel: CiPageViewModel = hiltViewModel(),
) {
    val input by viewModel.input.collectAsStateWithLifecycle()
    val tab by viewModel.tab.collectAsStateWithLifecycle()
    val filter by viewModel.filter.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    ClaimedFailedActionToast(failure, viewModel::claimFailure)
    // Durations of running runs and the runners' online state move with the clock.
    val live = input?.runs?.any { it.state == "pending" } == true || input?.runners?.isNotEmpty() == true ||
        input?.trains?.any { it.finishedAt == null } == true
    val now = rememberTickingNow(live)
    CiPageContent(
        input = input?.copy(now = Instant.ofEpochMilli(now).toString(), utcOffsetMinutes = utcOffsetMinutes(now)),
        state = CiPageView.State(tab = tab, filter = filter, phone = true),
        onBack = onBack,
        onEvent = { name, context, payload -> viewModel.onEvent(name, context, payload)?.let(onOpen) },
    )
}

/** The viewer's clock offset from UTC at [nowMs] (ci-page.json day sections). */
internal fun utcOffsetMinutes(nowMs: Long): Int =
    java.util.TimeZone.getDefault().getOffset(nowMs) / 60_000

/** The stateless page ([input] null = not read yet: nothing below the title). */
@Composable
fun CiPageContent(
    input: CiPage.Input?,
    state: CiPageView.State,
    onBack: (() -> Unit)?,
    onEvent: (name: String, context: JsonObject?, payload: JsonObject?) -> Unit,
    modifier: Modifier = Modifier,
) {
    // #EXP-2169 ("one look"): THE Devices page's frame (its More-menu
    // sibling): the pinned page header over the scrolling body, the same
    // 16 inset and bottom clearance.
    Scaffold(modifier = modifier, containerColor = Color.Transparent) { padding ->
        PinnedHeaderColumn(
            modifier = Modifier.padding(padding).fillMaxSize(),
            header = { PageHeader(title = CiPage.Copy.TITLE, onBack = onBack) },
        ) {
            if (input == null) return@PinnedHeaderColumn
            val tree = remember(input, state) { CiPageView.ciPageTree(input, state) }
            val counts = remember(input) { CiPage.tabCounts(input) }
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset)
                    .testTag(CiPage.TestIds.PAGE),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                CiTabs(
                    tabs = CiPage.TAB_ORDER,
                    selected = state.tab,
                    counts = counts,
                    onSelect = { tab -> onEvent(CiPageView.Events.TAB, null, buildJsonObject { put("value", tab) }) },
                )
                // One surface per tab: a tab's panel is a different tree (the
                // painter keeps a model per tab instead of re-rooting one).
                CiSurface(surfaceId = "ci-page-${state.tab}", tree = tree, onAction = onEvent)
            }
        }
    }
}

/** #EXP-2169 ("one look"): THE tab switcher = the board's segmented control
 *  (All issues / My issues): #EXP-2214 the four tabs, phones' labels
 *  copy.phoneTabs, each count after the label in muted text (none at zero). */
@Composable
internal fun CiTabs(
    tabs: List<String>,
    selected: String,
    counts: CiPage.TabCounts?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    fun label(tab: String) = CiPage.Copy.PHONE_TABS.getValue(tab)
    fun count(tab: String): Int? = counts?.of(tab)?.takeIf { it > 0 }
    Box(modifier.horizontalScroll(rememberScrollState()).testTag(CiPage.TestIds.TABS)) {
        GlassSegmentedControl(
            options = tabs,
            selected = selected,
            label = ::label,
            onSelect = onSelect,
            testTag = { tab -> "${CiPage.TestIds.TABS}-$tab" },
            trailing = { tab ->
                count(tab)?.let { n ->
                    {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "$n",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                            softWrap = false,
                        )
                    }
                }
            },
            description = { tab -> count(tab)?.let { "${CiPage.Copy.TABS.getValue(tab)}, $it" } },
            hug = true,
        )
    }
}
