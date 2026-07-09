package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.GithubReposResult
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.push.DeepLinkBus
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

// Backs [GithubRepoPickerSheet]: loads the user's installable repos over the
// `integrations.github.repos` query (sent with platform="mobile" so the install
// URL deep-links back into the app) and exposes a refresh so returning from the
// GitHub App install re-detects the new connection. Two return paths re-fetch:
// the exponential://github-connected deep link the server's post-install page fires
// (observed here via the DeepLinkBus), and the sheet's on-resume refresh as a
// fallback for servers without the deep-link page / a manually closed tab.
@HiltViewModel
class GithubRepoPickerViewModel @Inject constructor(
    private val integrationsApi: IntegrationsApi,
    private val deepLinkBus: DeepLinkBus,
) : ViewModel() {

    private val _result = MutableStateFlow<GithubReposResult?>(null)
    val result: StateFlow<GithubReposResult?> = _result.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private var lastAccountId: String? = null
    private var lastWorkspaceId: String? = null
    private var loadJob: Job? = null

    init {
        // The install Custom Tab ends on the server's "connected" page, which
        // fires exponential://github-connected — that lands here (viewModelScope stays
        // active while the activity is stopped behind the tab), so the sheet
        // the user returns to already shows the fresh repo list.
        viewModelScope.launch {
            deepLinkBus.target.collect { target ->
                if (target is DeepLinkBus.Target.GithubConnected) {
                    deepLinkBus.consume()
                    val account = lastAccountId
                    val workspace = lastWorkspaceId
                    if (account != null && workspace != null) {
                        load(account, workspace, refresh = true)
                    }
                }
            }
        }
    }

    fun load(accountId: String, workspaceId: String, refresh: Boolean = false) {
        lastAccountId = accountId
        lastWorkspaceId = workspaceId
        // The deep link and the sheet's on-resume refresh can fire back to back;
        // restarting keeps a single in-flight query.
        loadJob?.cancel()
        loadJob = viewModelScope.launch {
            _loading.value = true
            try {
                _result.value = integrationsApi.githubRepos(accountId, workspaceId, refresh)
                _error.value = null
                _loading.value = false
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _error.value = e.message
                _loading.value = false
            }
        }
    }
}
