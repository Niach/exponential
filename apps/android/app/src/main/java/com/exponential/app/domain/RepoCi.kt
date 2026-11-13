package com.exponential.app.domain

/**
 * #EXP-1617 E4 part 4 (fixtures/repo-ci.json: its `$comment` is the spec):
 * the repo page's Runs tab and the CI section of its Settings tab. Mirrors
 * the TS reference `packages/domain-contract/src/repo-ci.ts`; `RepoCiTest`
 * replays every case, comparing the WHOLE result. Run status = [CiRun.runStatus],
 * durations + glyphs = [PrChecks]'s.
 */
object RepoCi {

    /** repo-ci.json `copy`. */
    object Copy {
        object Runs {
            const val NO_REF = "—"
            const val COL_STATUS = "Status"
            const val COL_RUN = "Run"
            const val COL_COMMIT = "Commit"
            const val COL_REF = "Branch"
            const val COL_TRIGGER = "Trigger"
            const val COL_DURATION = "Duration"
            const val COL_STARTED = "Started"
        }

        val TRIGGERS: Map<String, String> = linkedMapOf(
            "push" to "Push",
            "pull_request" to "Pull request",
            "tag" to "Tag",
            "merge_train" to "Merge train",
            "manual" to "Manual",
        )

        object Ci {
            const val SECTION = "CI"
            const val NO_PIPELINE = "No runs yet. Add .exponential/ci.json to run CI here."
            const val REQUIRED_LABEL = "Required checks"
            const val REQUIRED_DESCRIPTION = "Checks that must pass before the merge train lands a pull request."
            const val REQUIRED_PLACEHOLDER = "Check name"
        }

        object Secrets {
            const val TITLE = "Secrets"
            const val COL_NAME = "Name"
            const val COL_UPDATED = "Updated"
            const val ADD = "Add secret"
            const val NAME = "Name"
            const val VALUE = "Value"
            const val SAVE = "Save"
            const val CANCEL = "Cancel"
            const val DELETE = "Delete"
            const val DELETE_TITLE = "Delete this secret?"
            const val DELETE_CONFIRM = "Delete"
            const val EMPTY = "No secrets yet."
            const val INVALID = "Use A–Z, 0–9 and _, not starting with a digit."
        }

        data class SwitchCopy(val label: String, val description: String)

        val SWITCHES: Map<Switch, SwitchCopy> = linkedMapOf(
            Switch.GithubStatuses to SwitchCopy("Report to GitHub", "Our checks also show on GitHub as commit statuses."),
            Switch.ActionsRunners to SwitchCopy(
                "GitHub Actions on team devices",
                "Team devices with a CI runner also run this repository's GitHub Actions jobs.",
            ),
        )
    }

    /** repo-ci.json `testIds`. */
    object TestIds {
        const val RUNS = "repo-runs"
        const val RUN_ROW = "repo-run"
        const val CI = "repo-ci"
        const val PIPELINES = "repo-ci-pipelines"
        const val REQUIRED = "repo-ci-required"
        const val SECRETS = "repo-ci-secrets"
        const val ADD_SECRET = "repo-ci-add-secret"
        const val GITHUB_STATUSES = "repo-ci-github-statuses"
        const val ACTIONS_RUNNERS = "repo-ci-actions-runners"
    }

    /** repo-ci.json `limits.runs`. */
    const val MAX_RUNS = 50

    /** The GitHub-only switches (wire = the `ci_config` key). */
    enum class Switch(val wire: String) { GithubStatuses("githubStatuses"), ActionsRunners("actionsRunners") }

    /** One `ci_runs` row as the Runs tab reads it. */
    data class Run(
        val id: String,
        val number: Int,
        val name: String,
        val sha: String = "",
        val ref: String? = null,
        val trigger: String = "",
        val state: String = "pending",
        val startedAt: String? = null,
        val completedAt: String? = null,
    )

    /** One `ci_jobs` row as the folds read it. */
    data class Job(val runId: String, val status: String = "", val conclusion: String? = null, val check: String = "")

    data class RunRow(
        val id: String,
        val identifier: String,
        val title: String,
        val status: CiRun.RunStatus,
        val statusLabel: String,
        val glyph: PrChecks.Glyph,
        val tone: PrChecks.Tone,
        val commit: String,
        val ref: String,
        val trigger: String,
        /** Phones: `ref · sha7 · trigger`. */
        val subtitle: String,
        val duration: String?,
        val startedAt: String?,
    )

    data class PipelineRow(val name: String, val runId: String, val state: String)

    private fun glyphOf(status: CiRun.RunStatus): Pair<PrChecks.Glyph, PrChecks.Tone> = when (status) {
        CiRun.RunStatus.Queued -> PrChecks.Glyph.Queued to PrChecks.Tone.Muted
        CiRun.RunStatus.Running -> PrChecks.Glyph.Running to PrChecks.Tone.Muted
        CiRun.RunStatus.Passed -> PrChecks.Glyph.Passed to PrChecks.Tone.Green
        CiRun.RunStatus.Failed -> PrChecks.Glyph.Failed to PrChecks.Tone.Red
        CiRun.RunStatus.Cancelled -> PrChecks.Glyph.Skipped to PrChecks.Tone.Muted
    }

