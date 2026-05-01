package com.exponential.app.data.push

import android.app.PendingIntent
import android.content.Intent
import android.net.Uri
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.exponential.app.ExponentialApp
import com.exponential.app.MainActivity
import com.exponential.app.R
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dagger.hilt.android.AndroidEntryPoint
import javax.inject.Inject

@AndroidEntryPoint
class FcmService : FirebaseMessagingService() {

    @Inject lateinit var pushTokenManager: PushTokenManager

    override fun onNewToken(token: String) {
        Log.i(TAG, "FCM token rotated")
        pushTokenManager.onNewToken(token)
    }

    override fun onMessageReceived(message: RemoteMessage) {
        val data = message.data
        val title = message.notification?.title ?: data["title"] ?: "Exponential"
        val body = message.notification?.body ?: data["body"]
        val issueId = data["issueId"]

        val intent = if (issueId != null) {
            Intent(Intent.ACTION_VIEW, Uri.parse("exp://issue/$issueId")).apply {
                setPackage(packageName)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            }
        } else {
            Intent(this, MainActivity::class.java).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            }
        }

        val pendingIntent = PendingIntent.getActivity(
            this,
            issueId?.hashCode() ?: 0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        val notification = NotificationCompat.Builder(this, ExponentialApp.ISSUE_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .build()

        val notificationId = issueId?.hashCode() ?: System.currentTimeMillis().toInt()
        try {
            NotificationManagerCompat.from(this).notify(notificationId, notification)
        } catch (err: SecurityException) {
            // POST_NOTIFICATIONS not granted — drop silently.
            Log.w(TAG, "notify denied: ${err.message}")
        }
    }

    companion object {
        private const val TAG = "FcmService"
    }
}
