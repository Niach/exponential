package com.exponential.app.domain

/**
 * #EXP-1710 (fixtures/merge-train.json, epic #EXP-1704): the merge train's
 * client half ×Android — the header STATUS pill, the `…` menu entries, the
 * Needs you stuck row and the `pr_merge` event copy. Mirrors the TS reference
 * `packages/domain-contract/src/merge-train.ts` (NEEDS YOU, THE STATUS PILL,
 * THE `…` MENU); `MergeTrainTest` replays the fixture's `statusPill`, `menu`
 * and `stuckRow` cases. The server (#EXP-1709) owns every state: clients only
 * read the synced `pull_requests.merge_*` columns and `teams.merge_mode`.
 */
object MergeTrain {

    /** merge-train.json `limits` (the ones a client reads). */
    object Limits {
        const val BOUNCES = 3
        const val OFFLINE_HOURS = 2
    }

    /** merge-train.json `copy`. */
    object Copy {
        object Pill {
            const val DRAFT = "Draft"
            const val CHECKS_RUNNING = "Checks running"
            const val SESSION_WORKING = "Session working"
            const val QUEUED = "Queued"
            const val QUEUED_AT = "Queued · {position}"
            const val TESTING = "Testing"
            const val TESTING_WITH = "Testing with {count}"
            const val MERGED = "Merged"
            const val CONFLICT = "Conflict"
            const val FAILING = "Checks failing"
            const val ON_HOLD = "On hold"
            const val STUCK = "Auto-merge stuck"
            const val NEEDS_APPROVAL = "Needs approval"
        }

        object Menu {
            const val MERGE_NOW = "Merge now"
            const val HOLD = "Hold auto-merge"
            const val RESUME = "Resume auto-merge"
        }

        object NeedsYou {
            const val TITLE = "Auto-merge stuck on #{number}"
            const val BOUNCES = "{count} bounces"
            const val TIMEOUT = "24 hours"
            const val OFFLINE = "Device offline"
            const val RETRY = "Retry"
            const val MERGE_NOW = "Merge now"
            const val CLOSE = "Close PR"
        }

        object Settings {
            const val LABEL = "Merge"
            const val AUTO = "Automatic"
            const val APPROVAL = "After approval"
        }

        /** `pr_merge` event lines by `mergeOutcome`. */
        val EVENTS: Map<String, String> = mapOf(
            "merged" to "Merged PR {pr}",
            "conflict" to "Conflict on PR {pr}",
            "failing" to "Checks failing on PR {pr}",
            "held" to "Auto-merge stopped on PR {pr}",
        )
    }

    /** merge-train.json `testIds`. */
    object TestIds {
        const val STATUS_PILL = "pr-merge-status"
        const val MERGE_NOW = "pr-merge-now"
        const val HOLD = "pr-merge-hold"
        const val RESUME = "pr-merge-resume"
        const val STUCK_ROW = "needs-you-merge-stuck"
        const val SETTINGS_ROW = "settings-merge-mode"
    }

    /** contract teamMergeMode. */
    const val MODE_AUTO = "auto"
    const val MODE_APPROVAL = "approval"

    /** The `pr_merge` event template for [outcome]; null = an unknown or
     *  missing outcome (the caller keeps its fallback copy). */
    fun eventTemplate(outcome: String?): String? = outcome?.let { Copy.EVENTS[it] }

    private const val HOUR_MS = 3_600_000L

    // ── NEEDS YOU ──────────────────────────────────────────────────────────

    enum class StuckAction(val wire: String) { Retry("retry"), MergeNow("merge-now"), ClosePr("close-pr") }

    data class StuckRow(val title: String, val meta: String, val actions: List<StuckAction>)

    /** The opener's device row as [offlineSince] reads it: THE presence +
     *  its last heartbeat (null = none synced). */
    data class OpenerDevice(val online: Boolean, val lastSeenAt: String?)

    /**
     * merge-train.json `offlineSince` (TS `offlineSince`): [stuckRow]'s
     * `deviceOfflineSince`. [opener] = the PR has an opener (its `sessionId`
     * session, else its root) with a device; null while that device is
     * online or there is none; else its last heartbeat, and an UNKNOWN
     * last-seen (no row synced, or `lastSeenAt` null) = the PR's
     * [updatedAt] (offline since the PR went conflict/failing), never online.
     */
    fun offlineSince(opener: Boolean, device: OpenerDevice?, updatedAt: String): String? {
        if (!opener || device?.online == true) return null
        return device?.lastSeenAt ?: updatedAt
    }

