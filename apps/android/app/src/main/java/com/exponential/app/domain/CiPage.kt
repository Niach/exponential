package com.exponential.app.domain

/**
 * #EXP-2172 (fixtures/ci-page.json): THE CI page (one root destination across
 * every member team) and its badge: our runs and GitHub Actions runs in one
 * list, the merge train's batches, the runner slots. Mirrors the TS reference
 * `packages/domain-contract/src/ci-page.ts`; `CiPageTest` replays the fixture.
 */
object CiPage {

    /** ci-page.json `copy`. */
    object Copy {
        const val TITLE = "CI"
        const val TAB_RUNS = "Runs"
        const val TAB_TRAIN = "Merge train"
        const val TAB_RUNNERS = "Runners"
        const val TILE_RUNNING = "Running"
        const val TILE_QUEUED = "Queued"
        const val TILE_FAILED_TODAY = "Failed today"
        const val TILE_RUNNERS = "Runners"
        const val TILE_RUNNERS_VALUE = "{online} of {total} online"
        val FILTERS: Map<String, String> = linkedMapOf("all" to "All", "running" to "Running", "failed" to "Failed", "mine" to "Mine")
        const val REPOSITORIES_ALL = "All repositories"
        const val SOURCES_ALL = "Every source"
        val SOURCES: Map<String, String> = linkedMapOf("exponential" to "Exponential", "github" to "GitHub Actions")
        const val COL_STATUS = "Status"
        const val COL_RUN = "Run"
        const val COL_REPOSITORY = "Repository"
        const val COL_SOURCE = "Source"
        const val COL_REF = "Branch"
        const val COL_TRIGGER = "Trigger"
        const val COL_DURATION = "Duration"
        const val COL_STARTED = "Started"
        const val RUNS_SECTION = "Runs"
        const val TRAIN_SECTION = "Merge train"
        const val PULL_REQUEST = "Pull request #{number}"
        val TRIGGERS: Map<String, String> = linkedMapOf(
            "push" to "Push",
            "pull_request" to "Pull request",
            "tag" to "Tag",
            "merge_train" to "Merge train",
            "manual" to "Manual",
            "schedule" to "Schedule",
        )
        const val UNKNOWN_REPOSITORY = "Repository"
        const val TRAIN_BATCH = "Testing {count} pull requests together"
        const val RUNNERS_BUSY = "{busy} of {slots} busy"
        const val RUNNERS_QUEUE = "{count} queued"
        const val RUNNERS_OFFLINE_SINCE = "Last seen {time}"
        const val RUNNERS_NO_LABELS = "No labels"
        const val EMPTY_RUNS = "No runs yet"
        const val EMPTY_FILTERED = "No runs match"
        const val EMPTY_TRAIN = "Nothing in the merge train"
        const val EMPTY_RUNNERS = "No runners"
    }

    /** ci-page.json `testIds`. */
    object TestIds {
        const val PAGE = "ci-page"
        const val TILES = "ci-tiles"
        const val TABS = "ci-tabs"
        const val FILTERS = "ci-filters"
        const val STATUS_FILTER = "ci-filter-status"
        const val REPO_FILTER = "ci-filter-repo"
        const val SOURCE_FILTER = "ci-filter-source"
        const val RUNS = "ci-runs"
        const val RUN_ROW = "ci-run-row"
        const val TRAIN = "ci-train"
        const val TRAIN_ROW = "ci-train-row"
        const val RUNNERS = "ci-runners"
        const val RUNNER_ROW = "ci-runner-row"
    }

    /** ci-page.json `limits`. */
    const val LIMIT_RUNS = 100
    const val RUNNER_ONLINE_MS = 120_000L

    data class Run(
        val id: String,
        val repositoryId: String,
        val number: Int,
        val name: String,
        val sha: String,
        val ref: String?,
        val trigger: String,
        val state: String,
        val pullRequestId: String?,
        val createdByUserId: String?,
        val provider: String,
        val startedAt: String?,
        val completedAt: String?,
        val createdAt: String,
    )

    data class Job(
        val runId: String,
        val status: String,
        val conclusion: String?,
        val runsOn: List<String>,
        val runnerDeviceId: String?,
        val provider: String,
    )

    data class Repo(val id: String, val fullName: String)

    data class Pr(
        val id: String,
        val number: Int,
        val title: String,
        val repositoryId: String,
        val rootSessionId: String?,
        val base: String,
        val mergeState: String?,
        val mergePosition: Int?,
        val mergeTrainId: String?,
    )

    /** A session row: its starter. */
    data class Session(val id: String, val userId: String)

    data class Runner(
        val id: String,
        val label: String,
        val platform: String?,
        val labels: List<String>,
        val slots: Int,
        val lastSeenAt: String?,
    )

