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
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
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
            combine(auth.instanceUrl, auth.token) { url, token -> url to token }
                .distinctUntilChanged()
                .collect { (url, token) ->
                    if (url != null && token != null) registerCurrentToken()
                }
        }
    }

    suspend fun registerCurrentToken() {
        try {
            val token = currentFcmToken() ?: return
            api.register(token)
            Log.i(TAG, "Registered FCM token with backend")
        } catch (err: Throwable) {
            Log.w(TAG, "Failed to register FCM token: ${err.message}")
        }
    }

    fun onNewToken(token: String) {
        // Called from FcmService.onNewToken on a background thread; just enqueue.
        scope.launch {
            try {
                if (auth.token.value != null) api.register(token)
            } catch (err: Throwable) {
                Log.w(TAG, "Failed to register rotated FCM token: ${err.message}")
            }
        }
    }

    fun unregisterAndForget() {
        scope.launch {
            val token = runCatching { currentFcmToken() }.getOrNull() ?: return@launch
            try {
                api.unregister(token)
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
    }
}
