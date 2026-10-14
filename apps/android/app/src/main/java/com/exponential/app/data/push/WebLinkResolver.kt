package com.exponential.app.data.push

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import java.net.URI
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.delay

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