    data class Input(
        val runs: List<Run> = emptyList(),
        val jobs: List<Job> = emptyList(),
        val repos: List<Repo> = emptyList(),
        val pullRequests: List<Pr> = emptyList(),
        val sessions: List<Session> = emptyList(),
        val runners: List<Runner> = emptyList(),
        val viewerId: String = "",
        val now: String = "",
    )

    data class RunFilter(val status: String = "all", val repositoryId: String? = null, val source: String? = null)

    data class RunListRow(
        val id: String,
        val identifier: String,
        val title: String,
        val status: CiRun.RunStatus,
        val statusLabel: String,
        val glyph: PrChecks.Glyph,
        val tone: PrChecks.Tone,
        val repositoryId: String,
        val repository: String,
        val source: String,
        val sourceLabel: String,
        val ref: String,
        val trigger: String,
        val duration: String?,
        val startedAt: String?,
        val subtitle: String,
    )

    data class Option(val label: String, val value: String)

    data class Tile(val label: String, val value: String, val tone: String, val delta: String?)

    data class TrainRow(
        val id: String,
        val identifier: String,
        val title: String,
        val subtitle: String?,
        val glyph: PrChecks.Glyph,
        val badge: String,
        val pullRequestId: String,
    )

    data class TrainGroup(val repositoryId: String, val title: String, val caption: String, val rows: List<TrainRow>)

    data class RunnerRow(
        val id: String,
        val title: String,
        val subtitle: String,
        val online: Boolean,
        val busy: String?,
        val queue: String?,
        val offlineSince: String?,
    )

    data class TabCounts(val train: Int, val runners: Int)

    private fun msOf(value: String?): Long? = value?.takeIf { it.isNotBlank() }?.let(WireTimestamps::parseEpochMs)

    private fun glyph(status: CiRun.RunStatus): Pair<PrChecks.Glyph, PrChecks.Tone> = when (status) {
        CiRun.RunStatus.Queued -> PrChecks.Glyph.Queued to PrChecks.Tone.Muted
        CiRun.RunStatus.Running -> PrChecks.Glyph.Running to PrChecks.Tone.Muted
        CiRun.RunStatus.Passed -> PrChecks.Glyph.Passed to PrChecks.Tone.Green
        CiRun.RunStatus.Failed -> PrChecks.Glyph.Failed to PrChecks.Tone.Red
        CiRun.RunStatus.Cancelled -> PrChecks.Glyph.Skipped to PrChecks.Tone.Muted
    }

    /** A run's source; anything but `github` is ours. */
    fun source(provider: String?): String = if (provider == "github") "github" else "exponential"

    private fun runAt(run: Run): Long = msOf(run.startedAt) ?: msOf(run.createdAt) ?: 0L

    /** Newest first; ties by id. */
    private fun newestFirst(runs: List<Run>): List<Run> =
        runs.sortedWith { a, b ->
            val d = runAt(b).compareTo(runAt(a))
            if (d != 0) d else a.id.compareTo(b.id)
        }

    private fun statusOf(run: Run, jobs: List<Job>): CiRun.RunStatus =
        CiRun.runStatus(run.state, jobs.filter { it.runId == run.id }.map { it.status to it.conclusion }).first

    /** `mine`: the viewer started it, or its PR's chat is the viewer's. */
    fun isMine(run: Run, viewerId: String, pullRequests: List<Pr>, sessions: List<Session>): Boolean {
        if (run.createdByUserId == viewerId) return true
        val prId = run.pullRequestId ?: return false
        val root = pullRequests.firstOrNull { it.id == prId }?.rootSessionId ?: return false
        return sessions.any { it.id == root && it.userId == viewerId }
    }

    private fun triggerText(run: Run, pullRequests: List<Pr>): String {
        if (run.trigger == "pull_request" && run.pullRequestId != null) {
            val pr = pullRequests.firstOrNull { it.id == run.pullRequestId }
            if (pr != null) return Copy.PULL_REQUEST.replace("{number}", pr.number.toString())
        }
        return Copy.TRIGGERS[run.trigger] ?: run.trigger
    }

    fun repoName(repos: List<Repo>, id: String): String = repos.firstOrNull { it.id == id }?.fullName ?: Copy.UNKNOWN_REPOSITORY

