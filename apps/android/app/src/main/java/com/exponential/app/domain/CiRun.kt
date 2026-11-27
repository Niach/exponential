package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.longOrNull

/**
 * #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, fixtures/runner-settings.json):
 * the pure half of the CI run page and of a device's CI runner section.
 * Mirrors the TS reference `packages/domain-contract/src/ci-run.ts`;
 * `CiRunTest` replays both fixtures. Durations and glyphs are [PrChecks]'s.
 */
object CiRun {

    /** ci-run.json `copy`. */
    object Copy {
        const val TITLE = "Run #{number}"
        const val JOBS = "Jobs"
        const val STEPS = "Steps"
        const val LOG = "Log"
        const val RERUN_FAILED = "Re-run failed"
        const val CANCEL = "Cancel"
        const val RERUN_ALL = "Re-run all jobs"
        const val MORE = "More actions"
        val JOBS_CAPTION: Map<RunStatus, String> = linkedMapOf(
            RunStatus.Failed to "{count} failed",
            RunStatus.Running to "{count} running",
            RunStatus.Queued to "{count} queued",
            RunStatus.Passed to "{count} passed",
            RunStatus.Cancelled to "{count} skipped",
        )
        const val SUMMARY_FAILED = "Failed after {duration}"
        const val SUMMARY_RUNNING = "Running for {duration}"
        const val SUMMARY_PASSED = "Passed in {duration}"
        const val SUMMARY_QUEUED = "Waiting for a runner"
        const val SUMMARY_SKIPPED = "Skipped"
        const val SUMMARY_ON = " on {runner}"
        const val FAILURE_TITLE = "Failed in {job}"
        const val FIX = "Fix with an agent"
        const val OPEN_EXTERNAL = "Open on GitHub"
        const val BREADCRUMB_ROOT = "CI"
        const val PULL_REQUEST = "Pull request #{number}"
        val SOURCES: Map<String, String> = linkedMapOf("exponential" to "Exponential", "github" to "GitHub Actions")
        const val FIX_HEAD = "CI run #{number} ({name}) failed on {repository} at {ref} ({sha})."
        const val FIX_JOB = "Failing job: {job}."
        const val FIX_READ = "Read its whole log with exponential_repositories {op: \"run_log\", runId: \"{runId}\"}, find the cause, fix it on {ref} and push."
        const val FIX_TAIL = "The end of its log:"
        const val EMPTY_LOG = "No log yet."
        const val PIPELINE_ERROR = "The pipeline file did not plan"
        const val NONE = "—"
        val STATUS: Map<RunStatus, String> = mapOf(
            RunStatus.Queued to "Queued",
            RunStatus.Running to "Running",
            RunStatus.Passed to "Passed",
            RunStatus.Failed to "Failed",
            RunStatus.Cancelled to "Cancelled",
        )
        val REASONS: Map<String, String> = mapOf(
            "fork" to "Not run: a pull request from a fork",
            "paths" to "Skipped: no changed path matches",
            "needs" to "Skipped: a job it needs did not pass",
            "runner_lost" to "The runner stopped answering",
            "reused" to "Kept from the run it re-runs",
        )

        fun title(number: Int): String = TITLE.replace("{number}", number.toString())
    }

    /** ci-run.json `testIds`. */
    object TestIds {
        const val PAGE = "ci-run"
        const val JOB_ROW = "ci-run-job"
        const val STEPS = "ci-run-steps"
        const val LOG = "ci-run-log"
        const val RERUN = "ci-run-rerun"
        const val CANCEL = "ci-run-cancel"
        const val RERUN_ALL = "ci-run-rerun-all"
        const val FIX = "ci-run-fix"
        const val OPEN_EXTERNAL = "ci-run-open-external"
        const val BREADCRUMB = "ci-run-breadcrumb"
        const val SOURCE = "ci-run-source"
        const val CHIPS = "ci-run-chips"
        const val MORE = "ci-run-more"
        const val FAILURE = "ci-run-failure"
        const val JOB_CARD = "ci-run-job-card"
    }

