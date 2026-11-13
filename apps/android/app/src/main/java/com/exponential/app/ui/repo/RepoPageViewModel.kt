package com.exponential.app.ui.repo

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.CiApi
import com.exponential.app.data.api.RepoCiConfig
import com.exponential.app.data.api.RepoCiSettings
import com.exponential.app.data.api.RepoSettings
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.CiJobEntity
import com.exponential.app.data.db.CiRunEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.RepoCi
import com.exponential.app.domain.RepoPage
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** A synced PR as the repo page's folds read it. */
internal fun PullRequestEntity.toRepoPr() = RepoPage.Pr(
    id = id,
    number = number ?: 0,
    title = title,
    head = head,
    base = base,
    state = state,
    checksState = checksState,
)

/** A synced CI run / job as repo-ci.json's folds read them. */
internal fun CiRunEntity.toRepoCiRun() = RepoCi.Run(
    id = id,
    number = number ?: 0,
    name = name,
    sha = sha,
    ref = ref,
    trigger = trigger,
    state = state,
    startedAt = startedAt,
    completedAt = completedAt,
)

internal fun CiJobEntity.toRepoCiJob() = RepoCi.Job(runId = runId, status = status, conclusion = conclusion, check = check)

/** The repo page's server reads (repositories are never synced). */
data class RepoPageState(
    /** False until the first `repositories.list` returned. */
    val loaded: Boolean = false,
    val repo: TeamRepo? = null,
    /** null = still loading. */
    val branches: List<Pair<String, String>>? = null,
    /** ecode only; null = still loading (or a GitHub repo). */
    val settings: RepoSettings? = null,
    /** #EXP-1617 `ci.repoSettings`; null = still loading. */
    val ci: RepoCiSettings? = null,
    val busy: Boolean = false,
)

