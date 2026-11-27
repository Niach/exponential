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
        val FILTERS: Map<String, String> = linkedMapOf("all" to "All runs", "running" to "Running", "failed" to "Failed", "mine" to "Mine")
        /** copy.filterLabels: the three pickers' labels (#EXP-2169). */
        val FILTER_LABELS: Map<String, String> = linkedMapOf("status" to "Status", "repository" to "Repository", "source" to "Source")
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
        const val IN_PROGRESS = "In progress"
        const val TODAY = "Today"
        const val YESTERDAY = "Yesterday"
        val MONTHS = listOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")
        const val QUEUED = "queued"
        const val BATCH_TITLE = "Testing {numbers} together"
        const val RUNNER_CHIP = "{online} of {total} runners"
        const val WAITING_FOR_RUNNER = "waiting for a runner"
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
        const val RUNNER_CHIP = "ci-runner-chip"
    }

    /** ci-page.json `statusTones`: THE status table (running = the only
     *  blue `info` mark; queued and cancelled stay neutral). */
    val STATUS_TONES: Map<CiRun.RunStatus, Pair<PrChecks.Glyph, String>> = mapOf(
        CiRun.RunStatus.Passed to (PrChecks.Glyph.Passed to "success"),
        CiRun.RunStatus.Failed to (PrChecks.Glyph.Failed to "danger"),
        CiRun.RunStatus.Running to (PrChecks.Glyph.Running to "info"),
        CiRun.RunStatus.Queued to (PrChecks.Glyph.Queued to "neutral"),
        CiRun.RunStatus.Cancelled to (PrChecks.Glyph.Skipped to "neutral"),
    )

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
        /** What the run is about (`ci_runs.title`). */
        val title: String? = null,
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

    /** A person a run names (its author), from the users shape. */
    data class User(val id: String, val name: String, val image: String?)

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
        val users: List<User> = emptyList(),
        val viewerId: String = "",
        val now: String = "",
        /** The viewer's clock offset from UTC (Today / Yesterday). */
        val utcOffsetMinutes: Int = 0,
    )

    data class RunFilter(val status: String = "all", val repositoryId: String? = null, val source: String? = null)

    data class RunListRow(
        val id: String,
        val status: CiRun.RunStatus,
        val statusLabel: String,
        val glyph: PrChecks.Glyph,
        val tone: String,
        /** The commit / PR line, else the merge train batch, else the pipeline. */
        val title: String,
        /** `name #n`. */
        val workflow: String,
        val repositoryId: String,
        val repository: String,
        val repo: String,
        val ref: String,
        val sha: String,
        val trigger: String,
        val source: String,
        val sourceLabel: String,
        val sourceIcon: String,
        val duration: String?,
        /** When it started, else when it was queued. */
        val at: String,
        val author: User?,
        val subtitle: String,
        val phoneSubtitle: String,
        val phoneMeta: String,
    )

    data class RunGroup(val id: String, val title: String, val count: Int?, val rows: List<RunListRow>)

    data class RunnerChip(val label: String, val online: Boolean)

    data class Option(val label: String, val value: String)

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

    /** A running merge train run is its batch: `Testing #a, #b together`. */
    private fun batchTitle(run: Run, pullRequests: List<Pr>): String? {
        if (run.trigger != "merge_train" || run.state != "pending") return null
        val members = pullRequests.filter { it.repositoryId == run.repositoryId && it.mergeState == "testing" }.map { it.number }.sorted()
        if (members.isEmpty()) return null
        return Copy.BATCH_TITLE.replace("{numbers}", members.joinToString(", ") { "#$it" })
    }

    /** One row of the list (both form factors' strings). */
    fun runRow(run: Run, input: Input): RunListRow {
        val at = msOf(input.now) ?: 0L
        val status = statusOf(run, input.jobs)
        val source = source(run.provider)
        val started = msOf(run.startedAt)
        val repository = repoName(input.repos, run.repositoryId)
        val repo = repository.substringAfterLast('/')
        val ref = run.ref ?: RepoCi.Copy.Runs.NO_REF
        val sha = run.sha.take(7)
        val workflow = "${run.name} #${run.number}"
        val trigger = triggerText(run, input.pullRequests)
        val pr = run.pullRequestId?.let { id -> input.pullRequests.firstOrNull { it.id == id } }
        val duration = started?.let { PrChecks.formatDuration((msOf(run.completedAt) ?: at) - it) }
        val (glyph, tone) = STATUS_TONES.getValue(status)
        return RunListRow(
            id = run.id,
            status = status,
            statusLabel = CiRun.Copy.STATUS.getValue(status),
            glyph = glyph,
            tone = tone,
            title = run.title ?: batchTitle(run, input.pullRequests) ?: pr?.title ?: run.name,
            workflow = workflow,
            repositoryId = run.repositoryId,
            repository = repository,
            repo = repo,
            ref = ref,
            sha = sha,
            trigger = trigger,
            source = source,
            sourceLabel = Copy.SOURCES.getValue(source),
            sourceIcon = if (source == "github") "github" else "brand-exponential",
            duration = duration,
            at = run.startedAt ?: run.createdAt,
            author = run.createdByUserId?.let { id -> input.users.firstOrNull { it.id == id } },
            subtitle = listOf(workflow, repo, ref, sha, trigger).joinToString(" · "),
            phoneSubtitle = listOf(repo, ref, workflow).joinToString(" · "),
            phoneMeta = duration ?: Copy.QUEUED,
        )
    }

    /** The Runs tab: the filter's runs, newest first, at most [LIMIT_RUNS]. */
    fun runList(input: Input, filter: RunFilter): List<RunListRow> {
        val rows = ArrayList<RunListRow>()
        for (run in newestFirst(input.runs)) {
            val status = statusOf(run, input.jobs)
            val source = source(run.provider)
            if (filter.repositoryId != null && run.repositoryId != filter.repositoryId) continue
            if (filter.source != null && source != filter.source) continue
            if (filter.status == "running" && status != CiRun.RunStatus.Running && status != CiRun.RunStatus.Queued) continue
            if (filter.status == "failed" && status != CiRun.RunStatus.Failed) continue
            if (filter.status == "mine" && !isMine(run, input.viewerId, input.pullRequests, input.sessions)) continue
            rows += runRow(run, input)
            if (rows.size >= LIMIT_RUNS) break
        }
        return rows
    }

    /** The local day of an instant, `yyyy-mm-dd`, at the viewer's offset. */
    private fun localDay(ms: Long, offsetMinutes: Int): String =
        java.time.Instant.ofEpochMilli(ms + offsetMinutes * 60_000L).atZone(java.time.ZoneOffset.UTC).toLocalDate().toString()

    /** A day's section title: Today, Yesterday, else `27 Nov` (+ the year
     *  when it is not this year's). */
    fun dayTitle(day: String, now: String, offsetMinutes: Int): String {
        val nowMs = msOf(now) ?: 0L
        val today = localDay(nowMs, offsetMinutes)
        if (day == today) return Copy.TODAY
        if (day == localDay(nowMs - 86_400_000L, offsetMinutes)) return Copy.YESTERDAY
        val (y, m, d) = day.split('-')
        val label = "${d.toInt()} ${Copy.MONTHS[m.toInt() - 1]}"
        return if (y == today.take(4)) label else "$label $y"
    }

    private fun live(row: RunListRow) = row.status == CiRun.RunStatus.Running || row.status == CiRun.RunStatus.Queued

    /** The Runs tab's sections: In progress (with a count), then one per
     *  local day, newest first. */
    fun runGroups(input: Input, filter: RunFilter): List<RunGroup> {
        val rows = runList(input, filter)
        val liveRows = rows.filter(::live)
        val groups = ArrayList<RunGroup>()
        if (liveRows.isNotEmpty()) groups += RunGroup("in-progress", Copy.IN_PROGRESS, liveRows.size, liveRows)
        for (row in rows) {
            if (live(row)) continue
            val day = localDay(msOf(row.at) ?: 0L, input.utcOffsetMinutes)
            val last = groups.lastOrNull()
            if (last != null && last.id == day) {
                groups[groups.size - 1] = last.copy(rows = last.rows + row)
            } else {
                groups += RunGroup(day, dayTitle(day, input.now, input.utcOffsetMinutes), null, listOf(row))
            }
        }
        return groups
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

    /** The toolbar's runner chip (it opens the Runners tab); null without runners. */
    fun runnerChip(input: Input): RunnerChip? {
        if (input.runners.isEmpty()) return null
        val at = msOf(input.now) ?: 0L
        val count = input.runners.count { online(it, at) }
        return RunnerChip(
            Copy.RUNNER_CHIP.replace("{online}", count.toString()).replace("{total}", input.runners.size.toString()),
            count > 0,
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
