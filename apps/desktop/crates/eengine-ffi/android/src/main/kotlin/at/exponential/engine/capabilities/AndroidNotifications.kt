// #VAPP-265: notifications on Android (see Notifications.kt for the rules).
package at.exponential.engine.capabilities

import android.Manifest
import android.annotation.SuppressLint
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/**
 * The channel and icon the notifications use, persisted so the alarm receiver
 * (which runs without the host) posts them the same way. Call
 * [AndroidNotifications.configure] once at startup to set a monochrome small
 * icon (default: the app's icon) and the channel's user-visible name.
 */
object AndroidNotifications {
    private const val PREFS = "at.exponential.engine.notifications"
    private const val CONFIG = "at.exponential.engine.notifications.config"
    const val CHANNEL_ID = "exponential-engine"

    fun configure(context: Context, smallIcon: Int, channelName: String) {
        context.getSharedPreferences(CONFIG, Context.MODE_PRIVATE).edit()
            .putInt("smallIcon", smallIcon).putString("channelName", channelName).apply()
    }

    /** The rules over the platform: what the capability and the receivers share. */
    fun center(context: Context, host: ActivityHost? = null): NotificationCenter {
        val app = context.applicationContext
        return NotificationCenter(AndroidNotificationPoster(app, host), AndroidAlarmScheduler(app), PrefsNotificationStore(app), System::currentTimeMillis)
    }

    fun capability(context: Context, host: ActivityHost?): NotificationsCapability {
        val app = context.applicationContext
        return NotificationsCapability(center(app, host), AndroidNotificationPoster(app, host))
    }

    internal fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    internal fun config(context: Context) = context.getSharedPreferences(CONFIG, Context.MODE_PRIVATE)

    /** A data URI per id, so PendingIntents for different ids never collide. */
    internal fun idUri(id: String): Uri = Uri.Builder().scheme("exponential-engine-notification").opaquePart(id).build()
}

class PrefsNotificationStore(context: Context) : NotificationStore {
    private val prefs = AndroidNotifications.prefs(context)

    override fun get(id: String): NotificationRecord? = prefs.getString(id, null)?.let(NotificationRecord::fromJson)
    override fun put(record: NotificationRecord) { prefs.edit().putString(record.id, record.toJson()).commit() }
    override fun remove(id: String) { prefs.edit().remove(id).commit() }
    override fun all(): List<NotificationRecord> = prefs.all.values.mapNotNull { (it as? String)?.let(NotificationRecord::fromJson) }
}

class AndroidNotificationPoster(private val context: Context, private val host: ActivityHost?) : NotificationPoster {
    private val manager = NotificationManagerCompat.from(context)

    override fun ensurePermission(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return true
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == android.content.pm.PackageManager.PERMISSION_GRANTED) return true
        return host?.requestAnyPermission(listOf(Manifest.permission.POST_NOTIFICATIONS)) ?: false
    }

    // POST_NOTIFICATIONS: asked in ensurePermission before a post; a revoked one is caught below.
    @SuppressLint("MissingPermission")
    override fun show(record: NotificationRecord) {
        ensureChannel()
        val config = AndroidNotifications.config(context)
        val icon = config.getInt("smallIcon", 0).takeIf { it != 0 } ?: context.applicationInfo.icon
        val builder = NotificationCompat.Builder(context, AndroidNotifications.CHANNEL_ID)
            .setSmallIcon(icon)
            .setContentTitle(record.title)
            .setAutoCancel(true)
            .setWhen(record.atMillis ?: System.currentTimeMillis())
            .setShowWhen(true)
        record.body?.let { builder.setContentText(it).setStyle(NotificationCompat.BigTextStyle().bigText(it)) }
        tapIntent(record)?.let(builder::setContentIntent)
        try {
            manager.notify(record.id, NotificationIds.of(record.id), builder.build())
        } catch (_: SecurityException) {
            // POST_NOTIFICATIONS was revoked since: nothing to show.
        }
    }

    override fun cancel(id: String) = manager.cancel(id, NotificationIds.of(id))

    override fun isActive(id: String): Boolean {
        val system = context.getSystemService(NotificationManager::class.java) ?: return false
        return system.activeNotifications.any { it.tag == id && it.id == NotificationIds.of(id) }
    }

    /** A tap opens the host's launch activity with the tap JSON (and the id) as extras. */
    private fun tapIntent(record: NotificationRecord): PendingIntent? {
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return null
        // Distinct per id (no data URI: the host's own deep-link handling never sees one).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) launch.identifier = record.id
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        for ((key, value) in NotificationTaps.extras(record)) launch.putExtra(key, value)
        return PendingIntent.getActivity(context, NotificationIds.of(record.id), launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    private fun ensureChannel() {
        val system = context.getSystemService(NotificationManager::class.java) ?: return
        val name = AndroidNotifications.config(context).getString("channelName", null) ?: "Notifications"
        system.createNotificationChannel(NotificationChannel(AndroidNotifications.CHANNEL_ID, name, NotificationManager.IMPORTANCE_DEFAULT))
    }
}

class AndroidAlarmScheduler(private val context: Context) : AlarmScheduler {
    private val alarms = context.getSystemService(AlarmManager::class.java)

    override fun schedule(id: String, atMillis: Long) {
        alarms?.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMillis, pending(id, PendingIntent.FLAG_UPDATE_CURRENT)!!)
    }

    override fun cancel(id: String) {
        pending(id, PendingIntent.FLAG_NO_CREATE)?.let {
            alarms?.cancel(it)
            it.cancel()
        }
    }

    private fun pending(id: String, flag: Int): PendingIntent? {
        val intent = Intent(context, NotificationAlarmReceiver::class.java)
            .setData(AndroidNotifications.idUri(id))
            .putExtra(NotificationTaps.EXTRA_ID, id)
        return PendingIntent.getBroadcast(context, NotificationIds.of(id), intent, flag or PendingIntent.FLAG_IMMUTABLE)
    }
}

/** A scheduled notification is due (declared in the library manifest, not exported). */
class NotificationAlarmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getStringExtra(NotificationTaps.EXTRA_ID) ?: return
        AndroidNotifications.center(context).fire(id)
    }
}

/** Alarms do not survive a reboot or an update: schedule the pending ones again. */
class NotificationBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> AndroidNotifications.center(context).reschedule()
        }
    }
}