    /** The Runs tab: the filter's runs, newest first, at most [LIMIT_RUNS]. */
    fun runList(input: Input, filter: RunFilter): List<RunListRow> {
        val at = msOf(input.now) ?: 0L
        val rows = ArrayList<RunListRow>()
        for (run in newestFirst(input.runs)) {
            val status = statusOf(run, input.jobs)
            val source = source(run.provider)
            if (filter.repositoryId != null && run.repositoryId != filter.repositoryId) continue
            if (filter.source != null && source != filter.source) continue
            if (filter.status == "running" && status != CiRun.RunStatus.Running && status != CiRun.RunStatus.Queued) continue
            if (filter.status == "failed" && status != CiRun.RunStatus.Failed) continue
            if (filter.status == "mine" && !isMine(run, input.viewerId, input.pullRequests, input.sessions)) continue
            val started = msOf(run.startedAt)
            val repository = repoName(input.repos, run.repositoryId)
            val ref = run.ref ?: RepoCi.Copy.Runs.NO_REF
            val sourceLabel = Copy.SOURCES.getValue(source)
            val (glyph, tone) = glyph(status)
            rows += RunListRow(
                id = run.id,
                identifier = "#${run.number}",
                title = "#${run.number} ${run.name}",
                status = status,
                statusLabel = CiRun.Copy.STATUS.getValue(status),
                glyph = glyph,
                tone = tone,
                repositoryId = run.repositoryId,
                repository = repository,
                source = source,
                sourceLabel = sourceLabel,
                ref = ref,
                trigger = triggerText(run, input.pullRequests),
                duration = started?.let { PrChecks.formatDuration((msOf(run.completedAt) ?: at) - it) },
                startedAt = run.startedAt,
                subtitle = "$repository · $ref · $sourceLabel",
            )
            if (rows.size >= LIMIT_RUNS) break
        }
        return rows
    }

    /** `All repositories`, then every repository with a run, by name. */
    fun repoOptions(input: Input): List<Option> {
        val named = input.runs.map { it.repositoryId }.distinct().map { Option(repoName(input.repos, it), it) }
            .sortedWith { a, b -> val d = a.label.compareTo(b.label); if (d != 0) d else a.value.compareTo(b.value) }
        return listOf(Option(Copy.REPOSITORIES_ALL, "all")) + named
    }

    private fun online(runner: Runner, now: Long): Boolean {
        val seen = msOf(runner.lastSeenAt) ?: return false
        return now - seen < RUNNER_ONLINE_MS
    }

    /** Running, Queued, Failed today, Runners. */
    fun tiles(input: Input): List<Tile> {
        val at = msOf(input.now) ?: 0L
        var running = 0
        var queued = 0
        val failed = ArrayList<Run>()
        for (run in input.runs) {
            when (statusOf(run, input.jobs)) {
                CiRun.RunStatus.Running -> running++
                CiRun.RunStatus.Queued -> queued++
                CiRun.RunStatus.Failed -> {
                    val done = msOf(run.completedAt)
                    if (done != null && at - done < 24 * 3_600_000L) failed += run
                }
                else -> Unit
            }
        }
        val newest = failed.sortedByDescending { msOf(it.completedAt) ?: 0L }.firstOrNull()
        val repo = newest?.let { repoName(input.repos, it.repositoryId) }
        return listOf(
            Tile(Copy.TILE_RUNNING, running.toString(), "neutral", null),
            Tile(Copy.TILE_QUEUED, queued.toString(), "neutral", null),
            Tile(
                Copy.TILE_FAILED_TODAY,
                failed.size.toString(),
                if (failed.isNotEmpty()) "danger" else "neutral",
                repo?.substringAfterLast('/'),
            ),
            Tile(
                Copy.TILE_RUNNERS,
                Copy.TILE_RUNNERS_VALUE
                    .replace("{online}", input.runners.count { online(it, at) }.toString())
                    .replace("{total}", input.runners.size.toString()),
                "neutral",
                null,
            ),
        )
    }

    /** nav-badges.json destination `ci`: the viewer's newest run per
     *  (repository, pipeline, ref): failed = attention, else live = muted. */
    fun badge(input: Input): NavBadges.Badge {
        val seen = HashSet<String>()
        var failed = 0
        var live = 0
        for (run in newestFirst(input.runs)) {
            if (!isMine(run, input.viewerId, input.pullRequests, input.sessions)) continue
            if (!seen.add("${run.repositoryId}\u0000${run.name}\u0000${run.ref.orEmpty()}")) continue
            when (statusOf(run, input.jobs)) {
                CiRun.RunStatus.Failed -> failed++
                CiRun.RunStatus.Running, CiRun.RunStatus.Queued -> live++
                else -> Unit
            }
        }
        return if (failed > 0) NavBadges.Badge(failed, NavBadges.Tone.Attention) else NavBadges.Badge(live, NavBadges.Tone.Muted)
    }

    private val TRAIN_WAITING = setOf("queued", "checks", "held", "conflict", "failing")

