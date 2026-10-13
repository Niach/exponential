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
        data class Found(val id: String, val accountId: String) : Resolution
        data object NotFound : Resolution
    }

    suspend fun resolve(target: DeepLinkBus.Target.WebIssueRef): Resolution =
        poll(target.host) { db -> db.issueDao().findIdByTeamRef(target.teamSlug, target.identifier) }

    /** EXP-1288: a `/t/{team}/threads/{id}` link → the synced thread. */
    suspend fun resolveThread(target: DeepLinkBus.Target.WebThreadRef): Resolution =
        poll(target.host) { db -> db.threadDao().findIdByTeamRef(target.teamSlug, target.threadId) }

    private suspend fun poll(
        host: String,
        lookup: suspend (com.exponential.app.data.db.ExponentialDatabase) -> String?,
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
                val id = runCatching { lookup(dbHolder.database(account.id)) }.getOrNull()
                if (id != null) return Resolution.Found(id, account.id)
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
