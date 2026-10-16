package com.exponential.app.data.api

import io.ktor.client.HttpClient
import io.ktor.client.request.get
import io.ktor.client.statement.bodyAsText
import io.ktor.http.isSuccess
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
data class OidcProvider(val id: String, val name: String)

@Serializable
data class AuthConfig(
    val passwordEnabled: Boolean,
    // Password sign-up is open on this instance — gates the "Create account"
    // hand-off (server: buildAuthConfig).
    val signupEnabled: Boolean,
    // The instance can send mail — gates the "Forgot password?" hand-off.
    val passwordResetEnabled: Boolean,
    val oidcProviders: List<OidcProvider>,
    val googleLoginEnabled: Boolean,
    val appleLoginEnabled: Boolean,
    // EXP-857: one-time code login is offered (the instance can send mail),
    // and passkey login is offered.
    val emailOtpEnabled: Boolean,
    val passkeyEnabled: Boolean,
)

/**
 * Browser hand-off targets for the two password flows the app deliberately
 * does not host natively (desktop `open_register` parity): registration and
 * password reset are web flows. The instance URL's trailing slash is trimmed
 * so the joined path never doubles up. EXP-759: `ref=android-app` rides the
 * register URL so the web signup claims it as the signup source
 * (users.signup_ref) — the account is created in the browser, so this is the
 * only trace that the signup STARTED here.
 */
object AuthWebUrls {
    fun register(instanceUrl: String): String =
        "${instanceUrl.trimEnd('/')}/auth/login?ref=android-app"

    fun forgotPassword(instanceUrl: String): String =
        "${instanceUrl.trimEnd('/')}/auth/forgot-password"
}

@Singleton
class AuthConfigApi @Inject constructor(
    private val client: HttpClient,
    private val json: Json,
) {
    suspend fun fetch(instanceUrl: String): Result<AuthConfig> {
        val url = "$instanceUrl/api/auth-config"
        return try {
            val response = client.get(url)
            if (!response.status.isSuccess()) {
                // Rendered on the login screen — keep the status + url for
                // diagnosing a wrong instance URL, never the raw body (EXP-219).
                return Result.failure(IllegalStateException("HTTP ${response.status.value} from $url"))
            }
            // Decode from text (like TrpcClient) instead of the typed
            // response.body(): some servers/proxies drop the Content-Type
            // header, which makes ktor's ContentNegotiation refuse to decode
            // (NoTransformationFoundException) — iOS/desktop ignore the
            // header, so match that tolerance.
            val text = response.bodyAsText()
            try {
                Result.success(json.decodeFromString<AuthConfig>(text))
            } catch (e: Exception) {
                // Exception class only — the raw body must not reach the login
                // screen (EXP-219), and kotlinx decoding exceptions embed an
                // input snippet in their message, so that stays out too.
                Result.failure(IllegalStateException("Decode failed (${e::class.simpleName})"))
            }
        } catch (e: Exception) {
            // Surface the concrete exception class — Ktor often throws with a null/empty message
            // (e.g. SocketTimeoutException, ConnectException, UnknownHostException, SSLHandshakeException),
            // and the bare fallback in LoginViewModel ("Failed to load auth config") hides which it was.
            val parts = buildList {
                add(e::class.qualifiedName ?: e::class.simpleName ?: "Unknown")
                e.message?.takeIf { it.isNotBlank() }?.let { add(it) }
                e.cause?.let { c ->
                    add("cause=${c::class.simpleName ?: "?"}${c.message?.let { ": $it" } ?: ""}")
                }
            }
            Result.failure(IllegalStateException("$url: ${parts.joinToString(" · ")}"))
        }
    }
}
