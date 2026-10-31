package com.exponential.app.data.push

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.domain.InboxRoute
import com.exponential.app.domain.PlanPage
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WireTimestamps
import java.net.URI
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first

/**
 * Resolves a verified App Link (EXP-92) — team slug + identifier from the
 * web URL — to a local issue id, under the signed-in account whose instanceUrl
 * host matches the link. The row may simply not have synced yet (cold launch /
 * brand-new issue), so a miss polls the DB briefly while sync runs
 * ([SyncManager] has no awaitable hook) before giving up; the caller falls
 * back to a Custom Tab on [Resolution.NotFound]. EXP-1288: thread links
 * ([resolveThread]) take the same path against the synced `threads` rows.
 */
@Singleton
class WebLinkResolver @Inject constructor(
    private val auth: AuthRepository,
    private val dbHolder: DatabaseHolder,
) {
    sealed interface Resolution {
        /** [id] = the local issue id ([resolve]) or thread id ([resolveThread]). */
        data class Found(
            val id: String,
            val accountId: String,
            /** Thread links only: the synced row is a ROOT (`parent_id IS
             *  NULL`); a child of a `/sessions/` link opens its session view. */
            val isRoot: Boolean = true,
        ) : Resolution
        data object NotFound : Resolution
    }

    suspend fun resolve(target: DeepLinkBus.Target.WebIssueRef): Resolution =
        poll(target.host) { db -> db.issueDao().findIdByTeamRef(target.teamSlug, target.identifier)?.let { it to true } }

    /** EXP-1288: a `/t/{team}/threads/{id}` (or EXP-1291 `/sessions/{id}`)
     *  link → the synced session (a thread IS a root session); [Found.isRoot]
     *  tells a root from a child. */
    suspend fun resolveThread(target: DeepLinkBus.Target.WebThreadRef): Resolution =
        poll(target.host) { db ->
            db.sessionDao().findIdByTeamRef(target.teamSlug, target.threadId)?.let { id ->
                id to (db.sessionDao().getById(id)?.isRoot ?: true)
            }
        }

    /** EXP-1291: whether the in-app session [sessionId] is a synced ROOT on
     *  the ACTIVE account; null when the row is not synced (yet). */
    suspend fun isLocalRoot(sessionId: String): Boolean? {
        val accountId = auth.activeAccountId.value ?: return null
        return runCatching { dbHolder.database(accountId).sessionDao().getById(sessionId)?.isRoot }.getOrNull()
    }

    /**
     * #EXP-1456 (fixtures/inbox-route.json `sessionCases`, the S-3 rule):
     * where the synced session [sessionId] opens for the signed-in viewer —
     * the agent's channel for my own agent conversation root, the thread for
     * a thread's root or child, else the session itself. A child resolves
     * its agent + owner through its root. Null when the row is not synced
     * (yet): the caller opens the session route's not-found state.
     */
    suspend fun sessionTarget(sessionId: String): InboxRoute.SessionTarget? = activeDb(null) { db ->
        val session = db.sessionDao().getById(sessionId) ?: return@activeDb null
        val rootId = session.rootId.takeIf { it.isNotEmpty() } ?: session.id
        val root = if (rootId == session.id) session else db.sessionDao().getById(rootId)
        InboxRoute.sessionTarget(
            id = session.id,
            rootId = rootId,
            agentId = root?.agentId ?: session.agentId,
            userId = root?.userId ?: session.userId,
            viewerId = auth.userId.value,
        )
    }

    /** #EXP-1493 (plan-page.json WAYS IN): the session's own waiting plan
     *  message ([PlanPage.planPageFor]) an inbox row / push opens THE plan
     *  page on; null = none (the session's own route). */
    suspend fun planPageTarget(sessionId: String): String? = activeDb(null) { db ->
        val messages = db.messageDao().observeBySession(sessionId).first().mapNotNull { row ->
            val message = SessionTree.message(row, sessionId)
            if (message.card?.get("kind")?.toString() != "\"plan\"") return@mapNotNull null
            PlanPage.Message(
                id = message.id,
                sessionId = sessionId,
                authorSessionId = message.authorSessionId,
                createdAt = WireTimestamps.parseEpochMs(message.createdAt) ?: 0L,
                card = message.card,
            )
        }
        PlanPage.planPageFor(sessionId, messages)
    }

    /**
     * Thread 58cb32df (chats.json): a notification / deep link that names
     * another team SWITCHES first, then opens the target. The team of the
     * synced issue / session [id] on the ACTIVE account (an issue's = its
     * board's); null when the row is not synced (yet).
     */
    suspend fun teamOfIssue(id: String, accountId: String? = null): String? = activeDb(accountId) { db ->
        val issue = db.issueDao().observeById(id).first() ?: return@activeDb null
        db.boardDao().observeAll().first().firstOrNull { it.id == issue.boardId }?.teamId
    }

    suspend fun teamOfSession(id: String, accountId: String? = null): String? = activeDb(accountId) { db ->
        db.sessionDao().getById(id)?.teamId?.takeIf { it.isNotEmpty() }
    }

    private suspend fun <T> activeDb(
        account: String?,
        block: suspend (com.exponential.app.data.db.ExponentialDatabase) -> T?,
    ): T? {
        val accountId = account ?: auth.activeAccountId.value ?: return null
        return runCatching { block(dbHolder.database(accountId)) }.getOrNull()
    }

    private suspend fun poll(
        host: String,
        lookup: suspend (com.exponential.app.data.db.ExponentialDatabase) -> Pair<String, Boolean>?,
    ): Resolution {
        // Signed-in accounts on the link's instance — active account first,
        // then most recently used (several accounts can share a host).
        val activeId = auth.activeAccountId.value
        val candidates = auth.accounts.value
            .filter { it.token != null && instanceHost(it.instanceUrl) == host }
            .sortedWith(
                compareByDescending<com.exponential.app.data.auth.ServerAccount> { it.id == activeId }
                    .thenByDescending { it.lastUsedAt }
            )
        if (candidates.isEmpty()) return Resolution.NotFound

        repeat(POLL_ATTEMPTS) { attempt ->
            for (account in candidates) {
                val hit = runCatching { lookup(dbHolder.database(account.id)) }.getOrNull()
                if (hit != null) return Resolution.Found(hit.first, account.id, isRoot = hit.second)
            }
            if (attempt < POLL_ATTEMPTS - 1) delay(POLL_INTERVAL_MS)
        }
        return Resolution.NotFound
    }

    private fun instanceHost(instanceUrl: String): String? =
        runCatching { URI(instanceUrl.trim()).host }.getOrNull()

    private companion object {
        // ~10s total: enough for the post-launch sync to land a fresh issue
        // without holding an unresolvable link hostage forever.
        const val POLL_ATTEMPTS = 20
        const val POLL_INTERVAL_MS = 500L
    }
}
