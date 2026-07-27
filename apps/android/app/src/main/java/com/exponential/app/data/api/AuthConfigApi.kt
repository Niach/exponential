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
    val passwordEnabled: Boolean = true,
    // Password sign-up is open on this instance — gates the "Create account"
    // hand-off (server: buildAuthConfig).
    val signupEnabled: Boolean = false,
    // The instance can send mail — gates the "Forgot password?" hand-off.
    // Both default to false: a server that doesn't publish the flag must hide
    // the affordance, never offer a link the server would dead-end.
    val passwordResetEnabled: Boolean = false,
    val oidcProviders: List<OidcProvider> = emptyList(),
    val googleLoginEnabled: Boolean = false,
    // Absent from pre-SIWA servers — the default keeps decoding tolerant.
    val appleLoginEnabled: Boolean = false,
)

/**
 * Browser hand-off targets for the two password flows the app deliberately
 * does not host natively (desktop `open_register` parity): registration and
 * password reset are web flows. The instance URL's trailing slash is trimmed
 * so the joined path never doubles up.
 */
object AuthWebUrls {
    fun register(instanceUrl: String): String =
        "${instanceUrl.trimEnd('/')}/auth/register"

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
            Result.failure(IllegalStateException("$url — ${parts.joinToString(" — ")}"))
        }
    }
}
