package com.exponential.app.domain

/**
 * #EXP-2172 (fixtures/ci-page.json): THE CI page (one root destination across
 * every member team) and its badge: our runs and GitHub Actions runs in one
 * list, the merge queue, the runner slots. #EXP-2214: four tabs, Merge queue
 * · Pull requests · Releases · Runners. Mirrors the TS reference
 * `packages/domain-contract/src/ci-page.ts`; `CiPageTest` replays the fixture.
 */
object CiPage {

    /** ci-page.json `copy` (`CiPageTest` compares every key). */
    object Copy {
        const val TITLE = "CI"
        /** copy.tabs / copy.phoneTabs, in [TAB_ORDER]. */
        val TABS: Map<String, String> = linkedMapOf("train" to "Merge queue", "prs" to "Pull requests", "releases" to "Releases", "runners" to "Runners")
        val PHONE_TABS: Map<String, String> = linkedMapOf("train" to "Queue", "prs" to "PRs", "releases" to "Releases", "runners" to "Runners")
        val FILTERS: Map<String, String> = linkedMapOf("all" to "All runs", "running" to "Running", "failed" to "Failed", "mine" to "Mine")
        /** copy.filterLabels (#EXP-2169). */
        val FILTER_LABELS: Map<String, String> = linkedMapOf("status" to "Status", "repository" to "Repository", "source" to "Source")
        const val REPOSITORIES_ALL = "All repositories"
        const val SOURCES_ALL = "Every source"
        val SOURCES: Map<String, String> = linkedMapOf("exponential" to "Exponential", "github" to "GitHub Actions")
        val COLUMNS: Map<String, String> = linkedMapOf(
            "status" to "Status", "run" to "Run", "repository" to "Repository", "source" to "Source",
            "ref" to "Branch", "trigger" to "Trigger", "duration" to "Duration", "started" to "Started",
        )
        const val PULL_REQUEST = "Pull request #{number}"
        val TRIGGERS: Map<String, String> = linkedMapOf(
            "push" to "Push",
            "pull_request" to "Pull request",
            "tag" to "Tag",
            "merge_train" to "Merge queue",
            "manual" to "Manual",
            "schedule" to "Schedule",
        )
        const val UNKNOWN_REPOSITORY = "Repository"
        const val RUNNERS_BUSY = "{busy} of {slots} busy"
        const val RUNNERS_QUEUE = "{count} queued"
        const val RUNNERS_OFFLINE_SINCE = "Last seen {time}"
        const val RUNNERS_NO_LABELS = "No labels"
        const val EMPTY_RUNS = "No runs yet"
        const val EMPTY_FILTERED = "No runs match"
        const val EMPTY_TRAIN = "Nothing in the merge queue"
        const val EMPTY_PRS = "No pull request runs yet"
        const val EMPTY_RELEASES = "No release runs yet"
        const val EMPTY_RUNNERS = "No runners"
        const val IN_PROGRESS = "In progress"
        const val TODAY = "Today"
        const val YESTERDAY = "Yesterday"
        val MONTHS = listOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")
        const val QUEUED = "queued"
        const val BATCH_TITLE = "Testing {numbers} together"
        const val BATCH_ONE = "Testing {number}"
        const val RUNNER_CHIP = "{online} of {total} runners"
        const val WAITING_FOR_RUNNER = "waiting for a runner"
        /** copy.prs: the Pull requests tab's sections. */
        val PRS: Map<String, String> = linkedMapOf("open" to "Open", "branches" to "Branches", "closed" to "Merged or closed")
        /** copy.kinds: what a run is for (the run page's Badge). */
        val KINDS: Map<String, String> = linkedMapOf("train" to "Merge queue", "pull_request" to "Pull request", "release" to "Release", "branch" to "Branch")