/**
 * #EXP-1616 THE REPO PAGE (repo-page.json): `repo/{repositoryId}?tab=`; the
 * repo row off `repositories.list`, its PRs off the synced `pull_requests`,
 * branches + settings over tRPC. A failed read or write = a toast.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class RepoPageViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val selection: TeamSelection,
    private val api: RepositoriesApi,
    private val ciApi: CiApi,
) : ViewModel() {

    val repositoryId: String = savedStateHandle.get<String>("repositoryId").orEmpty()
    val initialTab: String = RepoPage.repoTab(savedStateHandle.get<String>("tab"))

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The repo's synced PRs (every state; the folds keep the live ones). */
    val prs: StateFlow<List<PullRequestEntity>> = combine(dbFlow, selection.selectedId) { db, team -> db to team }
        .flatMapLatest { (db, team) ->
            if (db == null || team == null) flowOf(emptyList())
            else db.pullRequestDao().observeByTeam(team).map { rows -> rows.filter { it.repositoryId == repositoryId } }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** #EXP-1617 (repo-ci.json): the repo's synced CI runs + their jobs. */
    val ciRuns: StateFlow<List<RepoCi.Run>> = dbFlow
        .flatMapLatest { db ->
            if (db == null) flowOf(emptyList())
            else db.ciRunDao().observeByRepository(repositoryId).map { rows -> rows.map { it.toRepoCiRun() } }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val ciJobs: StateFlow<List<RepoCi.Job>> = dbFlow
        .flatMapLatest { db ->
            if (db == null) flowOf(emptyList())
            else db.ciJobDao().observeByRepository(repositoryId).map { rows -> rows.map { it.toRepoCiJob() } }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Owners own the default branch, the backups' Restore and the mirror. */
    val isOwner: StateFlow<Boolean> = combine(dbFlow, selection.selectedId, auth.userId) { db, team, user -> Triple(db, team, user) }
        .flatMapLatest { (db, team, user) ->
            if (db == null || team == null || user == null) flowOf(false)
            else db.teamMemberDao().observeByTeam(team).map { rows ->
                rows.any { it.userId == user && it.role == DomainContract.teamRoleOwner }
            }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    private val _state = MutableStateFlow(RepoPageState())
    val state: StateFlow<RepoPageState> = _state

    /** repo-ci.json: the repo's managers (the member who shared it, or a
     *  team owner) change its CI settings; for others every control is off. */
    val isManager: StateFlow<Boolean> = combine(isOwner, _state, auth.userId) { owner, s, user ->
        owner || (user != null && s.repo?.sharedBy?.id == user)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    private fun fail(verb: String, thing: String, error: Throwable, retry: (() -> Unit)? = null) {
        _failure.value = FailedAction(verb, thing, trpcErrorMessage(error, ""), retry = retry)
    }

    init {
        load()
    }

    fun load() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            val teamId = selection.selectedId.value
            if (accountId == null || teamId == null) {
                _state.value = _state.value.copy(loaded = true)
                return@launch
            }
            val repo = try {
                api.list(accountId, teamId).firstOrNull { it.id == repositoryId }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                fail("load", "the repository", t) { load() }
                null
            }
            _state.value = _state.value.copy(loaded = true, repo = repo)
            if (repo != null) loadDetails(accountId, repo)
        }
    }

    private suspend fun loadDetails(accountId: String, repo: TeamRepo) {
        val ecode = repo.provider == RepoPage.PROVIDER_ECODE
        val branches = viewModelScope.async {
            try {
                if (ecode) api.branches(accountId, repo.id).map { it.name to it.sha }
                else api.listBranches(accountId, repo.id).map { it to "" }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                fail("load", "the branches", t)
                emptyList()
            }
        }
        val settings = if (ecode) {
            viewModelScope.async {
                try {
                    api.settings(accountId, repo.id)
                } catch (t: Throwable) {
                    if (t is CancellationException) throw t
                    fail("load", "the repository settings", t)
                    null
                }
            }
        } else {
            null
        }
        val ci = viewModelScope.async {
            try {
                ciApi.repoSettings(accountId, repo.id)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                fail("load", "the CI settings", t)
                RepoCiSettings()
            }
        }
        _state.value = _state.value.copy(branches = branches.await(), settings = settings?.await(), ci = ci.await())
    }

    /** One CI write (no busy lock, the toggles answer at once), then a
     *  re-read of `ci.repoSettings`; a failure toasts with Retry and puts
     *  the stored values back. */
    private fun ciAct(verb: String, thing: String, onDone: (Boolean) -> Unit = {}, block: suspend (accountId: String) -> Unit) {
        val accountId = auth.activeAccountId.value ?: return onDone(false)
        viewModelScope.launch {
            val ok = try {
                block(accountId)
                true
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                fail(verb, thing, t) { ciAct(verb, thing, onDone, block) }
                false
            }
            val fresh = runCatching { ciApi.repoSettings(accountId, repositoryId) }.getOrNull()
            if (fresh != null) _state.value = _state.value.copy(ci = fresh)
            onDone(ok)
        }
    }

    private fun patchConfig(change: (RepoCiConfig) -> RepoCiConfig) {
        val ci = _state.value.ci ?: return
        _state.value = _state.value.copy(ci = ci.copy(config = change(ci.config)))
    }

    fun setRequiredChecks(checks: List<String>) {
        patchConfig { it.copy(requiredChecks = checks) }
        ciAct("save", "the required checks") { ciApi.setRepoConfig(it, repositoryId, requiredChecks = checks) }
    }

    fun setCiSwitch(switch: RepoCi.Switch, on: Boolean) {
        patchConfig {
            when (switch) {
                RepoCi.Switch.GithubStatuses -> it.copy(githubStatuses = on)
                RepoCi.Switch.ActionsRunners -> it.copy(actionsRunners = on)
            }
        }
        ciAct("save", "the CI settings") {
            when (switch) {
                RepoCi.Switch.GithubStatuses -> ciApi.setRepoConfig(it, repositoryId, githubStatuses = on)
                RepoCi.Switch.ActionsRunners -> ciApi.setRepoConfig(it, repositoryId, actionsRunners = on)
            }
        }
    }

    /** [onDone] = saved (the sheet closes) or not (it stays, the toast says why). */
    fun setSecret(name: String, value: String, onDone: (Boolean) -> Unit) =
        ciAct("save", "the secret", onDone) { ciApi.setSecret(it, repositoryId, name, value) }

    fun deleteSecret(name: String) =
        ciAct("delete", "the secret") { ciApi.deleteSecret(it, repositoryId, name) }

    /** One owner write, then a re-read; a failure toasts with Retry. */
    private fun act(verb: String, thing: String, block: suspend (accountId: String) -> Unit) {
        val accountId = auth.activeAccountId.value ?: return
        _state.value = _state.value.copy(busy = true)
        viewModelScope.launch {
            try {
                block(accountId)
                val repo = _state.value.repo
                if (repo != null) {
                    val teamId = selection.selectedId.value
                    val fresh = teamId?.let { team ->
                        runCatching { api.list(accountId, team).firstOrNull { it.id == repo.id } }.getOrNull()
                    } ?: repo
                    _state.value = _state.value.copy(repo = fresh)
                    loadDetails(accountId, fresh)
                }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                fail(verb, thing, t) { act(verb, thing, block) }
            } finally {
                _state.value = _state.value.copy(busy = false)
            }
        }
    }

    fun setDefaultBranch(branch: String) =
        act("set", "the default branch") { api.setDefaultBranch(it, repositoryId, branch) }

    fun setMirror(mirrorUrl: String?, enabled: Boolean) =
        act("save", "the mirror") { api.setMirror(it, repositoryId, mirrorUrl, enabled) }

    fun restore(backupId: String) =
        act("restore", "the backup") { api.restore(it, repositoryId, backupId) }
}