    /** ci-run.json `liveTailMs`: the log re-read while incomplete. */
    const val LIVE_TAIL_MS = 2_000L

    /** ci-run.json `logMaxLines`. */
    const val LOG_MAX_LINES_WIDE = 32
    const val LOG_MAX_LINES_PHONE = 20

    enum class RunStatus(val wire: String) {
        Queued("queued"), Running("running"), Passed("passed"), Failed("failed"), Cancelled("cancelled")
    }

    enum class Tone(val wire: String) { Neutral("neutral"), Info("info"), Success("success"), Danger("danger") }

    enum class StepStatus(val wire: String) {
        Pending("pending"), Running("running"), Done("done"), Skipped("skipped"), Error("error")
    }

    /** One step of a job as the runner reports it (epoch ms). */
    data class Step(
        val name: String,
        val status: String,
        val conclusion: String?,
        val startedAt: Long?,
        val completedAt: Long?,
    )

    /** One `ci_jobs` row, projected ([runnerLabel] = its runner device's label). */
    data class Job(
        val id: String,
        val check: String,
        val position: Int,
        val status: String,
        val conclusion: String?,
        val reason: String? = null,
        val runnerLabel: String? = null,
        val startedAt: String? = null,
        val completedAt: String? = null,
        val steps: List<Step> = emptyList(),
    )

    /** One `ci_runs` row, projected. */
    data class Run(
        val id: String,
        val number: Int,
        val name: String,
        val state: String,
        val error: String?,
        val sha: String,
        val startedAt: String?,
        val completedAt: String?,
        /** #EXP-2172: the run's ref, its source and its GitHub handle. */
        val ref: String? = null,
        val provider: String = "exponential",
        val externalUrl: String? = null,
        val externalAccess: String? = null,
        /** #EXP-2172 redesign: what the run is about (else the pipeline's name). */
        val title: String? = null,
        val createdAt: String = "",
    )

    data class JobRow(
        val id: String,
        val title: String,
        val subtitle: String?,
        val glyph: PrChecks.Glyph,
        val tone: PrChecks.Tone,
        val duration: String?,
        /** #EXP-2172: the run page's status and its tone (`statusTones`). */
        val status: RunStatus = RunStatus.Queued,
        val statusTone: Tone = Tone.Neutral,
    )

    data class TraceItem(val title: String, val status: StepStatus, val description: String?)

    data class LogView(val code: String, val highlight: List<Int>)

    /** THE one default button ([primary], null = not ours to change), Fix
     *  with an agent, and the ⋯ menu (`rerunAll`, `openExternal`). */
    data class Actions(val primary: String?, val fix: Boolean, val menu: List<String>)

    /** One chip under the title (`dot` = the status chip's colour token). */
    data class Chip(val id: String, val label: String, val icon: String?, val dot: String?)

    data class Failure(val title: String, val message: String)

    data class PullRequest(val number: Int, val base: String)

    /** ci-run.json `fixLogLines`: the log tail Fix with an agent quotes. */
    const val FIX_LOG_LINES = 40

    private fun check(status: String, conclusion: String?, startedAt: String? = null, completedAt: String? = null, name: String = "") =
        PrChecks.Check(name = name, status = status, conclusion = conclusion, startedAt = startedAt, completedAt = completedAt)

    private fun msOf(value: String?): Long? = value?.takeIf { it.isNotBlank() }?.let(WireTimestamps::parseEpochMs)

    /** The run's status for its first tile. */
    fun runStatus(state: String, jobs: List<Pair<String, String?>>): Pair<RunStatus, Tone> {
        val status = when (state) {
            "pending" -> if (jobs.any { it.first == DomainContract.prCheckStatusInProgress }) RunStatus.Running else RunStatus.Queued
            "success" -> RunStatus.Passed
            else -> {
                val failing = jobs.filter { PrChecks.isFailing(check(it.first, it.second)) }
                if (failing.isNotEmpty() && failing.all { it.second == "cancelled" }) RunStatus.Cancelled else RunStatus.Failed
            }
        }
        return status to toneOf(status)
    }

