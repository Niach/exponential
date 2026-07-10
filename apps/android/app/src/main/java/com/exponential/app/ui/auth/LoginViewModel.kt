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
        // Mirror a login failure reported from outside this screen (the OAuth
        // deep-link return, handled in MainActivity) into the form's error, then
        // consume it so it shows once.
        viewModelScope.launch {
            auth.loginError.collect { message ->
                if (message != null) {
                    _state.value = _state.value.copy(loading = false, error = message)
                    auth.consumeLoginError()
                }
            }
        }
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
        val instanceUrl = auth.instanceUrl.value ?: run {
            _state.value = _state.value.copy(error = "No instance URL set")
            return
        }
        _state.value = _state.value.copy(loading = true, error = null)
        viewModelScope.launch {
            when (val result = api.signInWithPassword(instanceUrl = instanceUrl, email = email, password = password)) {
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
     * URL the Custom Tab opens to start the OIDC flow. Better Auth's
     * /sign-in/oauth2 is POST-only and Custom Tabs only emit GETs, so we
     * route through /api/mobile-oauth-start which POSTs server-side and
     * 302s to the IdP. Each start mints a PKCE attempt (REV-13): the S256
     * code_challenge rides the start URL (base64url — URL-safe as-is), the
     * verifier stays in AuthRepository memory. The flow ends at
     * /api/mobile-oauth-return, which deep-links back via
     * exponential://oauth-return?code=…#code=… — a single-use code
     * MainActivity redeems through /api/mobile-oauth-exchange with the
     * verifier (pre-PKCE servers still deep-link the legacy #token=… form).
     */
    fun oidcStartUrl(providerId: String): String? {
        val baseUrl = auth.instanceUrl.value ?: return null
        return "$baseUrl/api/mobile-oauth-start?providerId=${encode(providerId)}&code_challenge=${auth.beginOauthAttempt()}"
    }

    fun googleStartUrl(): String? {
        val baseUrl = auth.instanceUrl.value ?: return null
        return "$baseUrl/api/mobile-oauth-start?provider=google&code_challenge=${auth.beginOauthAttempt()}"
    }

    fun appleStartUrl(): String? {
        val baseUrl = auth.instanceUrl.value ?: return null
        return "$baseUrl/api/mobile-oauth-start?provider=apple&code_challenge=${auth.beginOauthAttempt()}"
    }

    private fun encode(s: String): String =
        java.net.URLEncoder.encode(s, "UTF-8")
}
