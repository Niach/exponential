package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AgentPlanApi
import com.exponential.app.data.api.getCommentBodyText
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn

data class AgentPlanPanelState(
    val issue: IssueEntity? = null,
    val events: List<IssueEventEntity> = emptyList(),
    val usersById: Map<String, UserEntity> = emptyMap(),
    val planText: String? = null,
    val questionText: String? = null,
)

// States where the agent_runs row carries plan/question TEXT worth showing.
private val planTextStates = setOf("awaiting_approval", "awaiting_answer", "approved")

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AgentPlanPanelViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val agentPlanApi: AgentPlanApi,
    private val auth: AuthRepository,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val issueIdFlow = MutableStateFlow<String?>(null)
    private val dbAndIssueId = combine(dbFlow, issueIdFlow) { db, id -> db to id }

    private val issueFlow = dbAndIssueId.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(null) else db.issueDao().observeById(id)
    }
    private val eventsFlow = dbAndIssueId.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList<IssueEventEntity>()) else db.issueEventDao().observeByIssue(id)
    }

    // Plan/question TEXT now comes from the synced `agent_runs` shape — no
    // agentPlan.getState round-trip. plan_text/question are stored as the raw
    // jsonb `{ text }` string, so getCommentBodyText unwraps them (tolerant).
    private val agentRunFlow = dbAndIssueId.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(null) else db.agentRunDao().observeByIssue(id)
    }

    val state: StateFlow<AgentPlanPanelState> = combine(
        issueFlow,
        eventsFlow,
        dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
        agentRunFlow,
    ) { issue, events, users, run ->
        val showText = issue?.agentPlanState in planTextStates
        AgentPlanPanelState(
            issue = issue,
            events = events,
            usersById = users.associateBy { it.id },
            planText = if (showText) getCommentBodyText(run?.planText).ifBlank { null } else null,
            questionText = if (showText) getCommentBodyText(run?.question).ifBlank { null } else null,
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