    /** The merge train per repository (testing batches; [queue] adds the
     *  queued and waiting PRs). */
    fun trainGroups(input: Input, queue: Boolean): List<TrainGroup> {
        val pill = MergeTrain.Copy.Pill
        val byRepo = LinkedHashMap<String, MutableList<Pr>>()
        for (pr in input.pullRequests) {
            val waiting = queue && pr.mergeState != null && pr.mergeState in TRAIN_WAITING
            if (pr.mergeState != "testing" && !waiting) continue
            byRepo.getOrPut(pr.repositoryId) { ArrayList() } += pr
        }
        val groups = ArrayList<TrainGroup>()
        for ((repositoryId, prs) in byRepo) {
            val rows = ArrayList<TrainRow>()
            val batches = LinkedHashMap<String, MutableList<Pr>>()
            for (pr in prs.filter { it.mergeState == "testing" }.sortedBy { it.number }) {
                batches.getOrPut(pr.mergeTrainId ?: pr.id) { ArrayList() } += pr
            }
            for ((key, members) in batches.entries.sortedBy { it.value.first().number }) {
                val first = members.first()
                rows += TrainRow(
                    id = "batch.$key",
                    identifier = "#${first.number}",
                    title = if (members.size == 1) first.title else Copy.TRAIN_BATCH.replace("{count}", members.size.toString()),
                    subtitle = if (members.size == 1) null else members.joinToString(" · ") { "#${it.number} ${it.title}" },
                    glyph = PrChecks.Glyph.Running,
                    badge = pill.TESTING,
                    pullRequestId = first.id,
                )
            }
            if (queue) {
                val queued = prs.filter { it.mergeState == "queued" }.sortedWith(
                    compareBy<Pr> { it.mergePosition ?: Int.MAX_VALUE }.thenBy { it.number },
                )
                for (pr in queued) {
                    val position = pr.mergePosition
                    rows += TrainRow(
                        id = pr.id,
                        identifier = "#${pr.number}",
                        title = pr.title,
                        subtitle = null,
                        glyph = PrChecks.Glyph.Queued,
                        badge = if (position != null && position != 0) pill.QUEUED_AT.replace("{position}", MergeTrain.ordinal(position)) else pill.QUEUED,
                        pullRequestId = pr.id,
                    )
                }
                for (pr in prs.filter { it.mergeState != "queued" && it.mergeState != "testing" }.sortedBy { it.number }) {
                    val state = pr.mergeState
                    rows += TrainRow(
                        id = pr.id,
                        identifier = "#${pr.number}",
                        title = pr.title,
                        subtitle = null,
                        glyph = when (state) {
                            "checks" -> PrChecks.Glyph.Queued
                            "held" -> PrChecks.Glyph.Skipped
                            else -> PrChecks.Glyph.Failed
                        },
                        badge = when (state) {
                            "checks" -> pill.CHECKS_RUNNING
                            "held" -> pill.ON_HOLD
                            "conflict" -> pill.CONFLICT
                            else -> pill.FAILING
                        },
                        pullRequestId = pr.id,
                    )
                }
            }
            if (rows.isEmpty()) continue
            groups += TrainGroup(repositoryId, repoName(input.repos, repositoryId), prs.first().base, rows)
        }
        return groups.sortedWith { a, b -> val d = a.title.compareTo(b.title); if (d != 0) d else a.repositoryId.compareTo(b.repositoryId) }
    }

    /** A runner's labels: its OS, then its own lower-cased. */
    fun runnerLabels(runner: Runner): List<String> {
        val os = RunnerSettings.osLabel(runner.platform)
        return (listOfNotNull(os) + runner.labels.map { it.lowercase() }).distinct()
    }

    /** The Runners tab: online first, then by name. */
    fun runnerRows(input: Input): List<RunnerRow> {
        val at = msOf(input.now) ?: 0L
        return input.runners.map { runner ->
            val labels = runnerLabels(runner)
            val isOnline = online(runner, at)
            val busy = input.jobs.count { it.status == "in_progress" && it.runnerDeviceId == runner.id }
            val queued = input.jobs.count { job ->
                source(job.provider) == "exponential" && job.status == "queued" && job.runnerDeviceId == null &&
                    job.runsOn.all { it.lowercase() in labels }
            }
            RunnerRow(
                id = runner.id,
                title = runner.label,
                subtitle = if (labels.isNotEmpty()) labels.joinToString(", ") else Copy.RUNNERS_NO_LABELS,
                online = isOnline,
                busy = if (isOnline) Copy.RUNNERS_BUSY.replace("{busy}", busy.toString()).replace("{slots}", runner.slots.toString()) else null,
                queue = if (queued > 0) Copy.RUNNERS_QUEUE.replace("{count}", queued.toString()) else null,
                offlineSince = if (isOnline) null else runner.lastSeenAt,
            )
        }.sortedWith { a, b ->
            when {
                a.online != b.online -> if (a.online) -1 else 1
                a.title != b.title -> a.title.compareTo(b.title)
                else -> a.id.compareTo(b.id)
            }
        }
    }

    fun tabCounts(input: Input): TabCounts =
        TabCounts(train = trainGroups(input, true).sumOf { it.rows.size }, runners = input.runners.size)
}
