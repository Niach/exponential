package com.exponential.app.data

import com.exponential.app.data.api.OpenPullsRepo
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.domain.PullRepo
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * EXP-1244: the app-wide `repositories.openPulls` store (web
 * `lib/open-pulls-store.ts`) — the Reviews screen AND the Reviews tab's dot
 * read it, so the page and its dot never disagree. One entry per (account,
 * team) with its fetch time; one in-flight fetch per key (a second refresh
 * joins it). A team whose fetch fails lists nothing. The Reviews screen
 * force-refreshes on entry; the app shell ([watch], [onForeground]) refetches
 * only teams older than [STALE_MS].
 */
@Singleton
class OpenPullsStore internal constructor(
    private val fetch: suspend (accountId: String, teamId: String) -> List<OpenPullsRepo>,
    private val now: () -> Long,
    private val scope: CoroutineScope,
) {
    @Inject
    constructor(repositoriesApi: RepositoriesApi) : this(
        fetch = repositoriesApi::openPulls,
        now = System::currentTimeMillis,
        scope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
    )

    data class Key(val accountId: String, val teamId: String)

    data class Entry(val repos: List<PullRepo>, val fetchedAt: Long)

    private val _entries = MutableStateFlow<Map<Key, Entry>>(emptyMap())
    val entries: StateFlow<Map<Key, Entry>> = _entries

    private val inflight = HashMap<Key, Job>()

    /** The last team set the app shell watched — what [onForeground] refreshes. */
    @Volatile
    private var watched: Pair<String, List<String>>? = null

    /** The store's repos for [teamIds] (in that order) on [accountId]. */
    fun pulls(accountId: String?, teamIds: List<String>): Flow<List<PullRepo>> =
        _entries
            .map { entries ->
                if (accountId == null) emptyList()
                else teamIds.flatMap { entries[Key(accountId, it)]?.repos.orEmpty() }
            }
            .distinctUntilChanged()

    /** Fetch one team's pulls unless a fresh (or in-flight) fetch exists. */
    fun refresh(accountId: String, teamId: String, force: Boolean = false) {
        val key = Key(accountId, teamId)
        synchronized(inflight) {
            if (inflight[key]?.isActive == true) return
            val entry = _entries.value[key]
            if (!force && entry != null && now() - entry.fetchedAt < STALE_MS) return
            inflight[key] = scope.launch {
                val repos = try {
                    fetch(accountId, teamId).map { PullRepo(teamId, it.repositoryId, it.fullName, it.pulls) }
                } catch (t: CancellationException) {
                    throw t
                } catch (_: Throwable) {
                    emptyList()
                }
                _entries.update { it + (key to Entry(repos, now())) }
                synchronized(inflight) { inflight.remove(key) }
            }
        }
    }

    fun refreshAll(accountId: String, teamIds: List<String>, force: Boolean = false) {
        for (teamId in teamIds) refresh(accountId, teamId, force)
    }

    /** The app shell's team set changed: remember it, refetch its stale teams. */
    fun watch(accountId: String?, teamIds: List<String>) {
        watched = accountId?.let { it to teamIds }
        if (accountId != null) refreshAll(accountId, teamIds)
    }

    /** App foreground (ProcessLifecycleOwner ON_START): refetch stale watched teams. */
    fun onForeground() {
        val (accountId, teamIds) = watched ?: return
        refreshAll(accountId, teamIds)
    }

    /** A merged pull has no Electric echo: drop it locally. */
    fun removePull(accountId: String, repositoryId: String, number: Int) {
        _entries.update { entries ->
            entries.mapValues { (key, entry) ->
                if (key.accountId != accountId) entry
                else entry.copy(
                    repos = entry.repos.map { repo ->
                        if (repo.repositoryId != repositoryId) repo
                        else repo.copy(pulls = repo.pulls.filter { it.number != number })
                    },
                )
            }
        }
    }

    companion object {
        /** The app shell refetches a team older than this; the screen always does. */
        const val STALE_MS = 60_000L
    }
}
