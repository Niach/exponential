package com.exponential.app.data.call

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import com.exponential.app.MainActivity
import com.exponential.app.R

/**
 * NEW-69: the `microphone` foreground service a phone call holds while it
 * runs, so the microphone keeps capturing with the screen off or the app in
 * the background. Its one ongoing notification is titled with the call copy
 * (`Call {agent}`) and opens the app; [VoiceCallController] starts it at
 * the tap (#EXP-1942: the app is in front, so the microphone stays usable
 * when the host answers with another app in front) and stops it on every
 * end.
 *
 * #ERR-11, #EXP-1940: it is started with a plain `startService` (allowed
 * exactly when the microphone type is: the app in front), never
 * `startForegroundService`, so it owes Android no `startForeground`: a
 * stop before [onStartCommand] ran, and a `startForeground` Android refuses,
 * can never crash the app. A call whose service does not start, or is
 * refused, hears of it ([CallServiceLink]) and fails `dropped`.
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
            // Android refused the microphone type (the app left the
            // foreground since the start). Stopping is safe (a plain start
            // owes no startForeground); the call cannot capture: it fails.
            Log.w("VoiceCallService", "foreground start refused", t)
            stopSelf()
            link?.refused()
        }
        return START_NOT_STICKY
    }

    companion object {
        private const val CHANNEL_ID = "voice_call"
        private const val CHANNEL_NAME = "Call"
        private const val NOTIFICATION_ID = 6901
        private const val EXTRA_TITLE = "title"

        @Volatile private var link: CallServiceLink? = null

        /**
         * Starts the service for a call; false = Android refuses it now (the
         * app is not in front, RECORD_AUDIO not granted): the caller fails
         * the call. [onRefused] = Android refused it after all, later.
         */
        fun start(context: Context, title: String, onRefused: () -> Unit): Boolean {
            val app = context.applicationContext
            val link = link ?: CallServiceLink(object : CallServiceLink.Host {
                override fun visible() = ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
                override fun microphoneGranted() =
                    ContextCompat.checkSelfPermission(app, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                override fun startService(title: String) {
                    app.startService(Intent(app, VoiceCallService::class.java).putExtra(EXTRA_TITLE, title))
                }
                override fun stopService() {
                    app.stopService(Intent(app, VoiceCallService::class.java))
                }
            }).also { link = it }
            return link.start(title, onRefused).also { if (!it) Log.w("VoiceCallService", "not started") }
        }

        fun stop() {
            link?.stop()
        }
    }
}

/** The service's start, refusal and stop for one call at a time, apart from Android. */
internal class CallServiceLink(private val host: Host) {
    interface Host {
        fun visible(): Boolean
        fun microphoneGranted(): Boolean
        /** A plain start; throws when Android refuses it. */
        fun startService(title: String)
        fun stopService()
    }

    @Volatile private var onRefused: (() -> Unit)? = null

    /** False also while a call holds the service (#EXP-1942): one call, one service. */
    fun start(title: String, onRefused: () -> Unit): Boolean {
        if (this.onRefused != null || !host.visible() || !host.microphoneGranted()) return false
        this.onRefused = onRefused
        return runCatching { host.startService(title) }.onFailure { this.onRefused = null }.isSuccess
    }

    /** `startForeground` threw: the running call hears of it, once. */
    fun refused() {
        val tell = onRefused
        onRefused = null
        tell?.invoke()
    }

    fun stop() {
        onRefused = null
        runCatching { host.stopService() }
    }
}