    /** ci-page.json `statusTones`' tone as this page's [Tone]. */
    fun toneOf(status: RunStatus): Tone = when (CiPage.STATUS_TONES.getValue(status).second) {
        "info" -> Tone.Info
        "success" -> Tone.Success
        "danger" -> Tone.Danger
        else -> Tone.Neutral
    }

    private fun byPosition(jobs: List<Job>): List<Job> = jobs.sortedBy { it.position }

    private fun reasonText(reason: String?): String? = reason?.let { Copy.REASONS[it] }

    private fun glyphAndDuration(job: Job, nowMs: Long): Triple<PrChecks.Glyph, PrChecks.Tone, String?> {
        val c = check(job.status, job.conclusion, job.startedAt, job.completedAt, job.check)
        val (glyph, tone) = PrChecks.checkGlyph(c)
        return Triple(glyph, tone, PrChecks.checkDuration(c, nowMs))
    }

    /** The jobs in planner order. */
    fun jobRows(jobs: List<Job>, nowMs: Long): List<JobRow> = byPosition(jobs).map { job ->
        val (glyph, tone, duration) = glyphAndDuration(job, nowMs)
        JobRow(
            id = job.id,
            title = job.check,
            subtitle = if (job.status == DomainContract.prCheckStatusCompleted) reasonText(job.reason) else null,
            glyph = glyph,
            tone = tone,
            duration = duration,
            status = jobStatus(job.status, job.conclusion),
            statusTone = toneOf(jobStatus(job.status, job.conclusion)),
        )
    }

    /** A job's status in the run page's terms. */
    fun jobStatus(status: String, conclusion: String?): RunStatus = when {
        status == DomainContract.prCheckStatusQueued -> RunStatus.Queued
        status != DomainContract.prCheckStatusCompleted -> RunStatus.Running
        PrChecks.isFailing(check(status, conclusion)) && conclusion != "cancelled" -> RunStatus.Failed
        conclusion == "success" -> RunStatus.Passed
        else -> RunStatus.Cancelled
    }

    /** The Jobs section's caption: `1 failed · 3 passed · 1 skipped`. */
    fun jobsCaption(jobs: List<Pair<String, String?>>): String {
        val counts = jobs.groupingBy { jobStatus(it.first, it.second) }.eachCount()
        return Copy.JOBS_CAPTION.entries
            .filter { (counts[it.key] ?: 0) > 0 }
            .joinToString(" · ") { it.value.replace("{count}", counts.getValue(it.key).toString()) }
    }

    /** The selected job's card line: `Failed after 8m 25s on mint`. */
    fun jobSummary(job: Job, nowMs: Long): String {
        val status = jobStatus(job.status, job.conclusion)
        val duration = PrChecks.checkDuration(check(job.status, job.conclusion, job.startedAt, job.completedAt, job.check), nowMs)
        val head = when (status) {
            RunStatus.Queued -> return Copy.SUMMARY_QUEUED
            RunStatus.Cancelled -> return reasonText(job.reason) ?: Copy.SUMMARY_SKIPPED
            RunStatus.Failed -> Copy.SUMMARY_FAILED
            RunStatus.Running -> Copy.SUMMARY_RUNNING
            RunStatus.Passed -> Copy.SUMMARY_PASSED
        }.replace("{duration}", duration ?: Copy.NONE)
        val runner = job.runnerLabel
        return if (!runner.isNullOrEmpty()) head + Copy.SUMMARY_ON.replace("{runner}", runner) else head
    }

    private val DOT: Map<Tone, String> = mapOf(
        Tone.Success to "\$color.success",
        Tone.Danger to "\$color.destructive",
        Tone.Info to "\$color.info",
        Tone.Neutral to "\$color.mutedForeground",
    )

