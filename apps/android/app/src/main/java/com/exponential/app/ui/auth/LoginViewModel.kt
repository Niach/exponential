package com.exponential.app.ui.auth

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.AuthConfig
import com.exponential.app.data.api.AuthConfigApi
import com.exponential.app.data.api.SignInResult
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class LoginState(
    val loading: Boolean = false,
    val error: String? = null,
    val successEmail: String? = null,
    val configLoading: Boolean = true,
    val config: AuthConfig? = null,
    val configError: String? = null,
)

@HiltViewModel
class LoginViewModel @Inject constructor(
    private val api: AuthApi,
    private val authConfigApi: AuthConfigApi,
    private val auth: AuthRepository,
) : ViewModel() {
    private val _state = MutableStateFlow(LoginState())
    val state: StateFlow<LoginState> = _state.asStateFlow()

    init {
        loadConfig()
    }

    fun loadConfig() {
        val instanceUrl = auth.instanceUrl.value
        if (instanceUrl == null) {
            _state.value = _state.value.copy(configLoading = false, configError = "No instance URL")
            return
        }
        _state.value = _state.value.copy(configLoading = true, configError = null)
        viewModelScope.launch {
            authConfigApi.fetch(instanceUrl).fold(
                onSuccess = { config ->
                    _state.value = _state.value.copy(configLoading = false, config = config)
                },
                onFailure = { err ->
                    _state.value = _state.value.copy(
                        configLoading = false,
                        configError = err.message ?: "Failed to load auth config",
                    )
                },
            )
        }
    }

    fun signIn(email: String, password: String) {
        if (_state.value.loading) return
        _state.value = _state.value.copy(loading = true, error = null)
        viewModelScope.launch {
            when (val result = api.signInWithPassword(email = email, password = password)) {
                is SignInResult.Success -> {
                    _state.value = _state.value.copy(loading = false, successEmail = result.email)
                }
                is SignInResult.Failure -> {
                    _state.value = _state.value.copy(loading = false, error = result.message)
                }
            }
        }
    }

    /**
     * Returns the URL the Custom Tab should open to start the OIDC flow.
     * Better Auth handles the rest and redirects back to /api/mobile-oauth-return,
     * which deep-links into this app via exp://oauth-return.
     */
    fun oidcStartUrl(providerId: String): String? {
        val baseUrl = auth.instanceUrl.value ?: return null
        val callback = encode("$baseUrl/api/mobile-oauth-return")
        return "$baseUrl/api/auth/sign-in/oauth2?providerId=${encode(providerId)}&callbackURL=$callback"
    }

    fun googleStartUrl(): String? {
        val baseUrl = auth.instanceUrl.value ?: return null
        val callback = encode("$baseUrl/api/mobile-oauth-return")
        return "$baseUrl/api/auth/sign-in/social?provider=google&callbackURL=$callback"
    }

    private fun encode(s: String): String =
        java.net.URLEncoder.encode(s, "UTF-8")
}
