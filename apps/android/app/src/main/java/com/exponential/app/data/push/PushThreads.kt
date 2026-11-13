package com.exponential.app.data.push

import android.os.Bundle
import com.exponential.app.domain.NotificationDelivery

/**
 * #EXP-1815 (notification-delivery.json): how a posted push is keyed. Group
 * AND tag = its thread (the server's `thread`, else derived like the server
 * does), id = the notification id's hash, one group summary per thread; the
 * extras keep what a later dismiss push matches against.
 */
object PushThreads {
    const val EXTRA_NOTIFICATION_ID = "exp.notificationId"
    const val EXTRA_THREAD = "exp.thread"
    const val EXTRA_TEAM_ID = "exp.teamId"
    const val EXTRA_SUMMARY = "exp.summary"

    /** The summary's id under its thread's tag (a child's id is a hash, never this). */
    const val SUMMARY_ID = Int.MIN_VALUE

    /** The push's thread: the server's key, else derived from its routing data. */
    fun thread(data: Map<String, String>): String =
        data[NotificationDelivery.Keys.THREAD]?.takeIf { it.isNotEmpty() }
            ?: NotificationDelivery.notificationThread(
                type = data["type"].orEmpty(),
                issueId = data["issueId"],
                sessionRootId = data[QuestionPush.ROOT_ID_KEY]?.takeIf { it.isNotEmpty() } ?: data["sessionId"],
                deviceId = data["deviceId"],
            )

    /** id = the notification id's hash; none = [fallback] (the target's), else a fresh one. */
    fun notificationKey(notificationId: String?, fallback: Int?): Int {
        val key = notificationId?.takeIf { it.isNotEmpty() }?.hashCode() ?: fallback ?: System.currentTimeMillis().toInt()
        return if (key == SUMMARY_ID) key + 1 else key
    }

    fun extras(notificationId: String?, thread: String, teamId: String?): Bundle = Bundle().apply {
        notificationId?.takeIf { it.isNotEmpty() }?.let { putString(EXTRA_NOTIFICATION_ID, it) }
        putString(EXTRA_THREAD, thread)
        teamId?.takeIf { it.isNotEmpty() }?.let { putString(EXTRA_TEAM_ID, it) }
    }
}