    /** The chip strip: status · ref (→ base) · sha · #PR · author · duration · runners. */
    fun chips(run: Run, jobs: List<Job>, pullRequest: PullRequest?, author: String?, nowMs: Long): List<Chip> {
        val (status, tone) = runStatus(run.state, jobs.map { it.status to it.conclusion })
        val started = msOf(run.startedAt)
        val end = msOf(run.completedAt) ?: nowMs
        val runners = jobs.mapNotNull { it.runnerLabel?.takeIf(String::isNotEmpty) }.distinct()
        val chips = mutableListOf(Chip("status", Copy.STATUS.getValue(status), null, DOT.getValue(tone)))
        run.ref?.let { ref -> chips += Chip("ref", if (pullRequest != null) "$ref → ${pullRequest.base}" else ref, "git-branch", null) }
        chips += Chip("sha", run.sha.take(7), "hash", null)
        if (pullRequest != null) chips += Chip("pr", "#${pullRequest.number}", "git-pull-request", null)
        if (!author.isNullOrEmpty()) chips += Chip("author", author, "user", null)
        if (started != null) chips += Chip("duration", PrChecks.formatDuration(end - started), "clock", null)
        if (runners.isNotEmpty()) chips += Chip("runner", runners.joinToString(", "), "server", null)
        return chips
    }

    /** `Failed in {job}` and the first error line of its log as read. */
    fun failureSummary(jobCheck: String?, log: String?): Failure? {
        if (jobCheck == null || log.isNullOrEmpty()) return null
        val view = logView(log)
        val first = view.highlight.firstOrNull() ?: return null
        val line = view.code.split('\n').getOrNull(first - 1).orEmpty()
        return Failure(Copy.FAILURE_TITLE.replace("{job}", jobCheck), line.removePrefix("Error: "))
    }

    fun isFailing(job: Job): Boolean = PrChecks.isFailing(check(job.status, job.conclusion))

    /** The selected job: the requested one, else the first failing, running, any. */
    fun defaultJob(jobs: List<Job>, requested: String?): String? {
        val ordered = byPosition(jobs)
        if (!requested.isNullOrEmpty() && ordered.any { it.id == requested }) return requested
        return (
            ordered.firstOrNull(::isFailing)
                ?: ordered.firstOrNull { it.status == DomainContract.prCheckStatusInProgress }
                ?: ordered.firstOrNull()
            )?.id
    }

    private fun stepStatus(step: Step): StepStatus = when {
        step.status == DomainContract.prCheckStatusQueued -> StepStatus.Pending
        step.status != DomainContract.prCheckStatusCompleted -> StepStatus.Running
        step.conclusion == "success" -> StepStatus.Done
        step.conclusion == "skipped" || step.conclusion == "neutral" -> StepStatus.Skipped
        else -> StepStatus.Error
    }

    /** A job's steps as trace items. */
    fun steps(steps: List<Step>, nowMs: Long): List<TraceItem> = steps.map { step ->
        val end = when (step.status) {
            DomainContract.prCheckStatusCompleted -> step.completedAt
            DomainContract.prCheckStatusInProgress -> nowMs
            else -> null
        }
        TraceItem(
            title = step.name,
            status = stepStatus(step),
            description = if (step.startedAt != null && end != null) PrChecks.formatDuration(end - step.startedAt) else null,
        )
    }

    private const val GROUP = "##[group]"
    private const val END_GROUP = "##[endgroup]"
    private const val ERROR = "##[error]"
    private const val WARNING = "##[warning]"

    /** The log as shown: the runner's markers become plain lines; error lines
     *  are highlighted (1-based, displayed lines). */
    fun logView(text: String): LogView {
        val lines = text.split('\n').toMutableList()
        if (lines.isNotEmpty() && lines.last().isEmpty()) lines.removeAt(lines.size - 1)
        val out = ArrayList<String>(lines.size)
        val highlight = ArrayList<Int>()
        for (line in lines) {
            when {
                line.startsWith(END_GROUP) -> Unit
                line.startsWith(GROUP) -> out += "▸ ${line.substring(GROUP.length)}"
                line.startsWith(ERROR) -> {
                    out += "Error: ${line.substring(ERROR.length)}"
                    highlight += out.size
                }
                line.startsWith(WARNING) -> out += "Warning: ${line.substring(WARNING.length)}"
                else -> out += line
            }
        }
        return LogView(out.joinToString("\n"), highlight)
    }

