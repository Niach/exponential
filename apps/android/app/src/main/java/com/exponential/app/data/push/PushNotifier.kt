package com.exponential.app.data.push

import android.app.NotificationChannel
import android.app.NotificationChannelGroup
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.os.Build
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
            .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
            .addExtras(PushThreads.extras(null, post.thread, post.teamId).apply { putBoolean(PushThreads.EXTRA_SUMMARY, true) })
            .build()
        try {
            val manager = NotificationManagerCompat.from(context)
            manager.notify(post.thread, post.id, builder.build())
            manager.notify(post.thread, PushThreads.SUMMARY_ID, summary)
        } catch (err: SecurityException) {
            // POST_NOTIFICATIONS not granted — drop silently.
            Log.w(TAG, "notify denied: ${err.message}")
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
