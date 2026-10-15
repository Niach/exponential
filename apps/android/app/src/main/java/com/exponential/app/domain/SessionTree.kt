package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.SessionIssueEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull

/**
 * EXP-1291 Threads v2: the session tree's view models — the Android mirror of
 * `packages/domain-contract/src/session-tree.ts` (ONE reference ×4: desktop
 * `domain::session_tree`, iOS ExpCore `SessionTree.swift`). Pure and
 * byte-locked by the fixtures `SessionTreeTest` replays:
 *   fixtures/session-card.json   → [sessionCard]
 *   fixtures/session-thread.json → [buildSessionThread], [guidePullRequest],
 *                                  [composerYieldsToCard], [guideMenuEntries]
 *   fixtures/threads-list.json   → [partitionThreads] + [listAgents]
 * The copy / icons / tones / testIds below are the fixtures' (the test
 * proves equality).
 */
object SessionTree {

    // ── inputs (the synced rows, projected) ─────────────────────────────────

    data class Blocked(val window: String? = null, val resetsAt: String? = null)

    data class GuideEntry(val text: String? = null, val label: String? = null)

    data class Session(
        val id: String,
        val teamId: String,
        val parentId: String?,
        val rootId: String,
        val kind: String,
        val userId: String,
        val status: String,
        val busy: Boolean,
        val needsInput: Boolean,
        val blocked: Blocked?,
        val shared: Boolean,
        val title: String?,
        val guide: List<GuideEntry>?,
        val actionName: String?,
        val name: String?,
        val icon: String?,
        val color: String?,
        val sortOrder: Double,
        val startedAt: String?,
        val endedAt: String?,
        val createdAt: String,
        val updatedAt: String,
        /** The steer device that runs it (`devices.device_id`); null = none. */
        val deviceId: String? = null,
    )

    data class SessionIssue(val sessionId: String, val issueId: String, val sortOrder: Double)

    data class Issue(val id: String, val identifier: String, val title: String)

    data class Message(
        val id: String,
        val sessionId: String,
        val authorKind: String,
        val authorId: String?,
        val body: String,
        /** The card's jsonb object; null = a plain text message. */
        val card: JsonObject?,
        val answeredAt: String?,
        val consumedAt: String?,
        val createdAt: String,
    )

    data class PullRequest(val id: String, val number: Int)

    // ── copy (= the fixtures) ───────────────────────────────────────────────

    object CardCopy {
        const val SESSION = "Session"
        val STATES: Map<SessionState, String> = mapOf(
            SessionState.Queued to "Queued",
            SessionState.Working to "Working",
            SessionState.Waiting to "Waiting",
            SessionState.NeedsYou to "Needs you",
            SessionState.Blocked to "Usage limit",
            SessionState.Ended to "Ended",
        )
        /** Icon CONCEPT names (icons.json), per state. */
        val ICONS: Map<SessionState, String> = mapOf(
            SessionState.Queued to "ui-clock",
            SessionState.Working to "coding-running",
            SessionState.Waiting to "coding-assistant",
            SessionState.NeedsYou to "coding-needs-input",
            SessionState.Blocked to "notification-session-blocked",
            SessionState.Ended to "coding-ended",
        )
        /** Tone tokens: muted | foreground | amber | rose. */
        val TONES: Map<SessionState, String> = mapOf(
            SessionState.Queued to "muted",
            SessionState.Working to "foreground",
            SessionState.Waiting to "muted",
            SessionState.NeedsYou to "amber",
            SessionState.Blocked to "rose",
            SessionState.Ended to "muted",
        )
        const val TEST_ID_CARD = "session-card"
    }

