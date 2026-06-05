package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AgentPlanApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.UserEntity
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class AgentPlanPanelState(
    val issue: IssueEntity? = null,
    val events: List<IssueEventEntity> = emptyList(),
    val usersById: Map<String, UserEntity> = emptyMap(),
    val planText: String? = null,
    val questionText: String? = null,
)

// States where the server has plan/question TEXT to fetch via getState.
private val planTextStates = setOf("awaiting_approval", "awaiting_answer", "approved")

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AgentPlanPanelViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val agentPlanApi: AgentPlanApi,
    private val auth: AuthRepository,
) : ViewModel() {

    private val accountId = auth.activeAccountId.value ?: ""
    private val db = holder.database(forAccountId = accountId)

    private val issueIdFlow = MutableStateFlow<String?>(null)

    private val issueFlow = issueIdFlow.flatMapLatest { id ->
        if (id == null) flowOf(null) else db.issueDao().observeById(id)
    }
    private val eventsFlow = issueIdFlow.flatMapLatest { id ->
        if (id == null) flowOf(emptyList<IssueEventEntity>()) else db.issueEventDao().observeByIssue(id)
    }

    // Plan/question text is fetched server-side (not in Electric). Re-fetch only
    // when the issue id, plan state, or revision changes (mirrors the web panel's
    // effect dependency on [id, state, revision]). Seeded so the panel renders
    // immediately and fills the text in once the fetch returns.
    private val planText = MutableStateFlow<Pair<String?, String?>>(null to null)

    init {
        viewModelScope.launch {
            issueFlow
                // Keyed on (id, state, revision) — distinctUntilChanged re-emits
                // (and collectLatest re-fetches) whenever the agent re-plans, even
                // if the state label is unchanged but the revision bumped.
                .map { Triple(it?.id, it?.agentPlanState, it?.agentPlanRevision) }
                .distinctUntilChanged()
                .collectLatest { (id, state, _) ->
                    val acct = auth.activeAccountId.value
                    planText.value = if (id == null || acct == null || state !in planTextStates) {
                        null to null
                    } else {
                        val r = runCatching { agentPlanApi.getState(acct, id) }.getOrNull()
                        r?.planText to r?.question
                    }
                }
        }
    }

    val state: StateFlow<AgentPlanPanelState> = combine(
        issueFlow,
        eventsFlow,
        db.userDao().observeAll(),
        planText,
    ) { issue, events, users, plan ->
        AgentPlanPanelState(
            issue = issue,
            events = events,
            usersById = users.associateBy { it.id },
            planText = plan.first,
            questionText = plan.second,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AgentPlanPanelState())

    fun bind(issueId: String) {
        issueIdFlow.value = issueId
    }

    suspend fun approvePlan() {
        val issueId = issueIdFlow.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        runCatching { agentPlanApi.approvePlan(accountId, issueId) }
    }

    suspend fun requestChanges() {
        val issueId = issueIdFlow.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        runCatching { agentPlanApi.requestChanges(accountId, issueId) }
    }

    suspend fun retry() {
        val issueId = issueIdFlow.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        runCatching { agentPlanApi.retry(accountId, issueId) }
    }

    suspend fun answerQuestion(text: String) {
        val issueId = issueIdFlow.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        val trimmed = text.trim()
        if (trimmed.isEmpty()) return
        runCatching { agentPlanApi.answerQuestion(accountId, issueId, trimmed) }
    }
}
