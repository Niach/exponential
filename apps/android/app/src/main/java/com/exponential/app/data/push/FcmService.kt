package com.exponential.app.data.push

import android.util.Log
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage

/**
 * Minimal FirebaseMessagingService — for now we just log the device token
 * and any incoming messages. Server-side registration + delivery is a
 * separate piece (push_subscriptions tRPC + FCM admin send).
 */
class FcmService : FirebaseMessagingService() {

    override fun onNewToken(token: String) {
        Log.i(TAG, "FCM token: $token")
    }

    override fun onMessageReceived(message: RemoteMessage) {
        Log.i(TAG, "FCM message from=${message.from} data=${message.data}")
    }

    companion object {
        private const val TAG = "FcmService"
    }
}