    object ThreadCopy {
        const val UNTITLED = "Untitled"
        const val PLACEHOLDER = "Reply…"
        const val CONVERSATION = "Conversation"
        const val GUIDE = "Guide"
        /** Event card templates by contract `messageEventKind`. */
        val EVENTS: Map<String, String> = mapOf(
            "issue_linked" to "Linked {issue}",
            "issue_filed" to "Filed {issue}",
            "issue_unlinked" to "Unlinked {issue}",
            "pr_opened" to "Opened PR {pr}",
            "pr_merged" to "Merged PR {pr}",
            "pr_closed" to "Closed PR {pr}",
            "session_ended" to "Session ended",
        )
        const val TITLE_MAX = 80
        /** EXP-1316 `titles`. */
        const val TITLE_ACTION = "Run {action}"
        const val TITLE_ISSUE = "Implement {issue}"
        const val TITLE_ISSUES = "Implement {issue} + {more}"
        const val TEST_ID_TABS = "thread-tabs"
        const val TEST_ID_CONVERSATION = "thread-conversation"
        const val TEST_ID_COMPOSER = "thread-composer"
        /** EXP-1306: the Guide's stack band title. */
        const val STACK = "Stack"
        const val TEST_ID_GUIDE_STACK = "thread-guide-stack"
        const val TEST_ID_GUIDE_MERGE = "thread-guide-merge"
        /** EXP-1307: the header `…` on the Guide (Open on GitHub · Close PR). */
        const val TEST_ID_GUIDE_MENU = "thread-guide-menu"
    }

    object ListCopy {
        const val TITLE = "Threads"
        const val AGENTS = "Agents"
        val GROUPS: Map<ThreadsGroupKey, String> = mapOf(
            ThreadsGroupKey.NeedsYou to "Needs you",
            ThreadsGroupKey.Running to "Running",
            ThreadsGroupKey.Recent to "Recent",
        )
        const val EMPTY = "No sessions yet"
        const val AGENT_DEFAULT_ICON = "bot"
        const val AGENT_DEFAULT_COLOR = "#6366f1"
        const val TEST_ID_LIST = "threads-list"
        const val TEST_ID_ROW = "threads-row"
        const val TEST_ID_AGENT_ROW = "agents-row"
        /** EXP-1318: the rail band (web + IDE). */
        const val RAIL = "Active"
    }

    // ── session state + card ────────────────────────────────────────────────

    enum class SessionState(val wire: String) {
        Queued("queued"),
        Working("working"),
        Waiting("waiting"),
        NeedsYou("needs_you"),
        Blocked("blocked"),
        Ended("ended"),
    }

    private fun ms(iso: String?): Long? =
        if (iso.isNullOrEmpty()) null else WireTimestamps.parseEpochMs(iso)

    /** The work card's elapsed ladder: `37s`, `2m 04s`, `1h 03m`. */
    fun formatElapsed(durationMs: Long): String {
        val total = maxOf(0L, Math.floorDiv(durationMs, 1000L))
        val hours = total / 3600
        val minutes = (total % 3600) / 60
        val seconds = total % 60
        fun pad(value: Long) = value.toString().padStart(2, '0')
        return when {
            hours > 0 -> "${hours}h ${pad(minutes)}m"
            minutes > 0 -> "${minutes}m ${pad(seconds)}s"
            else -> "${seconds}s"
        }
    }

    /** One state word per session, the card's kind word and tone. */
    fun sessionState(session: Session): SessionState = when {
        session.status == DomainContract.sessionStatusEnded -> SessionState.Ended
        session.needsInput -> SessionState.NeedsYou
        session.blocked != null -> SessionState.Blocked
        session.status == DomainContract.sessionStatusQueued -> SessionState.Queued
        session.busy -> SessionState.Working
        else -> SessionState.Waiting
    }

    /** EXP-1312: a thread row's lead outside the Threads tab (the home's
     *  Running band) ×4: needs you beats working, else the bare mark
     *  (`threads-list.json` `markCases`). */
    fun threadRowMarkState(needsYou: Boolean, busy: Boolean): SessionState = when {
        needsYou -> SessionState.NeedsYou
        busy -> SessionState.Working
        else -> SessionState.Waiting
    }

    /** Sessions of one tree in creation order (ties by id): `Session N` names. */
    fun treeOrder(sessions: List<Session>): List<Session> =
        sessions.sortedWith(compareBy<Session>({ ms(it.createdAt) ?: 0L }, { it.id }))

    data class SessionCard(
        val sessionId: String,
        /** Icon CONCEPT name (`ui-clock`, `coding-running`, …). */
        val icon: String,
        val name: String,
        val kindWord: String,
        val state: SessionState,
        val tone: String,
        val elapsed: String?,
        /** A spinner while true. */
        val running: Boolean,
        val testId: String,
    )

