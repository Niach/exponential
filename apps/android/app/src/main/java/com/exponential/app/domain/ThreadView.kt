package com.exponential.app.domain

/**
 * EXP-1284: what a THREAD looks like, resolved once and rendered identically
 * ×4 (web `lib/thread-view.ts`, desktop `domain/src/thread_view.rs`, iOS
 * `ThreadView.swift`, here), locked by `packages/domain-contract/fixtures/
 * thread-view.json` (`ThreadViewTest` runs every case). A thread holds n
 * issues and m runs (`thread_items`); this turns the synced rows into:
 *
 *  * its TITLE (the stored one, else the first run's name unless that is the
 *    generic `Chat`, else the first issue's title, else `Chat`/`Thread`);
 *  * its CHILD STRIP: Conversation, one chip per issue in link order, the "+"
 *    and the Guide (dot when the thread's Guide has entries). On phones the
 *    strip IS the pager;
 *  * its CAPTIONS: the header's `2 runs · 2 devices` and a list row's
 *    `2 issues · 1 run · macbook` (the device part only while a run is live,
 *    naming the live runs' devices);
 *  * its CONVERSATION: every run in start order as a `Run N` block, the issues
 *    its agent linked as rows under it (`Linked` / `Filed and linked`), and the
 *    runs one parent started as ONE parallel card once there are two or more;
 *  * its LIST placement: Running (a live run of mine) vs Recent.
 *
 * Pure: callers pass what they synced; an item whose issue or run did not
 * resolve is dropped (trashed board, purged run).
 */
object ThreadView {

    data class Thread(
        val id: String,
        val title: String?,
        /** The raw `threads.results` jsonb text (null / `[]` = no Guide). */
        val results: String?,
        val updatedAt: String,
    )

    data class Item(
        val id: String,
        /** `issue` | `run` (contract `threadItemKind`). */
        val kind: String,
        val issueId: String?,
        val sessionId: String?,
        /** `user` | `agent` (contract `threadItemSource`). */
        val source: String,
        val addedBySessionId: String?,
        val sortOrder: Double,
        val createdAt: String,
    )

    data class Issue(
        val id: String,
        val identifier: String,
        val title: String,
        val createdAt: String,
    )

    data class Run(
        val id: String,
        /** The run's identity subject ([pastRunTitle]): issue title, action
         *  name, chat title…, used where a run is named on its own. */
        val name: String,
        val userId: String,
        val deviceLabel: String?,
        /** `running` | `in_review` | `ended`. */
        val status: String,
        val needsInput: Boolean,
        val agentBusy: Boolean,
        val issueId: String?,
        val parentSessionId: String?,
        val resumedFromId: String?,
        val startedAt: String,
        val endedAt: String?,
    )

    enum class RunState(val wire: String) {
        Live("live"),
        NeedsInput("needs_input"),
        Ended("ended"),
        ;

        /** The trailing state word a block header shows. */
        val word: String
            get() = when (this) {
                Live -> "live"
                NeedsInput -> "needs input"
                Ended -> "ended"
            }
    }

    sealed interface Child {
        data object Conversation : Child
        data class IssueChild(val issueId: String, val identifier: String, val title: String) : Child
        data object Add : Child
        data class Guide(val dot: Boolean) : Child
    }

    data class LinkRow(
        val itemId: String,
        val issueId: String,
        val identifier: String,
        val title: String,
        /** [LINKED_LABEL] | [FILED_LABEL] (the agent filed it, then linked it). */
        val label: String,
    )

    sealed interface Block {
        val label: String
        val caption: List<String>
        val state: RunState
        val links: List<LinkRow>
    }

    data class RunBlock(
        val runId: String,
        /** `Run 1`, … in start order across the whole thread. */
        override val label: String,
        /** The muted parts after the label: device, `resumed`, `started by Session K`. */
        override val caption: List<String>,
        override val state: RunState,
        /** Ended runs fold to one line until Show work opens them. */
        val folded: Boolean,
        override val links: List<LinkRow>,
    ) : Block

    data class ParallelRow(
        val runId: String,
        val name: String,
        val issueId: String?,
        val deviceLabel: String?,
        val state: RunState,
        val startedAt: String,
    )

    data class ParallelBlock(
        /** `Runs 3 and 4`, `Runs 3, 4 and 5`. */
        override val label: String,
        /** `started by Session 2`. */
        override val caption: List<String>,
        /** Live while any of its runs is. */
        override val state: RunState,
        val runs: List<ParallelRow>,
        override val links: List<LinkRow>,
    ) : Block