    /**
     * The Needs you row of a stuck PR, or null: `held` by the limits, or a
     * `conflict`/`failing` whose opener's device is offline 2 h+
     * ([deviceOfflineSince] = when it went offline, null while online). A
     * person's Hold is a decision already made: never a row. Actions: Retry
     * (= Resume), Merge now (owners), Close PR.
     */
    fun stuckRow(
        number: Long,
        state: String,
        holdReason: String?,
        deviceOfflineSince: String?,
        nowMs: Long,
        owner: Boolean,
    ): StuckRow? {
        val copy = Copy.NeedsYou
        val offlineMs = deviceOfflineSince?.let(WireTimestamps::parseEpochMs)
        val meta = when {
            state == "held" && holdReason == "bounces" -> copy.BOUNCES.replace("{count}", Limits.BOUNCES.toString())
            state == "held" && holdReason == "timeout" -> copy.TIMEOUT
            (state == "conflict" || state == "failing") && offlineMs != null &&
                nowMs - offlineMs >= Limits.OFFLINE_HOURS * HOUR_MS -> copy.OFFLINE
            else -> null
        } ?: return null
        return StuckRow(
            title = copy.TITLE.replace("{number}", number.toString()),
            meta = meta,
            actions = if (owner) {
                listOf(StuckAction.Retry, StuckAction.MergeNow, StuckAction.ClosePr)
            } else {
                listOf(StuckAction.Retry, StuckAction.ClosePr)
            },
        )
    }

    // ── THE STATUS PILL ────────────────────────────────────────────────────

    data class StatusPillInput(
        /** contract prMergeState. */
        val state: String,
        /** What a `checks` PR still waits for (`draft` | `failing` | `checks` | `session` | `approval`). */
        val wait: String?,
        val position: Int? = null,
        /** PRs in the testing batch, this one included. */
        val batchSize: Int? = null,
        /** `person` | `bounces` | `timeout`. */
        val holdReason: String? = null,
        val stack: Boolean,
        /** The PR's check rows (a status pill with none is readonly). */
        val checks: Int,
    )

    /** [glyph] = an icons.json concept (`spinner` = THE Spinner); [tone]
     *  `red` tints only the glyph; [tap] `approve` = the merge confirm then
     *  the queue, `checks` = the Checks submenu, null = readonly; [confirm]
     *  `merge` | `stack` on `approve`. */
    data class StatusPillView(
        val label: String,
        val glyph: String,
        val tone: String,
        val primary: Boolean,
        val tap: String?,
        val confirm: String? = null,
    )

    /** merge-train.json `statusPill` inputs, case for case in fixture order
     *  (the styleguide `pr-checks` specimen draws every one; MergeTrainTest
     *  proves the list equal to the fixture's). */
    val SPECIMEN: List<StatusPillInput> = listOf(
        StatusPillInput(state = "draft", wait = "draft", position = null, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "checks", wait = "checks", position = null, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "checks", wait = "session", position = null, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "queued", wait = null, position = 2, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "queued", wait = null, position = 1, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "queued", wait = null, position = 11, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "queued", wait = null, position = 23, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "testing", wait = null, position = null, batchSize = 4, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "testing", wait = null, position = null, batchSize = 1, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "merged", wait = null, position = null, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "conflict", wait = null, position = null, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "failing", wait = "failing", position = null, batchSize = null, holdReason = null, stack = false, checks = 3),
        StatusPillInput(state = "held", wait = null, position = null, batchSize = null, holdReason = "person", stack = false, checks = 3),
        StatusPillInput(state = "held", wait = null, position = null, batchSize = null, holdReason = "bounces", stack = false, checks = 3),
        StatusPillInput(state = "queued", wait = null, position = 3, batchSize = null, holdReason = null, stack = false, checks = 0),
        StatusPillInput(state = "checks", wait = "approval", position = null, batchSize = null, holdReason = null, stack = false, checks = 0),
        StatusPillInput(state = "checks", wait = "approval", position = null, batchSize = null, holdReason = null, stack = true, checks = 3),
    )

