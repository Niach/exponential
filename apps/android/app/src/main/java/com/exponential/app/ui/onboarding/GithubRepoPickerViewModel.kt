package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.GithubReposResult
import com.exponential.app.data.api.IntegrationsApi
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

// Backs [GithubRepoPickerSheet]: loads the user's installable repos over the
// `integrations.github.repos` query and exposes a refresh so returning from the
// GitHub App install (Custom Tab) re-detects the new connection.
@HiltViewModel
class GithubRepoPickerViewModel @Inject constructor(
    private val integrationsApi: IntegrationsApi,
) : ViewModel() {

    private val _result = MutableStateFlow<GithubReposResult?>(null)
    val result: StateFlow<GithubReposResult?> = _result.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    fun load(accountId: String, refresh: Boolean = false) {
        viewModelScope.launch {
            _loading.value = true
            runCatching { integrationsApi.githubRepos(accountId, refresh) }
                .onSuccess { _result.value = it; _error.value = null }
                .onFailure { _error.value = it.message }
            _loading.value = false
        }
    }
}