    /**
     * THE session card = the Work card's header row: icon, name, the muted
     * state word, trailing elapsed, a spinner while [SessionCard.running].
     * Tapping it opens that session's raw view. [ordinal] = its 1-based place
     * in the tree's creation order (names an untitled session `Session N`);
     * [now] = an ISO timestamp.
     */
    fun sessionCard(session: Session, ordinal: Int, now: String): SessionCard {
        val state = sessionState(session)
        val start = ms(session.startedAt)
        val end = if (state == SessionState.Ended) {
            ms(session.endedAt) ?: ms(session.updatedAt)
        } else {
            ms(now)
        }
        val elapsed = if (start != null && end != null && state != SessionState.Queued) {
            formatElapsed(end - start)
        } else {
            null
        }
        val title = session.title?.trim()
        return SessionCard(
            sessionId = session.id,
            icon = CardCopy.ICONS.getValue(state),
            name = if (!title.isNullOrEmpty()) title else "${CardCopy.SESSION} $ordinal",
            kindWord = CardCopy.STATES.getValue(state),
            state = state,
            tone = CardCopy.TONES.getValue(state),
            elapsed = elapsed,
            running = state == SessionState.Working,
            testId = CardCopy.TEST_ID_CARD,
        )
    }

    /** First line of a person's text, capped (a thread named by its request). */
    private fun firstLine(body: String, max: Int): String? {
        val line = body.split("\n")
            .map { it.trim() }
            .firstOrNull { it.isNotEmpty() && !it.startsWith("![") }
            ?: return null
        return if (line.length > max) "${line.substring(0, max - 1).trimEnd()}…" else line
    }

    /**
     * EXP-1316: a thread's title, never stored by a person (no rename ×4): an
     * agent's `name`; else `Run <action>`; else `Implement <IDENT>` (the first
     * synced issue by sortOrder, `+ <n-1>` with more); else the stored title
     * (the CLI's auto-name); else the first line of the root's first person
     * message (capped); else `Untitled`.
     */
    fun threadTitle(root: Session, issues: List<Issue>, messages: List<Message>): String {
        val name = root.name?.trim()
        if (root.kind == DomainContract.sessionKindAgent && !name.isNullOrEmpty()) return name
        val action = root.actionName?.trim()
        if (!action.isNullOrEmpty()) return ThreadCopy.TITLE_ACTION.replace("{action}", action)
        if (issues.isNotEmpty()) {
            val issue = issues[0].identifier
            return if (issues.size > 1) {
                ThreadCopy.TITLE_ISSUES.replace("{issue}", issue).replace("{more}", (issues.size - 1).toString())
            } else {
                ThreadCopy.TITLE_ISSUE.replace("{issue}", issue)
            }
        }
        val stored = root.title?.trim()
        if (!stored.isNullOrEmpty()) return stored
        val first = messages
            .filter { it.sessionId == root.id && it.authorKind == DomainContract.messageAuthorKindUser }
            .sortedBy { ms(it.createdAt) ?: 0L }
            .firstOrNull()
        return first?.let { firstLine(it.body, ThreadCopy.TITLE_MAX) } ?: ThreadCopy.UNTITLED
    }

    /** A session chip's lead ×4: the thread row mark of THAT session,
     *  [SessionState.Ended] once it ended. */
    fun sessionTabMarkState(session: Session): SessionState =
        if (session.status == DomainContract.sessionStatusEnded) {
            SessionState.Ended
        } else {
            threadRowMarkState(needsYou = session.needsInput, busy = session.busy)
        }

    private fun rootIssues(rootId: String, sessionIssues: List<SessionIssue>, issues: List<Issue>): List<Issue> {
        val byId = issues.associateBy { it.id }
        return sessionIssues
            .filter { it.sessionId == rootId }
            .sortedBy { it.sortOrder }
            .mapNotNull { byId[it.issueId] }
    }

    // ── the thread screen ───────────────────────────────────────────────────

