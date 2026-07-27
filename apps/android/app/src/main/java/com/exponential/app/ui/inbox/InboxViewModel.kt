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

/**
 * One merged stream (web/iOS/desktop parity): issue groups and synthetic
 * Support groups interleaved newest-first by each group's latest notification.
 */
sealed interface InboxEntry {
    val unread: Int

    /** Stable list key, mirroring the web/iOS `issue:`/`support:` key form. */
    val key: String

    data class Issue(val group: InboxGroup) : InboxEntry {
        override val unread: Int get() = group.unread
        override val key: String get() = "issue:${group.issue.id}"
    }

    data class Support(val group: SupportGroup) : InboxEntry {
        override val unread: Int get() = group.unread
        override val key: String get() = "support:${group.teamId ?: "generic"}"
    }
}

data class InboxState(
    val entries: List<InboxEntry> = emptyList(),
    val totalUnread: Int = 0,
)

/** First-seen registry key: one namespace across both entry kinds. */
private sealed interface GroupKey {
    data class Issue(val issueId: String) : GroupKey
    data class Support(val teamId: String?) : GroupKey
}

/**
 * Pure grouping core, extracted so unit tests can drive it directly.
 * `notifications` arrives newest-first (DAO orders created_at DESC); ONE
 * LinkedHashMap across both entry kinds keeps that order, so each group's
 * first element is its latest notification and the entries interleave by
 * latest activity (web `inbox-view.tsx` sorts all groups together).
 */
internal fun buildInboxState(
    notifications: List<NotificationEntity>,
    issues: List<IssueEntity>,
    teams: List<TeamEntity>,
): InboxState {
    val issueMap = issues.associateBy { it.id }
    val teamMap = teams.associateBy { it.id }
    val byKey = LinkedHashMap<GroupKey, MutableList<NotificationEntity>>()
    for (n in notifications) {
        val iid = n.issueId
        val key = if (iid == null) {
            // Issue-less rows are the helpdesk fan-out (`support_reply`,
            // EXP-180) — group them per ticket team instead of dropping them.
            // NULL/unknown team ids collapse into one generic bucket.
            if (n.type != DomainContract.notificationTypeSupportReply) continue
            GroupKey.Support(n.teamId?.takeIf { teamMap.containsKey(it) })
        } else {
            if (!issueMap.containsKey(iid)) continue
            GroupKey.Issue(iid)
        }
        byKey.getOrPut(key) { mutableListOf() }.add(n)
    }
    val entries = byKey.map { (key, ns) ->
        val unread = ns.count { it.readAt == null }
        when (key) {
            is GroupKey.Issue -> InboxEntry.Issue(
                InboxGroup(issueMap.getValue(key.issueId), ns, unread),
            )
            is GroupKey.Support -> InboxEntry.Support(
                SupportGroup(
                    teamId = key.teamId,
                    teamName = key.teamId?.let { teamMap.getValue(it).name },
                    notifications = ns,
                    unread = unread,
                ),
            )
        }
    }
    return InboxState(entries = entries, totalUnread = entries.sumOf { it.unread })
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