    data class View(
        val title: String,
        /** True when [title] is derived (the thread has no stored title). */
        val titleDerived: Boolean,
        val strip: List<Child>,
        val headerCaption: String,
        val listCaption: String,
        val live: Boolean,
        val needsInput: Boolean,
        val busy: Boolean,
        val conversation: List<Block>,
        /** Run ids in start order (Run 1 first). */
        val runIds: List<String>,
        /** Issue ids in link order. */
        val issueIds: List<String>,
    )

    data class ListEntry<T>(
        val threadId: String,
        val updatedAt: String,
        /** The viewer owns a live run in it. */
        val mineLive: Boolean,
        val value: T,
    )

    data class Partition<T>(val running: List<T>, val recent: List<T>)

    const val FALLBACK_TITLE = "Thread"
    const val LINKED_LABEL = "Linked"
    const val FILED_LABEL = "Filed and linked"

    /** An issue created at most this long before the agent linked it was
     *  FILED by that agent for the thread. */
    const val FILED_WINDOW_MS = 120_000L

    fun runState(status: String, needsInput: Boolean): RunState = when {
        status == DomainContract.codingSessionStatusEnded -> RunState.Ended
        needsInput -> RunState.NeedsInput
        else -> RunState.Live
    }

    private fun time(value: String?): Long =
        value?.let { WireTimestamps.parseEpochMs(it) } ?: 0L

    private fun plural(count: Int, one: String, many: String): String =
        "$count ${if (count == 1) one else many}"

    /** `3`, `3 and 4`, `3, 4 and 5`. */
    private fun joinOrdinals(ordinals: List<Int>): String {
        if (ordinals.size <= 1) return ordinals.joinToString("")
        return "${ordinals.dropLast(1).joinToString(", ")} and ${ordinals.last()}"
    }

    private fun devicesPart(runs: List<Run>): String? {
        val labels = runs.mapNotNull { it.deviceLabel?.trim()?.ifEmpty { null } }.distinct()
        if (labels.isEmpty()) return null
        return if (labels.size == 1) labels[0] else "${labels.size} devices"
    }

    /** The thread has a Guide: its results are a NON-EMPTY JSON array. */
    fun hasGuide(results: String?): Boolean {
        val raw = results?.trim() ?: return false
        if (!raw.startsWith("[")) return false
        return runCatching {
            kotlinx.serialization.json.Json.parseToJsonElement(raw) as? kotlinx.serialization.json.JsonArray
        }.getOrNull()?.isNotEmpty() == true
    }

