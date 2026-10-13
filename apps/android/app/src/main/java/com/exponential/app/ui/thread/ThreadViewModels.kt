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
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// EXP-1284: the phone's thread reads. Every surface that shows a thread (the
// Thread screen, the Agent page's Running + Recent, the issue page's Threads
// row, a run screen's link to its thread, the Guide readers) resolves it
// through the ONE model ([ThreadView], fixture-locked ×4) via [ThreadPage] —
// the twin of web `hooks/use-threads.ts`.

/** T-N2: how long a landing waits for a new session's thread item. */
const val THREAD_LANDING_WAIT_MS = 4_000L

/** The thread rows + their items — the links every lookup starts from. */
private data class ThreadLinks(
    val threads: List<ThreadEntity>,
    val items: List<ThreadItemEntity>,
)

/** The synced rows every thread bundle is built from (all member teams). */
private data class ThreadRows(
    val links: ThreadLinks,
    val sessions: List<CodingSessionEntity>,
    val issues: List<IssueEntity>,
    val devices: List<DeviceEntity>,
)

/**
 * The list surfaces + the lookups other screens make: every member team's
 * threads resolved ([bundles]), Running vs Recent ([list]), the thread each
 * run sits in ([threadBySession]) and the threads holding an issue.
 *
 * EXP-1288: ONE set of Room observers per model. [links] (threads + items) is
 * shared by [threadBySession] and [rows]; [rows] is shared by [bundles] and
 * [orphanLive]. A screen that reads only [threadBySession] (the Work and
 * Changes pages) opens just the two link observers.
 */
@HiltViewModel
class ThreadsViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val currentUserId: StateFlow<String?> = auth.userId

    private val links: Flow<ThreadLinks> = combine(
        dbFlow.scopedQuery(emptyList<ThreadEntity>()) { it.threadDao().observeAll() },
        dbFlow.scopedQuery(emptyList<ThreadItemEntity>()) { it.threadItemDao().observeAll() },
    ) { threads, items -> ThreadLinks(threads, items) }
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    /** [rows] + the viewer + the liveness clock, shared by every derivation. */
    private val clocked: Flow<Triple<ThreadRows, String?, Long>> = combine(
        combine(
            links,
            dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() },
            dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
            dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
        ) { links, sessions, issues, devices -> ThreadRows(links, sessions, issues, devices) },
        auth.userId,
        CodingSessionLiveness.minuteTicker(),
    ) { rows, userId, now -> Triple(rows, userId, now) }
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    val bundles: StateFlow<List<ThreadPage.Bundle>> = clocked
        .map { (rows, userId, now) ->
            ThreadPage.bundles(
                rows.links.threads, rows.links.items, rows.sessions, rows.issues, rows.devices, userId, now,
            )
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Running (a live run of mine) vs Recent, newest activity first. */
    val list: StateFlow<ThreadView.Partition<ThreadPage.Bundle>> = bundles
        .map { ThreadPage.list(it) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ThreadView.Partition(emptyList(), emptyList()))

    /** The member teams — the lists band per team once there are several. */
    val teams: StateFlow<List<TeamEntity>> = dbFlow
        .scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** run id → the ONE thread it sits in (rule 8's Guide readers). */
    val threadBySession: StateFlow<Map<String, ThreadEntity>> = links
        .map { (threads, items) ->
            val byId = threads.associateBy { it.id }
            items.mapNotNull { item ->
                val session = item.sessionId ?: return@mapNotNull null
                byId[item.threadId]?.let { session to it }
            }.toMap()
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /**
     * T4: my LIVE sessions no thread item names yet (placement failed, or
     * the thread-items shape lags the sessions shape), drawn in Running as
     * plain session rows — a live session never vanishes from the lists.
     */
    val orphanLive: StateFlow<List<com.exponential.app.ui.session.PastRunRow>> = clocked
        .map { (rows, userId, now) ->
            val issuesById = rows.issues.associateBy { it.id }
            ThreadPage.orphanLiveSessions(rows.sessions, rows.links.items, userId, now).map { session ->
                com.exponential.app.ui.session.PastRunRow(
                    session = session,
                    issue = session.issueId?.let(issuesById::get),
                    batchIssues = com.exponential.app.domain.batchRunIssues(session, rows.issues),
                    device = com.exponential.app.domain.resolveSessionDevice(session, rows.devices, now),
                )
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * T-N2: the thread a just-started session lands in — waits up to
     * [timeoutMs] for its synced thread item (the start places it server-side);
     * null = still unplaced, the caller falls back to the session's screen.
     */
    suspend fun awaitThreadOf(sessionId: String, timeoutMs: Long = THREAD_LANDING_WAIT_MS): String? =
        kotlinx.coroutines.withTimeoutOrNull(timeoutMs) {
            threadBySession.first { it.containsKey(sessionId) }.getValue(sessionId).id
        }

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

    /** What the one `threads.get` fallback concluded (T-N9). */
    private enum class Fetch { Idle, Fetching, Found, NotFound, Failed }

    private val fetch = MutableStateFlow(Fetch.Idle)

    /** The local row as an ANSWER: null until the store answered. */
    private val localAnswer: Flow<Pair<ThreadEntity?, Boolean>?> = dbFlow
        .flatMapLatest { db ->
            if (db == null) {
                flowOf(null)
            } else {
                combine(
                    db.threadDao().observeById(threadId),
                    db.threadItemDao().observeByThread(threadId).map { it.isNotEmpty() },
                ) { row, named -> row to named }
            }
        }

    /**
     * EXP-1284 (T14/T-N9): the screen's `thread-not-found` state — true only
     * once the store holds no such thread, no synced item names it, and ONE
     * `threads.get` also came back without it (a tapped link racing the sync
     * lands on the thread instead of a flash of "not found"). False = the
     * skeleton while any of that is pending.
     */
    val missing: StateFlow<Boolean> = combine(localAnswer, fetch) { answer, fetched ->
        val (row, named) = answer ?: return@combine false
        row == null && !named && (fetched == Fetch.NotFound || fetched == Fetch.Failed)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

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
        // T-N9: the first local answer without the thread fetches it ONCE;
        // a found thread is written through so the screen renders at once
        // (the shapes reconcile it later).
        viewModelScope.launch {
            val answer = localAnswer.filterNotNull().first()
            if (answer.first != null || answer.second) return@launch
            fetchThread()
        }
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

    private suspend fun fetchThread() {
        if (fetch.value != Fetch.Idle) return
        val accountId = auth.activeAccountId.value ?: run { fetch.value = Fetch.Failed; return }
        fetch.value = Fetch.Fetching
        fetch.value = try {
            val fetched = threadsApi.get(accountId, threadId)
            val db = dbFlow.filterNotNull().first()
            db.threadDao().upsert(fetched.thread)
            fetched.items.forEach { db.threadItemDao().upsert(it) }
            Fetch.Found
        } catch (e: CancellationException) {
            throw e
        } catch (e: com.exponential.app.data.api.TrpcException) {
            if (e.code == "NOT_FOUND" || e.code == "FORBIDDEN") Fetch.NotFound else Fetch.Failed
        } catch (_: Exception) {
            Fetch.Failed
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
