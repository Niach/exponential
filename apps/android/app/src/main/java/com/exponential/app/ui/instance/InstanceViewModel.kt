package com.exponential.app.ui.instance

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.AppConstants
import com.exponential.app.data.api.AuthConfig
import com.exponential.app.data.api.AuthConfigApi
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Fetches the CLOUD instance's auth-config so the welcome screen can offer
 * "Continue with Google/Apple" directly, gated on which providers the cloud
 * actually enables (EXP-14) — never hardcoded. The config is only used to
 * decide which primary buttons render; if the fetch fails (offline), the
 * screen falls back to the plain "Use Exponential Cloud" button which routes
 * to the full login screen where the fetch can retry.
 */
data class InstanceState(
    val cloudConfig: AuthConfig? = null,
)

@HiltViewModel
class InstanceViewModel @Inject constructor(
    private val authConfigApi: AuthConfigApi,
    private val auth: AuthRepository,
) : ViewModel() {
    private val _state = MutableStateFlow(InstanceState())
    val state: StateFlow<InstanceState> = _state.asStateFlow()

    init {
        loadCloudConfig()
    }

    fun loadCloudConfig() {
        viewModelScope.launch {
            authConfigApi.fetch(AppConstants.PUBLIC_CLOUD_URL).onSuccess { config ->
                _state.value = _state.value.copy(cloudConfig = config)
            }
        }
    }

    // Cloud OAuth start URL for the welcome screen's direct provider buttons.
    // Mints a PKCE attempt per call (REV-13) — call ONLY from an onClick
    // handler, never while composing, so the in-memory verifier isn't rotated
    // under an in-flight attempt (last-start-wins).
    fun cloudStartUrl(provider: String): String =
        "${AppConstants.PUBLIC_CLOUD_URL}/api/mobile-oauth-start?provider=$provider&code_challenge=${auth.beginOauthAttempt()}"
}
