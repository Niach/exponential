package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CommentsApi
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.ui.markdown.stripDraftImages
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
    // id -> name of every synced release, for release_added/release_removed
    // timeline phrases (EXP-56).
    val releaseNamesById: Map<String, String> = emptyMap(),
    val currentUserId: String? = null,
    val isAdmin: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class CommentThreadViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val commentsApi: CommentsApi,
    private val issueImagesApi: IssueImagesApi,
    private val auth: AuthRepository,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
) : ViewModel() {

    // Reactive account scoping: all queries re-scope on account switch (no
    // constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val issueIdFlow = MutableStateFlow<String?>(null)

    // Comments + activity events pre-combined into one flow so the outer combine
    // stays within the 5-arg typed overload.
    private val commentsAndEvents = combine(dbFlow, issueIdFlow) { db, id -> db to id }
        .flatMapLatest { (db, id) ->
            if (db == null || id == null) {
                flowOf(emptyList<CommentEntity>() to emptyList<IssueEventEntity>())
            } else {
                combine(
                    db.commentDao().observeByIssue(id),
                    db.issueEventDao().observeByIssue(id),
                ) { comments, events -> comments to events }
            }
        }

    // Users + releases pre-combined (same 5-arg-cap trick as commentsAndEvents);
    // releases resolve release_added/release_removed timeline names.
    private val usersAndReleases = combine(
        dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.releaseDao().observeAll() },
    ) { users, releases -> users to releases }

    val state: StateFlow<CommentThreadState> = combine(
        combine(dbFlow, issueIdFlow) { db, id -> db to id }
            .flatMapLatest { (db, id) ->
                if (db == null || id == null) flowOf(null) else db.issueDao().observeById(id)
            },
        commentsAndEvents,
        usersAndReleases,
        auth.userId,
        auth.isAdmin,
    ) { issue, (comments, events), (users, releases), userId, isAdmin ->
        CommentThreadState(
            issue = issue,
            comments = comments,
            events = events,
            usersById = users.associateBy { it.id },
            releaseNamesById = releases.associate { it.id to it.name },
            currentUserId = userId,
            isAdmin = isAdmin,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), CommentThreadState())

    fun bind(issueId: String) {
        issueIdFlow.value = issueId
    }

    // Returns true only when the comment was actually posted, so the composer
    // keeps the draft (and any in-flight image work) when the send is declined
    // (empty after sanitizing) or the request fails.
    suspend fun createComment(text: String): Boolean {
        val issueId = issueIdFlow.value ?: return false
        val accountId = auth.activeAccountId.value ?: return false
        // Never persist `draft://` placeholders from in-flight/failed uploads.
        val sanitized = stripDraftImages(text).trim()
        if (sanitized.isEmpty()) return false
        return runCatching { commentsApi.create(accountId, issueId, sanitized) }.isSuccess
    }

    /** Returns true when the edit was saved — the editor stays open otherwise. */
    suspend fun updateComment(id: String, text: String): Boolean {
        val accountId = auth.activeAccountId.value ?: return false
        val sanitized = stripDraftImages(text).trim()
        if (sanitized.isEmpty()) return false
        return runCatching { commentsApi.update(accountId, id, sanitized) }.isSuccess
    }

    suspend fun deleteComment(id: String) {
        val accountId = auth.activeAccountId.value ?: return
        runCatching { commentsApi.delete(accountId, id) }
    }

    // Comment images upload to the issue's attachments (same store as
    // descriptions) and embed as /api/attachments/{id} in the comment body.
    // Throws on upload failure (after logging) so the editor's per-row upload
    // state can surface the server's actual rejection (EXP-61); local read
    // failures stay a benign null.
    suspend fun uploadImage(uri: android.net.Uri): String? {
        val issueId = issueIdFlow.value ?: return null
        val accountId = auth.activeAccountId.value ?: return null
        val resolver = appContext.contentResolver
        val bytes = runCatching {
            resolver.openInputStream(uri)?.use { it.readBytes() }
        }.getOrNull() ?: return null
        val contentType = resolver.getType(uri) ?: "image/jpeg"
        val filename = run {
            resolver.query(
                uri,
                arrayOf(android.provider.OpenableColumns.DISPLAY_NAME),
                null,
                null,
                null,
            )?.use { cursor ->
                val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (cursor.moveToFirst() && idx >= 0) cursor.getString(idx) else null
            } ?: uri.lastPathSegment ?: "image"
        }
        try {
            return issueImagesApi.upload(accountId, issueId, bytes, filename, contentType).url
        } catch (cancel: kotlinx.coroutines.CancellationException) {
            throw cancel
        } catch (error: Throwable) {
            android.util.Log.w("CommentThreadViewModel", "Image upload failed (type=$contentType, ${bytes.size} bytes)", error)
            throw error
        }
    }
}