    fun build(thread: Thread, items: List<Item>, issues: List<Issue>, runs: List<Run>): View {
        val issuesById = issues.associateBy { it.id }
        val runsById = runs.associateBy { it.id }
        val sortedItems = items.sortedWith(
            compareBy<Item> { it.sortOrder }.thenBy { time(it.createdAt) }.thenBy { it.id },
        )

        val issueItems = sortedItems.filter {
            it.kind == DomainContract.threadItemKindIssue && it.issueId != null && issuesById.containsKey(it.issueId)
        }
        val threadIssues = issueItems.map { issuesById.getValue(it.issueId!!) }
        val threadRuns = sortedItems
            .filter { it.kind == DomainContract.threadItemKindRun && it.sessionId != null && runsById.containsKey(it.sessionId) }
            .map { runsById.getValue(it.sessionId!!) }
            .sortedWith(compareBy<Run> { time(it.startedAt) }.thenBy { it.id })

        // Named after its first prompt (the first run's own name), the first
        // issue only while that run is still the generic `Chat`.
        val stored = thread.title?.trim()?.ifEmpty { null }
        val firstRun = threadRuns.firstOrNull()?.name?.trim().orEmpty()
        val derived = (if (firstRun.isNotEmpty() && firstRun != CHAT_RUN_NAME) firstRun else "")
            .ifEmpty { threadIssues.firstOrNull()?.title?.trim().orEmpty() }
            .ifEmpty { firstRun }
            .ifEmpty { FALLBACK_TITLE }
        val title = stored ?: derived

        val strip = buildList {
            add(Child.Conversation)
            threadIssues.forEach { add(Child.IssueChild(it.id, it.identifier, it.title)) }
            add(Child.Add)
            add(Child.Guide(dot = hasGuide(thread.results)))
        }

        val liveRuns = threadRuns.filter { it.status != DomainContract.codingSessionStatusEnded }
        val runsPart = plural(threadRuns.size, "session", "sessions")
        val headerParts = listOfNotNull(runsPart, devicesPart(threadRuns))
        val issuesPart = if (threadIssues.isEmpty()) "no issues" else plural(threadIssues.size, "issue", "issues")
        val listParts = listOfNotNull(issuesPart, runsPart, if (liveRuns.isNotEmpty()) devicesPart(liveRuns) else null)

        val ordinal = threadRuns.withIndex().associate { (index, run) -> run.id to index + 1 }

        fun linksOf(runIds: Set<String>): List<LinkRow> = issueItems
            .filter {
                it.source == DomainContract.threadItemSourceAgent &&
                    it.addedBySessionId != null && it.addedBySessionId in runIds
            }
            .sortedWith(compareBy<Item> { time(it.createdAt) }.thenBy { it.id })
            .map { item ->
                val issue = issuesById.getValue(item.issueId!!)
                val filed = time(issue.createdAt) >= time(item.createdAt) - FILED_WINDOW_MS
                LinkRow(
                    itemId = item.id,
                    issueId = issue.id,
                    identifier = issue.identifier,
                    title = issue.title,
                    label = if (filed) FILED_LABEL else LINKED_LABEL,
                )
            }

        // Children one in-thread parent started, grouped once there are 2+.
        val childrenOf = LinkedHashMap<String, MutableList<Run>>()
        for (run in threadRuns) {
            val parent = run.parentSessionId
            if (parent != null && ordinal.containsKey(parent) && run.resumedFromId == null) {
                childrenOf.getOrPut(parent) { mutableListOf() }.add(run)
            }
        }

        val conversation = mutableListOf<Block>()
        val placed = mutableSetOf<String>()
        for (run in threadRuns) {
            if (run.id in placed) continue
            val siblings = if (run.parentSessionId != null && run.resumedFromId == null) {
                childrenOf[run.parentSessionId]
            } else {
                null
            }
            val startedBy = run.parentSessionId?.let { ordinal[it] }?.let { "started by Session $it" }
            if (siblings != null && siblings.size >= 2) {
                siblings.forEach { placed.add(it.id) }
                val states = siblings.map { runState(it.status, it.needsInput) }
                conversation.add(
                    ParallelBlock(
                        label = "Sessions ${joinOrdinals(siblings.map { ordinal.getValue(it.id) })}",
                        caption = listOfNotNull(startedBy),
                        state = when {
                            RunState.NeedsInput in states -> RunState.NeedsInput
                            RunState.Live in states -> RunState.Live
                            else -> RunState.Ended
                        },
                        runs = siblings.map {
                            ParallelRow(
                                runId = it.id,
                                name = it.name,
                                issueId = it.issueId,
                                deviceLabel = it.deviceLabel,
                                state = runState(it.status, it.needsInput),
                                startedAt = it.startedAt,
                            )
                        },
                        links = linksOf(siblings.map { it.id }.toSet()),
                    ),
                )
                continue
            }
            placed.add(run.id)
            val state = runState(run.status, run.needsInput)
            conversation.add(
                RunBlock(
                    runId = run.id,
                    label = "Session ${ordinal.getValue(run.id)}",
                    caption = listOfNotNull(
                        run.deviceLabel?.trim()?.ifEmpty { null },
                        if (run.resumedFromId != null) "resumed" else null,
                        startedBy,
                    ),
                    state = state,
                    folded = state == RunState.Ended,
                    links = linksOf(setOf(run.id)),
                ),
            )
        }

        return View(
            title = title,
            titleDerived = stored == null,
            strip = strip,
            headerCaption = headerParts.joinToString(" · "),
            listCaption = listParts.joinToString(" · "),
            live = liveRuns.isNotEmpty(),
            needsInput = liveRuns.any { it.needsInput },
            busy = liveRuns.any { it.agentBusy },
            conversation = conversation,
            runIds = threadRuns.map { it.id },
            issueIds = threadIssues.map { it.id },
        )
    }

    /** Running (a live run of mine, newest activity first) vs Recent (every
     *  other thread, newest activity first). */
    fun <T> partition(entries: List<ListEntry<T>>): Partition<ListEntry<T>> {
        val byActivity = entries.sortedWith(
            compareByDescending<ListEntry<T>> { time(it.updatedAt) }.thenBy { it.threadId },
        )
        return Partition(
            running = byActivity.filter { it.mineLive },
            recent = byActivity.filter { !it.mineLive },
        )
    }
}
