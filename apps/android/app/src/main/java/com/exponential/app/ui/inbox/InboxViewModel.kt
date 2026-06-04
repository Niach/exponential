package com.exponential.app.ui.inbox

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.NotificationsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.NotificationEntity
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class InboxGroup(
    val issue: IssueEntity,
    val notifications: List<NotificationEntity>,
    val unread: Int,
)

data class InboxState(
    val groups: List<InboxGroup> = emptyList(),
    val reviewIssues: List<IssueEntity> = emptyList(),
    val totalUnread: Int = 0,
)

@HiltViewModel
class InboxViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val notificationsApi: NotificationsApi,
) : ViewModel() {

    private val accountId = auth.activeAccountId.value ?: ""
    private val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId ?: ""
    private val db = holder.database(forAccountId = accountId)

    // The notifications shape is already scoped to the signed-in user server-side.
    private val notificationsFlow = db.notificationDao().observeByUser(userId)
    private val issuesFlow = db.issueDao().observeAll()

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
        val review = issues.filter {
            it.agentPlanState == "awaiting_approval" || it.prState == "open"
        }
        InboxState(groups, review, groups.sumOf { it.unread })
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), InboxState())

    fun markGroupRead(group: InboxGroup) {
        viewModelScope.launch {
            group.notifications.filter { it.readAt == null }.forEach {
                runCatching { notificationsApi.markRead(accountId, it.id) }
            }
        }
    }

    fun markAllRead() {
        viewModelScope.launch { runCatching { notificationsApi.markAllRead(accountId) } }
    }
}
