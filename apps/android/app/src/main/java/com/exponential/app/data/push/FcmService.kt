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
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.electric.SyncManager
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dagger.hilt.android.AndroidEntryPoint
import javax.inject.Inject

@AndroidEntryPoint
class FcmService : FirebaseMessagingService() {

    @Inject lateinit var pushTokenManager: PushTokenManager
    @Inject lateinit var auth: AuthRepository
    @Inject lateinit var syncManager: SyncManager

    override fun onNewToken(token: String) {
        Log.i(TAG, "FCM token rotated")
        pushTokenManager.onNewToken(token)
    }

    override fun onMessageReceived(message: RemoteMessage) {
        // Any push means server state changed; catching up now (rather than at
        // tap time) is what makes the tapped issue already local when the
        // detail screen opens. Free when the process is only cached (EXP-656):
        // the shape gate is closed, so the kick arms freshness and queues a
        // token that no loop acts on until the app is opened.
        syncManager.kick("push")

        val data = message.data
        val title = message.notification?.title ?: data["title"] ?: "Exponential"
        val body = message.notification?.body ?: data["body"]
        // support_reply pushes (EXP-180) carry a threadId and NO issue keys —
        // route their taps straight to the ticket conversation.
        val target = PushDeepLinks.target(
            type = data["type"],
            issueId = data["issueId"],
            threadId = data["threadId"],
        )

        // The push carries its recipient's server user id. A push for a
        // NON-ACTIVE signed-in account keeps its deep link (REV2-81) — the id
        // rides along and MainActivity switches to that account before
        // navigating, instead of dropping the tap into the wrong local
        // database (or, as before, dropping it entirely). Only a push for an
        // account no longer on this device falls through to a bare launch.
        val targetUserId = data["userId"]
        val account = PushDeepLinks.resolveAccount(
            accounts = auth.accounts.value,
            activeAccountId = auth.activeAccountId.value,
            targetUserId = targetUserId,
        )

        val intent = if (target != null && account != PushDeepLinks.Account.Unknown) {
            Intent(Intent.ACTION_VIEW, Uri.parse(PushDeepLinks.uri(target, targetUserId))).apply {
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
            target.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        val notification = NotificationCompat.Builder(this, ExponentialApp.ISSUE_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_notification)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .build()

        val notificationId = target?.hashCode() ?: System.currentTimeMillis().toInt()
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
