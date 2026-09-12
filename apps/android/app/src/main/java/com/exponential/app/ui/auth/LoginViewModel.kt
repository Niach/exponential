package com.exponential.app.ui.auth

import android.app.Activity
import androidx.credentials.CredentialManager
import androidx.credentials.GetCredentialRequest
import androidx.credentials.GetPublicKeyCredentialOption
import androidx.credentials.PublicKeyCredential
import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.AuthConfig
import com.exponential.app.data.api.AuthConfigApi
import com.exponential.app.data.api.SignInResult
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import java.net.URI
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * EXP-857 security: whether the passkey assertion options served by
 * [instanceUrl] are for THAT instance and may be signed on this device.
 *
 * The options are whatever the configured instance answers with, and the
 * ceremony runs against the `rpId` INSIDE them: Android checks that id against
 * this app's asset links, never against the server that sent it. A hostile
 * instance URL could otherwise serve a challenge it relayed from
 * app.exponential.at, have the user's real passkey sign it, and replay the
 * assertion there. So the id must equal the host of the instance we asked
 * (case-insensitively, trailing root dot ignored) — Better Auth derives its
 * rpID from the instance's own base URL, so a legitimate answer always
 * matches. Absent, blank, non-string or unparseable is a mismatch, and so is
 * a registrable-suffix rpId (`exponential.at` for `app.exponential.at`):
 * WebAuthn would allow it, our server never sends one.
 */
internal fun passkeyRpIdMatchesInstance(instanceUrl: String, requestJson: String): Boolean {
    val host = runCatching { URI(instanceUrl).host }.getOrNull().normalizedHost()
        ?: return false
    val rpId = runCatching {
        Json.parseToJsonElement(requestJson).jsonObject["rpId"]?.jsonPrimitive?.contentOrNull
    }.getOrNull().normalizedHost() ?: return false
    return rpId == host
}

private fun String?.normalizedHost(): String? =
    this?.trim()?.trimEnd('.')?.lowercase()?.takeIf { it.isNotEmpty() }

/**
 * Where the "Continue with email" step stands (EXP-857).
 *
 * [Hidden] is the provider-buttons-only screen; [Email] reveals the address
 * form (a code request, or the password form when the instance has no mail);
 * [CodeSent] is the 6-digit code form.
 */
enum class LoginEmailStep {
    Hidden,
    Email,
    CodeSent,
}

/** What the in-flight submit is doing — drives the button's loading label. */
enum class LoginBusy {
    None,

    /** "Sending code…" */
    SendingCode,

    /** "Checking…" — code verify, password sign-in, or a passkey ceremony. */
    Checking,
}

