package com.exponential.app.ui.inbox

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
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

    /** EXP-933: the latest is an agent's message about this issue — the row
     *  opens the issue's Results face (the run's report), not its Issue face. */
    val opensResults: Boolean get() = latest.type == DomainContract.notificationTypeAgentMessage
}

/**
 * One merged stream (web/iOS/desktop parity): issue groups and the issue-less
 * message rows interleaved newest-first by each entry's latest notification.
 * SLOP-4: a reporter's reply (`reporter_reply`) is issue-scoped, so it groups
 * under its issue like `issue_comment` — there is no Support entry any more.
 */
sealed interface InboxEntry {
    val unread: Int

    /** Stable list key, mirroring the web/iOS `issue:`/`message:` key form. */
    val key: String

    data class Issue(val group: InboxGroup) : InboxEntry {
        override val unread: Int get() = group.unread
        override val key: String get() = "issue:${group.issue.id}"
    }

    /**
     * One agent message (EXP-801): an issue-less `agent_message` row is its
     * own entry — never bundled, each is a distinct thing someone's agent
     * said. Tapping marks it read; there is nowhere to navigate.
     * `teamName` resolves the row's synced team (null when unknown).
     */
    data class Message(val notification: NotificationEntity, val teamName: String?) : InboxEntry {
        override val unread: Int get() = if (notification.readAt == null) 1 else 0
        override val key: String get() = "message:${notification.id}"
    }

    /**
     * One blocked coding run (EXP-980): an issue-less `session_blocked` row,
     * its own entry like an agent message. Tapping marks it read AND opens the
     * run; a row whose `session_id` is NULL (the run was pruned) still
     * renders, and its tap only marks read.
     */
    data class Session(val notification: NotificationEntity, val teamName: String?) : InboxEntry {
        override val unread: Int get() = if (notification.readAt == null) 1 else 0
        override val key: String get() = "session:${notification.id}"

        /** The run to open, or null once it is gone. */
        val sessionId: String? get() = notification.sessionId?.takeIf { it.isNotEmpty() }
    }
}

data class InboxState(
    val entries: List<InboxEntry> = emptyList(),
    val totalUnread: Int = 0,
)

/** First-seen registry key: one namespace across every entry kind. */
private sealed interface GroupKey {
    data class Issue(val issueId: String) : GroupKey
    data class Message(val notificationId: String) : GroupKey
    data class Session(val notificationId: String) : GroupKey
}

/**
 * Pure grouping core, extracted so unit tests can drive it directly.
 * `notifications` arrives newest-first (DAO orders created_at DESC); ONE
 * LinkedHashMap across every entry kind keeps that order, so each group's
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
            // Issue-less rows are an agent's message (`agent_message`,
            // EXP-801) or a blocked coding run (`session_blocked`, EXP-980),
            // one entry each; anything else without an issue is dropped.
            when (n.type) {
                DomainContract.notificationTypeAgentMessage -> GroupKey.Message(n.id)
                DomainContract.notificationTypeSessionBlocked -> GroupKey.Session(n.id)
                else -> continue
            }
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
            is GroupKey.Message -> InboxEntry.Message(
                notification = ns.single(),
                teamName = ns.single().teamId?.let { teamMap[it]?.name },
            )
            is GroupKey.Session -> InboxEntry.Session(
                notification = ns.single(),
                teamName = ns.single().teamId?.let { teamMap[it]?.name },
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
    // Teams resolve the issue-less rows' team names.
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
     * Tap on an agent message (EXP-801) or a blocked run (EXP-980): mark that
     * one row read. The blocked-run row's caller ALSO navigates to the run
     * when it still has one.
     */
    fun markMessageRead(notification: NotificationEntity) = markRead(listOf(notification))

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
