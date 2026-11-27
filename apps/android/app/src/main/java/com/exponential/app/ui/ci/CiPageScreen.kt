package com.exponential.app.ui.ci

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.ci.CiPageSource
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CiPage
import com.exponential.app.domain.CiPageView
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.rememberTickingNow
import dagger.hilt.android.lifecycle.HiltViewModel
import java.time.Instant
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// #EXP-2172 (fixtures/ci-page.json, view `ci`): THE CI page, the PHONE
// layout, as the genUI tree web builds ([CiPageView.ciPageTree]) painted by
// the core catalog. The screen adds the platform's title and back above it.

/** What a press on the page opens. */
sealed interface CiPageTarget {
    data class Run(val runId: String) : CiPageTarget
    data class PullRequest(val rootSessionId: String?, val url: String) : CiPageTarget
    data class Runner(val deviceRowId: String) : CiPageTarget
}

/**
 * #EXP-2172: THE CI page's state: the synced inputs ([CiPageSource]), the
 * repositories' full names (fetched per member team, the Settings ›
 * Repositories list, until the repositories shape lands), the tab and the
 * filter.
 */
@HiltViewModel
class CiPageViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    source: CiPageSource,
    private val repositoriesApi: RepositoriesApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val repoNames = MutableStateFlow<Map<String, String>>(emptyMap())
    private val askedTeams = HashSet<String>()

    private fun loadRepos(teamIds: Set<String>) {
        val accountId = auth.activeAccountId.value ?: return
        for (teamId in teamIds) {
            if (!synchronized(askedTeams) { askedTeams.add(teamId) }) continue
            viewModelScope.launch {
                runCatching { repositoriesApi.list(accountId, teamId) }
                    .onSuccess { repos -> repoNames.value = repoNames.value + repos.associate { it.id to it.fullName } }
                    .onFailure {
                        if (it is CancellationException) throw it
                        synchronized(askedTeams) { askedTeams.remove(teamId) }
                    }
            }
        }
    }

    private val teams = dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
        .onEach { rows -> loadRepos(rows.map { it.id }.toSet()) }

    private val pullRequests: StateFlow<List<PullRequestEntity>> =
        dbFlow.scopedQuery(emptyList<PullRequestEntity>()) { it.pullRequestDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Null until the first read. */
    val input: StateFlow<CiPage.Input?> = combine(source.input, repoNames, teams) { input, names, _ ->
        input.copy(repos = names.map { (id, name) -> CiPage.Repo(id, name) })
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _tab = MutableStateFlow("runs")
    val tab: StateFlow<String> = _tab
    private val _filter = MutableStateFlow(CiPage.RunFilter())
    val filter: StateFlow<CiPage.RunFilter> = _filter

    /** A tree event → the new state, or what it opens. */
    fun onEvent(name: String, context: JsonObject?, payload: JsonObject?): CiPageTarget? {
        fun str(o: JsonObject?, key: String) = (o?.get(key) as? JsonPrimitive)?.contentOrNull
        val value = str(payload, "value")
        val pick = value?.takeIf { it != "all" }
        when (name) {
            CiPageView.Events.TAB -> if (value in TABS) _tab.value = value!!
            // The runner chip opens the Runners tab.
            CiPageView.Events.RUNNERS -> _tab.value = "runners"
            CiPageView.Events.STATUS -> if (value != null && value in CiPage.Copy.FILTERS) _filter.value = _filter.value.copy(status = value)
            CiPageView.Events.REPOSITORY -> if (value != null) _filter.value = _filter.value.copy(repositoryId = pick)
            CiPageView.Events.SOURCE -> if (value != null) _filter.value = _filter.value.copy(source = pick)
            CiPageView.Events.OPEN_RUN -> {
                val run = str(context, "run") ?: str(payload, "id") ?: return null
                return CiPageTarget.Run(run)
            }
            CiPageView.Events.OPEN_PR -> {
                val id = str(context, "pr") ?: return null
                val pr = pullRequests.value.firstOrNull { it.id == id } ?: return null
                return CiPageTarget.PullRequest(pr.rootSessionId, pr.url)
            }
            CiPageView.Events.OPEN_RUNNER -> return str(context, "device")?.let(CiPageTarget::Runner)
        }
        return null
    }

    private companion object {
        val TABS = setOf("runs", "train", "runners")
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
    // Durations of running runs and the runners' online state move with the clock.
    val live = input?.runs?.any { it.state == "pending" } == true || input?.runners?.isNotEmpty() == true
    val now = rememberTickingNow(live)
    CiPageContent(
        input = input?.copy(now = Instant.ofEpochMilli(now).toString()),
        state = CiPageView.State(tab = tab, filter = filter, phone = true),
        onBack = onBack,
        onEvent = { name, context, payload -> viewModel.onEvent(name, context, payload)?.let(onOpen) },
    )
}

/** The stateless page ([input] null = not read yet: nothing below the title). */
@Composable
fun CiPageContent(
    input: CiPage.Input?,
    state: CiPageView.State,
    onBack: (() -> Unit)?,
    onEvent: (name: String, context: JsonObject?, payload: JsonObject?) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize().statusBarsPadding()) {
        PageHeader(title = CiPage.Copy.TITLE, onBack = onBack)
        if (input == null) return@Column
        val tree = remember(input, state) { CiPageView.ciPageTree(input, state) }
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 24.dp)
                .testTag(CiPage.TestIds.PAGE),
        ) {
            CiSurface(surfaceId = "ci-page", tree = tree, onAction = onEvent)
        }
    }
}
