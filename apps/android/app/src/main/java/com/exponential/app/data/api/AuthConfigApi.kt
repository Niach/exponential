package com.exponential.app.data.api

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.statement.bodyAsText
import io.ktor.http.isSuccess
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class OidcProvider(val id: String, val name: String)

@Serializable
data class AuthConfig(
    val passwordEnabled: Boolean = true,
    val oidcProviders: List<OidcProvider> = emptyList(),
    val googleLoginEnabled: Boolean = false,
)

@Singleton
class AuthConfigApi @Inject constructor(
    private val client: HttpClient,
) {
    suspend fun fetch(instanceUrl: String): Result<AuthConfig> {
        val url = "$instanceUrl/api/auth-config"
        return try {
            val response = client.get(url)
            if (!response.status.isSuccess()) {
                val body = runCatching { response.bodyAsText() }.getOrNull().orEmpty().take(200)
                return Result.failure(IllegalStateException("HTTP ${response.status.value} from $url${if (body.isNotEmpty()) ": $body" else ""}"))
            }
            try {
                Result.success(response.body<AuthConfig>())
            } catch (e: Exception) {
                val raw = runCatching { response.bodyAsText() }.getOrNull().orEmpty().take(200)
                Result.failure(IllegalStateException("Decode failed (${e::class.simpleName}: ${e.message ?: "no message"}); body=$raw"))
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
