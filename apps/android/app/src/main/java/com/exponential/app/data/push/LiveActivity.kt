package com.exponential.app.data.push

/**
 * #EXP-1639: a running session as a Live Update, the Android mirror of
 * `packages/domain-contract/src/live-activity.ts`, locked by
 * `fixtures/live-activity.json` (`LiveActivityTest`). The server decides and
 * seals the content; the phone only draws it ([LiveActivityNotifier]).
 * [follows] is mirrored to lock the rules, never run here.
 */
object LiveActivity {
    const val PUSH_TYPE = "session_activity"
    /** The FCM data key naming the event (`start` | `update` | `end`). */
    const val EVENT_KEY = "activity"
    const val EVENT_END = "end"

    const val ENCRYPTED_TITLE = "Encrypted session"
    const val PR_BADGE = "PR #{n}"
    const val BADGE_NEEDS_YOU = "needs-you"
    const val BADGE_TIMER = "timer"

    val PHASES: Map<String, String> = linkedMapOf(
        "starting" to "Starting",
        "working" to "Working",
        "needsYou" to "Needs you",
        "blocked" to "Blocked",
        "done" to "Done",
    )
    val SEGMENTS: List<String> = listOf("Working", "Needs you", "PR opened", "Done")

    const val MAX_ACTIVITIES = 3
    const val DISMISS_MINUTES = 15
    const val MIN_UPDATE_SECONDS = 30

    data class Session(
        val id: String,
        val parentId: String?,
        val userId: String,
        val agentId: String?,
        val startedByAgentId: String?,
        val title: String?,
        val sealedTitle: Boolean,
        val status: String,
        val busy: Boolean,
        val needsInput: Boolean,
        val blocked: Boolean,
        val startedAt: String?,
    )

    data class Content(
        val phase: String,
        val title: String,
        val subtitle: String,
        val detail: String?,
        /** `needs-you`, `PR #<n>` or `timer`. */
        val badge: String,
        val segment: Int,
    )

    fun phase(status: String, needsInput: Boolean, blocked: Boolean): String = when {
        needsInput -> "needsYou"
        status == "idle" || status == "ended" -> "done"
        blocked -> "blocked"
        status == "queued" -> "starting"
        else -> "working"
    }

    fun phase(session: Session): String = phase(session.status, session.needsInput, session.blocked)

    /** The sessions a member's phone follows, newest started first, capped. */
    fun follows(me: String, sessions: List<Session>): List<String> = sessions
        .filter {
            it.parentId == null && it.userId == me && it.startedByAgentId == null && it.agentId == null &&
                phase(it) != "done"
        }
        .sortedByDescending { it.startedAt.orEmpty() }
        .take(MAX_ACTIVITIES)
        .map { it.id }

    fun content(
        session: Session,
        deviceLabel: String?,
        prNumber: Int?,
        askPreview: String?,
        blockedLabel: String?,
    ): Content {
        val phase = phase(session)
        val label = PHASES.getValue(phase)
        val pr = prNumber?.let { PR_BADGE.replace("{n}", it.toString()) }
        return Content(
            phase = phase,
            title = if (session.sealedTitle || session.title.isNullOrEmpty()) ENCRYPTED_TITLE else session.title,
            subtitle = if (!deviceLabel.isNullOrEmpty()) "$label · $deviceLabel" else label,
            detail = when (phase) {
                "needsYou" -> askPreview
                "blocked" -> blockedLabel
                else -> null
            },
            badge = if (phase == "needsYou") BADGE_NEEDS_YOU else pr ?: BADGE_TIMER,
            segment = when {
                phase == "done" -> 4
                phase == "needsYou" -> 2
                pr != null -> 3
                else -> 1
            },
        )
    }

    /** The status-bar chip's text for a non-timer [badge] (null = the chronometer). */
    fun chipText(badge: String): String? = when (badge) {
        BADGE_TIMER -> null
        BADGE_NEEDS_YOU -> PHASES.getValue("needsYou")
        else -> badge
    }
}
