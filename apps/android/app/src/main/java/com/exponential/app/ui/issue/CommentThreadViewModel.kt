package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AgentPlanApi
import com.exponential.app.data.api.CommentsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CommentEntity
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
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn

data class CommentThreadState(
    val issue: IssueEntity? = null,
    val comments: List<CommentEntity> = emptyList(),
    val events: List<IssueEventEntity> = emptyList(),
    val usersById: Map<String, UserEntity> = emptyMap(),
    val currentUserId: String? = null,
    val isAdmin: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class CommentThreadViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val commentsApi: CommentsApi,
    private val agentPlanApi: AgentPlanApi,
    private val auth: AuthRepository,
) : ViewModel() {

    private val accountId = auth.activeAccountId.value ?: ""
    private val db = holder.database(forAccountId = accountId)

    private val issueIdFlow = MutableStateFlow<String?>(null)

    // Comments + activity events pre-combined into one flow so the outer combine
    // stays within the 5-arg typed overload.
    private val commentsAndEvents = issueIdFlow.flatMapLatest { id ->
        if (id == null) {
            flowOf(emptyList<CommentEntity>() to emptyList<IssueEventEntity>())
        } else {
            combine(
                db.commentDao().observeByIssue(id),
                db.issueEventDao().observeByIssue(id),
            ) { comments, events -> comments to events }
        }
    }

    val state: StateFlow<CommentThreadState> = combine(
        issueIdFlow.flatMapLatest { id ->
            if (id == null) flowOf(null) else db.issueDao().observeById(id)
        },
        commentsAndEvents,
        db.userDao().observeAll(),
        auth.userId,
        auth.isAdmin,
    ) { issue, (comments, events), users, userId, isAdmin ->
        CommentThreadState(
            issue = issue,
            comments = comments,
            events = events,
            usersById = users.associateBy { it.id },
            currentUserId = userId,
            isAdmin = isAdmin,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), CommentThreadState())

    fun bind(issueId: String) {
        issueIdFlow.value = issueId
    }

    suspend fun createComment(text: String) {
        val issueId = issueIdFlow.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        runCatching { commentsApi.create(accountId, issueId, text) }
    }

    suspend fun updateComment(id: String, text: String) {
        val accountId = auth.activeAccountId.value ?: return
        runCatching { commentsApi.update(accountId, id, text) }
    }

    suspend fun deleteComment(id: String) {
        val accountId = auth.activeAccountId.value ?: return
        runCatching { commentsApi.delete(accountId, id) }
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
}