data class LoginState(
    val loading: Boolean = false,
    val busy: LoginBusy = LoginBusy.None,
    val error: String? = null,
    val successEmail: String? = null,
    val configLoading: Boolean = true,
    val config: AuthConfig? = null,
    val configError: String? = null,
    val emailStep: LoginEmailStep = LoginEmailStep.Hidden,
    /** The address the current code was mailed to — the "We sent a …" line. */
    val codeEmail: String? = null,
    /** The user chose the password form over the code flow on an instance offering both. */
    val usePassword: Boolean = false,
    /**
     * A one-shot URL the screen must open in a Custom Tab: the browser handoff
     * the passkey path falls back to when the on-device ceremony is unavailable.
     * Consumed by [consumeFallbackUrl] once launched.
     */
    val fallbackUrl: String? = null,
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
                    _state.value = _state.value.copy(loading = false, busy = LoginBusy.None, error = message)
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
                        configError = trpcErrorMessage(err, "Failed to load auth config"),
                    )
                },
            )
        }
    }

    // ——— The email step (EXP-857) ————————————————————————————————————————

    /** "Continue with email" — reveal the address form in place. */
    fun continueWithEmail() {
        _state.value = _state.value.copy(emailStep = LoginEmailStep.Email, error = null)
    }

    /** "Use a password instead" — the password form on an instance offering both. */
    fun usePasswordInstead() {
        _state.value = _state.value.copy(
            usePassword = true,
            emailStep = LoginEmailStep.Email,
            codeEmail = null,
            error = null,
        )
    }

    /** "Use a different email" — back to the address form, code forgotten. */
    fun changeEmail() {
        _state.value = _state.value.copy(
            emailStep = LoginEmailStep.Email,
            codeEmail = null,
            error = null,
        )
    }

    /** "Resend code" — same address, a fresh code. */
    fun resendCode() {
        _state.value.codeEmail?.let { sendCode(it) }
    }

    fun sendCode(email: String) {
        if (_state.value.loading) return
        val instanceUrl = auth.instanceUrl.value ?: run {
            _state.value = _state.value.copy(error = "No instance URL set")
            return
        }
        _state.value = _state.value.copy(loading = true, busy = LoginBusy.SendingCode, error = null)
        viewModelScope.launch {
            api.sendSignInCode(instanceUrl = instanceUrl, email = email).fold(
                onSuccess = {
                    _state.value = _state.value.copy(
                        loading = false,
                        busy = LoginBusy.None,
                        emailStep = LoginEmailStep.CodeSent,
                        codeEmail = email,
                    )
                },
                onFailure = { err ->
                    _state.value = _state.value.copy(
                        loading = false,
                        busy = LoginBusy.None,
                        error = trpcErrorMessage(err, "Couldn't send the code"),
                    )
                },
            )
        }
    }

    fun verifyCode(code: String) {
        if (_state.value.loading) return
        val instanceUrl = auth.instanceUrl.value ?: run {
            _state.value = _state.value.copy(error = "No instance URL set")
            return
        }
        val email = _state.value.codeEmail ?: return
        _state.value = _state.value.copy(loading = true, busy = LoginBusy.Checking, error = null)
        viewModelScope.launch {
            finish(api.signInWithEmailCode(instanceUrl = instanceUrl, email = email, code = code.trim()))
        }
    }

    fun signIn(email: String, password: String) {
        if (_state.value.loading) return
        val instanceUrl = auth.instanceUrl.value ?: run {
            _state.value = _state.value.copy(error = "No instance URL set")
            return
        }
        _state.value = _state.value.copy(loading = true, busy = LoginBusy.Checking, error = null)
        viewModelScope.launch {
            finish(api.signInWithPassword(instanceUrl = instanceUrl, email = email, password = password))
        }
    }

    // ——— Passkey (EXP-857) ———————————————————————————————————————————————

    /**
     * "Login with passkey": run the WebAuthn assertion against the platform
     * authenticator with the options the server serves, then post the assertion
     * back. An explicit user cancellation is silent (they closed the sheet);
     * anything else — no passkey on this device, no provider, an instance whose
     * assetlinks don't cover this build — falls back to the browser handoff,
     * which the existing oauth-return deep link completes.
     */
    fun startPasskeyLogin(activity: Activity) {
        if (_state.value.loading) return
        val instanceUrl = auth.instanceUrl.value ?: run {
            _state.value = _state.value.copy(error = "No instance URL set")
            return
        }
        _state.value = _state.value.copy(loading = true, busy = LoginBusy.Checking, error = null)
        viewModelScope.launch {
            val options = api.passkeyAuthenticationOptions(instanceUrl).getOrNull()
                ?: return@launch fallBackToBrowser()
            // EXP-857 security: never sign someone else's relying party (see
            // [passkeyRpIdMatchesInstance]). Handled like any other ceremony
            // we cannot run — the browser handoff, which enforces the origin
            // itself.
            if (!passkeyRpIdMatchesInstance(instanceUrl, options.requestJson)) {
                return@launch fallBackToBrowser()
            }
            val responseJson = try {
                val credential = CredentialManager.create(activity).getCredential(
                    activity,
                    GetCredentialRequest(listOf(GetPublicKeyCredentialOption(options.requestJson))),
                ).credential
                (credential as? PublicKeyCredential)?.authenticationResponseJson
            } catch (e: GetCredentialCancellationException) {
                // The user dismissed the system sheet — not an error to report.
                _state.value = _state.value.copy(loading = false, busy = LoginBusy.None)
                return@launch
            } catch (e: Exception) {
                // NoCredentialException and every other ceremony failure.
                return@launch fallBackToBrowser()
            }
            if (responseJson == null) return@launch fallBackToBrowser()
            finish(
                api.verifyPasskeyAuthentication(
                    instanceUrl = instanceUrl,
                    cookies = options.cookies,
                    responseJson = responseJson,
                ),
            )
        }
    }

    private fun fallBackToBrowser() {
        val url = browserLoginStartUrl()
        _state.value = _state.value.copy(
            loading = false,
            busy = LoginBusy.None,
            fallbackUrl = url,
            error = if (url == null) "Couldn't start the passkey login. Please try again." else null,
        )
    }

    fun consumeFallbackUrl() {
        _state.value = _state.value.copy(fallbackUrl = null)
    }

    private fun finish(result: SignInResult) {
        _state.value = when (result) {
            is SignInResult.Success ->
                _state.value.copy(loading = false, busy = LoginBusy.None, successEmail = result.email)
            is SignInResult.Failure ->
                _state.value.copy(loading = false, busy = LoginBusy.None, error = result.message)
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
     * verifier.
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

    /**
     * EXP-857 browser handoff: the same PKCE hop as the provider buttons, but
     * with the web login page itself as the "provider" — the user finishes with
     * any method there and the page ends on the same oauth-return deep link.
     */
    fun browserLoginStartUrl(): String? {
        val baseUrl = auth.instanceUrl.value ?: return null
        return "$baseUrl/api/mobile-oauth-start?provider=browser&code_challenge=${auth.beginOauthAttempt()}"
    }

    private fun encode(s: String): String =
        java.net.URLEncoder.encode(s, "UTF-8")
}
