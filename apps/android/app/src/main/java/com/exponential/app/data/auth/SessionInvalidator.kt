package com.exponential.app.data.auth

import javax.inject.Inject
import javax.inject.Singleton

/** Is a response DEFINITIVE proof that the presented session is gone? */
enum class SessionSignal {
    /** The server rejected a bearer we hold: that account must re-login. */
    Invalidated,

    /** Tells us nothing about the session (offline, 5xx, a 403, no bearer). */
    Inconclusive,
}

/**
 * Classification of the dead-session signal (desktop `status_error_authed`
 * parity). PURE + unit-tested on purpose: everything else in the app treats
 * "the session died" as routed state, so the one decision that clears a token
 * has to be provably narrow.
 *
 * Only two answers are definitive:
 *  - HTTP 401 on a request that DID carry a bearer token.
 *  - `get-session` answering 2xx with no user while a bearer was presented —
 *    Better Auth reports a dead bearer as `200` + a null session, not a 401.
 *
 * Everything else is conservative. A 403 is an authorization verdict on a live
 * session (wrong team, non-owner) and must never sign anyone out; timeouts,
 * DNS failures and 5xx are indistinguishable from a server that is simply
 * unreachable; a 401 with NO bearer is bad credentials at sign-in.
 */
object SessionInvalidation {

    /** A non-2xx status from an authed call site. */
    fun classifyStatus(statusCode: Int, tokenPresented: Boolean): SessionSignal =
        if (tokenPresented && statusCode == HTTP_UNAUTHORIZED) {
            SessionSignal.Invalidated
        } else {
            SessionSignal.Inconclusive
        }

    /** A `/api/auth/get-session` read: [hasUser] is false when no user came back. */
    fun classifySessionRead(
        statusCode: Int,
        tokenPresented: Boolean,
        hasUser: Boolean,
    ): SessionSignal = when {
        !tokenPresented -> SessionSignal.Inconclusive
        statusCode == HTTP_UNAUTHORIZED -> SessionSignal.Invalidated
        statusCode in 200..299 && !hasUser -> SessionSignal.Invalidated
        else -> SessionSignal.Inconclusive
    }

    const val HTTP_UNAUTHORIZED = 401
}

/**
 * The dead-token surface: a rejected session becomes routed state instead of a
 * screen that 401s forever (desktop `AuthStore::handle_unauthorized` /
 * `SessionPhase::AuthExpired` parity, mirrored on the 426 `UpdateGate` rails).
 *
 * Reporting sites: `TrpcClient` (every authed tRPC response), the Electric
 * shape loops (401 only — `ShapeAuthException` also covers 403, which must NOT
 * sign anyone out) and `AuthApi.fetchSession` (the 2xx-with-no-user
 * dead-session answer).
 *
 * Invalidation is LOCAL-ONLY and clears exactly one account's token:
 *  - No `sign-out` revocation call: the session the token names is already
 *    gone, so the request could only fail (and its own 401 would report right
 *    back here).
 *  - No push-token unregister: that is an authed tRPC call, which would 401 too.
 *  - The account row and its Room cache STAY, exactly like a normal sign-out —
 *    the login screen keeps the server it lands on, and cached data resumes on
 *    re-login.
 *
 * Dropping the token is also what stops that account's 16 shape loops:
 * `SyncManager.start()` reconciles pipelines off the signed-in-account set, so
 * the pipeline is cancelled by the same emission that routes the UI to login
 * (`AppNavHost`'s `needsAuth` gate reads `token == null`).
 */
@Singleton
class SessionInvalidator @Inject constructor(
    private val auth: AuthRepository,
) {

    /** Report a non-2xx response from an authed call for [accountId]. */
    fun reportStatus(accountId: String, statusCode: Int, tokenPresented: Boolean) {
        if (SessionInvalidation.classifyStatus(statusCode, tokenPresented) == SessionSignal.Invalidated) {
            invalidate(accountId)
        }
    }

    /** Report a `get-session` read for [accountId]. */
    fun reportSessionRead(
        accountId: String,
        statusCode: Int,
        tokenPresented: Boolean,
        hasUser: Boolean,
    ) {
        if (SessionInvalidation.classifySessionRead(statusCode, tokenPresented, hasUser) ==
            SessionSignal.Invalidated
        ) {
            invalidate(accountId)
        }
    }

    /**
     * Clear [accountId]'s token locally. Idempotent: 16 shape loops can 401
     * within the same instant, and only the first caller finds a token.
     */
    fun invalidate(accountId: String) {
        val account = auth.accounts.value.firstOrNull { it.id == accountId } ?: return
        if (account.token == null) return
        android.util.Log.w(
            "SessionInvalidator",
            "session rejected on account $accountId; clearing the local token",
        )
        auth.clearToken(accountId)
    }
}
