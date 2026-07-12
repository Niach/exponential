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
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

@Singleton
class PushTokenManager @Inject constructor(
    private val auth: AuthRepository,
    private val api: PushTokensApi,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    fun start() {
        scope.launch {
            // Register the token for EVERY signed-in account, not just the
            // active one: the server keys registrations per (token, user), so
            // an account left holding a dead token after a rotation silently
            // stops receiving pushes until it happens to be made active again.
            auth.accounts
                .map { accounts -> accounts.filter { it.token != null }.map { it.id }.toSet() }
                .distinctUntilChanged()
                .collect { accountIds ->
                    if (accountIds.isEmpty()) return@collect
                    val token = currentFcmToken() ?: return@collect
                    accountIds.forEach { registerAccount(it, token) }
                }
        }
    }

    fun onNewToken(token: String) {
        // Called from FcmService.onNewToken on a background thread; just
        // enqueue. A rotation invalidates the old token for the whole device,
        // so every signed-in account needs the new one.
        scope.launch {
            auth.accounts.value
                .filter { it.token != null }
                .forEach { registerAccount(it.id, token) }
        }
    }

    private suspend fun registerAccount(accountId: String, token: String) {
        // Re-check right before the request: the account may have signed out
        // while this pass was in flight, and a late register would resurrect
        // the server row its unregister just deleted.
        if (auth.accounts.value.none { it.id == accountId && it.token != null }) return
        try {
            api.register(accountId, token)
            Log.i(TAG, "Registered FCM token with backend")
        } catch (err: Throwable) {
            Log.w(TAG, "Failed to register FCM token: ${err.message}")
        }
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
        withTimeoutOrNull(UNREGISTER_TIMEOUT_MS) {
            val token = currentFcmToken() ?: return@withTimeoutOrNull
            try {
                api.unregister(accountId, token)
                Log.i(TAG, "Unregistered FCM token with backend")
            } catch (err: Throwable) {
                Log.w(TAG, "Failed to unregister FCM token: ${err.message}")
            }
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
