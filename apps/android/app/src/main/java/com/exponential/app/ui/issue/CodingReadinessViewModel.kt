package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.GithubStatusResult
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.CodingReadiness
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.ui.steer.steerDeviceFlow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * EXP-1121: the INPUTS of the "Ready to code?" checklist for one issue's board
 * (web `use-coding-readiness.ts`), handed to the pure, fixture-locked
 * [CodingReadiness.derive]. Membership, steer availability and the synced
 * board come from the host (the issue model already has them); this owns the
 * rest: the devices shape (own + team-shared, online or not), the team's
 * repositories and its GitHub connection (tRPC — `repositories` is
 * server-only), plus the inline picker's `boards.setRepository`.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class CodingReadinessViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val repositoriesApi: RepositoriesApi,
    private val integrationsApi: IntegrationsApi,
) : ViewModel() {

    /** What the host knows: the issue's synced board, membership, relay switch. */
    data class Host(
        val board: BoardEntity?,
        val isMember: Boolean,
        val remoteStartEnabled: Boolean?,
    )

    /** A tRPC answer tagged with the team it was read for (a late answer for a
     *  previous team must never land on the current one). */
    private data class Tagged<T>(val teamId: String, val value: T?, val failed: Boolean)

    private data class DeviceSnapshot(val devices: List<CodingReadiness.Device>?, val nowMs: Long)

    data class UiState(
        val readiness: CodingReadiness.Readiness,
        val board: BoardEntity?,
        /** The team's repositories — the inline picker's list; null until the first answer. */
        val repos: List<TeamRepo>?,
        val accountId: String?,
    )

    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val host = MutableStateFlow(Host(board = null, isMember = false, remoteStartEnabled = null))
    private val teamId = host.map { it.board?.teamId }.distinctUntilChanged()
    private val active = host.map { it.isMember && it.remoteStartEnabled != false }.distinctUntilChanged()

    private val repos = MutableStateFlow<Tagged<List<TeamRepo>>?>(null)
    private val github = MutableStateFlow<Tagged<GithubStatusResult>?>(null)
    private val _setting = MutableStateFlow<String?>(null)
    /** The repository id a pick is writing (the row spins). */
    val setting: StateFlow<String?> = _setting
    private val _setError = MutableStateFlow<String?>(null)
    val setError: StateFlow<String?> = _setError

    private val teamName = combine(dbFlow, teamId) { db, id -> db to id }
        .flatMapLatest { (db, id) ->
            if (db == null || id == null) flowOf(null) else db.teamDao().observeById(id).map { it?.name }
        }

    // Own machines + servers shared with the team, ALL of them (the device
    // step's "last seen" line needs the offline ones), on the 30s liveness
    // tick — the snapshot carries the tick's clock so it is never deduped.
    private val devices = steerDeviceFlow(dbFlow, teamId, auth.userId).map { rows ->
        DeviceSnapshot(
            devices = rows?.map { device ->
                CodingReadiness.Device(
                    label = device.deviceLabel.ifBlank { device.deviceId },
                    own = device.owner == null,
                    online = device.online,
                    lastSeenAtMs = device.lastSeenAt?.let(WireTimestamps::parseEpochMs),
                )
            },
            nowMs = System.currentTimeMillis(),
        )
    }

    val state: StateFlow<UiState> = combine(
        host,
        teamName,
        repos,
        github,
        devices,
    ) { host, teamName, repos, github, snapshot ->
        val board = host.board
        val forTeam = board?.teamId
        val teamRepos = repos?.takeIf { it.teamId == forTeam }
        val status = github?.takeIf { it.teamId == forTeam }
        val repositoryId = board?.repositoryId
        val boardRepository = when {
            board == null || repositoryId == null -> null
            else -> teamRepos?.value?.firstOrNull { it.id == repositoryId }?.fullName ?: ""
        }
        // Asked only while the board has none; an unresolved board keeps the
        // whole checklist loading (null repository + null GitHub).
        val githubInput = if (board == null || repositoryId != null || teamRepos == null || status == null) {
            null
        } else {
            val list = teamRepos.value.orEmpty()
            val github = status.value
            // SLOP-7: "GitHub connected" = the viewer's own GitHub account is
            // linked with a live token, or the team already has repositories
            // (a teammate connected them — this person needs no GitHub of
            // their own to pick one). The label: the viewer's login, else the
            // first installed account, else the first repo's owner.
            val linked = github?.isLinked == true && github.needsReconnect != true
            CodingReadiness.Github(
                connected = list.isNotEmpty() || linked,
                label = github?.login?.takeIf { it.isNotBlank() }
                    ?: github?.installations?.firstOrNull { !it.accountLogin.isNullOrBlank() }?.accountLogin
                    ?: list.firstOrNull()?.fullName?.substringBefore('/')?.takeIf { it.isNotBlank() },
            )
        }
        val readiness = CodingReadiness.derive(
            CodingReadiness.Input(
                isMember = host.isMember,
                remoteStartEnabled = host.remoteStartEnabled,
                teamName = teamName.orEmpty(),
                boardName = board?.name.orEmpty(),
                boardRepository = boardRepository,
                github = githubInput,
                devices = if (board == null) null else snapshot.devices,
                nowMs = snapshot.nowMs,
            ),
        )
        UiState(
            readiness = readiness,
            board = board,
            repos = teamRepos?.value,
            accountId = auth.activeAccountId.value,
        )
    }.stateIn(
        viewModelScope,
        SharingStarted.WhileSubscribed(5_000),
        UiState(
            readiness = CodingReadiness.derive(
                CodingReadiness.Input(
                    isMember = false,
                    remoteStartEnabled = null,
                    teamName = "",
                    boardName = "",
                    boardRepository = null,
                    github = null,
                    devices = null,
                    nowMs = System.currentTimeMillis(),
                ),
            ),
            board = null,
            repos = null,
            accountId = null,
        ),
    )

    init {
        // The team's repositories, whenever the checklist is live for a team.
        viewModelScope.launch {
            combine(active, teamId, auth.activeAccountId) { on, team, account -> Triple(on, team, account) }
                .distinctUntilChanged()
                .collect { (on, team, account) ->
                    if (on && team != null && account != null) {
                        loadRepos(account, team)
                    }
                }
        }
        // GitHub state: asked only while the board has no repository.
        viewModelScope.launch {
            combine(active, host, auth.activeAccountId) { on, h, account ->
                val board = h.board
                if (on && board != null && board.repositoryId == null && account != null) {
                    account to board.teamId
                } else {
                    null
                }
            }
                .distinctUntilChanged()
                .collect { target -> target?.let { (account, team) -> loadGithub(account, team) } }
        }
        // A board pointed at a repository this copy of the list has never seen
        // (connected a moment ago here or on another client): re-list once per id.
        viewModelScope.launch {
            var relistedFor: String? = null
            combine(host, repos) { h, r -> h.board to r }.collect { (board, r) ->
                val repositoryId = board?.repositoryId ?: return@collect
                val list = r?.takeIf { it.teamId == board.teamId }?.value ?: return@collect
                if (list.any { it.id == repositoryId } || relistedFor == repositoryId) return@collect
                relistedFor = repositoryId
                reload()
            }
        }
    }

    fun bind(host: Host) {
        this.host.value = host
    }

    /** Re-list repositories + GitHub state (back from settings or the GitHub hop). */
    fun reload() {
        val h = host.value
        val board = h.board ?: return
        if (!h.isMember || h.remoteStartEnabled == false) return
        val account = auth.activeAccountId.value ?: return
        viewModelScope.launch { loadRepos(account, board.teamId) }
        if (board.repositoryId == null) viewModelScope.launch { loadGithub(account, board.teamId) }
    }

    private suspend fun loadRepos(accountId: String, teamId: String) {
        try {
            val rows = repositoriesApi.list(accountId, teamId)
            repos.value = Tagged(teamId, rows, failed = false)
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (_: Throwable) {
            // A failure keeps whatever this team had; a first failure resolves
            // to "no repositories" so the checklist is not stuck loading.
            val prior = repos.value?.takeIf { it.teamId == teamId }
            repos.value = Tagged(teamId, prior?.value, failed = true)
        }
    }

    private suspend fun loadGithub(accountId: String, teamId: String) {
        try {
            val status = integrationsApi.githubStatus(accountId, teamId)
            github.value = Tagged(teamId, status, failed = false)
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (_: Throwable) {
            val prior = github.value?.takeIf { it.teamId == teamId }
            github.value = Tagged(teamId, prior?.value, failed = true)
        }
    }

    /** The inline picker's pick: `boards.setRepository` (member-level), then
     *  [onDone] once it landed — the board row syncs the tick in. */
    fun setRepository(repositoryId: String, onDone: () -> Unit) {
        val board = host.value.board ?: return
        val account = auth.activeAccountId.value ?: return
        if (_setting.value != null) return
        _setting.value = repositoryId
        _setError.value = null
        viewModelScope.launch {
            try {
                repositoriesApi.setRepository(account, board.id, repositoryId)
                onDone()
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (t: Throwable) {
                _setError.value = trpcErrorMessage(t, "The repository could not be connected.")
            } finally {
                _setting.value = null
            }
        }
    }

    fun clearSetError() {
        _setError.value = null
    }

    /** The picker footer's "Add another repository from GitHub…": registers
     *  the repo with the team (`repositories.add`), then points the board at it
     *  (`boards.setRepository`, the picker's own call; iOS + web parity) so the
     *  repository step ticks, then re-lists so it shows. Throws on failure: the
     *  GitHub sheet renders it inline. `repositories.add`'s response is
     *  discarded by the API, so the new row's id comes from the re-list. */
    suspend fun addRepository(fullName: String, defaultBranch: String, isPrivate: Boolean) {
        val board = host.value.board ?: return
        val account = auth.activeAccountId.value ?: return
        repositoriesApi.add(account, board.teamId, fullName, defaultBranch, isPrivate)
        val added = repositoriesApi.list(account, board.teamId)
            .firstOrNull { it.fullName.equals(fullName, ignoreCase = true) }
        if (added != null) {
            repositoriesApi.setRepository(account, board.id, added.id)
        }
        loadRepos(account, board.teamId)
    }
}
