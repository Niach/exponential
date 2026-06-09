package com.exponential.app.data.api

import com.exponential.app.data.auth.AuthRepository
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.header
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
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

@Serializable
data class SignInRequest(val email: String, val password: String)

@Serializable
data class SignInResponse(val token: String? = null, val user: AuthUser? = null)

@Serializable
data class AuthUser(
    val id: String,
    val email: String,
    val name: String? = null,
    val isAdmin: Boolean = false,
)

data class SessionInfo(
    val email: String,
    val userId: String,
    val isAdmin: Boolean,
    val onboardingCompletedAt: String? = null,
)

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
    suspend fun signInWithPassword(instanceUrl: String, email: String, password: String): SignInResult {
        val baseUrl = instanceUrl

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
                applyLogin(baseUrl, parsed.token, parsed.user.email, parsed.user.id, parsed.user.isAdmin)
                return SignInResult.Success(parsed.token, parsed.user.email)
            }

            // Fallback: read raw set-cookie value `better-auth.session_token=<token>`.
            val cookies = response.headers.getAll("set-cookie").orEmpty()
            val token = cookies
                .firstOrNull { it.contains("session_token=", ignoreCase = true) }
                ?.let { Regex("""session_token=([^;]+)""").find(it)?.groupValues?.get(1) }

            if (token != null) {
                val resolvedEmail = parsed.user?.email ?: email
                applyLogin(baseUrl, token, resolvedEmail, parsed.user?.id, parsed.user?.isAdmin == true)
                SignInResult.Success(token, resolvedEmail)
            } else {
                SignInResult.Failure("Sign-in succeeded but no session token returned")
            }
        } catch (e: Exception) {
            SignInResult.Failure(e.message ?: "Network error")
        }
    }

    // Set the active token together with the onboarding flag, captured from the
    // session in the SAME step — so the onboarding nav gate never momentarily
    // sees a stale "not onboarded" for a returning user. Falls back to the
    // sign-in fields if the session fetch fails (a brand-new user is null anyway).
    private suspend fun applyLogin(
        baseUrl: String,
        token: String,
        email: String,
        userId: String?,
        isAdmin: Boolean,
    ) {
        // Preserve a previously-captured onboarding flag so a transient session
        // fetch failure on re-login doesn't downgrade a returning user back to the
        // onboarding wizard.
        val prior = auth.accounts.value.firstOrNull { it.instanceUrl == baseUrl }?.onboardingCompletedAt
        val info = fetchSession(baseUrl, token)
        auth.setToken(
            token = token,
            email = info?.email ?: email,
            userId = info?.userId ?: userId,
            isAdmin = info?.isAdmin ?: isAdmin,
            onboardingCompletedAt = info?.onboardingCompletedAt ?: prior,
        )
    }

    suspend fun fetchSession(accountId: String): SessionInfo? {
        val account = auth.accounts.value.firstOrNull { it.id == accountId } ?: return null
        return fetchSession(account.instanceUrl, account.token)
    }

    // Core session read. Takes baseUrl + token explicitly so a login flow can
    // capture session fields (incl. onboardingCompletedAt) BEFORE persisting the
    // token, avoiding any window where the account looks "not onboarded".
    suspend fun fetchSession(baseUrl: String, token: String?): SessionInfo? {
        return try {
            val response = client.get("$baseUrl/api/auth/get-session") {
                if (token != null) header("Authorization", "Bearer $token")
            }
            if (!response.status.isSuccess()) return null
            val body = response.bodyAsText()
            val parsed = json.parseToJsonElement(body) as? JsonObject ?: return null
            val user = parsed["user"] as? JsonObject ?: return null
            val email = user["email"]?.jsonPrimitive?.content ?: return null
            val id = user["id"]?.jsonPrimitive?.content ?: return null
            val isAdmin = (user["isAdmin"]?.jsonPrimitive?.booleanOrNull) ?: false
            // better-auth additionalField (type date, input:false) — returned on
            // session reads as an ISO string or null, exactly like the web gate.
            val onboarding = user["onboardingCompletedAt"]?.jsonPrimitive?.contentOrNull
            SessionInfo(email = email, userId = id, isAdmin = isAdmin, onboardingCompletedAt = onboarding)
        } catch (e: Exception) {
            null
        }
    }
}