    sealed interface ThreadTab {
        data object Conversation : ThreadTab
        data class IssueTab(val issueId: String, val identifier: String) : ThreadTab
        data object Add : ThreadTab
        /** EXP-1316: a session chip of the viewed level; [markState] = its lead. */
        data class SessionTab(val sessionId: String, val name: String, val markState: SessionState) : ThreadTab
        data class Guide(val dot: Boolean) : ThreadTab
    }

    sealed interface ThreadRow {
        val id: String

        data class MessageRow(
            override val id: String,
            /** `user` | `agent` | `system`. */
            val author: String,
            val authorId: String?,
            val body: String,
            /** A person's message the device has not consumed yet. */
            val queued: Boolean,
        ) : ThreadRow

        data class SessionRow(override val id: String, val card: SessionCard) : ThreadRow

        data class Question(
            override val id: String,
            val question: String,
            val options: List<String>,
            /** A free-text "Other" answer is offered (always, EXP-1316). */
            val custom: Boolean,
            val answer: String?,
            val answered: Boolean,
        ) : ThreadRow

        data class Plan(override val id: String, val plan: String, val decision: String?) : ThreadRow

        data class Subagent(override val id: String, val title: String, val status: String) : ThreadRow

        data class Picture(
            override val id: String,
            val attachmentId: String,
            val width: Int?,
            val height: Int?,
            val caption: String?,
        ) : ThreadRow

        data class Event(override val id: String, val event: String, val text: String) : ThreadRow
    }

    data class Composer(val sessionId: String, val placeholder: String)

    data class ThreadView(
        val rootId: String,
        val title: String,
        val private: Boolean,
        /** The session open in place (`focus`), null on the thread itself. */
        val focusId: String?,
        val tabs: List<ThreadTab>,
        val rows: List<ThreadRow>,
        val composer: Composer,
        val live: Boolean,
        val busy: Boolean,
        val needsYou: Boolean,
    )

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    /** JS `String(x ?? "")` for a card field. */
    private fun JsonObject.text(key: String): String = when (val value = this[key]) {
        null, is JsonNull -> ""
        is JsonPrimitive -> value.content
        else -> value.toString()
    }

    private fun JsonObject.int(key: String): Int? {
        val value = this[key] as? JsonPrimitive ?: return null
        if (value.isString) return null
        return value.doubleOrNull?.toInt()
    }

    private fun eventText(card: JsonObject, issues: Map<String, Issue>, prs: Map<String, PullRequest>): String? {
        val event = card.text("event")
        val template = ThreadCopy.EVENTS[event] ?: return null
        val issue = card.str("issueId")?.let { issues[it] }
        val pr = card.str("pullRequestId")?.let { prs[it] }
        if (template.contains("{issue}")) {
            if (issue == null) return null
            return template.replaceFirst("{issue}", issue.identifier)
        }
        if (template.contains("{pr}")) {
            if (pr == null) return null
            return template.replaceFirst("{pr}", "#${pr.number}")
        }
        return template
    }

    private val messageOrder = compareBy<Message>({ ms(it.createdAt) ?: 0L }, { it.id })

