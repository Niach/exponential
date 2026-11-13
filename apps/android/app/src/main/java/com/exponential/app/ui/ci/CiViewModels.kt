package com.exponential.app.ui.ci

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CiApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CiJobEntity
import com.exponential.app.data.db.CiRunEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CiRun
import com.exponential.app.domain.CiRunView
import com.exponential.app.domain.RunnerSettings
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** The selected job's log as read so far ([complete] = no more reads). */
data class CiLogState(
    val jobId: String? = null,
    val text: String = "",
    val loaded: Boolean = false,
    val complete: Boolean = false,
)

/** A synced `ci_jobs` row as the page's [CiRun.Job] ([labels] = device row id → label). */
internal fun CiJobEntity.toJob(labels: Map<String, String>): CiRun.Job = CiRun.Job(
    id = id,
    check = check,
    position = position ?: 0,
    status = status,
    conclusion = conclusion,
    reason = reason,
    runnerLabel = runnerDeviceId?.let { labels[it] },
    startedAt = startedAt,
    completedAt = completedAt,
    steps = CiRun.parseSteps(steps),
)

internal fun CiRunEntity.toRun(): CiRun.Run = CiRun.Run(
    id = id,
    number = number ?: 0,
    name = name,
    state = state,
    error = error,
    sha = sha,
    startedAt = startedAt,
    completedAt = completedAt,
)

/**
 * #EXP-1617 / #EXP-1804 (ci-run.json): THE run page (`ci-run/{runId}?job=`).
 * The run and its jobs are synced rows; the selected job's LOG is read on
 * demand (`ci.log`): once from 0, then from `nextOffset` every
 * [CiRun.LIVE_TAIL_MS] while it is not `complete`.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class CiRunViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val api: CiApi,
) : ViewModel() {

    val runId: String = savedStateHandle["runId"] ?: ""
    private val requestedJob: String? = savedStateHandle.get<String>("job")?.takeIf { it.isNotEmpty() }

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** Null while not synced (yet). */
    val run: StateFlow<CiRunEntity?> = dbFlow.scopedQuery(null as CiRunEntity?) { it.ciRunDao().observeById(runId) }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val devices = dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() }

    val jobs: StateFlow<List<CiRun.Job>> = combine(
        dbFlow.scopedQuery(emptyList<CiJobEntity>()) { it.ciJobDao().observeByRun(runId) },
        devices,
    ) { rows, devices ->
        val labels = devices.associate { it.id to it.label.ifBlank { it.deviceId } }
        rows.map { it.toJob(labels) }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The person's pick (`?job=`, then opening a job); null = the rule's pick. */
    private val picked = MutableStateFlow<String?>(requestedJob)

    /** The selected job: [CiRun.defaultJob] over the picked one. */
    val selectedJobId: StateFlow<String?> = combine(jobs, picked) { jobs, picked -> CiRun.defaultJob(jobs, picked) }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    fun selectJob(jobId: String) {
        picked.value = jobId
    }

    private val _log = MutableStateFlow(CiLogState())
    val log: StateFlow<CiLogState> = _log

    private var tail: Job? = null

    init {
        viewModelScope.launch {
            selectedJobId.collect { jobId -> startTail(jobId) }
        }
    }

    private fun startTail(jobId: String?) {
        tail?.cancel()
        _log.value = CiLogState(jobId = jobId)
        if (jobId == null) return
        tail = viewModelScope.launch {
            var offset = 0L
            while (true) {
                val accountId = auth.activeAccountId.value
                val read = accountId?.let { id ->
                    runCatching { api.log(id, jobId, offset) }
                        .onFailure { if (it is CancellationException) throw it }
                        .getOrNull()
                }
                if (read != null) {
                    offset = maxOf(offset, read.nextOffset)
                    _log.value = _log.value.copy(text = _log.value.text + read.text, loaded = true, complete = read.complete)
                    if (read.complete) break
                }
                delay(CiRun.LIVE_TAIL_MS)
            }
        }
    }

    private val _busy = MutableStateFlow<CiRunView.Busy?>(null)
    val busy: StateFlow<CiRunView.Busy?> = _busy

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure
    fun claimFailure(failure: FailedAction): Boolean = _failure.compareAndSet(failure, null)

    /** The new run Re-run failed made: the page opens it. */
    private val _opened = MutableSharedFlow<String>(extraBufferCapacity = 1)
    val opened: SharedFlow<String> = _opened

    fun rerunFailed() {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _busy.value = CiRunView.Busy.Rerun
            runCatching { api.rerunFailed(accountId, runId) }
                .onSuccess { _opened.tryEmit(it) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _failure.value = FailedAction("re-run", "the failed jobs", trpcErrorMessage(t, ""), retry = ::rerunFailed)
                }
            _busy.value = null
        }
    }

    fun cancel() {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _busy.value = CiRunView.Busy.Cancel
            runCatching { api.cancel(accountId, runId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _failure.value = FailedAction("cancel", "the run", trpcErrorMessage(t, ""), retry = ::cancel)
                }
            _busy.value = null
        }
    }
}

