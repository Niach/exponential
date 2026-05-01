package com.exponential.app.data.api

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
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
    val googleCalendarEnabled: Boolean = false,
)

@Singleton
class AuthConfigApi @Inject constructor(
    private val client: HttpClient,
) {
    suspend fun fetch(instanceUrl: String): Result<AuthConfig> = runCatching {
        val response = client.get("$instanceUrl/api/auth-config")
        require(response.status.isSuccess()) { "HTTP ${response.status.value}" }
        response.body<AuthConfig>()
    }
}