    /** Which buttons the run shows (#EXP-2172: a GitHub run re-runs and
     *  cancels only once its installation accepted Actions: write; it always
     *  offers Open on GitHub). */
    fun actions(
        state: String,
        @Suppress("UNUSED_PARAMETER") error: String?,
        provider: String? = null,
        externalAccess: String? = null,
        externalUrl: String? = null,
    ): Actions {
        val github = provider == "github"
        val writable = !github || externalAccess == "write"
        val primary = when {
            !writable -> null
            state == "pending" -> "cancel"
            state == "failure" -> "rerunFailed"
            else -> "rerunAll"
        }
        val menu = buildList {
            if (writable && primary != "rerunAll") add("rerunAll")
            if (github && !externalUrl.isNullOrEmpty()) add("openExternal")
        }
        return Actions(primary, fix = state == "failure", menu = menu)
    }

    fun actions(run: Run): Actions = actions(run.state, run.error, run.provider, run.externalAccess, run.externalUrl)

    /** The first message of Fix with an agent (the composer's text). */
    fun fixPrompt(run: Run, repository: String, job: String?, log: String?): String {
        val ref = run.ref ?: Copy.NONE
        fun fill(text: String) = text
            .replace("{number}", run.number.toString())
            .replace("{name}", run.name)
            .replace("{repository}", repository)
            .replace("{ref}", ref)
            .replace("{sha}", run.sha.take(7))
            .replace("{runId}", run.id)
            .replace("{job}", job.orEmpty())
        val lines = mutableListOf(fill(Copy.FIX_HEAD))
        if (!job.isNullOrEmpty()) lines += fill(Copy.FIX_JOB)
        lines += fill(Copy.FIX_READ)
        val code = if (log.isNullOrEmpty()) "" else logView(log).code
        if (code.isNotEmpty()) {
            lines += listOf("", Copy.FIX_TAIL, "```text") + code.split('\n').takeLast(FIX_LOG_LINES) + "```"
        }
        return lines.joinToString("\n")
    }

    /** `/t/<team>/ci/<runId>` (query + hash ignored) → (team, runId); null = another path. */
    fun parsePath(path: String): Pair<String, String>? {
        val bare = path.substringBefore('#').substringBefore('?')
        val parts = bare.split('/').filter { it.isNotEmpty() }
        return if (parts.size == 4 && parts[0] == "t" && parts[2] == "ci") parts[1] to parts[3] else null
    }

    /** The `job` query parameter of a run link, null when absent. */
    fun jobParam(path: String): String? {
        val query = path.substringBefore('#').substringAfter('?', "")
        return query.split('&').firstOrNull { it.startsWith("job=") }?.substringAfter("job=")?.takeIf { it.isNotEmpty() }
    }

    private val lenient = Json { ignoreUnknownKeys = true }

    private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull

    private fun JsonObject.long(key: String): Long? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.let {
        it.longOrNull ?: it.contentOrNull?.toDoubleOrNull()?.toLong()
    }

    /** The synced `ci_jobs.steps` jsonb text; anything unreadable = none. */
    fun parseSteps(json: String?): List<Step> {
        if (json.isNullOrBlank()) return emptyList()
        val array = runCatching { lenient.parseToJsonElement(json) as? JsonArray }.getOrNull() ?: return emptyList()
        return array.mapNotNull { element ->
            val o = element as? JsonObject ?: return@mapNotNull null
            Step(
                name = o.str("name") ?: return@mapNotNull null,
                status = o.str("status") ?: DomainContract.prCheckStatusQueued,
                conclusion = o.str("conclusion"),
                startedAt = o.long("startedAt"),
                completedAt = o.long("completedAt"),
            )
        }
    }

