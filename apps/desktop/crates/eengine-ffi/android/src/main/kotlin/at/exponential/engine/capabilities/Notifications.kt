// #VAPP-265: `notifications` (post, cancel). Android: NotificationManagerCompat
// on one channel (POST_NOTIFICATIONS asked on API 33+), a future `at` through
// AlarmManager (inexact setAndAllowWhileIdle, no exact-alarm permission) and
// NotificationAlarmReceiver. Every posted or scheduled notification is a
// record in SharedPreferences (`pending` until shown, then `shown`) so cancel
// knows whether one existed and alarms survive a reboot (BootReceiver).
// The notification's tag = its id (`<app>/<id>`), its numeric id = [NotificationIds.of].
// A tap launches the host's launch activity with the stored tap JSON as an
// extra; the host reads it with [NotificationTaps.fromIntent] and hands it
// VERBATIM to the app instance as `event(tap)`.
package at.exponential.engine.capabilities

/** One posted or scheduled notification. `tapJson` = the bridge's tap, verbatim (null = none). */
data class NotificationRecord(
    val id: String,
    val title: String,
    val body: String?,
    val atMillis: Long?,
    val tapJson: String?,
    val state: State,
) {
    enum class State { PENDING, SHOWN }

    fun toJson(): String = Json.write(
        linkedMapOf(
            "id" to id, "title" to title, "body" to body, "at" to atMillis,
            "tap" to tapJson, "state" to state.name.lowercase(),
        ),
    )

    companion object {
        fun fromJson(text: String): NotificationRecord? = try {
            val m = Json.parse(text) as Map<*, *>
            NotificationRecord(
                id = m["id"] as String,
                title = m["title"] as String,
                body = m["body"] as String?,
                atMillis = (m["at"] as Number?)?.toLong(),
                tapJson = m["tap"] as String?,
                state = if (m["state"] == "shown") State.SHOWN else State.PENDING,
            )
        } catch (_: Exception) {
            null
        }
    }
}

/** A stable numeric id for a string id (32-bit FNV-1a over its UTF-8). */
object NotificationIds {
    fun of(id: String): Int {
        var h = 0x811c9dc5.toInt()
        for (b in id.toByteArray(Charsets.UTF_8)) {
            h = h xor (b.toInt() and 0xff)
            h *= 0x01000193
        }
        return h
    }
}

/** The extras a tap carries and the host reads back. */
object NotificationTaps {
    const val EXTRA_TAP = "at.exponential.engine.NOTIFICATION_TAP"
    const val EXTRA_ID = "at.exponential.engine.NOTIFICATION_ID"

    /** The extras of the tap intent: the tap JSON (when there is one) and the id. */
    fun extras(record: NotificationRecord): Map<String, String> =
        linkedMapOf(EXTRA_ID to record.id).also { m -> record.tapJson?.let { m[EXTRA_TAP] = it } }

    /** The tap JSON back from the extras (null = not a notification tap, or one without a screen). */
    fun fromExtras(extra: (String) -> String?): String? = extra(EXTRA_TAP)

    /**
     * The tap JSON of an intent that opened (or reached) the launch activity,
     * for `instance.event(tap)`; null when it is not a notification tap. Call
     * from onCreate and onNewIntent. The extra is removed so a recreated
     * activity does not deliver it twice.
     */
    @JvmStatic
    fun fromIntent(intent: android.content.Intent?): String? {
        if (intent == null) return null
        val tap = fromExtras { intent.getStringExtra(it) }
        if (tap != null) intent.removeExtra(EXTRA_TAP)
        return tap
    }
}

/** Where records live (SharedPreferences on Android). */
interface NotificationStore {
    fun get(id: String): NotificationRecord?
    fun put(record: NotificationRecord)
    fun remove(id: String)
    fun all(): List<NotificationRecord>
}

/** The platform's notification shade. */
interface NotificationPoster {
    /** Permission to post (asks on API 33+ when it can); false = denied. */
    fun ensurePermission(): Boolean

    /** Show (or replace) it now. */
    fun show(record: NotificationRecord)

    /** Withdraw a shown one. */
    fun cancel(id: String)

    /** Whether a shown one is still in the shade (the person may have dismissed it). */
    fun isActive(id: String): Boolean
}

/** The platform's alarms (inexact, survive idle). */
interface AlarmScheduler {
    fun schedule(id: String, atMillis: Long)
    fun cancel(id: String)
}

/** The scheduling rules, shared by the capability and the alarm/boot receivers. */
class NotificationCenter(
    private val poster: NotificationPoster,
    private val alarms: AlarmScheduler,
    private val store: NotificationStore,
    private val now: () -> Long,
) {
    /** Post now (`at` null or not in the future) or schedule; an existing id is replaced. */
    fun post(id: String, title: String, body: String?, atMillis: Long?, tapJson: String?) {
        prune()
        val previous = store.get(id)
        if (previous != null) {
            alarms.cancel(id)
            if (previous.state == NotificationRecord.State.SHOWN) poster.cancel(id)
        }
        val due = atMillis == null || atMillis <= now()
        val record = NotificationRecord(id, title, body, atMillis, tapJson, if (due) NotificationRecord.State.SHOWN else NotificationRecord.State.PENDING)
        if (due) {
            poster.show(record)
            store.put(record)
        } else {
            store.put(record)
            alarms.schedule(id, atMillis)
        }
    }

    /** Withdraw a pending or shown one: true when one existed. */
    fun cancel(id: String): Boolean {
        val record = store.get(id) ?: return false
        val existed = when (record.state) {
            NotificationRecord.State.PENDING -> true
            NotificationRecord.State.SHOWN -> poster.isActive(id)
        }
        alarms.cancel(id)
        poster.cancel(id)
        store.remove(id)
        return existed
    }

    /** An alarm fired: show the pending one (a cancelled or replaced one is gone or rescheduled). */
    fun fire(id: String) {
        val record = store.get(id) ?: return
        if (record.state != NotificationRecord.State.PENDING) return
        val at = record.atMillis
        if (at != null && at > now()) {
            alarms.schedule(id, at)
            return
        }
        val shown = record.copy(state = NotificationRecord.State.SHOWN)
        poster.show(shown)
        store.put(shown)
    }

    /** After a reboot or an update: alarms are gone, schedule the pending ones again (overdue ones show now). */
    fun reschedule() {
        for (record in store.all()) {
            if (record.state != NotificationRecord.State.PENDING) continue
            val at = record.atMillis
            if (at == null || at <= now()) fire(record.id) else alarms.schedule(record.id, at)
        }
    }

    /** Forget shown records the person already dismissed. */
    private fun prune() {
        for (record in store.all()) {
            if (record.state == NotificationRecord.State.SHOWN && !poster.isActive(record.id)) store.remove(record.id)
        }
    }
}

class NotificationsCapability(
    private val center: NotificationCenter,
    private val poster: NotificationPoster,
    mainThread: MainThreadCheck = AndroidMainThread,
) : NativeCapability("notifications", mainThread) {
    override val toolNames = setOf("cancel", "post")

    override fun handle(tool: String, args: Args): Map<String, Any?> {
        val id = args.string("id")
        if (tool == "cancel") return mapOf("cancelled" to center.cancel(id))
        if (!poster.ensurePermission()) throw ToolFailure.failed("permission denied")
        // The tap object exactly as the bridge wrote it, stored and handed back verbatim.
        val tap = if (args.obj("tap") != null) args.rawMember("tap") else null
        center.post(id, args.string("title"), args.optString("body"), args.optString("at")?.let(Times::parse), tap)
        return emptyMap()
    }
}
