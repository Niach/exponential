package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.ThreadEntity
import com.exponential.app.data.db.ThreadItemEntity

/**
 * EXP-1284: the phone's thread page rules around the shared model
 * ([ThreadView], fixture-locked ×4 — never forked here). The twin of web
 * `lib/thread-page.ts` + `hooks/use-threads.ts`: how synced rows become the
 * model's inputs (a run named by its session identity, its host's renamed
 * label), where `Reply in this thread` goes, whether a child is fresh enough
 * to fade in with a ring, which PR the thread's Guide merges, and the Guide
 * a reader shows for a run.
 */
object ThreadPage {

    /** A run's state as the thread shows it: a heartbeat-dead row reads
     *  ended (the lists' rule), a sweep end (EXP-888) still live. */
    fun runStatus(row: CodingSessionEntity, nowMs: Long): String = when {
        !isSessionLive(row, nowMs) -> DomainContract.codingSessionStatusEnded
        row.status == DomainContract.codingSessionStatusEnded -> DomainContract.codingSessionStatusRunning
        else -> row.status
    }

    fun toThread(thread: ThreadEntity) = ThreadView.Thread(
        id = thread.id,
        title = thread.title,
        results = thread.results,
        updatedAt = thread.updatedAt,
    )

    fun toItem(item: ThreadItemEntity) = ThreadView.Item(
        id = item.id,
        kind = if (item.kind == DomainContract.threadItemKindRun) {
            DomainContract.threadItemKindRun
        } else {
            DomainContract.threadItemKindIssue
        },
        issueId = item.issueId,
        sessionId = item.sessionId,
        source = if (item.source == DomainContract.threadItemSourceAgent) {
            DomainContract.threadItemSourceAgent
        } else {
            DomainContract.threadItemSourceUser
        },
        addedBySessionId = item.addedBySessionId,
        sortOrder = item.sortOrder,
        createdAt = item.createdAt,
    )

    fun toIssue(issue: IssueEntity) = ThreadView.Issue(
        id = issue.id,
        identifier = issue.identifier,
        title = issue.title,
        createdAt = issue.createdAt,
    )

    /** A synced run → the model's run; [name] = its session identity subject
     *  ([pastRunTitle]), [deviceLabel] its host's live label. */
    fun toRun(row: CodingSessionEntity, name: String, deviceLabel: String?, nowMs: Long) = ThreadView.Run(
        id = row.id,
        name = name,
        userId = row.userId,
        deviceLabel = deviceLabel,
        status = runStatus(row, nowMs),
        needsInput = row.needsInput,
        agentBusy = row.agentBusy,
        issueId = row.issueId,
        parentSessionId = row.parentSessionId,
        resumedFromId = row.resumedFromId,
        startedAt = row.startedAt,
        endedAt = row.endedAt,
    )

    // ── The reply box ───────────────────────────────────────────────────────

    /** Where `Reply in this thread` goes (web `threadReplyTarget`). */
    sealed interface ReplyTarget {
        /** The latest LIVE run the viewer owns — the existing steer send. */
        data class Send(val runId: String) : ReplyTarget

        /** No live run of theirs, but the thread's latest run IS theirs:
         *  resume it (the resume stays in the thread), then send. */
        data class Resume(val runId: String) : ReplyTarget

        /** Otherwise a new run in this thread. */
        data object Start : ReplyTarget
    }

    /** Null = the viewer cannot steer at all (not a member, no relay). */
    fun replyTarget(runs: List<ThreadView.Run>, viewerId: String?, canSteer: Boolean): ReplyTarget? {
        if (!canSteer || viewerId == null) return null
        val ordered = runs.sortedWith(
            compareByDescending<ThreadView.Run> { epoch(it.startedAt) }.thenByDescending { it.id },
        )
        ordered.firstOrNull { it.userId == viewerId && it.status != DomainContract.codingSessionStatusEnded }
            ?.let { return ReplyTarget.Send(it.id) }
        val latest = ordered.firstOrNull()
        if (latest != null && latest.userId == viewerId) return ReplyTarget.Resume(latest.id)
        return ReplyTarget.Start
    }

    // ── A child the viewer has not seen yet ─────────────────────────────────

    /** A child created at most this long before it FIRST renders fades in
     *  with a ring (the agent just linked it). */
    const val FRESH_CHILD_MS = 10_000L

    fun isFreshChild(createdAt: String?, firstSeenAtMs: Long): Boolean {
        val created = createdAt?.let { WireTimestamps.parseEpochMs(it) } ?: return false
        return firstSeenAtMs - created <= FRESH_CHILD_MS && firstSeenAtMs >= created - 1_000
    }

    // ── The thread's PR ─────────────────────────────────────────────────────

    /** The PR the thread's Guide and Merge capsule target ×4
     *  ([ThreadView.prTarget]): the newest OPEN PR on the thread's sessions
     *  and issues, else the newest MERGED one. */
    fun prUrl(runs: List<CodingSessionEntity>, issues: List<IssueEntity>): String? =
        ThreadView.prTarget(
            runs.mapNotNull { run ->
                run.prUrl?.takeIf { it.isNotBlank() }?.let { ThreadView.PrCandidate(it, run.prState, run.updatedAt) }
            } + issues.mapNotNull { issue ->
                issue.prUrl?.takeIf { it.isNotBlank() }?.let { ThreadView.PrCandidate(it, issue.prState, issue.updatedAt) }
            },
        )

