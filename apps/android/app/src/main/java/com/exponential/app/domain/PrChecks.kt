package com.exponential.app.domain

import com.exponential.app.data.db.PullRequestCheckEntity

/**
 * #EXP-1611 (pr-checks.json CHECKS LIST, the UI ×4): CI on the
 * chat's PR, read from the synced `pull_request_checks` rows and
 * `pull_requests.checks_state`; no client calls GitHub. Mirrors the TS
 * reference `packages/domain-contract/src/pr-checks.ts` (bottom section);
 * `PrChecksTest` replays the fixture's UI cases.
 */
object PrChecks {

    /** pr-checks.json `copy`. */
    object Copy {
        const val CHECKS = "Checks"
        const val SUMMARY_FAILING = "{count} failing"
        const val SUMMARY_RUNNING = "{count} running"
        const val SUMMARY_PASSED = "{count} passed"
    }

    /** pr-checks.json `testIds`. */
    const val TEST_ID_CHECKS_MENU = "pr-checks-menu"
    const val TEST_ID_CHECK_ROW = "pr-check-row"

    /** One synced check row, projected ([status] contract prCheckStatus,
     *  [conclusion] contract prCheckConclusion or null while running). */
    data class Check(
        val name: String,
        val status: String,
        val conclusion: String?,
        val startedAt: String? = null,
        val completedAt: String? = null,
        val detailsUrl: String? = null,
        val required: Boolean = false,
    )

    fun of(row: PullRequestCheckEntity): Check = Check(
        name = row.name,
        status = row.status,
        conclusion = row.conclusion,
        startedAt = row.startedAt,
        completedAt = row.completedAt,
        detailsUrl = row.detailsUrl,
        required = row.required,
    )

    enum class Glyph(val wire: String) {
        Queued("ci-queued"),
        Running("ci-running"),
        Passed("ci-passed"),
        Failed("ci-failed"),
        Skipped("ci-skipped"),
    }

    enum class Tone(val wire: String) { Muted("muted"), Green("green"), Red("red") }

    data class ListRow(
        val name: String,
        val glyph: Glyph,
        val tone: Tone,
        val duration: String?,
        val url: String?,
    )

    data class Summary(val glyph: Glyph, val tone: Tone, val value: String)

    private val FAILING = DomainContract.prChecksFailingConclusions.toSet()

    fun isFailing(check: Check): Boolean =
        check.status == DomainContract.prCheckStatusCompleted && check.conclusion != null && check.conclusion in FAILING

    /** none (no rows) · failure (any red; wins over pending) · pending · success. */
    fun aggregate(checks: List<Check>): String = when {
        checks.isEmpty() -> DomainContract.prChecksStateNone
        checks.any(::isFailing) -> DomainContract.prChecksStateFailure
        checks.any { it.status != DomainContract.prCheckStatusCompleted } -> DomainContract.prChecksStatePending
        else -> DomainContract.prChecksStateSuccess
    }

    /** `42s` · `3m 5s` · `1h 2m` (floored; negative = 0s). */
    fun formatDuration(ms: Long): String {
        val seconds = maxOf(0L, Math.floorDiv(ms, 1000L))
        if (seconds < 60) return "${seconds}s"
        val minutes = seconds / 60
        if (minutes < 60) return "${minutes}m ${seconds % 60}s"
        return "${minutes / 60}h ${minutes % 60}m"
    }

    private fun msOf(value: String?): Long? = value?.takeIf { it.isNotBlank() }?.let(WireTimestamps::parseEpochMs)

    /** completed = completedAt - startedAt; in_progress = now - startedAt;
     *  else null (queued, or no start). */
    fun checkDuration(check: Check, nowMs: Long): String? {
        val started = msOf(check.startedAt) ?: return null
        return when (check.status) {
            DomainContract.prCheckStatusCompleted -> msOf(check.completedAt)?.let { formatDuration(it - started) }
            DomainContract.prCheckStatusInProgress -> formatDuration(nowMs - started)
            else -> null
        }
    }

    fun checkGlyph(check: Check): Pair<Glyph, Tone> = when {
        check.status == DomainContract.prCheckStatusQueued -> Glyph.Queued to Tone.Muted
        check.status != DomainContract.prCheckStatusCompleted -> Glyph.Running to Tone.Muted
        isFailing(check) -> Glyph.Failed to Tone.Red
        check.conclusion == DomainContract.prCheckConclusionSuccess -> Glyph.Passed to Tone.Green
        else -> Glyph.Skipped to Tone.Muted
    }

    private val GLYPH_ORDER = mapOf(
        Glyph.Failed to 0,
        Glyph.Running to 1,
        Glyph.Queued to 2,
        Glyph.Passed to 3,
        Glyph.Skipped to 4,
    )

    /** The `…` menu's Checks rows: failed, running, queued, passed, skipped;
     *  ties by name (code-point order). */
    fun checksList(checks: List<Check>, nowMs: Long): List<ListRow> =
        checks.map { check ->
            val (glyph, tone) = checkGlyph(check)
            ListRow(
                name = check.name,
                glyph = glyph,
                tone = tone,
                duration = checkDuration(check, nowMs),
                url = check.detailsUrl?.takeIf { it.isNotEmpty() },
            )
        }.sortedWith(compareBy<ListRow> { GLYPH_ORDER.getValue(it.glyph) }.thenBy { it.name })

    /** The Checks submenu entry's glyph + value; null = no rows (no entry). */
    fun checksSummary(checks: List<Check>): Summary? {
        fun fill(template: String, count: Int) = template.replace("{count}", count.toString())
        return when (aggregate(checks)) {
            DomainContract.prChecksStateFailure ->
                Summary(Glyph.Failed, Tone.Red, fill(Copy.SUMMARY_FAILING, checks.count(::isFailing)))
            DomainContract.prChecksStatePending -> Summary(
                Glyph.Running,
                Tone.Muted,
                fill(Copy.SUMMARY_RUNNING, checks.count { it.status != DomainContract.prCheckStatusCompleted }),
            )
            DomainContract.prChecksStateSuccess -> Summary(Glyph.Passed, Tone.Green, fill(Copy.SUMMARY_PASSED, checks.size))
            else -> null
        }
    }

    /** pr-checks.json `specimen` (the styleguide's `pr-checks`; the test
     *  proves it equal to the fixture's). */
    const val SPECIMEN_NOW = "2026-11-01T12:10:00.000Z"
    val SPECIMEN: List<Check> = listOf(
        Check("lint", "completed", "success", "2026-11-01T12:00:00.000Z", "2026-11-01T12:00:38.000Z", "https://github.com/o/r/actions/runs/1/job/5"),
        Check("e2e", "in_progress", null, "2026-11-01T12:07:57.000Z", null, "https://github.com/o/r/actions/runs/1/job/2"),
        Check("docs", "completed", "skipped", null, null, null),
        Check("gate", "completed", "failure", "2026-11-01T12:00:00.000Z", "2026-11-01T12:04:12.000Z", "https://github.com/o/r/actions/runs/1/job/1"),
        Check("android", "queued", null, null, null, "https://github.com/o/r/actions/runs/1/job/3"),
    )
}