        /** copy.queue: the Merge queue tab and the run page's What happened. */
        object Queue {
            const val TESTING = "Testing now"
            const val BASE = "{base} @ {sha}"
            const val RERUN = "re-run"
            const val SPLIT = "split: {count} more to test"
            const val UNTESTED = "Merge now, untested"
            const val RUNNING_FOR = "Running for {duration}"
            const val LEFT = "about {n} min left"
            const val ALMOST_DONE = "almost done"
            const val BATCH = "In this batch"
            const val OPEN_RUN = "Open run"
            const val UP_NEXT = "Up next"
            const val POSITION = "{ordinal} in line"
            const val MERGE_NOW = "Merge now · skips CI, lands next"
            const val NOT_READY = "Not ready yet"
            const val SENT_BACK = "Sent back"
            const val FAILED_IN = "Failed in {checks}"
            const val CONFLICT_IN = "Conflict in {files}"
            const val TRIES = "{n} of {max} tries"
            const val LANDED = "Landed"
            const val LANDED_WITH = "with {numbers}"
            const val MERGED_NOW = "Merge now · untested"
            const val OPEN_LOG = "Open log"
            const val RESUME = "Resume"
            const val MORE = "More"
            const val INTO = "into {base}"
            const val WHAT_HAPPENED = "What happened"
            val MENU: Map<String, String> = linkedMapOf("mergeNow" to "Merge now", "hold" to "Hold")
            val STATS: Map<String, String> = linkedMapOf(
                "inQueue" to "In the queue", "landedToday" to "Landed today", "sentBackToday" to "Sent back today", "typicalRun" to "Typical run",
            )
            val WAITS: Map<String, String> = linkedMapOf(
                "checks" to "Checks running", "session" to "Session working", "approval" to "Needs approval", "stack" to "Waiting for its base",
                "draft" to "Draft", "failing" to "Checks failing", "conflict" to "Conflict",
            )
            val HOLDS: Map<String, String> = linkedMapOf(
                "person" to "On hold", "bounces" to "On hold after 3 tries", "timeout" to "On hold after 24 hours",
                "no_checks" to "On hold: no checks", "conflict" to "On hold: conflict",
            )
            val OUTCOME: Map<String, String> = linkedMapOf(
                "landed" to "Landed as {sha}", "untested" to "Merge now, landed untested",
                "failing" to "Sent back: failed in {checks} · {tries}", "conflict" to "Sent back: conflict in {files} · {tries}",
                "testing" to "Being tested", "requeued" to "Back in the queue",
            )
        }
    }

    /** The tab order (#EXP-2214: the merge queue opens first). */
    val TAB_ORDER = listOf("train", "prs", "releases", "runners")

    /** ci-page.json `testIds`. */
    object TestIds {
        const val PAGE = "ci-page"
        const val TABS = "ci-tabs"
        const val FILTERS = "ci-filters"
        const val STATUS_FILTER = "ci-filter-status"
        const val REPO_FILTER = "ci-filter-repo"
        const val SOURCE_FILTER = "ci-filter-source"
        const val RUNS = "ci-runs"
        const val RUN_ROW = "ci-run-row"
        const val RUNNERS = "ci-runners"
        const val RUNNER_ROW = "ci-runner-row"
        const val RUNNER_CHIP = "ci-runner-chip"
        const val QUEUE = "ci-queue"
        const val QUEUE_REPO = "ci-queue-repo"
        const val QUEUE_STATS = "ci-queue-stats"
        const val QUEUE_NOW = "ci-queue-now"
        const val QUEUE_ROW = "ci-queue-row"
        const val PRS = "ci-prs"
        const val PR_ROW = "ci-pr-row"
        const val RELEASES = "ci-releases"
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
    const val LIMIT_SUBJECTS = 50
    const val LIMIT_SUBJECT_RUNS = 10
    const val LIMIT_HISTORY = 10
    const val HISTORY_HOURS = 48
    const val ETA_SAMPLES = 3

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
        /** #EXP-2214: the merge queue run it tests (`ci_runs.merge_train_id`). */
        val mergeTrainId: String? = null,
    )

    data class Job(
        /** #EXP-2214: the Sent back row's Open log names a job. */
        val id: String,
        val runId: String,
        val check: String,
        val status: String,
        val conclusion: String?,
        val runsOn: List<String>,
        val runnerDeviceId: String?,
        val provider: String,
    )

    /** [defaultBranch] (#EXP-2214): a push to it is a release run (null = `main`). */
    data class Repo(val id: String, val fullName: String, val defaultBranch: String? = null)

    data class Pr(
        val id: String,
        val number: Int,
        val title: String,
        val repositoryId: String,
        val rootSessionId: String?,
        val base: String,
        /** #EXP-2214: open | draft | merged | closed. */
        val state: String,
        val mergeState: String?,
        /** What a `checks` PR waits for (merge-train.json `mergeWait`). */
        val mergeWait: String?,
        /** person | bounces | timeout | no_checks | conflict. */
        val mergeHold: String?,
        val mergeNow: Boolean,
        val mergeAttempts: Int,
        val mergePosition: Int?,
        val mergeTrainId: String?,
    )

    /** #EXP-2214: one merge queue run (`merge_trains`, the synced columns). */
    data class Train(
        val id: String,
        val repositoryId: String,
        val base: String,
        val baseSha: String?,
        /** PR ids per unit (a stack bottom first). */
        val units: List<List<String>>,
        val state: String,
        val attempt: Int,
        val bisectOf: String?,
        /** The bisect halves still to test after it. */
        val pending: List<List<List<String>>>,
        val untested: Boolean,
        val testedSha: String?,
        val createdAt: String,
        val finishedAt: String?,
    )

