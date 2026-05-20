package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CommentsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CommentDao
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.UserDao
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
    val comments: List<CommentEntity> = emptyList(),
    val usersById: Map<String, UserEntity> = emptyMap(),
    val currentUserId: String? = null,
    val isAdmin: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class CommentThreadViewModel @Inject constructor(
    private val commentDao: CommentDao,
    private val userDao: UserDao,
    private val commentsApi: CommentsApi,
    private val auth: AuthRepository,
) : ViewModel() {

    private val issueIdFlow = MutableStateFlow<String?>(null)

    val state: StateFlow<CommentThreadState> = combine(
        issueIdFlow.flatMapLatest { id ->
            if (id == null) flowOf(emptyList()) else commentDao.observeByIssue(id)
        },
        userDao.observeAll(),
        auth.userId,
        auth.isAdmin,
    ) { comments, users, userId, isAdmin ->
        CommentThreadState(
            comments = comments,
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
        runCatching { commentsApi.create(issueId, text) }
    }

    suspend fun updateComment(id: String, text: String) {
        runCatching { commentsApi.update(id, text) }
    }

    suspend fun deleteComment(id: String) {
        runCatching { commentsApi.delete(id) }
    }
}