    /** ci-run.json `specimen` (the styleguide's `ci-run`; the test proves it
     *  equal to the fixture's). */
    const val SPECIMEN_NOW = "2026-11-13T10:14:00.000Z"
    val SPECIMEN_RUN = Run(
        id = "r12",
        number = 12,
        name = "ci",
        state = "failure",
        error = null,
        sha = "8f3c2a9d1e7b4c6a5f0e9d8c7b6a5f4e3d2c1b0a",
        startedAt = "2026-11-13T10:00:00.000Z",
        completedAt = "2026-11-13T10:12:40.000Z",
        ref = "exp/APP-33",
        provider = "exponential",
        title = "Mirror prunes deleted branches",
        createdAt = "2026-11-13T09:59:50.000Z",
    )
    const val SPECIMEN_REPOSITORY = "acme/engine"
    val SPECIMEN_PULL_REQUEST = PullRequest(213, "main")
    const val SPECIMEN_AUTHOR = "Mara Lind"
    val SPECIMEN_JOBS: List<Job> = listOf(
        Job("j1", "build", 0, "completed", "success", null, "mint", "2026-11-13T10:00:00.000Z", "2026-11-13T10:04:10.000Z"),
        Job(
            "j2", "test (linux)", 1, "completed", "failure", null, "mint", "2026-11-13T10:04:15.000Z", "2026-11-13T10:12:40.000Z",
            steps = listOf(
                Step("Checkout", "completed", "success", 1794564255000, 1794564259000),
                Step("Install", "completed", "success", 1794564259000, 1794564301000),
                Step("Unit tests", "completed", "failure", 1794564301000, 1794564760000),
                Step("Upload report", "completed", "skipped", null, null),
            ),
        ),
        Job("j3", "deploy", 2, "completed", "skipped", "needs", null, null, "2026-11-13T10:12:40.000Z"),
    )
    const val SPECIMEN_SELECTED = "j2"
    const val SPECIMEN_LOG = "##[group]Checkout 8f3c2a9d1e7b\n##[endgroup]\n##[group]Install\nbun install v1.3.14\n" +
        "1652 packages installed [611.00ms]\n##[endgroup]\n##[group]Unit tests\n" +
        "\u001b[32m✓\u001b[0m src/lib/ci/state.test.ts (6 tests) 12ms\n" +
        "\u001b[32m✓\u001b[0m src/lib/ci/logs.test.ts (3 tests) 9ms\n" +
        "\u001b[31m×\u001b[0m src/lib/ci/train.test.ts > squashes each PR in order\n" +
        "  AssertionError: expected 'conflict' to be 'composed'\n Test Files  1 failed | 2 passed (3)\n" +
        "##[error]exited with code 1\n##[endgroup]\n"
}

/**
 * #EXP-1617 / #EXP-1804 (fixtures/runner-settings.json): the CI runner
 * section of a device's settings. Mirrors the bottom half of `ci-run.ts`.
 */
object RunnerSettings {

    /** runner-settings.json `copy`. */
    object Copy {
        const val SECTION = "CI runner"
        const val ENABLED_LABEL = "Run CI jobs"
        const val ENABLED_DESCRIPTION = "Runs your team's pipelines on this device."
        const val LABELS_LABEL = "Labels"
        const val LABELS_PLACEHOLDER = "gpu, big"
        const val LABELS_INVALID = "Use lowercase letters, digits, dots, dashes or underscores."
        const val SLOTS = "Parallel jobs"
        const val TIMEOUT = "Job timeout"
        const val MEMORY = "Memory per job"
        const val PAUSE = "Pause for games and full-screen apps"
        const val RECENT = "Recent jobs"
        const val NO_JOBS = "No jobs yet."
        const val RUN_SUBTITLE = "Run #{number} · {repo}"
        val NOTICE: Map<Notice, NoticeCopy> = mapOf(
            Notice.Unsupported to NoticeCopy(
                "info",
                "Not on this computer yet",
                "A job could read this app's data on macOS and Windows. Run CI on Linux or a server.",
            ),
            Notice.Phone to NoticeCopy(
                "info",
                "Set up here, runs on the device",
                "A phone never runs CI jobs: this sets up the device's runner.",
            ),
            Notice.Update to NoticeCopy("warning", "Update this device", "Its app is too old to run CI jobs."),
        )
    }

    data class NoticeCopy(val type: String, val title: String, val message: String)