/**
 * #EXP-1617 / #EXP-1804 (runner-settings.json): a device's CI runner section:
 * the jobs it ran (with their run's number and repo) and the one write,
 * `ci.setRunner`, the whole settings object per change.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class RunnerSettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val api: CiApi,
    private val repositoriesApi: RepositoriesApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val deviceRowId = MutableStateFlow<String?>(null)

    fun bind(rowId: String?) {
        deviceRowId.value = rowId
    }

    /** repository id → `owner/name`, filled per team on demand. */
    private val repoNames = MutableStateFlow<Map<String, String>>(emptyMap())
    private val askedTeams = HashSet<String>()

    private fun loadRepos(teamIds: Set<String>) {
        val accountId = auth.activeAccountId.value ?: return
        for (teamId in teamIds) {
            if (!synchronized(askedTeams) { askedTeams.add(teamId) }) continue
            viewModelScope.launch {
                runCatching { repositoriesApi.list(accountId, teamId) }
                    .onSuccess { repos -> repoNames.value = repoNames.value + repos.associate { it.id to it.fullName } }
                    .onFailure {
                        if (it is CancellationException) throw it
                        synchronized(askedTeams) { askedTeams.remove(teamId) }
                    }
            }
        }
    }

    private val rows = deviceRowId.flatMapLatest { id ->
        if (id == null) {
            flowOf(emptyList<Pair<CiJobEntity, CiRunEntity?>>())
        } else {
            dbFlow.flatMapLatest { db ->
                if (db == null) {
                    flowOf(emptyList())
                } else {
                    db.ciJobDao().observeByRunner(id, RECENT_WINDOW).flatMapLatest { jobs ->
                        val runIds = jobs.map { it.runId }.distinct()
                        if (runIds.isEmpty()) {
                            flowOf(emptyList())
                        } else {
                            db.ciRunDao().observeByIds(runIds).map { runs ->
                                val byId = runs.associateBy { it.id }
                                jobs.map { it to byId[it.runId] }
                            }
                        }
                    }
                }
            }
        }
    }

    val jobs: StateFlow<List<RunnerSettings.RunnerJob>> = combine(rows, repoNames) { rows, names ->
        loadRepos(rows.mapNotNull { it.second?.teamId }.toSet())
        rows.map { (job, run) ->
            RunnerSettings.RunnerJob(
                id = job.id,
                runnerDeviceId = job.runnerDeviceId,
                check = job.check,
                runNumber = run?.number ?: 0,
                repo = run?.repositoryId?.let { names[it] }.orEmpty(),
                status = job.status,
                conclusion = job.conclusion,
                startedAt = job.startedAt,
                completedAt = job.completedAt,
                runId = job.runId,
            )
        }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure
    fun claimFailure(failure: FailedAction): Boolean = _failure.compareAndSet(failure, null)

    /** `ci.setRunner` with the WHOLE object; a failure toasts and [onFailed]
     *  hands the caller its optimistic value back. */
    fun setRunner(deviceId: String, settings: RunnerSettings.Settings, onFailed: () -> Unit = {}) {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            runCatching {
                api.setRunner(accountId, deviceId, settings.enabled, settings.labels, settings.slots, settings.timeoutMin)
            }.onFailure { t ->
                if (t is CancellationException) throw t
                onFailed()
                _failure.value = FailedAction("save", "the CI runner", trpcErrorMessage(t, ""), retry = { setRunner(deviceId, settings, onFailed) })
            }
        }
    }

    private companion object {
        /** Rows read before [RunnerSettings.recentJobs] sorts and caps them. */
        const val RECENT_WINDOW = 25
    }
}