    /**
     * The thread screen: the header tabs (Conversation · one chip per issue ·
     * + · session chips · Guide; the session chips = the viewed level: the
     * root's own run + its direct children, or with [focus] that session +
     * ITS direct children), the root's conversation as rows (the root's own session
     * card first, then its messages oldest first; a `session` /
     * `thread_started` card becomes that session's card) and the composer
     * target (the root: a message to an ended thread continues it).
     */
    fun buildSessionThread(
        root: Session,
        sessions: List<Session>,
        sessionIssues: List<SessionIssue>,
        issues: List<Issue>,
        messages: List<Message>,
        pullRequests: List<PullRequest>,
        now: String,
        focus: String? = null,
    ): ThreadView {
        val tree = treeOrder(sessions.filter { it.rootId == root.id || it.id == root.id }).toMutableList()
        if (tree.none { it.id == root.id }) tree.add(0, root)
        val ordinals = tree.mapIndexed { index, s -> s.id to index + 1 }.toMap()
        val byId = sessions.associateBy { it.id }.toMutableMap()
        byId[root.id] = root
        val linked = rootIssues(root.id, sessionIssues, issues)
        val issueMap = issues.associateBy { it.id }
        val prMap = pullRequests.associateBy { it.id }
        val own = messages.filter { it.sessionId == root.id }.sortedWith(messageOrder)
        val title = threadTitle(root, linked, own)
        val focused = focus?.let { id -> tree.firstOrNull { it.id == id } }
        val level = focused ?: root
        val sessionTabs = (listOf(level) + tree.filter { it.parentId == level.id && it.id != level.id }).map {
            ThreadTab.SessionTab(
                sessionId = it.id,
                name = sessionCard(it, ordinals[it.id] ?: 1, now).name,
                markState = sessionTabMarkState(it),
            )
        }

        val rows = mutableListOf<ThreadRow>(ThreadRow.SessionRow(root.id, sessionCard(root, 1, now)))
        for (message in own) {
            val card = message.card
            if (card == null) {
                val author = when (message.authorKind) {
                    DomainContract.messageAuthorKindUser, DomainContract.messageAuthorKindAgent -> message.authorKind
                    else -> DomainContract.messageAuthorKindSystem
                }
                if (message.body.isBlank()) continue
                rows.add(
                    ThreadRow.MessageRow(
                        id = message.id,
                        author = author,
                        authorId = message.authorId,
                        body = message.body,
                        queued = author == DomainContract.messageAuthorKindUser && message.consumedAt == null,
                    ),
                )
                continue
            }
            when (card.str("kind")) {
                DomainContract.messageCardKindSession, DomainContract.messageCardKindThreadStarted -> {
                    val target = card.str("sessionId")?.let { byId[it] } ?: continue
                    rows.add(ThreadRow.SessionRow(message.id, sessionCard(target, ordinals[target.id] ?: 1, now)))
                }
                DomainContract.messageCardKindQuestion -> rows.add(
                    ThreadRow.Question(
                        id = message.id,
                        question = card.text("question"),
                        options = (card["options"] as? JsonArray)?.map { option ->
                            if (option is JsonPrimitive) option.content else option.toString()
                        } ?: emptyList(),
                        custom = true,
                        answer = card.str("answer"),
                        answered = message.answeredAt != null,
                    ),
                )
                DomainContract.messageCardKindPlan -> rows.add(
                    ThreadRow.Plan(id = message.id, plan = card.text("plan"), decision = card.str("decision")),
                )
                DomainContract.messageCardKindSubagent -> rows.add(
                    ThreadRow.Subagent(id = message.id, title = card.text("title"), status = card.text("status")),
                )
                DomainContract.messageCardKindPicture -> {
                    val attachmentId = card.str("attachmentId") ?: continue
                    rows.add(
                        ThreadRow.Picture(
                            id = message.id,
                            attachmentId = attachmentId,
                            width = card.int("width"),
                            height = card.int("height"),
                            caption = card.str("caption"),
                        ),
                    )
                }
                DomainContract.messageCardKindEvent -> {
                    val text = eventText(card, issueMap, prMap)
                    if (text != null) rows.add(ThreadRow.Event(message.id, card.text("event"), text))
                }
                else -> Unit
            }
        }

        val guideDot = root.guide.orEmpty().any { entry ->
            (entry.text ?: "").trim().isNotEmpty() || entry.label != null
        }
        val live = tree.filter { it.status != DomainContract.sessionStatusEnded }
        return ThreadView(
            rootId = root.id,
            title = title,
            private = !root.shared,
            focusId = focused?.id,
            tabs = buildList {
                add(ThreadTab.Conversation)
                linked.forEach { add(ThreadTab.IssueTab(it.id, it.identifier)) }
                add(ThreadTab.Add)
                addAll(sessionTabs)
                add(ThreadTab.Guide(guideDot))
            },
            rows = rows,
            composer = Composer(root.id, ThreadCopy.PLACEHOLDER),
            live = live.isNotEmpty(),
            busy = live.any { it.busy },
            needsYou = live.any { it.needsInput },
        )
    }

    // ── the Threads list + the agents ───────────────────────────────────────