    /** runner-settings.json `testIds`. */
    object TestIds {
        const val SECTION = "runner-settings"
        const val SWITCH = "runner-enabled"
        const val LABELS = "runner-labels"
        const val SLOTS = "runner-slots"
        const val TIMEOUT = "runner-timeout"
        const val RECENT = "runner-recent"
        const val MEMORY = "runner-memory"
        const val PAUSE = "runner-pause"
    }

    /** `devices.runner`; [DEFAULTS] fills a null one. */
    data class Settings(
        val enabled: Boolean,
        val labels: List<String>,
        val slots: Int,
        val timeoutMin: Int,
        /** #EXP-2193: the job's memory cap (MB) and pause-while-a-game-runs. */
        val memoryMaxMb: Int = 8192,
        val pauseWhenBusy: Boolean = true,
    )

    val DEFAULTS = Settings(enabled = false, labels = emptyList(), slots = 1, timeoutMin = 60, memoryMaxMb = 8192, pauseWhenBusy = true)
    const val MAX_LABELS = 16
    const val MIN_SLOTS = 1
    const val MAX_SLOTS = 16
    const val MAX_RECENT = 5

    /** runner-settings.json `timeoutOptions` (value, label). */
    val TIMEOUT_OPTIONS: List<Pair<String, String>> = listOf(
        "15" to "15 min",
        "30" to "30 min",
        "60" to "1 h",
        "120" to "2 h",
        "240" to "4 h",
        "360" to "6 h",
    )

    /** runner-settings.json `memoryOptions` (value, label). */
    val MEMORY_OPTIONS: List<Pair<String, String>> = listOf(
        "2048" to "2 GB",
        "4096" to "4 GB",
        "8192" to "8 GB",
        "16384" to "16 GB",
        "32768" to "32 GB",
        "65536" to "64 GB",
    )

    enum class Notice(val wire: String) { Unsupported("unsupported"), Phone("phone"), Update("update") }

    private val LABEL = Regex("^[a-z0-9][a-z0-9._-]{0,31}$")

    /** A typed label as stored, null = refused. */
    fun normalizeLabel(input: String): String? {
        val label = input.trim().lowercase()
        return label.takeIf { LABEL.matches(it) }
    }

    private val PHONES = setOf("ios", "android", "web-mobile")

    /** #EXP-2193: the desktop app on macOS or Windows gets no runner yet (a
     *  job could read the app's data there); a server daemon keeps it. */
    fun unsupported(kind: String?, platform: String?): Boolean =
        kind == "desktop" && platform in setOf("macos", "darwin", "windows")

    /** The section's notice: an unsupported desktop, phones never run jobs,
     *  an old app cannot. */
    fun notice(client: String, caps: List<String>, kind: String?, platform: String?): Notice? = when {
        unsupported(kind, platform) -> Notice.Unsupported
        client in PHONES -> Notice.Phone
        "ci-runner" !in caps -> Notice.Update
        else -> null
    }

    /** The OS label a runner always has. */
    fun osLabel(platform: String?): String? = when (platform) {
        "linux" -> "linux"
        "macos", "darwin" -> "mac"
        "windows" -> "windows"
        else -> null
    }

    /** A job this runner ran (`ci_jobs` + its run's number + repo). */
    data class RunnerJob(
        val id: String,
        val runnerDeviceId: String?,
        val check: String,
        val runNumber: Int,
        val repo: String,
        val status: String,
        val conclusion: String?,
        val startedAt: String?,
        val completedAt: String?,
        /** The run it belongs to (opens the run page); not part of the rule. */
        val runId: String = "",
    )

    /** The runner's newest jobs. */
    fun recentJobs(deviceId: String, jobs: List<RunnerJob>, nowMs: Long): List<CiRun.JobRow> = jobs
        .filter { it.runnerDeviceId == deviceId }
        .sortedByDescending { it.startedAt?.takeIf(String::isNotBlank)?.let(WireTimestamps::parseEpochMs) ?: Long.MIN_VALUE }
        .take(MAX_RECENT)
        .map { job ->
            val check = PrChecks.Check(job.check, job.status, job.conclusion, job.startedAt, job.completedAt)
            val (glyph, tone) = PrChecks.checkGlyph(check)
            CiRun.JobRow(
                id = job.id,
                title = job.check,
                subtitle = Copy.RUN_SUBTITLE.replace("{number}", job.runNumber.toString()).replace("{repo}", job.repo),
                glyph = glyph,
                tone = tone,
                duration = PrChecks.checkDuration(check, nowMs),
            )
        }