    /** 1st 2nd 3rd 4th 11th 12th 13th 21st. */
    fun ordinal(n: Int): String {
        val rem100 = n % 100
        if (rem100 in 11..13) return "${n}th"
        val suffix = when (n % 10) {
            1 -> "st"
            2 -> "nd"
            3 -> "rd"
            else -> "th"
        }
        return "$n$suffix"
    }

    fun statusPill(input: StatusPillInput): StatusPillView {
        val copy = Copy.Pill
        fun status(label: String, glyph: String, tone: String = "muted") = StatusPillView(
            label = label,
            glyph = glyph,
            tone = tone,
            primary = false,
            tap = if (input.checks > 0) "checks" else null,
        )
        return when (input.state) {
            "draft" -> status(copy.DRAFT, "pr-draft")
            "queued" -> status(
                input.position?.takeIf { it > 0 }?.let { copy.QUEUED_AT.replace("{position}", ordinal(it)) } ?: copy.QUEUED,
                "ci-queued",
            )
            "testing" -> {
                val others = (input.batchSize ?: 1) - 1
                status(if (others > 0) copy.TESTING_WITH.replace("{count}", others.toString()) else copy.TESTING, "spinner")
            }
            "merged" -> status(copy.MERGED, "pr-merged")
            "conflict" -> status(copy.CONFLICT, "pr-conflict", "red")
            "failing" -> status(copy.FAILING, "ci-failed", "red")
            "held" -> if (input.holdReason == "person") status(copy.ON_HOLD, "pr-hold") else status(copy.STUCK, "pr-hold", "red")
            // `checks` (and an unknown state from a newer server).
            else -> when (input.wait) {
                "approval" -> StatusPillView(
                    label = copy.NEEDS_APPROVAL,
                    glyph = "pr-merged",
                    tone = "muted",
                    primary = true,
                    tap = "approve",
                    confirm = if (input.stack) "stack" else "merge",
                )
                "session" -> status(copy.SESSION_WORKING, "spinner")
                else -> status(copy.CHECKS_RUNNING, "spinner")
            }
        }
    }

    /** Web `prMergeState`: the PR's own state wins (merged → `merged`,
     *  draft → `draft`), else its synced `merge_state`. */
    fun prMergeState(state: String, mergeState: String): String = when (state) {
        "merged" -> "merged"
        "draft" -> "draft"
        else -> mergeState
    }

    /** A viewer (not a member) cannot approve: `Needs approval` reads as a
     *  plain status (same label and glyph, not primary, tap = the Checks
     *  submenu when the PR has rows, else readonly). */
    fun forViewer(view: StatusPillView, member: Boolean, checks: Int): StatusPillView =
        if (member || view.tap != "approve") view
        else view.copy(primary = false, tap = if (checks > 0) "checks" else null, confirm = null)

    // ── THE `…` MENU ───────────────────────────────────────────────────────

    enum class MenuEntry(val wire: String) {
        Checks("checks"),
        MergeNow("merge-now"),
        Hold("hold"),
        Resume("resume"),
        ClosePr("close-pr"),
    }

    /**
     * The header `…` (the chat menu): checks
     * (any viewer, any PR with rows), Merge now (owners; a live PR not draft,
     * merged or conflict), Hold | Resume auto-merge (members; a live PR not
     * draft or merged: Resume while held), Close PR (members, live PR).
     * [state] = contract prState of the guide PR (null = none).
     */
    fun mergeMenuEntries(
        state: String?,
        member: Boolean,
        owner: Boolean,
        checks: Int,
        mergeState: String?,
    ): List<MenuEntry> {
        if (state == null) return emptyList()
        val entries = mutableListOf<MenuEntry>()
        if (checks > 0) entries += MenuEntry.Checks
        val live = state == "open" || state == "draft"
        if (!live || !member) return entries
        val ms = mergeState ?: "checks"
        if (owner && ms != "draft" && ms != "merged" && ms != "conflict") entries += MenuEntry.MergeNow
        if (ms != "draft" && ms != "merged") entries += if (ms == "held") MenuEntry.Resume else MenuEntry.Hold
        entries += MenuEntry.ClosePr
        return entries
    }

    /** `Testing with N`'s batch: the synced PRs sharing [trainId] (null = none). */
    fun batchSize(trainId: String?, trainIds: List<String?>): Int? =
        trainId?.let { id -> trainIds.count { it == id } }
}