    enum class ThreadsGroupKey(val wire: String) {
        NeedsYou("needs_you"),
        Running("running"),
        Recent("recent"),
    }

    data class ThreadsListRow(
        val sessionId: String,
        val teamId: String,
        val title: String,
        val private: Boolean,
        val busy: Boolean,
        val needsYou: Boolean,
        val activityAt: String,
        /** The ROOT's device (the row's trailing device icon); null = none. */
        val deviceId: String? = null,
    )

    data class ThreadsListGroup(val key: ThreadsGroupKey, val title: String, val rows: List<ThreadsListRow>)

    /**
     * The Threads page list: roots of kind `thread` in scope ([teamId] = the
     * ACTIVE team; null = every team, phones band rows per team), grouped
     * Needs you · Running · Recent, each by the tree's newest `updatedAt`
     * desc. Empty groups are omitted.
     */
    fun partitionThreads(
        sessions: List<Session>,
        sessionIssues: List<SessionIssue>,
        issues: List<Issue>,
        messages: List<Message>,
        teamId: String?,
    ): List<ThreadsListGroup> {
        val roots = sessions.filter {
            it.parentId == null && it.kind == DomainContract.sessionKindThread &&
                (teamId == null || it.teamId == teamId)
        }
        val byRoot = LinkedHashMap<String, MutableList<Session>>()
        for (session in sessions) byRoot.getOrPut(session.rootId) { mutableListOf() }.add(session)
        val buckets = linkedMapOf<ThreadsGroupKey, MutableList<ThreadsListRow>>(
            ThreadsGroupKey.NeedsYou to mutableListOf(),
            ThreadsGroupKey.Running to mutableListOf(),
            ThreadsGroupKey.Recent to mutableListOf(),
        )
        for (root in roots) {
            val tree = byRoot[root.id] ?: listOf(root)
            val live = tree.filter { it.status != DomainContract.sessionStatusEnded }
            val needsYou = live.any { it.needsInput }
            val running = live.any {
                it.status == DomainContract.sessionStatusQueued || it.status == DomainContract.sessionStatusRunning
            }
            val activity = tree.fold(root.updatedAt) { latest, s ->
                if ((ms(s.updatedAt) ?: 0L) > (ms(latest) ?: 0L)) s.updatedAt else latest
            }
            val title = threadTitle(root, rootIssues(root.id, sessionIssues, issues), messages)
            val row = ThreadsListRow(
                sessionId = root.id,
                teamId = root.teamId,
                title = title,
                private = !root.shared,
                busy = live.any { it.busy },
                needsYou = needsYou,
                activityAt = activity,
                deviceId = root.deviceId,
            )
            val key = when {
                needsYou -> ThreadsGroupKey.NeedsYou
                running -> ThreadsGroupKey.Running
                else -> ThreadsGroupKey.Recent
            }
            buckets.getValue(key).add(row)
        }
        val order = compareByDescending<ThreadsListRow> { ms(it.activityAt) ?: 0L }.thenBy { it.sessionId }
        return buckets.entries
            .map { (key, rows) -> ThreadsListGroup(key, ListCopy.GROUPS.getValue(key), rows.sortedWith(order)) }
            .filter { it.rows.isNotEmpty() }
    }

    data class AgentRow(
        val sessionId: String,
        val teamId: String,
        val name: String,
        /** A curated board icon name (contract `boardIcon`). */
        val icon: String,
        /** `#rrggbb`. */
        val color: String,
        val busy: Boolean,
        val needsYou: Boolean,
    )

    /**
     * The Agents list (rail above Boards, Settings → Agents): one row per
     * agent root of the team, icon + color like boards, by `sortOrder` then
     * name. Its spinner / needs-you dot covers its own row only.
     */
    fun listAgents(sessions: List<Session>, teamId: String?): List<AgentRow> =
        sessions
            .filter {
                it.parentId == null && it.kind == DomainContract.sessionKindAgent &&
                    (teamId == null || it.teamId == teamId)
            }
            .sortedWith(
                compareBy<Session>({ it.sortOrder }, { it.name ?: "" }, { it.id }),
            )
            .map {
                AgentRow(
                    sessionId = it.id,
                    teamId = it.teamId,
                    name = it.name ?: "",
                    icon = it.icon ?: ListCopy.AGENT_DEFAULT_ICON,
                    color = it.color ?: ListCopy.AGENT_DEFAULT_COLOR,
                    busy = it.busy,
                    needsYou = it.needsInput,
                )
            }

