package com.exponential.app.ui.inbox

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.NotificationsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.NotificationEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DomainContract
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

/**
 * Synthetic Support group (EXP-180): issue-less `support_reply` notifications
 * bucketed per helpdesk team — the Android mirror of the web inbox's
 * synthetic "Support" group. `teamId`/`teamName` are null for the generic
 * bucket: legacy rows without a `team_id`, or a `team_id` the local teams
 * table doesn't know, all collapse into one group.
 */
data class SupportGroup(
    val teamId: String?,
    val teamName: String?,
    // Newest first, like InboxGroup.
    val notifications: List<NotificationEntity>,
    val unread: Int,
) {
    /** The newest notification — drives the row's preview and time. */
    val latest: NotificationEntity get() = notifications.first()
}

data class InboxState(
    val groups: List<InboxGroup> = emptyList(),
    val supportGroups: List<SupportGroup> = emptyList(),
    val totalUnread: Int = 0,
)

/**
 * Pure grouping core, extracted so unit tests can drive it directly.
 * `notifications` arrives newest-first (DAO orders created_at DESC);
 * LinkedHashMap keeps that order per group, so each group's first element is
 * its latest notification and groups are ordered by their latest row.
 */
internal fun buildInboxState(
    notifications: List<NotificationEntity>,
    issues: List<IssueEntity>,
    teams: List<TeamEntity>,
): InboxState {
    val issueMap = issues.associateBy { it.id }
    val teamMap = teams.associateBy { it.id }
    val byIssue = LinkedHashMap<String, MutableList<NotificationEntity>>()
    val bySupportTeam = LinkedHashMap<String?, MutableList<NotificationEntity>>()
    for (n in notifications) {
        val iid = n.issueId
        if (iid == null) {
            // Issue-less rows are the helpdesk fan-out (`support_reply`,
            // EXP-180) — group them per ticket team instead of dropping them.
            // NULL/unknown team ids collapse into one generic bucket.
            if (n.type != DomainContract.notificationTypeSupportReply) continue
            val key = n.teamId?.takeIf { teamMap.containsKey(it) }
            bySupportTeam.getOrPut(key) { mutableListOf() }.add(n)
            continue
        }
        if (!issueMap.containsKey(iid)) continue
        byIssue.getOrPut(iid) { mutableListOf() }.add(n)
    }
    val groups = byIssue.map { (iid, ns) ->
        InboxGroup(issueMap.getValue(iid), ns, ns.count { it.readAt == null })
    }
    val supportGroups = bySupportTeam.map { (tid, ns) ->
        SupportGroup(
            teamId = tid,
            teamName = tid?.let { teamMap.getValue(it).name },
            notifications = ns,
            unread = ns.count { it.readAt == null },
        )
    }
    return InboxState(
        groups = groups,
        supportGroups = supportGroups,
        totalUnread = groups.sumOf { it.unread } + supportGroups.sumOf { it.unread },
    )
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class InboxViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val notificationsApi: NotificationsApi,
    private val teamSelection: TeamSelection,
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
    // Teams resolve the Support groups' display names.
    private val teamsFlow = dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() }

    val state: StateFlow<InboxState> = combine(
        notificationsFlow,
        issuesFlow,
        teamsFlow,
    ) { notifications, issues, teams ->
        buildInboxState(notifications, issues, teams)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), InboxState())

    fun markGroupRead(group: InboxGroup) = markRead(group.notifications)

    /**
     * Tap on a Support group: mark it read and select its team (when known)
     * so the Support tab the caller navigates to opens on the right helpdesk.
     * Generic-bucket groups (null team) just mark read.
     */
    fun openSupportGroup(group: SupportGroup) {
        group.teamId?.let { teamSelection.select(it) }
        markRead(group.notifications)
    }

    private fun markRead(notifications: List<NotificationEntity>) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            notifications.filter { it.readAt == null }.forEach {
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
