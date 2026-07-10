package com.exponential.app.data.api

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
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
import kotlinx.coroutines.delay
import kotlinx.serialization.SerialName
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

@Serializable
data class OauthExchangeRequest(
    val code: String,
    @SerialName("code_verifier") val codeVerifier: String,
)

@Serializable
data class OauthExchangeResponse(val token: String? = null)

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
                // Better Auth's CSRF check 403s POSTs without an Origin header
                // (MISSING_OR_NULL_ORIGIN); send the instance's own origin like
                // a same-origin browser request would.
                header("Origin", baseUrl.trimEnd('/'))
                setBody(SignInRequest(email = email, password = password))
            }
            if (!response.status.isSuccess()) {
                val body = response.bodyAsText()
                return SignInResult.Failure("HTTP ${response.status.value}: $body")
            }

            // Better Auth's email sign-in returns either { token, user } when bearer
            // plugin is enabled, or { user } with a Set-Cookie header. Try both.
            // Decode from text: proxies/dev servers may drop Content-Type, which
            // makes ktor's typed body() throw NoTransformationFoundException.
            val parsed: SignInResponse = json.decodeFromString(response.bodyAsText())
            if (parsed.token != null && parsed.user != null) {
                return if (completeLogin(baseUrl, parsed.token, parsed.user.id, parsed.user.email, parsed.user.isAdmin)) {
                    SignInResult.Success(parsed.token, parsed.user.email)
                } else {
                    SignInResult.Failure("Couldn't verify your account. Please try again.")
                }
            }

            // Fallback: read raw set-cookie value `better-auth.session_token=<token>`.
            val cookies = response.headers.getAll("set-cookie").orEmpty()
            val token = cookies
                .firstOrNull { it.contains("session_token=", ignoreCase = true) }
                ?.let { Regex("""session_token=([^;]+)""").find(it)?.groupValues?.get(1) }

            if (token != null) {
                val resolvedEmail = parsed.user?.email ?: email
                if (completeLogin(baseUrl, token, parsed.user?.id, resolvedEmail, parsed.user?.isAdmin == true)) {
                    SignInResult.Success(token, resolvedEmail)
                } else {
                    SignInResult.Failure("Couldn't verify your account. Please try again.")
                }
            } else {
                SignInResult.Failure("Sign-in succeeded but no session token returned")
            }
        } catch (e: Exception) {
            SignInResult.Failure(e.message ?: "Network error")
        }
    }

    // Redeem an oauth-return PKCE code for the session token (REV-13):
    // POST /api/mobile-oauth-exchange with the code from the deep link and the
    // in-memory verifier the attempt started with. Null on any failure
    // (unknown/expired/replayed code, wrong verifier, network) — the caller
    // surfaces a login error.
    suspend fun exchangeOauthCode(baseUrl: String, code: String, codeVerifier: String): String? {
        return try {
            val response = client.post("$baseUrl/api/mobile-oauth-exchange") {
                contentType(ContentType.Application.Json)
                setBody(OauthExchangeRequest(code = code, codeVerifier = codeVerifier))
            }
            if (!response.status.isSuccess()) return null
            val parsed: OauthExchangeResponse = json.decodeFromString(response.bodyAsText())
            parsed.token
        } catch (e: Exception) {
            null
        }
    }

    // Resolve the session (userId + onboarding + isAdmin) and persist the token
    // as a per-user account. A userId is MANDATORY — it keys the per-user
    // account, so without one (session read fails ×3 AND no id in the sign-in
    // body) we refuse to persist the token and return false, and the caller
    // surfaces a login error. Also captures the onboarding flag in the same step
    // so the nav gate never momentarily sees a returning user as "not onboarded".
    suspend fun completeLogin(
        baseUrl: String,
        token: String,
        userIdHint: String?,
        emailHint: String?,
        isAdminHint: Boolean,
    ): Boolean {
        var info: SessionInfo? = null
        var attempt = 0
        while (info == null && attempt < 3) {
            if (attempt > 0) delay(500)
            info = fetchSession(baseUrl, token)
            attempt++
        }
        val userId = info?.userId ?: userIdHint ?: return false
        // Preserve a returning user's onboarding flag if the session read failed
        // but a hint gave us the id, so we don't bounce them back into the wizard.
        val prior = auth.accounts.value.firstOrNull { it.id == ServerAccount.makeId(baseUrl, userId) }
        auth.setToken(
            token = token,
            email = info?.email ?: emailHint,
            userId = userId,
            isAdmin = info?.isAdmin ?: isAdminHint,
            onboardingCompletedAt = info?.onboardingCompletedAt ?: prior?.onboardingCompletedAt,
            // Mark the flag authoritative only when the session read succeeded
            // (or it already was); a failed fetch must not start the wizard.
            onboardingKnown = info != null || prior?.onboardingKnown == true,
        )
        return true
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
