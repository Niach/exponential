package com.exponential.app.ui.thread

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.ThreadsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.ThreadEntity
import com.exponential.app.data.db.ThreadItemEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.ThreadPage
import com.exponential.app.domain.ThreadView
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.issueRefTarget
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// EXP-1284: the phone's thread reads. Every surface that shows a thread (the
// Thread screen, the Agent page's Running + Recent, the issue page's Threads
// row, a run screen's link to its thread, the Guide readers) resolves it
// through the ONE model ([ThreadView], fixture-locked ×4) via [ThreadPage] —
// the twin of web `hooks/use-threads.ts`.

/** The synced rows every thread bundle is built from (all member teams). */
private data class ThreadRows(
    val threads: List<ThreadEntity>,
    val items: List<ThreadItemEntity>,
    val sessions: List<CodingSessionEntity>,
    val issues: List<IssueEntity>,
    val devices: List<DeviceEntity>,
)

@OptIn(ExperimentalCoroutinesApi::class)
private fun threadRowsFlow(
    dbFlow: Flow<com.exponential.app.data.db.ExponentialDatabase?>,
): Flow<ThreadRows> = combine(
    dbFlow.scopedQuery(emptyList<ThreadEntity>()) { it.threadDao().observeAll() },
    dbFlow.scopedQuery(emptyList<ThreadItemEntity>()) { it.threadItemDao().observeAll() },
    dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() },
    dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
    dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
) { threads, items, sessions, issues, devices -> ThreadRows(threads, items, sessions, issues, devices) }

/**
 * The list surfaces + the lookups other screens make: every member team's
 * threads resolved ([bundles]), Running vs Recent ([list]), the thread each
 * run sits in ([threadBySession]) and the threads holding an issue.
 */
