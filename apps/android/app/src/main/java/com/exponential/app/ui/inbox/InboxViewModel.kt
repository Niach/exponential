package com.exponential.app.ui.inbox

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.NotificationsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.NotificationEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class InboxGroup(
    val issue: IssueEntity,
    // Newest first (the DAO orders created_at DESC and grouping preserves it).
    val notifications: List<NotificationEntity>,
    val unread: Int,
) {
    /** The newest notification — drives the row's icon, sentence, and time. */
    val latest: NotificationEntity get() = notifications.first()
}

data class InboxState(
    val groups: List<InboxGroup> = emptyList(),
    val totalUnread: Int = 0,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class InboxViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val notificationsApi: NotificationsApi,
) : ViewModel() {

    // Reactive account scoping: all queries re-scope on account switch (no
    // constructor-time DB/user snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    // The notifications shape is already scoped to the signed-in user server-side.
    private val notificationsFlow = combine(dbFlow, auth.userId) { db, userId -> db to userId }
        .flatMapLatest { (db, userId) ->
            if (db == null || userId == null) flowOf(emptyList())
            else db.notificationDao().observeByUser(userId)
        }
    private val issuesFlow = dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }

    val state: StateFlow<InboxState> = combine(
        notificationsFlow,
        issuesFlow,
    ) { notifications, issues ->
        val issueMap = issues.associateBy { it.id }
        // notifications arrive newest-first; LinkedHashMap keeps that order per issue.
        val byIssue = LinkedHashMap<String, MutableList<NotificationEntity>>()
        for (n in notifications) {
            val iid = n.issueId ?: continue
            if (!issueMap.containsKey(iid)) continue
            byIssue.getOrPut(iid) { mutableListOf() }.add(n)
        }
        val groups = byIssue.map { (iid, ns) ->
            InboxGroup(issueMap.getValue(iid), ns, ns.count { it.readAt == null })
        }
        InboxState(groups, groups.sumOf { it.unread })
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), InboxState())

    fun markGroupRead(group: InboxGroup) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            group.notifications.filter { it.readAt == null }.forEach {
                runCatching { notificationsApi.markRead(accountId, it.id) }
            }
        }
    }

    fun markAllRead() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { notificationsApi.markAllRead(accountId) }
        }
    }
}
