package com.exponential.app.data.push

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.exponential.app.R
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.data.sealed.TreeKeyStore
import dagger.hilt.android.qualifiers.ApplicationContext
import java.time.Instant
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull

/**
 * #EXP-1639 (live-activity.json): a running session the member started as
 * ONE ongoing notification per session in the low-importance "Running
 * sessions" channel, never a sound. The server seals the content
 * ([LiveActivity.content]); this only draws it. API 36+: a promoted
 * `Notification.ProgressStyle` Live Update (the fixture's four segments, the
 * status-bar chip); below, an ongoing notification with a determinate bar.
 * `end` leaves the final content, dismissable, gone after `dismissMinutes`.
 * A sealed title is filled in from the local store when this install opens
 * it; nothing is ever sent back.
 */
@Singleton
class LiveActivityNotifier @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    private val dbs: DatabaseHolder,
    private val treeKeyStore: TreeKeyStore,
) {
    /** [event] = `start` | `update` | `end`; [payload] = the opened `s`. */
    internal fun show(event: String, payload: SealedPushPayload, contentIntent: (String, String?) -> PendingIntent) {
        val data = payload.data
        if (data["type"] != LiveActivity.PUSH_TYPE) return
        val sessionId = data["sessionId"]?.takeIf { it.isNotEmpty() } ?: return
        val userId = data["userId"]
        val ended = event == LiveActivity.EVENT_END || data["phase"] == "done"
        val title = (payload.title ?: LiveActivity.ENCRYPTED_TITLE).let { shown ->
            if (shown == LiveActivity.ENCRYPTED_TITLE) localTitle(sessionId, userId) ?: shown else shown
        }
        val subtitle = payload.body.orEmpty()
        val detail = data["detail"]?.takeIf { it.isNotEmpty() }
        val badge = data["badge"] ?: LiveActivity.BADGE_TIMER
        val segment = data["segment"]?.toIntOrNull()?.coerceIn(0, LiveActivity.SEGMENTS.size) ?: 1
        val startedAt = data["startedAt"]?.let { runCatching { Instant.parse(it).toEpochMilli() }.getOrNull() }
        val chip = LiveActivity.chipText(badge)
        val timer = chip == null && startedAt != null && !ended
        val tap = contentIntent(sessionId, userId)
        ensureChannel()

        val notification: Notification = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.BAKLAVA) {
            val segmentLength = PROGRESS_MAX / LiveActivity.SEGMENTS.size
            val style = Notification.ProgressStyle()
                .setStyledByProgress(true)
                .setProgressSegments(LiveActivity.SEGMENTS.indices.map { Notification.ProgressStyle.Segment(segmentLength).setId(it) })
                .setProgress(segment * segmentLength)
            Notification.Builder(context, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_stat_notification)
                .setContentTitle(title)
                // The detail (a question, the blocked label) below the phase line.
                .setContentText(listOfNotNull(subtitle, detail).joinToString("\n"))
                .setStyle(style)
                .setContentIntent(tap)
                .setOnlyAlertOnce(true)
                .setCategory(Notification.CATEGORY_PROGRESS)
                .apply {
                    if (timer) {
                        setUsesChronometer(true)
                        setWhen(startedAt)
                        setShowWhen(true)
                    }
                    if (chip != null) setShortCriticalText(chip)
                    if (ended) {
                        setOngoing(false)
                        setAutoCancel(true)
                        setTimeoutAfter(DISMISS_MS)
                    } else {
                        setOngoing(true)
                        // setRequestPromotedOngoing lands in API 36.1; its extra is API 36's.
                        extras.putBoolean(EXTRA_REQUEST_PROMOTED_ONGOING, true)
                    }
                }
                .build()
        } else {
            NotificationCompat.Builder(context, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_stat_notification)
                .setContentTitle(title)
                .setContentText(subtitle)
                .setStyle(NotificationCompat.BigTextStyle().bigText(listOfNotNull(subtitle, detail).joinToString("\n")))
                .setProgress(LiveActivity.SEGMENTS.size, segment, false)
                .setContentIntent(tap)
                .setOnlyAlertOnce(true)
                .setSilent(true)
                .setCategory(NotificationCompat.CATEGORY_PROGRESS)
                .apply {
                    if (chip != null) setSubText(chip)
                    if (timer) {
                        setUsesChronometer(true)
                        setWhen(startedAt)
                        setShowWhen(true)
                    }
                    if (ended) {
                        setOngoing(false)
                        setAutoCancel(true)
                        setTimeoutAfter(DISMISS_MS)
                    } else {
                        setOngoing(true)
                    }
                }
                .build()
        }
        try {
            NotificationManagerCompat.from(context).notify(notificationId(sessionId), notification)
        } catch (err: SecurityException) {
            Log.w(TAG, "notify denied: ${err.message}")
        }
    }

    /** The session's title opened locally (the push's account, its tree key held), else null. */
    private fun localTitle(sessionId: String, userId: String?): String? = runCatching {
        val accountId = when (val account = PushDeepLinks.resolveAccount(auth.accounts.value, auth.activeAccountId.value, userId)) {
            PushDeepLinks.Account.Active -> auth.activeAccountId.value
            is PushDeepLinks.Account.Switch -> account.id
            PushDeepLinks.Account.Unknown -> null
        } ?: return null
        val row = runBlocking {
            withTimeoutOrNull(LOOKUP_MS) { dbs.database(accountId).sessionDao().getById(sessionId) }
        } ?: return null
        if (row.sealedTitle == null) return null
        val rootId = row.rootId.ifEmpty { row.id }
        val keys = SealedKeys.of(rootId) + treeKeyStore.vault.all(accountId)[rootId].orEmpty()
        if (keys.isEmpty()) return null
        val opened = SealedContent.session(row, mapOf(rootId to keys))
        opened.takeIf { it !== row }?.title?.trim()?.takeIf { it.isNotEmpty() }
    }.getOrNull()

    private fun ensureChannel() {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(CHANNEL_ID) != null) return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, CHANNEL_NAME, NotificationManager.IMPORTANCE_LOW).apply {
                setSound(null, null)
                enableVibration(false)
            },
        )
    }

    companion object {
        private const val TAG = "LiveActivity"
        const val CHANNEL_ID = "running_sessions"
        private const val CHANNEL_NAME = "Running sessions"
        private const val EXTRA_REQUEST_PROMOTED_ONGOING = "android.requestPromotedOngoing"
        private const val PROGRESS_MAX = 100
        private const val LOOKUP_MS = 2_000L
        private val DISMISS_MS = LiveActivity.DISMISS_MINUTES * 60_000L

        /** One notification per session, apart from that session's pushes. */
        fun notificationId(sessionId: String): Int = "live-activity:$sessionId".hashCode()
    }
}
