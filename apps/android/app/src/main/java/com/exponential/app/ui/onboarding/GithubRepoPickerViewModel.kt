package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.isOfflineError
import com.exponential.app.data.api.OFFLINE_MESSAGE
import com.exponential.app.ui.components.FailedAction
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

    // FEED-79: a failed tapped re-list, connect hop, add or offline lookup =
    // ONE error toast; an automatic load (open, resume) stays quiet.
    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure.asStateFlow()

    fun clearFailure() {
        _failure.value = null
    }

    // A connect hop being minted (the ticket round-trip before the tab opens).
    private val _connecting = MutableStateFlow(false)
    val connecting: StateFlow<Boolean> = _connecting.asStateFlow()

    // FEED-30: the footer's "Add by name" escape hatch — its own busy flag and
    // the field's own line (the server's message verbatim: it names the real
    // reason; invalid input, FEED-79).
    private val _lookupBusy = MutableStateFlow(false)
    val lookupBusy: StateFlow<Boolean> = _lookupBusy.asStateFlow()

    private val _lookupError = MutableStateFlow<String?>(null)
    val lookupError: StateFlow<String?> = _lookupError.asStateFlow()

    // FEED-42: tap adds. The add runs HERE (viewModelScope) so the sheet stays
    // open while it's in flight and on failure, and dismisses on success.
    private val _adding = MutableStateFlow(false)
    val adding: StateFlow<Boolean> = _adding.asStateFlow()

    // The FORBIDDEN arm of a failed add: [GithubCopy.ADD_FORBIDDEN] + its one
    // fix, "Reconnect GitHub" (a missing prerequisite, not an error line).
    private val _addForbidden = MutableStateFlow(false)
    val addForbidden: StateFlow<Boolean> = _addForbidden.asStateFlow()

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
                    _failure.value = FailedAction("connect", "GitHub", GithubCopy.LINK_FAILED)
                    return@collect
                }
                reloadAfterHop()
            }
        }
        // The link-ticket hop's outcome (EXP-1126 link mode): a linked GitHub
        // re-lists with the cache bypassed; a failure shows its reason.
        viewModelScope.launch {
            auth.linkResult.drop(1).collect { result ->
                if (result.providerId != GithubConnectStarter.PROVIDER) return@collect
                if (result.error != null) {
                    _failure.value = FailedAction("connect", "GitHub", result.error)
                } else {
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

    // Typing clears a previous lookup failure (web parity).
    fun clearLookupError() {
        _lookupError.value = null
    }

    // A freshly opened sheet starts clean — the VM outlives one presentation.
    fun resetTransient() {
        _addForbidden.value = false
        _lookupError.value = null
        _failure.value = null
    }

    /**
     * SLOP-26: Connect (or reconnect) the viewer's GitHub account — the
     * link-ticket hop, the guided page ([GithubReposResult.connectUrl]) as the
     * fallback — handing the URL to [open] (a Custom Tab).
     */
    fun connectGithub(open: (String) -> Unit) {
        val account = lastAccountId ?: return
        if (_connecting.value) return
        _connecting.value = true
        viewModelScope.launch {
            try {
                val hop = connectStarter.start(account, _result.value?.connectUrl)
                if (hop == null) {
                    _failure.value = FailedAction("connect", "GitHub", GithubCopy.LINK_FAILED) { connectGithub(open) }
                } else {
                    open(hop.url)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _failure.value = FailedAction("connect", "GitHub", trpcErrorMessage(e, GithubCopy.LINK_FAILED)) {
                    connectGithub(open)
                }
            } finally {
                _connecting.value = false
            }
        }
    }

    /**
     * FEED-42: add [repo] through the host's [onAdd] (which throws on failure),
     * then [onAdded] (the sheet dismisses). A failure is a toast with the
     * server message verbatim (Retry adds again); the FORBIDDEN arm is the
     * picker's Reconnect GitHub notice instead.
     */
    fun add(repo: GithubPickerRepo, onAdd: suspend (GithubPickerRepo) -> Unit, onAdded: () -> Unit) {
        if (_adding.value) return
        viewModelScope.launch {
            _adding.value = true
            _addForbidden.value = false
            try {
                onAdd(repo)
                _adding.value = false
                onAdded()
            } catch (e: CancellationException) {
                _adding.value = false
                throw e
            } catch (e: Exception) {
                val trpc = e as? TrpcException
                if (GithubCopy.isGrantForbidden(trpc?.code, trpc?.status?.value, trpc?.message)) {
                    _addForbidden.value = true
                } else {
                    _failure.value = FailedAction("add", repo.fullName, trpcErrorMessage(e, "")) {
                        add(repo, onAdd, onAdded)
                    }
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
                // The server's reason is about the typed name: the field's
                // own line. Offline is a failed action, a toast.
                if (isOfflineError(e)) {
                    _failure.value = FailedAction("look up", "the repository", OFFLINE_MESSAGE) {
                        lookup(fullName, onFound)
                    }
                } else {
                    _lookupError.value = trpcErrorMessage(e, "Couldn’t look up the repository")
                }
                _lookupBusy.value = false
            }
        }
    }

    /**
     * FEED-79: the list loads on open and on every resume, so those fail
     * quietly (the sheet keeps what it has); only a re-list the person
     * [tapped] (Refresh, "I've done that") toasts its failure.
     */
    fun load(accountId: String, teamId: String, refresh: Boolean = false, tapped: Boolean = false) {
        lastAccountId = accountId
        lastTeamId = teamId
        // The deep link and the sheet's on-resume refresh can fire back to back;
        // restarting keeps a single in-flight query.
        loadJob?.cancel()
        loadJob = viewModelScope.launch {
            _loading.value = true
            try {
                _result.value = integrationsApi.githubRepos(accountId, teamId, refresh)
                _loading.value = false
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if (tapped) {
                    _failure.value = FailedAction("load", "your GitHub repositories", trpcErrorMessage(e, "")) {
                        load(accountId, teamId, refresh = true, tapped = true)
                    }
                }
                _loading.value = false
            }
        }
    }
}