    // ── the Guide's PR + the session composer (EXP-1306) ────────────────────

    /**
     * The ONE pull request the thread's Guide speaks for (its stack rail + the
     * bottom Merge capsule): the newest OPEN one, else the newest MERGED one
     * (createdAt, ties by the higher number); a closed PR never counts.
     * Replays `session-thread.json` `guidePullRequestCases`.
     */
    fun guidePullRequest(pullRequests: List<PullRequestEntity>): PullRequestEntity? {
        fun newest(state: String): PullRequestEntity? = pullRequests
            .filter { it.state == state }
            .fold<PullRequestEntity, PullRequestEntity?>(null) { best, pr ->
                if (best == null) return@fold pr
                val diff = (ms(pr.createdAt) ?: 0L) - (ms(best.createdAt) ?: 0L)
                if (diff > 0 || (diff == 0L && (pr.number ?: 0) > (best.number ?: 0))) pr else best
            }
        return newest(DomainContract.prStateOpen) ?: newest(DomainContract.prStateMerged)
    }

    /** EXP-820 ×4: a question or plan card waiting on the viewer takes the
     *  session composer's place (`composerCases`). */
    fun composerYieldsToCard(live: Boolean, pendingAsk: Boolean): Boolean = live && pendingAsk

    /** EXP-1309/1318 (`threads-list.json` `copy.words`): a Threads row's ONE
     *  quiet word ([wire] = the fixture key); the lock rides beside it. */
    enum class ThreadRowWord(val wire: String, val text: String) {
        Stack("stack", "stack"),
        ReadyToMerge("readyToMerge", "ready to merge"),
        Merged("merged", "merged"),
        Question("question", "question"),

        /** An agent row waiting on a person. */
        NeedsInput("needsInput", "needs input"),
    }

    /** A PR's stack edge (`threads-list.json` `stackCases`). */
    data class StackPr(val id: String, val state: String, val basePullRequestId: String?)

    /**
     * An open PR of the thread sits in an open stack: based on another open
     * PR, or another open PR is based on it. [teamPrs] = the team's PRs (a
     * stack edge may cross threads). Replays `stackCases`.
     */
    fun threadInStack(pullRequests: List<StackPr>, teamPrs: List<StackPr> = pullRequests): Boolean {
        val openIds = teamPrs.filter { it.state == DomainContract.prStateOpen }.map { it.id }.toSet()
        return pullRequests.filter { it.state == DomainContract.prStateOpen }.any { pr ->
            (pr.basePullRequestId != null && pr.basePullRequestId in openIds) ||
                teamPrs.any { it.state == DomainContract.prStateOpen && it.basePullRequestId == pr.id }
        }
    }

    /**
     * Word order (`wordCases`): an open PR in a stack → [ThreadRowWord.Stack];
     * an open PR while nothing works → [ThreadRowWord.ReadyToMerge] (working =
     * no word); no open PR but a merged one → [ThreadRowWord.Merged]; else
     * waiting on a person → [ThreadRowWord.Question]; else none. [states] =
     * the root's PR states.
     */
    fun threadRowWord(states: List<String>, busy: Boolean, needsYou: Boolean, inStack: Boolean): ThreadRowWord? {
        if (states.any { it == DomainContract.prStateOpen }) {
            if (inStack) return ThreadRowWord.Stack
            return if (busy) null else ThreadRowWord.ReadyToMerge
        }
        if (states.any { it == DomainContract.prStateMerged }) return ThreadRowWord.Merged
        return if (needsYou) ThreadRowWord.Question else null
    }

    private fun stackPr(row: PullRequestEntity) = StackPr(row.id, row.state, row.basePullRequestId)

