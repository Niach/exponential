package com.exponential.app.data.push

import android.util.Log
import com.exponential.app.data.api.PushTokensApi
import com.exponential.app.data.auth.AuthRepository
import com.google.firebase.messaging.FirebaseMessaging
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlin.math.min

/** Retry cadence of the reconcile loop (REV2-45), mirroring iOS PushTokenManager.swift. */
internal const val PUSH_BASE_RETRY_MS = 2_000L
internal const val PUSH_MAX_RETRY_MS = 300_000L

/** Backoff step: double while passes keep failing, snap back once one succeeds. */
internal fun nextPushRetryMs(currentMs: Long, failed: Boolean): Long =
    if (failed) min(currentMs * 2, PUSH_MAX_RETRY_MS) else PUSH_BASE_RETRY_MS

/**
 * Registration bookkeeping for the reconcile loop. A registration that failed
 * transiently (server blip at sign-in, FCM rotation while the app server is
 * unreachable) used to leave the account silently push-dead until the next
 * cold start; the loop retries against this state instead (REV2-45).
 */
internal class PushRegistrationState {
    private val lock = Any()
    private var token: String? = null

    // Accounts whose registration of `token` the server acknowledged.
    private val registered = mutableSetOf<String>()

    // Accounts deliberately unregistered (sign-out in progress): the loop must
    // not re-register them between the unregister call and the credentials
    // actually being dropped, and a register that was already in flight must
    // compensate for the row it resurrected.
    private val suppressed = mutableSetOf<String>()

    // Accounts with a register in flight right now.
    private val inFlight = mutableSetOf<String>()

    val currentToken: String? get() = synchronized(lock) { token }

    /** Records the device token. A rotation invalidates every acknowledgement. */
    fun setToken(value: String): Boolean = synchronized(lock) {
        if (token == value) return false
        token = value
        registered.clear()
        true
    }

    /** Accounts still owing a registration, with the bookkeeping pruned to [signedIn]. */
    fun pending(signedIn: Set<String>): List<String> = synchronized(lock) {
        registered.retainAll(signedIn)
        // Suppression is dropped once the account is gone, so a later re-login
        // registers again — except while its own register is still in flight,
        // whose post-flight check is the only thing that can undo that row.
        suppressed.retainAll { it in signedIn || it in inFlight }
        signedIn.filter { it !in registered && it !in suppressed && it !in inFlight }.sorted()
    }

    /**
     * Claims [accountId] for a register of [forToken]; false means skip it —
     * the account signed out, or the token rotated, since the pass started.
     */
    fun beginRegister(accountId: String, forToken: String): Boolean = synchronized(lock) {
        if (accountId in suppressed || token != forToken) return false
        inFlight.add(accountId)
        true
    }

    /**
     * Post-flight bookkeeping for a register that succeeded. Returns true when
     * the account signed out while the call was in flight — its own unregister
     * may have lost the race to the row this call just (re)created, and once
     * the credentials are gone nothing else can delete it.
     */
    fun completeRegister(accountId: String, forToken: String): Boolean = synchronized(lock) {
        inFlight.remove(accountId)
        if (accountId in suppressed) return true
        // Only acknowledge when the token did not rotate mid-flight: this call
        // posted the OLD token, and marking the account registered would skip
        // posting the new one — FCM kills the old token and pushes just stop.
        if (token == forToken) registered.add(accountId)
        false
    }

    fun failRegister(accountId: String) {
        synchronized(lock) { inFlight.remove(accountId) }
    }

    /** Sign-out: drop the acknowledgement and block re-registration. */
    fun suppress(accountId: String) {
        synchronized(lock) {
            registered.remove(accountId)
            suppressed.add(accountId)
        }
    }
}

/**
 * One reconcile pass over the signed-in accounts, expressed against function
 * seams so it is unit-testable without Firebase or a live tRPC client.
 */