    private val lenient = Json { ignoreUnknownKeys = true }

    /** The synced `devices.runner` jsonb text; null = never set up. */
    fun parse(json: String?): Settings? {
        if (json.isNullOrBlank()) return null
        val o = runCatching { lenient.parseToJsonElement(json) as? JsonObject }.getOrNull() ?: return null
        val labels = (o["labels"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty()
        return Settings(
            enabled = (o["enabled"] as? JsonPrimitive)?.booleanOrNull ?: DEFAULTS.enabled,
            labels = labels,
            slots = (o["slots"] as? JsonPrimitive)?.intOrNull ?: DEFAULTS.slots,
            timeoutMin = (o["timeoutMin"] as? JsonPrimitive)?.intOrNull ?: DEFAULTS.timeoutMin,
            memoryMaxMb = (o["memoryMaxMb"] as? JsonPrimitive)?.intOrNull ?: DEFAULTS.memoryMaxMb,
            pauseWhenBusy = (o["pauseWhenBusy"] as? JsonPrimitive)?.booleanOrNull ?: DEFAULTS.pauseWhenBusy,
        )
    }

    /** runner-settings.json `specimen`: mint's enabled runner, owner viewing,
     *  with recentJobs[0]'s jobs (and its now). */
    const val SPECIMEN_DEVICE_ID = "d1"
    const val SPECIMEN_LABEL = "mint"
    const val SPECIMEN_PLATFORM = "linux"
    const val SPECIMEN_KIND = "desktop"
    val SPECIMEN_CAPS = listOf("ci-runner")
    val SPECIMEN_SETTINGS = Settings(enabled = true, labels = listOf("gpu"), slots = 2, timeoutMin = 60)
    const val SPECIMEN_NOW = "2026-11-13T10:30:00.000Z"
    val SPECIMEN_JOBS: List<RunnerJob> = listOf(
        RunnerJob("a", "d1", "build", 10, "niach/exponential", "completed", "success", "2026-11-13T09:00:00.000Z", "2026-11-13T09:02:00.000Z"),
        RunnerJob("b", "d2", "build", 11, "niach/exponential", "completed", "success", "2026-11-13T09:10:00.000Z", "2026-11-13T09:12:00.000Z"),
        RunnerJob("c", "d1", "test (linux)", 12, "niach/exponential", "in_progress", null, "2026-11-13T10:20:00.000Z", null),
        RunnerJob("d", "d1", "lint", 12, "niach/exponential", "completed", "failure", "2026-11-13T10:19:00.000Z", "2026-11-13T10:19:45.000Z"),
        RunnerJob("e", "d1", "e2e", 9, "niach/spezl", "completed", "success", "2026-11-13T08:00:00.000Z", "2026-11-13T08:30:00.000Z"),
        RunnerJob("f", "d1", "docs", 8, "niach/spezl", "completed", "success", "2026-11-13T07:00:00.000Z", "2026-11-13T07:00:20.000Z"),
        RunnerJob("g", "d1", "old", 7, "niach/spezl", "completed", "success", "2026-11-13T06:00:00.000Z", "2026-11-13T06:00:20.000Z"),
    )
}

/** #EXP-2172: `/ci` (query + hash ignored), THE CI page. */
fun isCiPagePath(path: String): Boolean =
    path.substringBefore('#').substringBefore('?').trimEnd('/') == "/ci"

/** #EXP-1804: [href] is a run page of OUR CI on this instance ([origin]):
 *  `/t/<team>/ci/<runId>` (a check's `details_url`), so it opens in the app. */
fun isCiRunLink(href: String, origin: String): Boolean {
    val link = classifyAppLink(href, origin)
    return link is AppLink.App && CiRun.parsePath(link.path) != null
}