    /** [threadRowWord] off synced PR rows. */
    fun threadRowWord(
        busy: Boolean,
        needsYou: Boolean,
        prs: List<PullRequestEntity>,
        teamPrs: List<PullRequestEntity> = prs,
    ): ThreadRowWord? = threadRowWord(
        states = prs.map { it.state },
        busy = busy,
        needsYou = needsYou,
        inStack = threadInStack(prs.map(::stackPr), teamPrs.map(::stackPr)),
    )

    /**
     * EXP-1318 the rail's Active band (`railCases`): every Needs you row,
     * every Running row, then the Recent rows whose word still asks for a
     * merge (ready to merge or stack), in group order.
     */
    fun railThreads(groups: List<ThreadsListGroup>, wordOf: (ThreadsListRow) -> ThreadRowWord?): List<ThreadsListRow> =
        groups.flatMap { group ->
            if (group.key == ThreadsGroupKey.Recent) {
                group.rows.filter {
                    val word = wordOf(it)
                    word == ThreadRowWord.ReadyToMerge || word == ThreadRowWord.Stack
                }
            } else {
                group.rows
            }
        }

    /** EXP-1307: one entry of the thread header's `…` on the Guide. */
    enum class GuideMenuEntry(val wire: String) { OpenGithub("open-github"), ClosePr("close-pr") }

    /**
     * EXP-1307: the thread header's `…` on the Guide: Open on GitHub whenever
     * the Guide has a PR ([state] = its state, null = none), Close PR while it
     * is open and the viewer is a [member]; empty = no trigger
     * (`guideMenuCases`).
     */
    fun guideMenuEntries(state: String?, member: Boolean): List<GuideMenuEntry> = when {
        state == null -> emptyList()
        state == DomainContract.prStateOpen && member -> listOf(GuideMenuEntry.OpenGithub, GuideMenuEntry.ClosePr)
        else -> listOf(GuideMenuEntry.OpenGithub)
    }

    // ── synced rows → the pure inputs ───────────────────────────────────────

    private val json = Json { ignoreUnknownKeys = true }

    private fun parseObject(raw: String?): JsonObject? {
        if (raw.isNullOrBlank()) return null
        return runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonObject
    }

    private fun parseArray(raw: String?): JsonArray? {
        if (raw.isNullOrBlank()) return null
        return runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonArray
    }

    fun session(row: SessionEntity): Session = Session(
        id = row.id,
        teamId = row.teamId,
        parentId = row.parentId,
        rootId = row.rootId.ifEmpty { row.id },
        kind = row.kind,
        userId = row.userId,
        status = row.status,
        busy = row.busy,
        needsInput = row.needsInput,
        blocked = parseObject(row.blocked)?.let { Blocked(it.str("window"), it.str("resetsAt")) },
        shared = row.shared,
        title = row.title,
        guide = parseArray(row.guide)?.map { element ->
            val obj = element as? JsonObject
            GuideEntry(text = obj?.str("text"), label = obj?.str("label"))
        },
        actionName = row.actionName,
        name = row.name,
        icon = row.icon,
        color = row.color,
        sortOrder = row.sortOrder,
        startedAt = row.startedAt,
        endedAt = row.endedAt,
        createdAt = row.createdAt,
        updatedAt = row.updatedAt,
        deviceId = row.deviceId.ifBlank { null },
    )

    fun sessionIssue(row: SessionIssueEntity): SessionIssue =
        SessionIssue(sessionId = row.sessionId, issueId = row.issueId, sortOrder = row.sortOrder)

    fun issue(row: IssueEntity): Issue = Issue(id = row.id, identifier = row.identifier, title = row.title)

    fun message(row: MessageEntity): Message = Message(
        id = row.id,
        sessionId = row.sessionId,
        authorKind = row.authorKind,
        authorId = row.authorId,
        body = row.body,
        card = parseObject(row.card),
        answeredAt = row.answeredAt,
        consumedAt = row.consumedAt,
        createdAt = row.createdAt,
    )

    fun pullRequest(row: PullRequestEntity): PullRequest? =
        row.number?.let { PullRequest(id = row.id, number = it) }

    /** The ISO `now` the view models take, off the wall clock. */
    fun nowIso(nowMs: Long = System.currentTimeMillis()): String = java.time.Instant.ofEpochMilli(nowMs).toString()
}