internal class PushRegistrationReconciler(
    private val state: PushRegistrationState,
    private val signedInAccounts: () -> Set<String>,
    private val deviceToken: suspend () -> String?,
    private val register: suspend (accountId: String, token: String) -> Unit,
    private val unregister: suspend (accountId: String, token: String) -> Unit,
    // One slow/stuck register must not stall the pass and starve the other
    // accounts. Registration is idempotent, so aborting and retrying is safe.
    private val registerTimeoutMs: Long = REGISTER_TIMEOUT_MS,
) {
    /** Returns true when work is still outstanding, i.e. the loop should retry. */
    suspend fun reconcile(): Boolean {
        val signedIn = signedInAccounts()
        // Prunes departed accounts out of the bookkeeping even when there is
        // nothing to post, so a re-login is never mistaken for registered.
        if (state.pending(signedIn).isEmpty()) return false
        // A Firebase failure here dropped the whole pass before REV2-45; now it
        // is just a retry, because we know registrations are outstanding.
        val token = deviceToken() ?: return true
        state.setToken(token)
        var retry = false
        for (accountId in state.pending(signedIn)) {
            if (!state.beginRegister(accountId, token)) continue
            val posted = try {
                withTimeoutOrNull(registerTimeoutMs) { register(accountId, token) } != null
            } catch (err: Throwable) {
                Log.w(TAG, "Failed to register FCM token: ${err.message}")
                false
            }
            if (!posted) {
                state.failRegister(accountId)
                retry = true
                continue
            }
            if (state.completeRegister(accountId, token)) {
                // Signed out mid-flight: compensate, or the departed account
                // keeps receiving this device's pushes until the 30-day sweep.
                try {
                    withTimeoutOrNull(registerTimeoutMs) { unregister(accountId, token) }
                    Log.i(TAG, "Unregistered FCM token registered mid sign-out")
                } catch (err: Throwable) {
                    Log.w(TAG, "Failed to unregister FCM token: ${err.message}")
                }
            } else {
                Log.i(TAG, "Registered FCM token with backend")
            }
        }
        return retry
    }

    companion object {
        private const val TAG = "PushTokenMgr"
        private const val REGISTER_TIMEOUT_MS = 10_000L
    }
}

@Singleton
class PushTokenManager @Inject constructor(
    private val auth: AuthRepository,
    private val api: PushTokensApi,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val state = PushRegistrationState()

    // CONFLATED: a burst of account changes collapses into one pass — the
    // reconcile is over the whole account set anyway.
    private val wake = Channel<Unit>(Channel.CONFLATED)

    private val reconciler = PushRegistrationReconciler(
        state = state,
        signedInAccounts = { signedInAccountIds() },
        deviceToken = { currentFcmToken() },
        register = { accountId, token -> api.register(accountId, token) },
        unregister = { accountId, token -> api.unregister(accountId, token) },
    )

    fun start() {
        scope.launch {
            // Register the token for EVERY signed-in account, not just the
            // active one: the server keys registrations per (token, user), so
            // an account left holding a dead token after a rotation silently
            // stops receiving pushes until it happens to be made active again.
            auth.accounts
                .map { accounts -> accounts.filter { it.token != null }.map { it.id }.toSet() }
                .distinctUntilChanged()
                .collect { wake.trySend(Unit) }
        }
        scope.launch { reconcileLoop() }
    }

    fun onNewToken(token: String) {
        // Called from FcmService.onNewToken on a background thread; just stash
        // and wake the loop. A rotation invalidates the old token for the whole
        // device, so every signed-in account needs the new one.
        state.setToken(token)
        wake.trySend(Unit)
    }

    /**
     * Unregisters this device's FCM token for [accountId] on the server.
     * Must be awaited BEFORE the account's credentials are cleared: the tRPC
     * client resolves the bearer token at request time, so a fire-and-forget
     * call racing clearToken()/removeAccount() sends an unauthenticated
     * request that the server rejects, leaving the signed-out device still
     * receiving pushes. Bounded so sign-out can never hang on Firebase or
     * the network.
     */
    suspend fun unregisterToken(accountId: String) {
        // Suppress first and unconditionally: a register already in flight
        // checks this post-flight and compensates for the row it recreated.
        state.suppress(accountId)
        withTimeoutOrNull(UNREGISTER_TIMEOUT_MS) {
            val token = state.currentToken ?: currentFcmToken() ?: return@withTimeoutOrNull
            try {
                api.unregister(accountId, token)
                Log.i(TAG, "Unregistered FCM token with backend")
            } catch (err: Throwable) {
                Log.w(TAG, "Failed to unregister FCM token: ${err.message}")
            }
        }
    }

    private fun signedInAccountIds(): Set<String> =
        auth.accounts.value.filter { it.token != null }.map { it.id }.toSet()

    /**
     * Parks until there is something to do, then reconciles. Failed passes
     * back off exponentially instead of hammering an unreachable (or
     * credential-rejecting) server; an account change cuts the wait short.
     */
    private suspend fun reconcileLoop() {
        var retryMs = PUSH_BASE_RETRY_MS
        var retry = false
        while (true) {
            if (retry) withTimeoutOrNull(retryMs) { wake.receive() } else wake.receive()
            retry = reconciler.reconcile()
            retryMs = nextPushRetryMs(retryMs, retry)
        }
    }

    private suspend fun currentFcmToken(): String? = try {
        suspendCancellableCoroutine { cont ->
            FirebaseMessaging.getInstance().token
                .addOnSuccessListener { cont.resume(it) }
                .addOnFailureListener { cont.resumeWithException(it) }
        }
    } catch (err: Throwable) {
        Log.w(TAG, "FCM getToken failed: ${err.message}")
        null
    }

    companion object {
        private const val TAG = "PushTokenMgr"
        private const val UNREGISTER_TIMEOUT_MS = 3_000L
    }
}
