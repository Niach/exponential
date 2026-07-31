package com.exponential.app.data.api

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.auth.SessionInvalidator
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
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
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
    val name: String? = null,
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
    private val sessionInvalidator: SessionInvalidator,
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
                // Rendered verbatim on the login screen — show Better Auth's
                // `message` field, never the raw response body (EXP-219).
                val message = authErrorMessage(response.bodyAsText())
                return SignInResult.Failure(message ?: "Sign-in failed (HTTP ${response.status.value})")
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

    /**
     * `POST /api/auth/sign-out` — best-effort server-side session revocation
     * (desktop login.rs parity). Without it a leaked bearer token would survive
     * local sign-out for the full 60-day sliding session expiry. Local sign-out
     * must proceed even when this fails (offline sign-out is legal), so the
     * call is timeout-capped and never throws; callers await it AFTER the
     * push-token unregister (which still needs a live session) and BEFORE the
     * token is dropped locally.
     *
     * Deliberately NOT on the SessionInvalidator path: that token's session is
     * already gone, so revoking it could only fail, and its 401 answer never
     * reaches the invalidator (this call reports nothing) — a dead session can
     * never loop through here.
     */
    suspend fun signOut(baseUrl: String, token: String) {
        withTimeoutOrNull(SIGN_OUT_TIMEOUT_MS) {
            try {
                client.post("$baseUrl/api/auth/sign-out") {
                    contentType(ContentType.Application.Json)
                    // Same-origin Origin header as signInWithPassword — Better
                    // Auth's CSRF check 403s POSTs without one.
                    header("Origin", baseUrl.trimEnd('/'))
                    header("Authorization", "Bearer $token")
                    setBody("{}")
                }
            } catch (e: Exception) {
                // Best-effort: offline/unreachable servers must not block sign-out.
            }
        }
    }

    /**
     * Extract the user-presentable `message` from a Better Auth error body
     * (`{"code": "...", "message": "Invalid email or password"}`).
     */
    private fun authErrorMessage(body: String): String? = runCatching {
        (json.parseToJsonElement(body) as? JsonObject)
            ?.get("message")?.jsonPrimitive?.contentOrNull
    }.getOrNull()?.takeIf { it.isNotBlank() }

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
            name = info?.name,
            userId = userId,
            isAdmin = info?.isAdmin ?: isAdminHint,
            onboardingCompletedAt = info?.onboardingCompletedAt ?: prior?.onboardingCompletedAt,
            // Mark the flag authoritative only when the session read succeeded
            // (or it already was); a failed fetch must not start the wizard.
            onboardingKnown = info != null || prior?.onboardingKnown == true,
        )
        return true
    }

    // Session read for a STORED account, so the dead-session answer can be
    // acted on: Better Auth reports a rejected bearer as 2xx with a null
    // session (not a 401), which the old "any failure collapses to null" read
    // made indistinguishable from being offline — the account then 401'd
    // forever instead of being routed back to login.
    suspend fun fetchSession(accountId: String): SessionInfo? {
        val account = auth.accounts.value.firstOrNull { it.id == accountId } ?: return null
        val read = readSession(account.instanceUrl, account.token)
        if (read.statusCode != null) {
            sessionInvalidator.reportSessionRead(
                accountId = accountId,
                statusCode = read.statusCode,
                tokenPresented = account.token != null,
                hasUser = read.hasUser,
            )
        }
        return read.info
    }

    // Core session read. Takes baseUrl + token explicitly so a login flow can
    // capture session fields (incl. onboardingCompletedAt) BEFORE persisting the
    // token, avoiding any window where the account looks "not onboarded".
    suspend fun fetchSession(baseUrl: String, token: String?): SessionInfo? =
        readSession(baseUrl, token).info

    /**
     * One `get-session` read. [statusCode] is null when the request never got an
     * answer (transport failure, timeout) — the case that must NEVER be read as
     * a dead session. A 2xx that carries no usable user leaves [info] null with
     * the status intact, which is the definitive dead-bearer signal.
     */
    private suspend fun readSession(baseUrl: String, token: String?): SessionRead {
        val response = try {
            client.get("$baseUrl/api/auth/get-session") {
                if (token != null) header("Authorization", "Bearer $token")
            }
        } catch (e: Exception) {
            return SessionRead(statusCode = null, info = null, hasUser = false)
        }
        val status = response.status.value
        if (!response.status.isSuccess()) {
            return SessionRead(statusCode = status, info = null, hasUser = false)
        }
        return try {
            val body = response.bodyAsText()
            val root = json.parseToJsonElement(body)
            // Better Auth answers a dead bearer with JSON `null` or an object
            // with no user; any other root (array, string, number) is a broken
            // response, not proof — drop the status so it can't invalidate a
            // live session (iOS parity).
            if (root !is JsonNull && root !is JsonObject) {
                return SessionRead(statusCode = null, info = null, hasUser = false)
            }
            val parsed = root as? JsonObject
            val user = parsed?.get("user") as? JsonObject
            val email = user?.get("email")?.jsonPrimitive?.contentOrNull
            val id = user?.get("id")?.jsonPrimitive?.contentOrNull
            // A user that IS there but decodes short is a broken payload, not a
            // dead session — hasUser keeps it out of the invalidating case.
            if (email == null || id == null) {
                return SessionRead(statusCode = status, info = null, hasUser = user != null)
            }
            val name = user["name"]?.jsonPrimitive?.contentOrNull
            val isAdmin = (user["isAdmin"]?.jsonPrimitive?.booleanOrNull) ?: false
            // better-auth additionalField (type date, input:false) — returned on
            // session reads as an ISO string or null, exactly like the web gate.
            val onboarding = user["onboardingCompletedAt"]?.jsonPrimitive?.contentOrNull
            SessionRead(
                statusCode = status,
                info = SessionInfo(
                    email = email,
                    userId = id,
                    name = name,
                    isAdmin = isAdmin,
                    onboardingCompletedAt = onboarding,
                ),
                hasUser = true,
            )
        } catch (e: Exception) {
            // An unparseable body is a broken response, not proof of anything:
            // drop the status so it can't invalidate a live session.
            SessionRead(statusCode = null, info = null, hasUser = false)
        }
    }

    private class SessionRead(
        val statusCode: Int?,
        val info: SessionInfo?,
        val hasUser: Boolean,
    )

    companion object {
        // Mirrors PushTokenManager.UNREGISTER_TIMEOUT_MS — both are awaited
        // best-effort network calls on the sign-out path.
        private const val SIGN_OUT_TIMEOUT_MS = 3_000L
    }
}
