package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.GithubPickerRepo
import com.exponential.app.data.api.GithubReposResult
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.GithubConnectStarter
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.domain.GithubCopy
import com.exponential.app.domain.isRepoFullName
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch

// Backs [GithubRepoPickerSheet]: loads the viewer's push-able repos over the
// `integrations.github.repos` query (mobile-marked) and exposes a refresh so
// returning from a GitHub hop re-detects the new state. Three return paths
// re-fetch: the `oauth-return?linked=github` deep link of the link-ticket hop
// ([AuthRepository.linkResult]), the exponential://github-connected deep link
// the guided page / the App's setup page fires (via the DeepLinkBus), and the
// sheet's on-resume refresh as the fallback for a manually closed tab.
/** FEED-42: a failed add, rendered inline in the still-open picker. */
data class GithubAddError(
    val message: String,
    /** The FORBIDDEN arm: [GithubCopy.ADD_FORBIDDEN] + "Reconnect GitHub". */
    val forbidden: Boolean = false,
)

@HiltViewModel
class GithubRepoPickerViewModel @Inject constructor(
    private val integrationsApi: IntegrationsApi,
    private val deepLinkBus: DeepLinkBus,
    private val auth: AuthRepository,
    private val connectStarter: GithubConnectStarter,
) : ViewModel() {

    private val _result = MutableStateFlow<GithubReposResult?>(null)
    val result: StateFlow<GithubReposResult?> = _result.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    // A failed connect hop's message — separate from `error`, which belongs
    // to the repos query.
    private val _connectError = MutableStateFlow<String?>(null)
    val connectError: StateFlow<String?> = _connectError.asStateFlow()

    // A connect hop being minted (the ticket round-trip before the tab opens).
    private val _connecting = MutableStateFlow(false)
    val connecting: StateFlow<Boolean> = _connecting.asStateFlow()

    // FEED-30: the footer's "Add by name" escape hatch — its own busy flag and
    // inline error (the server's message verbatim: it names the real reason).
    private val _lookupBusy = MutableStateFlow(false)
    val lookupBusy: StateFlow<Boolean> = _lookupBusy.asStateFlow()

    private val _lookupError = MutableStateFlow<String?>(null)
    val lookupError: StateFlow<String?> = _lookupError.asStateFlow()

    // FEED-42: tap adds. The add runs HERE (viewModelScope) so the sheet stays
    // open while it's in flight and on failure, and dismisses on success.
    private val _adding = MutableStateFlow(false)
    val adding: StateFlow<Boolean> = _adding.asStateFlow()

    private val _addError = MutableStateFlow<GithubAddError?>(null)
    val addError: StateFlow<GithubAddError?> = _addError.asStateFlow()

    private var lastAccountId: String? = null
    private var lastTeamId: String? = null
    private var loadJob: Job? = null

    init {
        // The guided page / the App's setup page fire exponential://
        // github-connected — that lands here (viewModelScope stays active
        // while the activity is stopped behind the tab), so the sheet the user
        // returns to already shows the fresh list. Event counter, not a
        // consumed one-shot (EXP-365): team settings may be collecting too,
        // and both must refresh. drop(1) skips the StateFlow replay. An error
        // slug means the hop FAILED: say so instead of refreshing.
        viewModelScope.launch {
            deepLinkBus.githubConnected.drop(1).collect { event ->
                if (event.error != null) {
                    _connectError.value = GithubCopy.LINK_FAILED
                    return@collect
                }
                _connectError.value = null
                reloadAfterHop()
            }
        }
        // The link-ticket hop's outcome (EXP-1126 link mode): a linked GitHub
        // re-lists with the cache bypassed; a failure shows its reason.
        viewModelScope.launch {
            auth.linkResult.drop(1).collect { result ->
                if (result.providerId != GithubConnectStarter.PROVIDER) return@collect
                if (result.error != null) {
                    _connectError.value = result.error
                } else {
                    _connectError.value = null
                    reloadAfterHop()
                }
            }
        }
    }

    private fun reloadAfterHop() {
        val account = lastAccountId
        val team = lastTeamId
        if (account != null && team != null) load(account, team, refresh = true)
    }

    // A fresh connect attempt clears the previous failure.
    fun clearConnectError() {
        _connectError.value = null
    }

    // Typing clears a previous lookup failure (web parity).
    fun clearLookupError() {
        _lookupError.value = null
    }

    // A freshly opened sheet starts clean — the VM outlives one presentation.
    fun resetTransient() {
        _addError.value = null
        _lookupError.value = null
        _connectError.value = null
    }

    /**
     * SLOP-26: Connect (or reconnect) the viewer's GitHub account — the
     * link-ticket hop, the guided page ([GithubReposResult.connectUrl]) as the
     * fallback — handing the URL to [open] (a Custom Tab).
     */
    fun connectGithub(open: (String) -> Unit) {
        val account = lastAccountId ?: return
        if (_connecting.value) return
        _connectError.value = null
        _connecting.value = true
        viewModelScope.launch {
            try {
                val hop = connectStarter.start(account, _result.value?.connectUrl)
                if (hop == null) {
                    _connectError.value = GithubCopy.LINK_FAILED
                } else {
                    open(hop.url)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _connectError.value = trpcErrorMessage(e, GithubCopy.LINK_FAILED)
            } finally {
                _connecting.value = false
            }
        }
    }

    /**
     * FEED-42: add [repo] through the host's [onAdd] (which throws on failure),
     * then [onAdded] (the sheet dismisses). A failure stays inline: the plan cap,
     * the FORBIDDEN arm, or the server message verbatim.
     */
    fun add(repo: GithubPickerRepo, onAdd: suspend (GithubPickerRepo) -> Unit, onAdded: () -> Unit) {
        if (_adding.value) return
        viewModelScope.launch {
            _adding.value = true
            _addError.value = null
            try {
                onAdd(repo)
                _adding.value = false
                onAdded()
            } catch (e: CancellationException) {
                _adding.value = false
                throw e
            } catch (e: Exception) {
                val message = trpcErrorMessage(e, "Couldn’t add the repository")
                val trpc = e as? TrpcException
                _addError.value = when {
                    GithubCopy.isGrantForbidden(trpc?.code, trpc?.status?.value, trpc?.message) ->
                        GithubAddError(GithubCopy.ADD_FORBIDDEN, forbidden = true)
                    else -> GithubAddError(message)
                }
                _adding.value = false
            }
        }
    }

    // FEED-30: integrations.github.lookupRepo for the typed `owner/name` — a
    // hit is handed to [onFound] exactly like a row pick, a miss lands in
    // [lookupError] (the server's message: it runs the connect gate, so it
    // names the real reason). Shape-invalid names and a lookup already in
    // flight are ignored (the button is disabled for both).
    fun lookup(fullName: String, onFound: (GithubPickerRepo) -> Unit) {
        val account = lastAccountId ?: return
        val team = lastTeamId ?: return
        val name = fullName.trim()
        if (_lookupBusy.value || !isRepoFullName(name)) return
        viewModelScope.launch {
            _lookupBusy.value = true
            _lookupError.value = null
            try {
                val repo = integrationsApi.lookupRepo(account, team, name)
                _lookupBusy.value = false
                onFound(repo)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _lookupError.value = trpcErrorMessage(e, "Couldn’t look up the repository")
                _lookupBusy.value = false
            }
        }
    }

    fun load(accountId: String, teamId: String, refresh: Boolean = false) {
        lastAccountId = accountId
        lastTeamId = teamId
        // The deep link and the sheet's on-resume refresh can fire back to back;
        // restarting keeps a single in-flight query.
        loadJob?.cancel()
        loadJob = viewModelScope.launch {
            _loading.value = true
            try {
                _result.value = integrationsApi.githubRepos(accountId, teamId, refresh)
                _error.value = null
                _loading.value = false
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _error.value = trpcErrorMessage(e, "Couldn’t load your GitHub repositories")
                _loading.value = false
            }
        }
    }
}