    private fun msOf(value: String?): Long? = value?.takeIf { it.isNotBlank() }?.let(WireTimestamps::parseEpochMs)

    fun triggerLabel(trigger: String): String = Copy.TRIGGERS[trigger] ?: trigger

    /** The Runs tab: newest first, at most [MAX_RUNS]. */
    fun runRows(runs: List<Run>, jobs: List<Job>, nowMs: Long): List<RunRow> {
        val byRun = jobs.groupBy { it.runId }
        return runs.sortedByDescending { it.number }.take(MAX_RUNS).map { run ->
            val (status, _) = CiRun.runStatus(run.state, byRun[run.id].orEmpty().map { it.status to it.conclusion })
            val (glyph, tone) = glyphOf(status)
            val started = msOf(run.startedAt)
            val commit = run.sha.take(7)
            val ref = run.ref ?: Copy.Runs.NO_REF
            val trigger = triggerLabel(run.trigger)
            RunRow(
                id = run.id,
                identifier = "#${run.number}",
                title = run.name,
                status = status,
                statusLabel = CiRun.Copy.STATUS.getValue(status),
                glyph = glyph,
                tone = tone,
                commit = commit,
                ref = ref,
                trigger = trigger,
                subtitle = "$ref · $commit · $trigger",
                duration = started?.let { PrChecks.formatDuration((msOf(run.completedAt) ?: nowMs) - it) },
                startedAt = run.startedAt,
            )
        }
    }

    /** Code-point order (TS `<` on strings for the BMP). */
    private val byCodePoint = Comparator<String> { a, b ->
        val x = a.codePoints().toArray()
        val y = b.codePoints().toArray()
        for (i in 0 until minOf(x.size, y.size)) {
            val d = x[i] - y[i]
            if (d != 0) return@Comparator if (d < 0) -1 else 1
        }
        x.size - y.size
    }

    /** One row per pipeline name (by name): its newest run's state. */
    fun pipelineRows(runs: List<Run>): List<PipelineRow> {
        val newest = LinkedHashMap<String, Run>()
        for (run in runs) {
            val seen = newest[run.name]
            if (seen == null || run.number > seen.number) newest[run.name] = run
        }
        return newest.values.sortedWith(compareBy(byCodePoint) { it.name }).map { run ->
            PipelineRow(run.name, run.id, if (run.state == "success" || run.state == "failure") run.state else "pending")
        }
    }

    /** The checks of each pipeline's newest run, minus the required ones, sorted. */
    fun requiredSuggestions(runs: List<Run>, jobs: List<Job>, required: List<String>): List<String> {
        val newest = pipelineRows(runs).map { it.runId }.toSet()
        val names = jobs.filter { it.runId in newest }.map { it.check }.toMutableSet()
        names.removeAll(required.toSet())
        return names.sortedWith(byCodePoint)
    }

    private val SECRET_NAME = Regex("^[A-Z_][A-Z0-9_]*$")

    /** A secret name the server stores (an env var name, ≤ 128). */
    fun secretName(input: String): Boolean = input.length <= 128 && SECRET_NAME.matches(input)

    /** The GitHub-only switches of the CI section, in order. */
    fun switches(provider: String?): List<Switch> =
        if (provider == RepoPage.PROVIDER_GITHUB) listOf(Switch.GithubStatuses, Switch.ActionsRunners) else emptyList()

    // ── the styleguide specimen (sg_repo-runs, sg_repo-ci): runRows[0] ──

    const val SPECIMEN_NOW = "2026-11-13T10:30:00.000Z"
    val SPECIMEN_RUNS: List<Run> = listOf(
        Run("r10", 10, "ci", "aaaaaaa1111111", "main", "push", "success", "2026-11-13T09:00:00.000Z", "2026-11-13T09:04:30.000Z"),
        Run("r12", 12, "ci", "ccccccc3333333", "feat/x", "pull_request", "pending", "2026-11-13T10:25:00.000Z", null),
        Run("r11", 11, "nightly", "bbbbbbb2222222", null, "manual", "failure", "2026-11-13T10:00:00.000Z", "2026-11-13T10:01:00.000Z"),
        Run("r13", 13, "ci", "ddddddd4444444", "main", "merge_train", "pending", null, null),
        Run("r9", 9, "ci", "eeeeeee5555555", "v1.0", "tag", "failure", "2026-11-13T08:00:00.000Z", "2026-11-13T08:00:10.000Z"),
    )
    val SPECIMEN_JOBS: List<Job> = listOf(
        Job("r12", "in_progress", null, "test (linux)"),
        Job("r12", "completed", "success", "build"),
        Job("r11", "completed", "failure", "nightly"),
        Job("r13", "queued", null, "build"),
        Job("r9", "completed", "cancelled", "build"),
        Job("r10", "completed", "success", "build"),
    )
    val SPECIMEN_REQUIRED: List<String> = listOf("build")
    /** Secret names + their `updatedAt`. */
    val SPECIMEN_SECRETS: List<Pair<String, String>> = listOf(
        "NPM_TOKEN" to "2026-11-12T09:00:00.000Z",
        "SENTRY_AUTH" to "2026-11-06T09:00:00.000Z",
    )
}