    /** EXP-1284 (rule 8): the Guide a reader shows for a run — its THREAD's
     *  once the thread has one, else the run's own (web `guideOfRun`). */
    fun guideResults(runResults: String?, threadResults: String?): String? =
        threadResults ?: runResults

    /** A session BLOCK's Guide: only the entries that session wrote, out of
     *  the thread's Guide (else its own row's mirror); unstamped legacy
     *  entries belong to the thread's first session
     *  ([ThreadView.sessionGuideEntries]). */
    fun sessionGuide(run: CodingSessionEntity, thread: ThreadEntity?, firstSessionId: String?): String? =
        ThreadView.sessionGuideEntries(guideResults(run.results, thread?.results), run.id, firstSessionId)

    // ── Bundles (web `useThreadBundles`) ────────────────────────────────────

    /** One thread, resolved. */
    data class Bundle(
        val thread: ThreadEntity,
        val items: List<ThreadItemEntity>,
        /** The runs its run items name, start order. */
        val runs: List<CodingSessionEntity>,
        /** The issues its issue items name (visible ones), link order. */
        val issues: List<IssueEntity>,
        val view: ThreadView.View,
        /** The viewer owns a live session in it ([ThreadView.partition]). */
        val mineLive: Boolean,
        /** The viewer has a session in it or created it (Recent lists only these). */
        val mine: Boolean,
        /** The newest live run of the viewer's. */
        val myLiveRun: CodingSessionEntity?,
        /** The newest live run of anyone's (its state = a list row's mark). */
        val liveRun: CodingSessionEntity?,
    )

    fun bundles(
        threads: List<ThreadEntity>,
        items: List<ThreadItemEntity>,
        sessions: List<CodingSessionEntity>,
        issues: List<IssueEntity>,
        devices: List<DeviceEntity>,
        viewerId: String?,
        nowMs: Long,
    ): List<Bundle> {
        val issuesById = issues.associateBy { it.id }
        val runsById = sessions.associateBy { it.id }
        val itemsByThread = items.groupBy { it.threadId }
        return threads.map { thread ->
            val own = itemsByThread[thread.id].orEmpty()
            val threadRuns = own.mapNotNull { item -> item.sessionId?.let(runsById::get) }
                .sortedWith(compareBy<CodingSessionEntity> { epoch(it.startedAt) }.thenBy { it.id })
            val ordered = own.sortedWith(
                compareBy<ThreadItemEntity> { it.sortOrder }.thenBy { epoch(it.createdAt) }.thenBy { it.id },
            )
            val threadIssues = ordered.mapNotNull { item -> item.issueId?.let(issuesById::get) }
            val view = ThreadView.build(
                thread = toThread(thread),
                items = own.map(::toItem),
                issues = threadIssues.map(::toIssue),
                runs = threadRuns.map { run ->
                    toRun(
                        run,
                        name = pastRunTitle(run, run.issueId?.let(issuesById::get), batchRunIssues(run, issues)),
                        deviceLabel = resolveSessionDevice(run, devices, nowMs).label,
                        nowMs = nowMs,
                    )
                },
            )
            val liveRuns = threadRuns.filter { runStatus(it, nowMs) != DomainContract.codingSessionStatusEnded }
            val myLive = liveRuns.filter { it.userId == viewerId }
            Bundle(
                thread = thread,
                items = own,
                runs = threadRuns,
                issues = threadIssues,
                view = view,
                mineLive = myLive.isNotEmpty(),
                mine = viewerId != null && (thread.createdBy == viewerId || threadRuns.any { it.userId == viewerId }),
                myLiveRun = myLive.lastOrNull(),
                liveRun = liveRuns.lastOrNull(),
            )
        }
    }

    /** The list surfaces' model: Running (a live session of mine) vs Recent
     *  (my other threads), newest activity first. Phones list EVERY team
     *  ([teamId] null) and band the rows per team (T6). A thread whose every
     *  child is gone (trashed board, purged sessions) has nothing to show. */
    fun list(bundles: List<Bundle>, teamId: String? = null): ThreadView.Partition<Bundle> {
        val parts = ThreadView.partition(
            bundles
                .filter { it.runs.isNotEmpty() || it.issues.isNotEmpty() }
                .map {
                    ThreadView.ListEntry(
                        threadId = it.thread.id,
                        teamId = it.thread.teamId,
                        updatedAt = it.thread.updatedAt,
                        mineLive = it.mineLive,
                        mine = it.mine,
                        value = it,
                    )
                },
            teamId,
        )
        return ThreadView.Partition(
            running = parts.running.map { it.value },
            recent = parts.recent.map { it.value },
        )
    }

    /** T4: my LIVE sessions no thread item names yet (placement failed or
     *  the thread-items shape lags) — listed in Running as plain session
     *  rows so a live session never vanishes. Heartbeat-aware; newest first. */
    fun orphanLiveSessions(
        sessions: List<CodingSessionEntity>,
        items: List<ThreadItemEntity>,
        viewerId: String?,
        nowMs: Long,
    ): List<CodingSessionEntity> {
        if (viewerId == null) return emptyList()
        val placed = items.mapNotNullTo(HashSet()) { it.sessionId }
        return sessions
            .filter { it.userId == viewerId && it.id !in placed && isSessionLive(it, nowMs) }
            .sortedByDescending { epoch(it.startedAt) }
    }

    private fun epoch(value: String?): Long = value?.let { WireTimestamps.parseEpochMs(it) } ?: 0L
}