@HiltViewModel
class ThreadsViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val currentUserId: StateFlow<String?> = auth.userId

    val bundles: StateFlow<List<ThreadPage.Bundle>> = combine(
        threadRowsFlow(dbFlow),
        auth.userId,
        CodingSessionLiveness.minuteTicker(),
    ) { rows, userId, now ->
        ThreadPage.bundles(rows.threads, rows.items, rows.sessions, rows.issues, rows.devices, userId, now)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Running (a live run of mine) vs Recent, newest activity first. */
    val list: StateFlow<ThreadView.Partition<ThreadPage.Bundle>> = bundles
        .map { ThreadPage.list(it) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ThreadView.Partition(emptyList(), emptyList()))

    /** The member teams — the lists band per team once there are several. */
    val teams: StateFlow<List<TeamEntity>> = dbFlow
        .scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** run id → the ONE thread it sits in (rule 8's Guide readers). */
    val threadBySession: StateFlow<Map<String, ThreadEntity>> = combine(
        dbFlow.scopedQuery(emptyList<ThreadEntity>()) { it.threadDao().observeAll() },
        dbFlow.scopedQuery(emptyList<ThreadItemEntity>()) { it.threadItemDao().observeAll() },
    ) { threads, items ->
        val byId = threads.associateBy { it.id }
        items.mapNotNull { item ->
            val session = item.sessionId ?: return@mapNotNull null
            byId[item.threadId]?.let { session to it }
        }.toMap()
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** The threads holding [issueId] (the issue page's Threads row). */
    fun threadsOfIssue(bundles: List<ThreadPage.Bundle>, issueId: String): List<ThreadPage.Bundle> =
        bundles.filter { bundle -> bundle.items.any { it.issueId == issueId } }
            .sortedByDescending { com.exponential.app.domain.WireTimestamps.parseEpochMs(it.thread.updatedAt) ?: 0L }
}

/**
 * The Thread screen's model (route `thread/{threadId}`): the bundle, the
 * issue glyphs, the link candidates (THE search list), the steer gate and the
 * thread verbs — link, unlink, rename — each a `Couldn't …` toast on refusal.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ThreadScreenViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val threadsApi: ThreadsApi,
    private val issuesApi: IssuesApi,
    private val steerApi: SteerApi,
) : ViewModel() {

    val threadId: String = savedStateHandle.get<String>("threadId").orEmpty()

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val currentUserId: StateFlow<String?> = auth.userId

    private val thread: StateFlow<ThreadEntity?> = dbFlow
        .scopedQuery<ThreadEntity?>(null) { it.threadDao().observeById(threadId) }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    private val teamId: Flow<String?> = thread.map { it?.teamId }.distinctUntilChanged()

    val bundle: StateFlow<ThreadPage.Bundle?> = combine(
        thread,
        dbFlow.scopedQuery(emptyList<ThreadItemEntity>()) { it.threadItemDao().observeByThread(threadId) },
        teamId.flatMapLatest { team ->
            if (team == null) flowOf(emptyList())
            else dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeByTeam(team) }
        },
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
        combine(
            dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
            auth.userId,
            CodingSessionLiveness.minuteTicker(),
        ) { devices, userId, now -> Triple(devices, userId, now) },
    ) { row, items, sessions, issues, (devices, userId, now) ->
        row?.let {
            ThreadPage.bundles(listOf(it), items, sessions, issues, devices, userId, now).firstOrNull()
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The team's resolved statuses — the issue tabs' and chips' glyphs. */
    val statuses: StateFlow<List<ResolvedIssueStatus>> = teamId
        .flatMapLatest { team ->
            if (team == null) flowOf(emptyList())
            else dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) { it.issueStatusDao().observeByTeam(team) }
        }
        .map { rows -> if (rows.isEmpty()) IssueStatusResolver.builtinDefaults else IssueStatusResolver.teamStatuses(rows) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueStatusResolver.builtinDefaults)

    /** The team's issues as `#` targets — Link an issue's search list rows. */
    val linkCandidates: StateFlow<List<IssueRefTarget>> = teamId
        .flatMapLatest { team ->
            if (team == null) {
                flowOf(emptyList())
            } else {
                combine(
                    dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
                    dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeByTeam(team) },
                    statuses,
                ) { issues, boards, resolved ->
                    val boardIds = boards.map { it.id }.toSet()
                    issues.filter { it.boardId in boardIds }
                        .sortedByDescending { it.createdAt }
                        .map { issueRefTarget(it, resolved) }
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** A board of the team — where "New issue in this thread" files. The
     *  first linked issue's board wins, else the team's first board. */
    val newIssueBoardId: StateFlow<String?> = combine(
        bundle,
        teamId.flatMapLatest { team ->
            if (team == null) flowOf(emptyList())
            else dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeByTeam(team) }
        },
    ) { bundle, boards ->
        bundle?.issues?.firstOrNull()?.boardId ?: boards.firstOrNull()?.id
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The machines the runs' host labels resolve against. */
    val devices: StateFlow<List<DeviceEntity>> = dbFlow
        .scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    /** Whether runs can be steered/started at all (the relay is configured). */
    val steerEnabled: StateFlow<Boolean?> = _steerEnabled

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    init {
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _steerEnabled.value = null
                _steerEnabled.value = if (accountId == null) {
                    false
                } else {
                    runCatching { steerApi.config(accountId).enabled }.getOrDefault(false)
                }
            }
        }
    }

    /** `threads.link` — the `+` menu's Link an issue pick. */
    fun linkIssue(issueId: String) = write(FailedAction("link", "the issue")) { accountId ->
        threadsApi.linkIssue(accountId, threadId, issueId)
    }

    /** `threads.unlink` — an issue tab's Unlink. */
    fun unlinkIssue(issueId: String) = write(FailedAction("unlink", "the issue")) { accountId ->
        threadsApi.unlinkIssue(accountId, threadId, issueId)
    }

    /** `threads.update` — a blank [title] clears back to the derived one. */
    fun rename(title: String) = write(FailedAction("rename", "the thread")) { accountId ->
        threadsApi.update(accountId, threadId, title)
    }

    /** EXP-892: `issues.search` over the thread's team behind the local rows. */
    suspend fun searchIssues(query: String): List<IssueRefTarget> {
        val accountId = auth.activeAccountId.value ?: return emptyList()
        val team = thread.value?.teamId ?: return emptyList()
        return try {
            issuesApi.search(accountId, team, query).map(::issueRefTarget)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            emptyList()
        }
    }

    private fun write(failed: FailedAction, call: suspend (accountId: String) -> Any?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                call(accountId)
            } catch (e: CancellationException) {
                throw e
            } catch (t: Throwable) {
                _failure.value = failed.copy(message = trpcErrorMessage(t, ""))
            }
        }
    }
}