    /** #EXP-2214: one row of the queue's history (`merge_train_events`). */
    data class TrainEvent(
        val id: String,
        val repositoryId: String,
        val pullRequestId: String,
        val mergeTrainId: String?,
        /** landed | failing | conflict | held | merge_now | resumed. */
        val kind: String,
        val base: String,
        val attempt: Int,
        val reason: String?,
        val checks: List<String>,
        val ciJobId: String?,
        val jobUrl: String?,
        val files: List<String>,
        val sha: String?,
        val byUserId: String?,
        val createdAt: String,
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
        /** #EXP-2214: the merge queue's runs and history. */
        val trains: List<Train> = emptyList(),
        val trainEvents: List<TrainEvent> = emptyList(),
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
        /** A tag run's tag, else the commit / PR line, else the merge queue batch, else the pipeline. */
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

    data class RunnerRow(
        val id: String,
        val title: String,
        val subtitle: String,
        val online: Boolean,
        val busy: String?,
        val queue: String?,
        val offlineSince: String?,
    )

    /** A pill: text + tone (`neutral` | `info` | `success` | `danger`). */
    data class Pill(val text: String, val tone: String)

    /** One subject of the Pull requests tab: a PR (its runs) or a branch. */
    data class SubjectRow(
        /** The PR's id, else `branch.<repositoryId>.<ref>`. */
        val id: String,
        val pullRequestId: String?,
        val identifier: String?,
        val title: String,
        val status: CiRun.RunStatus,
        val glyph: PrChecks.Glyph,
        val tone: String,
        val subtitle: String,
        val phoneSubtitle: String,
        val meta: String,
        val pill: Pill?,
        val runId: String,
        val runs: List<RunListRow>,
    )

    /** [id] = open | branches | closed. */
    data class SubjectGroup(val id: String, val title: String, val open: Boolean, val count: Int, val rows: List<SubjectRow>)

    data class QueueMember(val pullRequestId: String, val identifier: String, val title: String, val author: User?)

    /** The run testing now (one Card). */
    data class QueueNow(
        val trainId: String,
        val title: String,
        val subtitle: String,
        val members: List<QueueMember>,
        val status: CiRun.RunStatus,
        val runId: String?,
        val elapsed: String,
        val eta: String?,
        val progress: Int?,
    )

    data class QueueRow(
        val id: String,
        val pullRequestId: String,
        val identifier: String,
        val title: String,
        val subtitle: String?,
        /** An icon name (`ci-queued`, `zap`, `pr-conflict` …). */
        val glyph: String,
        /** neutral | success | danger | warning. */
        val tone: String,
        val badge: Pill?,
        val at: String?,
        /** `open-log` | `resume` | null. */
        val action: String?,
        val runId: String?,
        val jobId: String?,
        val url: String?,
        /** Up next: the ⋯ menu (Merge now, Hold). */
        val menu: Boolean,
    )

    data class QueueStats(val inQueue: Int, val landedToday: Int, val sentBackToday: Int, val typicalRun: String?)

    data class QueueRepo(
        val repositoryId: String,
        val title: String,
        val caption: String,
        val stats: QueueStats,
        val testing: List<QueueNow>,
        val upNext: List<QueueRow>,
        val waiting: List<QueueRow>,
        val sentBack: List<QueueRow>,
        val landed: List<QueueRow>,
    )

    data class Eta(val eta: String, val progress: Int)

    data class TabCounts(val train: Int, val prs: Int, val releases: Int, val runners: Int) {
        fun of(tab: String): Int = when (tab) {
            "train" -> train
            "prs" -> prs
            "releases" -> releases
            else -> runners
        }
    }

    data class RunOutcome(
        val pullRequestId: String,
        val identifier: String,
        val title: String,
        val text: String,
        val glyph: String,
        val tone: String,
        /** A failing PR's red job IN THIS RUN (Open log selects it). */
        val jobId: String?,
    )

    /** `runContextCases`: why the run exists ([kind] = train | pull_request | release | branch). */
    data class RunContext(
        val kind: String,
        val badge: String,
        val title: String?,
        val caption: String?,
        val outcome: List<RunOutcome>,
    )

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

    /** #EXP-2214 (`kindCases`): what a run is for: `train` (its trigger, its
     *  train, an `exp-train/` ref), `pull_request` (its PR or the trigger),
     *  `release` (tag, manual, schedule, a push to the default branch, `main`
     *  when unknown), else `branch`. */
    fun runKind(trigger: String, mergeTrainId: String?, ref: String?, pullRequestId: String?, repositoryId: String, repos: List<Repo>): String {
        if (trigger == "merge_train" || !mergeTrainId.isNullOrEmpty() || (ref ?: "").startsWith("exp-train/")) return "train"
        if (!pullRequestId.isNullOrEmpty() || trigger == "pull_request") return "pull_request"
        if (trigger == "tag" || trigger == "manual" || trigger == "schedule") return "release"
        val branch = repos.firstOrNull { it.id == repositoryId }?.defaultBranch ?: "main"
        return if (trigger == "push" && ref == branch) "release" else "branch"
    }

    fun runKind(run: Run, repos: List<Repo>): String =
        runKind(run.trigger, run.mergeTrainId, run.ref, run.pullRequestId, run.repositoryId, repos)

    private fun triggerText(run: Run, pullRequests: List<Pr>): String {
        if (run.trigger == "pull_request" && run.pullRequestId != null) {
            val pr = pullRequests.firstOrNull { it.id == run.pullRequestId }
            if (pr != null) return Copy.PULL_REQUEST.replace("{number}", pr.number.toString())
        }
        return Copy.TRIGGERS[run.trigger] ?: run.trigger
    }

    fun repoName(repos: List<Repo>, id: String): String = repos.firstOrNull { it.id == id }?.fullName ?: Copy.UNKNOWN_REPOSITORY

    private fun numbered(numbers: List<Int>) = numbers.joinToString(", ") { "#$it" }

    /** A merge queue run is its batch: its train's units, else, running,
     *  the repo's testing PRs. */
    private fun batchTitle(run: Run, pullRequests: List<Pr>, trains: List<Train>): String? {
        val train = run.mergeTrainId?.let { id -> trains.firstOrNull { it.id == id } }
        if (train != null) {
            val numbers = train.units.flatten().mapNotNull { id -> pullRequests.firstOrNull { it.id == id }?.number }
            if (numbers.size == 1) return Copy.BATCH_ONE.replace("{number}", "#${numbers[0]}")
            if (numbers.size > 1) return Copy.BATCH_TITLE.replace("{numbers}", numbered(numbers))
        }
        if (run.trigger != "merge_train" || run.state != "pending") return null
        val members = pullRequests.filter { it.repositoryId == run.repositoryId && it.mergeState == "testing" }.map { it.number }.sorted()
        if (members.isEmpty()) return null
        return Copy.BATCH_TITLE.replace("{numbers}", numbered(members))
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
            // #EXP-2214: a tag run reads as its tag.
            title = (if (run.trigger == "tag" && !run.ref.isNullOrEmpty()) run.ref else null)
                ?: run.title ?: batchTitle(run, input.pullRequests, input.trains) ?: pr?.title ?: run.name,
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

    /** The filter's runs (of [kinds] when given), newest first, at most [LIMIT_RUNS]. */
    fun runList(input: Input, filter: RunFilter, kinds: List<String>? = null): List<RunListRow> {
        val rows = ArrayList<RunListRow>()
        for (run in newestFirst(input.runs)) {
            if (kinds != null && runKind(run, input.repos) !in kinds) continue
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

    /** Run sections: In progress (with a count), then one per local day,
     *  newest first (#EXP-2214: the Releases tab = [kinds] `release`). */
    fun runGroups(input: Input, filter: RunFilter, kinds: List<String>? = null): List<RunGroup> {
        val rows = runList(input, filter, kinds)
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

    // ── #EXP-2214 PULL REQUESTS TAB ─────────────────────────────────────

    private val STATUS_RANK = listOf(
        CiRun.RunStatus.Failed, CiRun.RunStatus.Running, CiRun.RunStatus.Queued, CiRun.RunStatus.Passed, CiRun.RunStatus.Cancelled,
    )

    /** A PR's merge queue state as a pill (merge-train.json copy.pill). */
    fun mergePill(pr: Pr): Pill? {
        val pill = MergeTrain.Copy.Pill
        val waits = Copy.Queue.WAITS
        return when (pr.mergeState) {
            "testing" -> Pill(pill.TESTING, "info")
            "queued" -> {
                val position = pr.mergePosition
                Pill(if (position != null && position != 0) pill.QUEUED_AT.replace("{position}", MergeTrain.ordinal(position)) else pill.QUEUED, "neutral")
            }
            "merged" -> Pill(pill.MERGED, "success")
            "conflict" -> Pill(pill.CONFLICT, "danger")
            "failing" -> Pill(pill.FAILING, "danger")
            "held" -> Pill(pill.ON_HOLD, "neutral")
            "draft" -> Pill(pill.DRAFT, "neutral")
            "checks" -> Pill(
                pr.mergeWait?.let { waits[it] } ?: pill.CHECKS_RUNNING,
                when {
                    pr.mergeWait == "failing" -> "danger"
                    (pr.mergeWait ?: "checks") == "checks" -> "info"
                    else -> "neutral"
                },
            )
            else -> null
        }
    }

    private class Subject(val pr: Pr?, val ref: String?, val repositoryId: String, val runs: MutableList<Run> = ArrayList())

    /** THE PULL REQUESTS TAB (`prGroupsCases`): pull request runs grouped by
     *  PR, branch runs by (repository, ref); a subject's status rolls up the
     *  newest run of each pipeline name (failed, running, queued, passed,
     *  cancelled: the first present wins; the newest run with it decides the
     *  row). Open, Branches, Merged or closed (folded); each at most
     *  [LIMIT_SUBJECTS]; empty sections are left out. The filter applies to
     *  the SUBJECT. */
    fun prGroups(input: Input, filter: RunFilter): List<SubjectGroup> {
        val subjects = LinkedHashMap<String, Subject>()
        for (run in newestFirst(input.runs)) {
            val kind = runKind(run, input.repos)
            if (kind != "pull_request" && kind != "branch") continue
            if (filter.repositoryId != null && run.repositoryId != filter.repositoryId) continue
            val pr = run.pullRequestId?.let { id -> input.pullRequests.firstOrNull { it.id == id } }
            val key = run.pullRequestId ?: "branch.${run.repositoryId}.${run.ref.orEmpty()}"
            subjects.getOrPut(key) { Subject(pr, run.ref, run.repositoryId) }.runs += run
        }
        val groups = linkedMapOf<String, MutableList<SubjectRow>>("open" to ArrayList(), "branches" to ArrayList(), "closed" to ArrayList())
        for ((key, subject) in subjects) {
            val newestPerName = LinkedHashMap<String, Run>()
            for (run in subject.runs) newestPerName.putIfAbsent(run.name, run)
            val latest = newestPerName.values.map { it to statusOf(it, input.jobs) }
            val status = STATUS_RANK.firstOrNull { s -> latest.any { it.second == s } } ?: CiRun.RunStatus.Queued
            val deciding = latest.first { it.second == status }.first
            if (filter.status == "running" && status != CiRun.RunStatus.Running && status != CiRun.RunStatus.Queued) continue
            if (filter.status == "failed" && status != CiRun.RunStatus.Failed) continue
            if (filter.status == "mine" && subject.runs.none { isMine(it, input.viewerId, input.pullRequests, input.sessions) }) continue
            val row = runRow(deciding, input)
            val prId = subject.runs.first().pullRequestId
            val branch = prId == null
            val group = when {
                branch -> "branches"
                subject.pr != null && (subject.pr.state == "merged" || subject.pr.state == "closed") -> "closed"
                else -> "open"
            }
            val rows = groups.getValue(group)
            if (rows.size >= LIMIT_SUBJECTS) continue
            val (glyph, tone) = STATUS_TONES.getValue(status)
            rows += SubjectRow(
                id = key,
                pullRequestId = subject.pr?.id ?: prId,
                identifier = subject.pr?.let { "#${it.number}" },
                title = subject.pr?.title ?: if (branch) subject.ref ?: RepoCi.Copy.Runs.NO_REF else row.title,
                status = status,
                glyph = glyph,
                tone = tone,
                subtitle = listOf(row.repo, row.ref, row.workflow).joinToString(" · "),
                phoneSubtitle = listOf(row.repo, row.workflow).joinToString(" · "),
                meta = row.phoneMeta,
                pill = subject.pr?.let(::mergePill),
                runId = deciding.id,
                runs = subject.runs.take(LIMIT_SUBJECT_RUNS).map { runRow(it, input) },
            )
        }
        return listOf("open" to true, "branches" to true, "closed" to false).mapNotNull { (id, open) ->
            val rows = groups.getValue(id)
            if (rows.isEmpty()) null else SubjectGroup(id, Copy.PRS.getValue(id), open, rows.size, rows)
        }
    }

    // ── #EXP-2214 MERGE QUEUE TAB ───────────────────────────────────────

    private val LIVE_TRAIN = setOf("dispatching", "composing", "testing", "green")

    /** The finished green merge queue runs of a repository: per train, the
     *  span of its CI runs, newest first, at most [ETA_SAMPLES] * 2. */
    private fun trainDurations(input: Input, repositoryId: String): List<Long> {
        val byTrain = LinkedHashMap<String, MutableList<Run>>()
        for (run in input.runs) {
            val train = run.mergeTrainId
            if (run.repositoryId != repositoryId || train.isNullOrEmpty()) continue
            byTrain.getOrPut(train) { ArrayList() } += run
        }
        val spans = ArrayList<Pair<Long, Long>>()
        for (runs in byTrain.values) {
            if (!runs.all { statusOf(it, input.jobs) == CiRun.RunStatus.Passed }) continue
            val starts = runs.map { msOf(it.startedAt) }
            val ends = runs.map { msOf(it.completedAt) }
            if (starts.any { it == null } || ends.any { it == null }) continue
            val start = starts.minOf { it!! }
            val end = ends.maxOf { it!! }
            spans += end to (end - start)
        }
        return spans.sortedByDescending { it.first }.take(ETA_SAMPLES * 2).map { it.second }
    }

    /** The median of the repository's finished green merge queue runs, null
     *  with fewer than [ETA_SAMPLES]. */
    fun typicalTrainMs(input: Input, repositoryId: String): Long? {
        val spans = trainDurations(input, repositoryId).sorted()
        if (spans.size < ETA_SAMPLES) return null
        val mid = spans.size / 2
        return if (spans.size % 2 == 1) spans[mid] else Math.round((spans[mid - 1] + spans[mid]) / 2.0)
    }

    /** `etaCases`: what is left of a run going [elapsedMs] against the typical run. */
    fun trainEta(typicalMs: Long?, elapsedMs: Long): Eta? {
        if (typicalMs == null || typicalMs <= 0) return null
        val left = typicalMs - elapsedMs
        return Eta(
            eta = if (left > 60_000) Copy.Queue.LEFT.replace("{n}", Math.ceil(left / 60_000.0).toLong().toString()) else Copy.Queue.ALMOST_DONE,
            progress = Math.floor((elapsedMs.toDouble() / typicalMs.toDouble()) * 100).toInt().coerceIn(0, 99),
        )
    }

    private fun authorOf(pr: Pr, input: Input): User? {
        val starter = pr.rootSessionId?.let { root -> input.sessions.firstOrNull { it.id == root }?.userId } ?: return null
        return input.users.firstOrNull { it.id == starter }
    }

    private fun shortList(items: List<String>, max: Int): String =
        if (items.size <= max) items.joinToString(", ") else "${items.take(max).joinToString(", ")} +${items.size - max}"

    private fun tries(n: Int) = Copy.Queue.TRIES.replace("{n}", n.toString()).replace("{max}", MergeTrain.Limits.BOUNCES.toString())

    private fun fileName(path: String) = path.substringAfterLast('/')

    private val QUEUE_PR_STATES = setOf("queued", "testing", "held", "conflict", "failing")

    private fun eventMs(e: TrainEvent) = msOf(e.createdAt) ?: 0L

    /** THE MERGE QUEUE TAB (`queueCases`), per repository by name: Testing
     *  now, Up next, Not ready yet, Sent back, Landed and the stats (the TS
     *  reference's `ciQueue` doc comment is the rule). */
    fun queue(input: Input): List<QueueRepo> {
        val copy = Copy.Queue
        val now = msOf(input.now) ?: 0L
        val since = now - HISTORY_HOURS * 3_600_000L
        val today = localDay(now, input.utcOffsetMinutes)
        fun prOf(id: String) = input.pullRequests.firstOrNull { it.id == id }
        val repoIds = LinkedHashSet<String>()
        for (t in input.trains) if (t.state in LIVE_TRAIN && t.finishedAt.isNullOrEmpty()) repoIds += t.repositoryId
        for (pr in input.pullRequests) if (pr.state == "open" && pr.mergeState != null && pr.mergeState in QUEUE_PR_STATES) repoIds += pr.repositoryId
        for (e in input.trainEvents) if (eventMs(e) >= since) repoIds += e.repositoryId
        val out = ArrayList<QueueRepo>()
        for (repositoryId in repoIds) {
            val typical = typicalTrainMs(input, repositoryId)
            val liveTrains = input.trains
                .filter { it.repositoryId == repositoryId && it.state in LIVE_TRAIN && it.finishedAt.isNullOrEmpty() }
                .sortedWith { a, b ->
                    val d = (msOf(a.createdAt) ?: 0L).compareTo(msOf(b.createdAt) ?: 0L)
                    if (d != 0) d else if (a.id < b.id) -1 else 1
                }
            val testing = liveTrains.map { train ->
                val members = train.units.flatten().mapNotNull { id ->
                    prOf(id)?.let { pr -> QueueMember(pr.id, "#${pr.number}", pr.title, authorOf(pr, input)) }
                }
                val runs = newestFirst(input.runs.filter { it.mergeTrainId == train.id })
                val statuses = runs.map { it to statusOf(it, input.jobs) }
                val status = STATUS_RANK.firstOrNull { s -> statuses.any { it.second == s } } ?: CiRun.RunStatus.Queued
                val deciding = statuses.firstOrNull { it.second == status }?.first
                val starts = runs.mapNotNull { msOf(it.startedAt) }
                val started = starts.minOrNull()
                val eta = started?.let { trainEta(typical, now - it) }
                val parts = ArrayList<String>()
                parts += train.baseSha?.takeIf { it.isNotEmpty() }?.let { copy.BASE.replace("{base}", train.base).replace("{sha}", it.take(7)) } ?: train.base
                if (train.attempt > 0) parts += copy.RERUN
                if (!train.bisectOf.isNullOrEmpty() && train.pending.isNotEmpty()) parts += copy.SPLIT.replace("{count}", train.pending.size.toString())
                if (train.untested) parts += copy.UNTESTED
                QueueNow(
                    trainId = train.id,
                    title = if (members.size == 1) members[0].title else Copy.BATCH_TITLE.replace("{numbers}", members.joinToString(", ") { it.identifier }),
                    subtitle = parts.joinToString(" · "),
                    members = members,
                    status = status,
                    runId = deciding?.id,
                    elapsed = if (started == null) Copy.WAITING_FOR_RUNNER else copy.RUNNING_FOR.replace("{duration}", PrChecks.formatDuration(now - started)),
                    eta = eta?.eta,
                    progress = eta?.progress,
                )
            }
            val open = input.pullRequests.filter { it.repositoryId == repositoryId && it.state == "open" }
            val upNext = open.filter { it.mergeState == "queued" }
                .sortedWith(compareBy<Pr> { it.mergePosition ?: Int.MAX_VALUE }.thenBy { it.number })
                .map { pr ->
                    val position = pr.mergePosition
                    QueueRow(
                        id = pr.id,
                        pullRequestId = pr.id,
                        identifier = "#${pr.number}",
                        title = pr.title,
                        subtitle = when {
                            pr.mergeNow -> copy.MERGE_NOW
                            position != null && position != 0 -> copy.POSITION.replace("{ordinal}", MergeTrain.ordinal(position))
                            else -> null
                        },
                        glyph = if (pr.mergeNow) "zap" else "ci-queued",
                        tone = if (pr.mergeNow) "warning" else "neutral",
                        badge = null, at = null, action = null, runId = null, jobId = null, url = null,
                        menu = true,
                    )
                }
            val waiting = open.filter { it.mergeState == "checks" || it.mergeState == "held" || it.mergeState == "conflict" || it.mergeState == "failing" }
                .sortedBy { it.number }
                .map { pr ->
                    val held = pr.mergeState == "held"
                    val text = when {
                        held -> copy.HOLDS[pr.mergeHold ?: "person"] ?: copy.HOLDS.getValue("person")
                        pr.mergeState == "checks" -> copy.WAITS[pr.mergeWait ?: "checks"] ?: copy.WAITS.getValue("checks")
                        else -> copy.WAITS.getValue(pr.mergeState!!)
                    }
                    val tone = when {
                        pr.mergeState == "conflict" || pr.mergeState == "failing" || (pr.mergeState == "checks" && pr.mergeWait == "failing") -> "danger"
                        pr.mergeState == "checks" && (pr.mergeWait ?: "checks") == "checks" -> "info"
                        else -> "neutral"
                    }
                    QueueRow(
                        id = pr.id,
                        pullRequestId = pr.id,
                        identifier = "#${pr.number}",
                        title = pr.title,
                        subtitle = null,
                        glyph = if (held) "ci-skipped" else if (tone == "danger") "ci-failed" else "ci-queued",
                        tone = if (tone == "info") "neutral" else tone,
                        badge = Pill(text, tone),
                        at = null,
                        action = if (held && pr.mergeHold != null && pr.mergeHold != "person") "resume" else null,
                        runId = null, jobId = null, url = null,
                        menu = false,
                    )
                }
            val events = input.trainEvents
                .filter { it.repositoryId == repositoryId && eventMs(it) >= since }
                .sortedWith { a, b ->
                    val d = eventMs(b).compareTo(eventMs(a))
                    if (d != 0) d else if (a.id < b.id) -1 else 1
                }
            val sentBack = events.filter { it.kind == "failing" || it.kind == "conflict" }.take(LIMIT_HISTORY).mapNotNull { e ->
                val pr = prOf(e.pullRequestId) ?: return@mapNotNull null
                val job = e.ciJobId?.let { id -> input.jobs.firstOrNull { it.id == id } }
                val subtitle = if (e.kind == "failing") {
                    "${copy.FAILED_IN.replace("{checks}", shortList(e.checks, 2))} · ${tries(e.attempt)}"
                } else {
                    "${copy.CONFLICT_IN.replace("{files}", shortList(e.files.map(::fileName), 2))} · ${tries(e.attempt)}"
                }
                val action = if (e.kind == "failing" && (job != null || !e.jobUrl.isNullOrEmpty())) "open-log" else null
                QueueRow(
                    id = e.id,
                    pullRequestId = pr.id,
                    identifier = "#${pr.number}",
                    title = pr.title,
                    subtitle = subtitle,
                    glyph = if (e.kind == "conflict") "pr-conflict" else "ci-failed",
                    tone = "danger",
                    badge = null,
                    at = e.createdAt,
                    action = action,
                    runId = if (action == "open-log") job?.runId else null,
                    jobId = if (action == "open-log") job?.id else null,
                    url = if (action == "open-log" && job == null) e.jobUrl else null,
                    menu = false,
                )
            }
            // A PR sent back in the window is listed there, not again here.
            val sentBackPrs = sentBack.map { it.pullRequestId }.toSet()
            val notReady = waiting.filter { row ->
                val pr = prOf(row.pullRequestId)!!
                !((pr.mergeState == "conflict" || pr.mergeState == "failing") && pr.id in sentBackPrs)
            }
            val landedEvents = events.filter { it.kind == "landed" }
            val landed = landedEvents.take(LIMIT_HISTORY).mapNotNull { e ->
                val pr = prOf(e.pullRequestId) ?: return@mapNotNull null
                val train = e.mergeTrainId?.let { id -> input.trains.firstOrNull { it.id == id } }
                val sha = e.sha?.takeIf { it.isNotEmpty() }?.take(7)
                val others = landedEvents
                    .filter { o -> !o.mergeTrainId.isNullOrEmpty() && o.mergeTrainId == e.mergeTrainId && o.pullRequestId != e.pullRequestId }
                    .mapNotNull { o -> prOf(o.pullRequestId)?.number }
                    .sorted()
                val untested = train?.untested == true
                val parts = ArrayList<String>()
                if (untested) parts += copy.MERGED_NOW else if (others.isNotEmpty()) parts += copy.LANDED_WITH.replace("{numbers}", numbered(others))
                if (sha != null) parts += sha
                QueueRow(
                    id = e.id,
                    pullRequestId = pr.id,
                    identifier = "#${pr.number}",
                    title = pr.title,
                    subtitle = if (parts.isNotEmpty()) parts.joinToString(" · ") else null,
                    glyph = if (untested) "zap" else "ci-passed",
                    tone = if (untested) "warning" else "success",
                    badge = null, at = e.createdAt, action = null, runId = null, jobId = null, url = null,
                    menu = false,
                )
            }
            fun isToday(e: TrainEvent) = localDay(eventMs(e), input.utcOffsetMinutes) == today
            val base = if (testing.isNotEmpty()) {
                liveTrains.first().base
            } else {
                open.firstOrNull { it.mergeState == "queued" }?.base ?: events.firstOrNull()?.base ?: open.firstOrNull()?.base ?: "main"
            }
            out += QueueRepo(
                repositoryId = repositoryId,
                title = repoName(input.repos, repositoryId),
                caption = copy.INTO.replace("{base}", base),
                stats = QueueStats(
                    inQueue = testing.sumOf { it.members.size } + upNext.size,
                    landedToday = landedEvents.count(::isToday),
                    sentBackToday = events.count { (it.kind == "failing" || it.kind == "conflict") && isToday(it) },
                    typicalRun = typical?.let(PrChecks::formatDuration),
                ),
                testing = testing,
                upNext = upNext,
                waiting = notReady,
                sentBack = sentBack,
                landed = landed,
            )
        }
        return out.sortedWith { a, b ->
            val d = a.title.compareTo(b.title)
            if (d != 0) d else if (a.repositoryId < b.repositoryId) -1 else 1
        }
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

    /** #EXP-2214 tab counts (0 = none): the merge queue's PRs (testing + up
     *  next), the Open pull requests, the release runs in progress, the runners. */
    fun tabCounts(input: Input): TabCounts {
        val all = RunFilter()
        return TabCounts(
            train = queue(input).sumOf { it.stats.inQueue },
            prs = prGroups(input, all).firstOrNull { it.id == "open" }?.count ?: 0,
            releases = runGroups(input, all, listOf("release")).firstOrNull { it.id == "in-progress" }?.count ?: 0,
            runners = input.runners.size,
        )
    }

    // ── #EXP-2214 THE RUN PAGE'S CONTEXT ────────────────────────────────

    /** `runContextCases`: what the run page says about WHY the run exists
     *  (the TS reference's `ciRunContext` doc comment is the rule). */
    fun runContext(run: Run, input: Input): RunContext {
        val kind = runKind(run, input.repos)
        val pr = run.pullRequestId?.let { id -> input.pullRequests.firstOrNull { it.id == id } }
        val badge = if (kind == "pull_request" && pr != null) Copy.PULL_REQUEST.replace("{number}", pr.number.toString()) else Copy.KINDS.getValue(kind)
        val train = run.mergeTrainId?.let { id -> input.trains.firstOrNull { it.id == id } }
        if (kind != "train" || train == null) {
            return RunContext(kind, badge, if (kind == "train") batchTitle(run, input.pullRequests, input.trains) else null, null, emptyList())
        }
        val out = Copy.Queue.OUTCOME
        val outcome = train.units.flatten().mapNotNull { id ->
            val member = input.pullRequests.firstOrNull { it.id == id } ?: return@mapNotNull null
            val event = input.trainEvents
                .filter { it.pullRequestId == id && it.mergeTrainId == train.id && (it.kind == "landed" || it.kind == "failing" || it.kind == "conflict") }
                .sortedByDescending(::eventMs)
                .firstOrNull()
            fun row(text: String, glyph: String, tone: String, jobId: String? = null) =
                RunOutcome(id, "#${member.number}", member.title, text, glyph, tone, jobId)
            when {
                event?.kind == "landed" -> if (train.untested) {
                    row(out.getValue("untested"), "zap", "warning")
                } else {
                    row(out.getValue("landed").replace("{sha}", (event.sha ?: "").take(7)), "ci-passed", "success")
                }
                event?.kind == "failing" -> {
                    val job = event.ciJobId?.let { jobId -> input.jobs.firstOrNull { it.id == jobId && it.runId == run.id } }
                    row(
                        out.getValue("failing").replace("{checks}", shortList(event.checks, 2)).replace("{tries}", tries(event.attempt)),
                        "ci-failed", "danger", job?.id,
                    )
                }
                event?.kind == "conflict" -> row(
                    out.getValue("conflict").replace("{files}", shortList(event.files.map(::fileName), 2)).replace("{tries}", tries(event.attempt)),
                    "pr-conflict", "danger",
                )
                member.mergeState == "testing" && member.mergeTrainId == train.id -> row(out.getValue("testing"), "ci-running", "info")
                else -> row(out.getValue("requeued"), "ci-queued", "neutral")
            }
        }
        return RunContext(
            kind = kind,
            badge = badge,
            title = batchTitle(run, input.pullRequests, input.trains),
            caption = train.baseSha?.takeIf { it.isNotEmpty() }?.let { Copy.Queue.BASE.replace("{base}", train.base).replace("{sha}", it.take(7)) } ?: train.base,
            outcome = outcome,
        )
    }
}
