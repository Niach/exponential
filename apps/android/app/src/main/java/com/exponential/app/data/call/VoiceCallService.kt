package com.exponential.app.data.call

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.exponential.app.MainActivity
import com.exponential.app.R

/**
 * NEW-69: the `microphone` foreground service a phone call holds while it
 * runs, so the microphone keeps capturing with the screen off or the app in
 * the background. Its one ongoing notification is titled with the call copy
 * (`Call {agent}`) and opens the app; [VoiceCallController] starts it when a
 * call starts and stops it on every end.
 *
 * A stop that lands before [onStartCommand] ran never calls `stopService`
 * (the OS would crash the app: `startForegroundService` without
 * `startForeground`): it clears [wanted], and [onStartCommand] then goes
 * foreground first and stops itself at once.
 */
class VoiceCallService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val title = intent?.getStringExtra(EXTRA_TITLE).orEmpty()
        val manager = getSystemService(NotificationManager::class.java)
        manager?.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, CHANNEL_NAME, NotificationManager.IMPORTANCE_LOW),
        )
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_notification)
            .setContentTitle(title)
            .setContentIntent(open)
            .setOngoing(true)
            .setSilent(true)
            .setCategory(Notification.CATEGORY_CALL)
            .build()
        try {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                notification,
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0,
            )
        } catch (t: Throwable) {
            // A start the OS refuses (background start limits) leaves the call
            // running in the foreground only; it never fails the call.
            Log.w("VoiceCallService", "foreground start refused", t)
            stopSelf()
            return START_NOT_STICKY
        }
        val keep = synchronized(lock) {
            foreground = wanted
            wanted
        }
        if (!keep) {
            // The call ended before the service came up.
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        synchronized(lock) { foreground = false }
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL_ID = "voice_call"
        private const val CHANNEL_NAME = "Call"
        private const val NOTIFICATION_ID = 6901
        private const val EXTRA_TITLE = "title"

        private val lock = Any()
        /** A call wants the service (start without a stop since). */
        private var wanted = false
        /** [onStartCommand] went foreground and the service still runs. */
        private var foreground = false

        fun start(context: Context, title: String) {
            synchronized(lock) { wanted = true }
            runCatching {
                context.startForegroundService(Intent(context, VoiceCallService::class.java).putExtra(EXTRA_TITLE, title))
            }.onFailure { Log.w("VoiceCallService", "start refused", it) }
        }

        fun stop(context: Context) {
            val running = synchronized(lock) {
                wanted = false
                foreground
            }
            if (running) runCatching { context.stopService(Intent(context, VoiceCallService::class.java)) }
        }
    }
}
