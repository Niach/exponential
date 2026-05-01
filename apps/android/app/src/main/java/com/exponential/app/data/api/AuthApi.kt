package com.exponential.app.data.api

import com.exponential.app.data.auth.AuthRepository
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

@Serializable
data class SignInRequest(val email: String, val password: String)

@Serializable
data class SignInResponse(val token: String? = null, val user: AuthUser? = null)

@Serializable
data class AuthUser(val id: String, val email: String, val name: String? = null)

sealed interface SignInResult {
    data class Success(val token: String, val email: String) : SignInResult
    data class Failure(val message: String) : SignInResult
}

@Singleton
class AuthApi @Inject constructor(
    private val client: HttpClient,
    private val auth: AuthRepository,
    private val json: Json,
) {
    suspend fun signInWithPassword(email: String, password: String): SignInResult {
        val baseUrl = auth.instanceUrl.value
            ?: return SignInResult.Failure("No instance URL set")

        return try {
            val response = client.post("$baseUrl/api/auth/sign-in/email") {
                contentType(ContentType.Application.Json)
                setBody(SignInRequest(email = email, password = password))
            }
            if (!response.status.isSuccess()) {
                val body = response.bodyAsText()
                return SignInResult.Failure("HTTP ${response.status.value}: $body")
            }

            // Better Auth's email sign-in returns either { token, user } when bearer
            // plugin is enabled, or { user } with a Set-Cookie header. Try both.
            val parsed: SignInResponse = response.body()
            if (parsed.token != null && parsed.user != null) {
                auth.setToken(parsed.token, parsed.user.email)
                return SignInResult.Success(parsed.token, parsed.user.email)
            }

            // Fallback: read raw set-cookie value `better-auth.session_token=<token>`.
            val cookies = response.headers.getAll("set-cookie").orEmpty()
            val token = cookies
                .firstOrNull { it.contains("session_token=", ignoreCase = true) }
                ?.let { Regex("""session_token=([^;]+)""").find(it)?.groupValues?.get(1) }

            if (token != null) {
                val email = parsed.user?.email ?: email
                auth.setToken(token, email)
                SignInResult.Success(token, email)
            } else {
                SignInResult.Failure("Sign-in succeeded but no session token returned")
            }
        } catch (e: Exception) {
            SignInResult.Failure(e.message ?: "Network error")
        }
    }

    suspend fun fetchSession(): String? {
        val baseUrl = auth.instanceUrl.value ?: return null
        return try {
            val response = client.get("$baseUrl/api/auth/get-session")
            if (!response.status.isSuccess()) return null
            val body = response.bodyAsText()
            val parsed = json.parseToJsonElement(body) as? JsonObject ?: return null
            (parsed["user"] as? JsonObject)?.get("email")?.jsonPrimitive?.content
        } catch (e: Exception) {
            null
        }
    }
}
