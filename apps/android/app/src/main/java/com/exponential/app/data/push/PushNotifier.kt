package com.exponential.app.data.push

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationChannelGroup
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.os.Build
import android.service.notification.StatusBarNotification
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.exponential.app.R
import com.exponential.app.domain.NotificationDelivery

/**
 * #EXP-1815 (notification-delivery.json): the shade side of a push. A post
 * goes out with group AND tag = its thread plus that thread's group summary,
 * so one chat stacks as one, on its source's channel (created on first use);
 * a dismiss push cancels every active notification it matches.
 */
object PushNotifier {
    private const val TAG = "PushNotifier"

    data class Post(
        val thread: String,
        /** The Android id ([PushThreads.notificationKey]). */
        val id: Int,
        /** The inbox row's id, kept for a later dismiss. */
        val notificationId: String?,
        val teamId: String?,
        val channel: NotificationDelivery.AndroidChannel,
        val title: String,
        val body: String?,
    )

    fun post(
        context: Context,
        post: Post,
        contentIntent: PendingIntent,
        actions: List<NotificationCompat.Action> = emptyList(),
        /** The app icon's number ([AppIconBadge]); 0 = none. */
        number: Int = 0,
    ) {
        val channel = ensureChannel(context, post.channel)
        val builder = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_stat_notification)
            .setContentTitle(post.title)
            .setContentText(post.body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(post.body))
            .setAutoCancel(true)
            .setContentIntent(contentIntent)
            .setGroup(post.thread)
            .addExtras(PushThreads.extras(post.notificationId, post.thread, post.teamId))
        actions.forEach(builder::addAction)
        // The thread's summary: the shade draws the group under it.
        val summary = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_stat_notification)
            .setContentTitle(post.title)
            .setContentText(post.body)
            .setAutoCancel(true)
            .setContentIntent(contentIntent)
            .setGroup(post.thread)
            .setGroupSummary(true)
            .setNumber(number)
            .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
            .addExtras(PushThreads.extras(null, post.thread, post.teamId).apply { putBoolean(PushThreads.EXTRA_SUMMARY, true) })
            .build()
        try {
            val manager = NotificationManagerCompat.from(context)
            manager.notify(post.thread, post.id, builder.build())
            manager.notify(post.thread, PushThreads.SUMMARY_ID, summary)
            setNumber(context, number, post.thread)
        } catch (err: SecurityException) {
            // POST_NOTIFICATIONS not granted — drop silently.
            Log.w(TAG, "notify denied: ${err.message}")
        }
    }

    private fun summaries(context: Context): List<StatusBarNotification> {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return emptyList()
        return runCatching { manager.activeNotifications.toList() }.getOrDefault(emptyList())
            .filter { it.notification.extras.getBoolean(PushThreads.EXTRA_SUMMARY) }
    }

    fun hasSummary(context: Context): Boolean = summaries(context).isNotEmpty()

    /**
     * The launcher adds the numbers of an app's notifications up, so ONE
     * summary carries [number] ([thread]'s, else the one that has it, else the
     * newest) and the others none; a summary whose number stands is left alone.
     */
    fun setNumber(context: Context, number: Int, thread: String? = null) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val all = summaries(context)
        val carrier = all.firstOrNull { thread != null && it.tag == thread }
            ?: all.firstOrNull { it.notification.number > 0 }
            ?: all.maxByOrNull { it.postTime }
        for (sbn in all) {
            val next = if (sbn.key == carrier?.key) number else 0
            // [post] already drew its own summary with the number.
            if (sbn.notification.number == next || (thread != null && sbn.tag == thread)) continue
            runCatching {
                val rebuilt = Notification.Builder.recoverBuilder(context, sbn.notification).setNumber(next).setOnlyAlertOnce(true).build()
                manager.notify(sbn.tag, sbn.id, rebuilt)
            }
        }
    }

    /** The channel [channel] names, created on first use (the system keeps its sound and importance). */
    fun ensureChannel(context: Context, channel: NotificationDelivery.AndroidChannel): String {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return channel.id
        if (channel.name == null || manager.getNotificationChannel(channel.id) != null) return channel.id
        channel.group?.let { group ->
            // getNotificationChannelGroup is API 28; creating an existing group is a no-op rename on 26-27.
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P || manager.getNotificationChannelGroup(group) == null) {
                manager.createNotificationChannelGroup(
                    NotificationChannelGroup(group, NotificationDelivery.Channels.AGENTS_GROUP_NAME),
                )
            }
        }
        manager.createNotificationChannel(
            NotificationChannel(channel.id, channel.name, NotificationManager.IMPORTANCE_DEFAULT).apply {
                group = channel.group
            },
        )
        return channel.id
    }

    /** Cancels every active notification [payload] matches, then the summaries left without a child. */
    fun dismiss(context: Context, payload: Map<String, String>) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val active = runCatching { manager.activeNotifications.toList() }.getOrDefault(emptyList())
        val delivered = active.map { sbn ->
            val extras = sbn.notification.extras
            Triple(
                sbn,
                extras.getBoolean(PushThreads.EXTRA_SUMMARY),
                NotificationDelivery.Delivered(
                    notificationId = extras.getString(PushThreads.EXTRA_NOTIFICATION_ID),
                    thread = extras.getString(PushThreads.EXTRA_THREAD),
                    teamId = extras.getString(PushThreads.EXTRA_TEAM_ID),
                ),
            )
        }
        val children = delivered.filter { !it.second }
        val gone = children.filter { NotificationDelivery.dismissMatches(payload, it.third) }
        gone.forEach { manager.cancel(it.first.tag, it.first.id) }
        val goneKeys = gone.map { it.first.key }.toSet()
        val living = children.filter { it.first.key !in goneKeys }.mapNotNull { it.third.thread }.toSet()
        delivered
            .filter { it.second && it.third.thread !in living }
            .forEach { manager.cancel(it.first.tag, it.first.id) }
    }
}
