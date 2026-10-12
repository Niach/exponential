package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import java.time.LocalDate
import java.time.LocalDateTime

// EXP-1253 (EXP-1259 D1): the Agent page is HOME — "Evening, Danny" over the
// composer, Recent under it. The pure half, hand-copied from
// `packages/domain-contract/fixtures/composer-home.json` and locked to it by
// HomeGreetingTest (which replays its `cases`). TS reference:
// `packages/domain-contract/src/home-greeting.ts`.

data class HomeGreetingBucket(
    val id: String,
    /** First hour, inclusive. */
    val from: Int,
    /** Last hour, inclusive; below [from] = the bucket wraps midnight. */
    val to: Int,
    val variants: List<String>,
)

object AgentHome {
    const val HOME_TEST_ID = "agent-home"
    const val GREETING_TEST_ID = "agent-home-greeting"
    const val MARK_TEST_ID = "agent-home-mark"
    const val RECENT_TEST_ID = "agent-home-recent"
    const val RECENT_ROW_TEST_ID = "agent-home-recent-row"

    const val PLACEHOLDER = "What do you want to build?"

    /** Geometry in dp (`geometry`); phones take the `*Phone` sizes. */
    const val MARK = 32
    const val MARK_PHONE = 26
    const val GAP = 12
    const val HEADLINE = 30
    const val HEADLINE_PHONE = 24
    const val HEADLINE_WEIGHT = 600
    const val HEADLINE_TO_CARD = 24
    const val OPTIONS_TO_RECENT = 32

    const val RECENT_LABEL = "Recent"
    const val RECENT_COUNT = 3

    /** The greeting day starts at 05:00: one night keeps one variant. */
    const val DAY_STARTS_AT_HOUR = 5

    val BUCKETS: List<HomeGreetingBucket> = listOf(
        HomeGreetingBucket(
            "morning", 5, 11,
            listOf("Good morning, {name}", "Coffee and code, {name}?", "Morning, {name}", "Back at it, {name}"),
        ),
        HomeGreetingBucket(
            "afternoon", 12, 17,
            listOf("Good afternoon, {name}", "What's next, {name}?", "Afternoon, {name}", "Back at it, {name}"),
        ),
        HomeGreetingBucket(
            "evening", 18, 22,
            listOf("Good evening, {name}", "How was your day, {name}?", "Evening, {name}", "Back at it, {name}"),
        ),
        HomeGreetingBucket(
            "night", 23, 4,
            listOf("Hello, night owl", "Burning the midnight oil, {name}?", "Night owl, {name}?", "Still up, {name}?"),
        ),
    )

    private val LOCAL = Regex("""^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})""")

    /** `YYYY-MM-DDTHH:mm` of a local date-time (the device's own zone). */
    fun localWallClock(time: LocalDateTime): String =
        "%04d-%02d-%02dT%02d:%02d".format(time.year, time.monthValue, time.dayOfMonth, time.hour, time.minute)

    private fun bucketFor(hour: Int): HomeGreetingBucket =
        BUCKETS.firstOrNull { bucket ->
            if (bucket.from <= bucket.to) hour in bucket.from..bucket.to else hour >= bucket.from || hour <= bucket.to
        } ?: BUCKETS.first()

    /** The first whitespace-separated token of a user row's name, or `""`. */
    fun greetingFirstName(name: String?): String =
        name.orEmpty().trim().split(Regex("\\s+")).firstOrNull().orEmpty()

    /**
     * The greeting for a local wall-clock time (`YYYY-MM-DDTHH:mm`) and a user
     * row's name. Computed once per page mount — it never flips while typing.
     */
    fun homeGreeting(local: String, name: String?): String {
        val match = LOCAL.find(local) ?: return ""
        val (year, month, day, hour) = match.destructured.toList().take(4).map { it.toInt() }
        val bucket = bucketFor(hour)
        val shifted = LocalDate.of(year, month, day).toEpochDay() * 24 + hour - DAY_STARTS_AT_HOUR
        val dayIndex = Math.floorDiv(shifted, 24L)
        val count = bucket.variants.size
        val variant = bucket.variants[Math.floorMod(dayIndex, count.toLong()).toInt()]
        val first = greetingFirstName(name)
        return if (first.isNotEmpty()) variant.replace("{name}", first) else variant.replace(", {name}", "")
    }

    /**
     * RECENT: the caller's last [count] OWN top-level runs in [teamId] (no
     * parent run), any status, newest first by `updatedAt` then `createdAt`.
     * A resume succession is ONE run (its newest row), the way every session
     * list draws it.
     */
    fun recentRuns(
        sessions: List<CodingSessionEntity>,
        teamId: String?,
        userId: String?,
        count: Int = RECENT_COUNT,
    ): List<CodingSessionEntity> {
        if (teamId == null || userId == null) return emptyList()
        val own = sessions
            .distinctBy { it.id }
            .filter { it.userId == userId && it.teamId == teamId && it.parentSessionId == null }
        return sessionTree(own)
            .filterIsInstance<SessionTreeNode.Session>()
            .map { it.session }
            .sortedWith(
                compareByDescending<CodingSessionEntity> { stamp(it.updatedAt) }
                    .thenByDescending { stamp(it.createdAt) }
                    .thenBy { it.id },
            )
            .take(count)
    }

    private fun stamp(value: String?): Long =
        value?.trim()?.takeIf { it.isNotEmpty() }?.let(WireTimestamps::parseEpochMs) ?: 0L
}
