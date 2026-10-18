package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.SessionIssueEntity
import com.exponential.app.data.db.SessionReadEntity
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
        /** EXP-1323: the live status line (absent = null). */
        val caption: String? = null,
        /** EXP-1323: a `/btw` aside child (absent = false). */
        val aside: Boolean = false,
    )

    /** EXP-1323: the viewer's read marks (`session_reads`, already user-scoped). */
    data class SessionRead(val sessionId: String, val readAt: String)

    /** [assignedSessionId] (EXP-1341) = the tree session working on it (null = the root's). */
    data class SessionIssue(
        val sessionId: String,
        val issueId: String,
        val sortOrder: Double,
        val assignedSessionId: String? = null,
    )

    /** [parentId] (EXP-1334) = the issue's `parent` relation (null = none):
     *  a linked sub-issue folds under its nearest LINKED ancestor. */
    data class Issue(val id: String, val identifier: String, val title: String, val parentId: String? = null)

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
        /** EXP-1323: the session that posted it (absent = null). */
        val authorSessionId: String? = null,
    )

    data class PullRequest(val id: String, val number: Int)

    // ── copy (= the fixtures) ───────────────────────────────────────────────

    object CardCopy {
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
        const val PLACEHOLDER = "Type / for commands"
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
        const val TEST_ID_TABS = "thread-tabs"
        const val TEST_ID_CONVERSATION = "thread-conversation"
        const val TEST_ID_COMPOSER = "thread-composer"
        /** EXP-1306: the Guide's stack band title. */
        const val STACK = "Stack"
        const val TEST_ID_GUIDE_STACK = "thread-guide-stack"
        const val TEST_ID_GUIDE_MERGE = "thread-guide-merge"
        /** EXP-1307: the header `…` on the Guide (Close PR, EXP-1338). */
        const val TEST_ID_GUIDE_MENU = "thread-guide-menu"
        /** EXP-1323: a child's report row. */
        const val REPORT = "Report from {session}"
    }

    /** EXP-1323 `copy.suggestions`. */
    object SuggestionsCopy {
        const val ONE = "1 suggested task"
        const val MANY = "{count} suggested tasks"
        const val STARTED = "{count} started"
        const val START_ALL = "Start all"
        const val START = "Start"
        const val HERE = "Do it here"
        const val OPTIONS = "Options"
        const val START_ON = "Start on…"
        const val START_WITH = "Start with…"
        const val FILE = "File as issue"
        const val DISMISS = "Dismiss"
        const val SENT_HERE = "Sent here"
        const val FILED = "Filed {issue}"
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

    /** Sessions of one tree in creation order (ties by id). */
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
        /** EXP-1323: the live status line under the name; an ended session's is
         *  dropped unless it is an aside (its answer's first line stays). */
        val caption: String? = null,
    )

    /**
     * THE session card = the Work card's header row: icon, name, the muted
     * state word, trailing elapsed, a spinner while [SessionCard.running].
     * Tapping it opens that session's raw view. [title] = its [sessionTitle]
     * (EXP-1341, never `Session N`); [now] = an ISO timestamp.
     */
    fun sessionCard(session: Session, title: String, now: String): SessionCard {
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
        val caption = session.caption?.trim()
        return SessionCard(
            sessionId = session.id,
            icon = CardCopy.ICONS.getValue(state),
            name = title,
            kindWord = CardCopy.STATES.getValue(state),
            state = state,
            tone = CardCopy.TONES.getValue(state),
            elapsed = elapsed,
            running = state == SessionState.Working,
            testId = CardCopy.TEST_ID_CARD,
            caption = caption?.takeIf { it.isNotEmpty() && (state != SessionState.Ended || session.aside) },
        )
    }

    // ── EXP-1341: titles via [SessionTitle] ─────────────────────────────────

    private fun titleSession(s: Session) =
        SessionTitle.TitleSession(id = s.id, rootId = s.rootId, name = s.name, title = s.title, actionName = s.actionName)

    private fun titleLink(link: SessionIssue) =
        SessionTitle.TitleLink(link.sessionId, link.issueId, link.sortOrder, link.assignedSessionId)

    private fun titleIssue(issue: Issue) = SessionTitle.TitleIssue(issue.id, issue.identifier, issue.title)

    private fun titleMessage(m: Message) = SessionTitle.TitleMessage(
        id = m.id,
        sessionId = m.sessionId,
        authorKind = m.authorKind,
        authorSessionId = m.authorSessionId,
        body = m.body,
        hasCard = m.card != null,
        createdAt = m.createdAt,
    )

    /** THE display title of [session] (fixture `session-title.json`). */
    fun sessionTitle(
        session: Session,
        sessionIssues: List<SessionIssue>,
        issues: List<Issue>,
        messages: List<Message>,
    ): String = SessionTitle.sessionTitle(
        titleSession(session),
        sessionIssues.map(::titleLink),
        issues.map(::titleIssue),
        messages.map(::titleMessage),
    )

    /** Every given session's title, keyed by id. */
    fun sessionTitles(
        sessions: List<Session>,
        sessionIssues: List<SessionIssue>,
        issues: List<Issue>,
        messages: List<Message>,
    ): Map<String, String> = SessionTitle.sessionTitles(
        sessions.map(::titleSession),
        sessionIssues.map(::titleLink),
        issues.map(::titleIssue),
        messages.map(::titleMessage),
    )

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

    /** EXP-1334: one row of a folded chip's subtree; [depth] relative to the chip. */
    data class NestedIssue(val issueId: String, val identifier: String, val depth: Int)

    data class NestedSession(val sessionId: String, val name: String, val markState: SessionState, val depth: Int)

    sealed interface ThreadTab {
        data object Add : ThreadTab
        data object Conversation : ThreadTab
        /** EXP-1334: a TOP-LEVEL linked issue; [nested] = the linked sub-issues folded under it. */
        data class IssueTab(
            val issueId: String,
            val identifier: String,
            val nested: List<NestedIssue> = emptyList(),
        ) : ThreadTab
        /** EXP-1334: a DIRECT child of the dispatcher; [markState] = its lead,
         *  [nested] = every deeper session folded under it. */
        data class SessionTab(
            val sessionId: String,
            val name: String,
            val markState: SessionState,
            val nested: List<NestedSession> = emptyList(),
        ) : ThreadTab
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
            /** EXP-1323: the session that posted it (a child asking its parent). */
            val authorSessionId: String? = null,
        ) : ThreadRow

        data class SessionRow(override val id: String, val card: SessionCard) : ThreadRow

        /** EXP-1323: a child's end on its parent: `Report from <name>` + the summary. */
        data class Report(
            override val id: String,
            val sessionId: String,
            val text: String,
            val summary: String,
        ) : ThreadRow

        /** EXP-1323: ONE suggestions card; [sessionId] = the posting session. */
        data class Suggestions(
            override val id: String,
            val sessionId: String,
            val header: String,
            val startAll: Boolean,
            val items: List<SuggestionRow>,
        ) : ThreadRow

        data class Question(
            override val id: String,
            /** EXP-1334: the session whose card it is (the root, or a child). */
            val sessionId: String,
            val question: String,
            val options: List<QuestionChoice>,
            /** A free-text "Other" answer is offered (default true; EXP-1280 `custom`). */
            val custom: Boolean,
            val answer: String?,
            val answered: Boolean,
            /** EXP-1280: several picks allowed. */
            val multi: Boolean = false,
            /** EXP-1280: the inline Exponential UI surface, if any. */
            val surface: CardSurface? = null,
            /** EXP-1280: the picked labels (null on a legacy card = [answer] alone). */
            val answers: List<String>? = null,
            /** EXP-1280: the surface's data model as the person answered it. */
            val surfaceData: JsonObject? = null,
        ) : ThreadRow

        /** One choice of a question card: the answer is the [label] (EXP-1322);
         *  [attachmentId] = its picture (EXP-1280 plan-card.json `pictureFor`). */
        data class QuestionChoice(val label: String, val description: String?, val attachmentId: String? = null)

        /** EXP-1280: one choice question of a plan card. */
        data class PlanQuestion(
            val id: String,
            val prompt: String,
            val multi: Boolean,
            val custom: Boolean,
            val options: List<QuestionChoice>,
        )

        /** [fresh] = an approval that started in a fresh context (ask-dock.json). */
        data class Plan(
            override val id: String,
            val sessionId: String,
            val plan: String,
            val decision: String?,
            val fresh: Boolean = false,
            /** EXP-1280: the choice questions, the inline surface. */
            val questions: List<PlanQuestion> = emptyList(),
            val surface: CardSurface? = null,
            /** EXP-1280: the picked labels per question id, the note. */
            val answers: Map<String, List<String>>? = null,
            val feedback: String? = null,
            /** EXP-1280: the surface's data model as the person answered it. */
            val surfaceData: JsonObject? = null,
        ) : ThreadRow

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

    /** EXP-1323: one suggested task's state (dismissed items drop). */
    enum class SuggestionState(val wire: String) { Open("open"), Started("started"), Here("here"), Filed("filed") }

    /** EXP-1323: one suggested task: `open` (× · Options · Do it here · Start),
     *  `started` (the child's card once synced), `here` (sent to the posting
     *  session), `filed` (the issue it became). */
    data class SuggestionRow(
        val id: String,
        val title: String,
        val text: String,
        val state: SuggestionState,
        val card: SessionCard?,
        val issueId: String?,
        val identifier: String?,
    )

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
        /** EXP-1323: the live tail (Tasks card + status line) reads this
         *  session's relay state; null once the root ended. */
        val liveSessionId: String? = null,
    )

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    /** JS `String(x ?? "")` for a card field. */
    private fun JsonObject.text(key: String): String = when (val value = this[key]) {
        null, is JsonNull -> ""
        is JsonPrimitive -> value.content
        else -> value.toString()
    }

    private fun JsonObject.isTrue(key: String): Boolean =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true"

    private fun JsonObject.isFalse(key: String): Boolean =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "false"

    // ── EXP-1280: the rich plan / question card (plan-card.json) ────────────

    /** A `question` / `plan` message's row (null = another kind). */
    fun askRow(message: Message): ThreadRow? {
        val card = message.card ?: return null
        return when (card.str("kind")) {
            DomainContract.messageCardKindQuestion -> ThreadRow.Question(
                id = message.id,
                sessionId = message.sessionId,
                question = card.text("question"),
                options = questionOptions(card),
                custom = !card.isFalse("custom"),
                answer = card.str("answer"),
                answered = message.answeredAt != null,
                multi = card.isTrue("multi"),
                surface = cardSurface(card["surface"]),
                answers = (card["answers"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content },
                surfaceData = card["surfaceData"] as? JsonObject,
            )
            DomainContract.messageCardKindPlan -> ThreadRow.Plan(
                id = message.id,
                sessionId = message.sessionId,
                plan = card.text("plan"),
                decision = card.str("decision"),
                fresh = card.isTrue("fresh"),
                questions = planQuestions(card),
                surface = cardSurface(card["surface"]),
                answers = planAnswers(card["answers"]),
                feedback = card.str("feedback")?.takeIf { it.isNotEmpty() },
                surfaceData = card["surfaceData"] as? JsonObject,
            )
            else -> null
        }
    }

    /**
     * The session view's waiting cards a TOOL posted (`exponential_plan_propose`
     * / `exponential_sessions_ask`: no `questionId`, so no open relay ask
     * mirrors them), oldest first; answered through `sessions.answer` only.
     */
    fun toolPostedAsks(messages: List<Message>): List<ThreadRow> = messages
        .filter { it.card != null && it.card.str("questionId") == null }
        .sortedWith(compareBy<Message>({ it.createdAt }, { it.id }))
        .mapNotNull(::askRow)
        .filter { row ->
            (row is ThreadRow.Plan && row.decision == null) || (row is ThreadRow.Question && !row.answered)
        }

    /** The question card's options: `options` = plain labels, descriptions ride
     *  the parallel `optionDescriptions` sidecar (null / "" = none, EXP-1322),
     *  pictures the `optionPictures` sidecar of picture LABELS resolved through
     *  `pictures`; zipped by ORIGINAL index before non-string options drop. */
    fun questionOptions(card: JsonObject): List<ThreadRow.QuestionChoice> {
        val options = card["options"] as? JsonArray ?: return emptyList()
        val descriptions = card["optionDescriptions"] as? JsonArray
        val pictureLabels = card["optionPictures"] as? JsonArray
        val pictures = PlanCard.cardPictures(card["pictures"])
        return options.mapIndexedNotNull { index, option ->
            val label = (option as? JsonPrimitive)?.takeIf { it.isString }?.content ?: return@mapIndexedNotNull null
            val description = (descriptions?.getOrNull(index) as? JsonPrimitive)
                ?.takeIf { it.isString && it.content.isNotEmpty() }?.content
            val picture = (pictureLabels?.getOrNull(index) as? JsonPrimitive)?.takeIf { it.isString }?.content
            ThreadRow.QuestionChoice(label, description, PlanCard.pictureFor(pictures, picture))
        }
    }

    /** A plan card's `questions`; a malformed question (no string id/prompt)
     *  or option (no string label) drops. */
    fun planQuestions(card: JsonObject): List<ThreadRow.PlanQuestion> {
        val questions = card["questions"] as? JsonArray ?: return emptyList()
        val pictures = PlanCard.cardPictures(card["pictures"])
        return questions.mapNotNull { raw ->
            val question = raw as? JsonObject ?: return@mapNotNull null
            val id = question.str("id") ?: return@mapNotNull null
            val prompt = question.str("prompt") ?: return@mapNotNull null
            val options = (question["options"] as? JsonArray).orEmpty().mapNotNull { entry ->
                val option = entry as? JsonObject ?: return@mapNotNull null
                val label = option.str("label") ?: return@mapNotNull null
                ThreadRow.QuestionChoice(
                    label = label,
                    description = option.str("description")?.takeIf { it.isNotEmpty() },
                    attachmentId = PlanCard.pictureFor(pictures, option.str("picture")),
                )
            }
            ThreadRow.PlanQuestion(id, prompt, question.isTrue("multi"), !question.isFalse("custom"), options)
        }
    }

    /** A card's inline surface: an object with a `root` object, else null. */
    fun cardSurface(raw: kotlinx.serialization.json.JsonElement?): CardSurface? {
        val surface = raw as? JsonObject ?: return null
        val root = surface["root"] as? JsonObject ?: return null
        return CardSurface(root, surface["data"] as? JsonObject)
    }

    /** A plan card's `answers` (question id → labels), strings only. */
    fun planAnswers(raw: kotlinx.serialization.json.JsonElement?): Map<String, List<String>>? {
        val answers = raw as? JsonObject ?: return null
        return answers.mapNotNull { (id, labels) ->
            val list = labels as? JsonArray ?: return@mapNotNull null
            id to list.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }
        }.toMap()
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

    private fun suggestionsRow(
        message: Message,
        card: JsonObject,
        byId: Map<String, Session>,
        titles: Map<String, String>,
        issues: Map<String, Issue>,
        now: String,
    ): ThreadRow? {
        val raws = card["items"] as? JsonArray ?: return null
        val items = mutableListOf<SuggestionRow>()
        for (element in raws) {
            val raw = element as? JsonObject ?: continue
            val id = raw.str("id") ?: continue
            if ((raw["dismissed"] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true") continue
            val sessionId = raw.str("sessionId")
            val issueId = raw.str("issueId")
            val target = sessionId?.let { byId[it] }
            val here = (raw["here"] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true"
            val state = when {
                sessionId != null -> SuggestionState.Started
                here -> SuggestionState.Here
                issueId != null -> SuggestionState.Filed
                else -> SuggestionState.Open
            }
            items.add(
                SuggestionRow(
                    id = id,
                    title = raw.text("title"),
                    text = raw.text("text"),
                    state = state,
                    card = target?.let { sessionCard(it, titles[it.id] ?: "", now) },
                    issueId = issueId,
                    identifier = issueId?.let { issues[it]?.identifier },
                ),
            )
        }
        if (items.isEmpty()) return null
        val started = items.count { it.state == SuggestionState.Started }
        val count = if (items.size == 1) {
            SuggestionsCopy.ONE
        } else {
            SuggestionsCopy.MANY.replace("{count}", items.size.toString())
        }
        return ThreadRow.Suggestions(
            id = message.id,
            sessionId = message.sessionId,
            header = if (started > 0) "$count · ${SuggestionsCopy.STARTED.replace("{count}", started.toString())}" else count,
            startAll = items.count { it.state == SuggestionState.Open } > 1,
            items = items,
        )
    }

    private val messageOrder = compareBy<Message>({ ms(it.createdAt) ?: 0L }, { it.id })

    /** Depth-first walk of a forest given each node's FOLD parent (null = a
     *  top-level node), children in the given order. */
    private fun <T> foldForest(
        items: List<T>,
        id: (T) -> String,
        foldParent: (T) -> String?,
    ): List<Pair<T, List<Pair<T, Int>>>> {
        val children = LinkedHashMap<String?, MutableList<T>>()
        for (item in items) children.getOrPut(foldParent(item)) { mutableListOf() }.add(item)
        val seen = mutableSetOf<String>()
        fun fold(top: T): Pair<T, List<Pair<T, Int>>> {
            val nested = mutableListOf<Pair<T, Int>>()
            seen.add(id(top))
            fun walk(parentId: String, depth: Int) {
                for (child in children[parentId].orEmpty()) {
                    if (!seen.add(id(child))) continue
                    nested.add(child to depth)
                    walk(id(child), depth + 1)
                }
            }
            walk(id(top), 1)
            return top to nested.toList()
        }
        val out = children[null].orEmpty().map(::fold).toMutableList()
        // A fold-parent cycle (a ↔ b) reaches no top: its first unreached
        // item (input order) becomes a top-level chip, the rest fold under it.
        for (item in items) if (id(item) !in seen) out.add(fold(item))
        return out
    }

    /** EXP-1334: a linked issue's fold parent = its nearest LINKED ancestor
     *  (an unlinked parent is skipped; a cycle or none = top-level). */
    private fun issueFoldParent(issue: Issue, linked: Set<String>, all: Map<String, Issue>): String? {
        val seen = mutableSetOf(issue.id)
        var parent = issue.parentId
        while (parent != null && parent !in seen) {
            if (parent in linked) return parent
            seen.add(parent)
            parent = all[parent]?.parentId
        }
        return null
    }

    /**
     * The thread screen (EXP-1334: the conversation IS the dispatcher): the
     * header tabs (+ · Conversation · top-level issue chips (sub-issues
     * folded) · the dispatcher's direct children (deeper sessions folded) ·
     * Guide), the root's conversation as rows (its messages oldest first plus
     * the children's own question / plan cards; a `session` /
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
        val byId = sessions.associateBy { it.id }.toMutableMap()
        byId[root.id] = root
        val titles = sessionTitles(byId.values.toList(), sessionIssues, issues, messages)
        val linked = rootIssues(root.id, sessionIssues, issues)
        val issueMap = issues.associateBy { it.id }
        val prMap = pullRequests.associateBy { it.id }
        val own = messages.filter { it.sessionId == root.id }.sortedWith(messageOrder)
        val title = titles.getValue(root.id)
        val focused = focus?.let { id -> tree.firstOrNull { it.id == id } }
        // EXP-1334: the conversation IS the dispatcher (no chip of its own);
        // the chips = its DIRECT children, every deeper session folded under
        // its top-level ancestor (the chip's dropdown).
        fun nameOf(s: Session) = sessionCard(s, titles[s.id] ?: "", now).name
        val treeIds = tree.map { it.id }.toSet()
        val sessionTabs = foldForest(
            tree.filter { it.id != root.id },
            { it.id },
            { s -> if (s.parentId == root.id || s.parentId == null || s.parentId !in treeIds) null else s.parentId },
        ).map { (top, nested) ->
            ThreadTab.SessionTab(
                sessionId = top.id,
                name = nameOf(top),
                markState = sessionTabMarkState(top),
                nested = nested.map { (item, depth) ->
                    NestedSession(item.id, nameOf(item), sessionTabMarkState(item), depth)
                },
            )
        }
        val linkedIds = linked.map { it.id }.toSet()
        val issueTabs = foldForest(linked, { it.id }, { issueFoldParent(it, linkedIds, issueMap) }).map { (top, nested) ->
            ThreadTab.IssueTab(
                issueId = top.id,
                identifier = top.identifier,
                nested = nested.map { (item, depth) -> NestedIssue(item.id, item.identifier, depth) },
            )
        }

        // EXP-1334: no root card; a CHILD's own question / plan card surfaces
        // in the Conversation (answered here, straight back to that child).
        val asks = messages.filter { m ->
            m.sessionId != root.id && m.sessionId in treeIds &&
                m.card?.str("kind").let {
                    it == DomainContract.messageCardKindQuestion || it == DomainContract.messageCardKindPlan
                }
        }
        val conversation = (own + asks).sortedWith(messageOrder)
        val rows = mutableListOf<ThreadRow>()
        for (message in conversation) {
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
                        authorSessionId = message.authorSessionId,
                    ),
                )
                continue
            }
            when (card.str("kind")) {
                DomainContract.messageCardKindSession, DomainContract.messageCardKindThreadStarted -> {
                    val target = card.str("sessionId")?.let { byId[it] } ?: continue
                    rows.add(ThreadRow.SessionRow(message.id, sessionCard(target, titles[target.id] ?: "", now)))
                }
                DomainContract.messageCardKindQuestion, DomainContract.messageCardKindPlan ->
                    askRow(message)?.let(rows::add)
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
                DomainContract.messageCardKindReport -> {
                    val target = card.str("sessionId")?.let { byId[it] } ?: continue
                    rows.add(
                        ThreadRow.Report(
                            id = message.id,
                            sessionId = target.id,
                            text = ThreadCopy.REPORT.replace(
                                "{session}",
                                sessionCard(target, titles[target.id] ?: "", now).name,
                            ),
                            summary = card.str("summary") ?: message.body,
                        ),
                    )
                }
                DomainContract.messageCardKindSuggestions -> {
                    suggestionsRow(message, card, byId, titles, issueMap, now)?.let(rows::add)
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
                add(ThreadTab.Add)
                add(ThreadTab.Conversation)
                addAll(issueTabs)
                addAll(sessionTabs)
                add(ThreadTab.Guide(guideDot))
            },
            rows = rows,
            composer = Composer(root.id, ThreadCopy.PLACEHOLDER),
            live = live.isNotEmpty(),
            busy = live.any { it.busy },
            needsYou = live.any { it.needsInput },
            liveSessionId = if (root.status == DomainContract.sessionStatusEnded) null else root.id,
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
        val busy: Boolean,
        val needsYou: Boolean,
        val activityAt: String,
        /** The ROOT's device (the row's trailing device icon); null = none. */
        val deviceId: String? = null,
        /** EXP-1323: 0 = the thread (opens the thread), >0 = a nested child
         *  (opens that session); [parentId] null on a root. */
        val depth: Int = 0,
        val parentId: String? = null,
        val rootId: String = sessionId,
        /** EXP-1323: bold title (a root: the tree's newest agent message is
         *  newer than the viewer's read of the root; a child: its own). */
        val unread: Boolean = false,
        /** EXP-1323: the live caption (the session card rule). */
        val caption: String? = null,
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
        /** EXP-1323: the viewer's `session_reads` (empty = none read). */
        reads: List<SessionRead> = emptyList(),
    ): List<ThreadsListGroup> {
        val readAt = reads.associate { it.sessionId to (ms(it.readAt) ?: 0L) }
        val rootOf = sessions.associate { it.id to it.rootId }
        val newestAgent = HashMap<String, Long>()
        val newestAgentInTree = HashMap<String, Long>()
        for (message in messages) {
            if (message.authorKind != DomainContract.messageAuthorKindAgent) continue
            val at = ms(message.createdAt) ?: 0L
            newestAgent[message.sessionId] = maxOf(newestAgent[message.sessionId] ?: 0L, at)
            val rootId = rootOf[message.sessionId] ?: continue
            newestAgentInTree[rootId] = maxOf(newestAgentInTree[rootId] ?: 0L, at)
        }
        fun unreadAt(newest: Long?, sessionId: String) = newest != null && newest > (readAt[sessionId] ?: -1L)
        val now = ""
        // Converted once: titles are resolved per row below.
        val titleLinks = sessionIssues.map(::titleLink)
        val titleIssues = issues.map(::titleIssue)
        val titleMessages = messages.map(::titleMessage)
        fun titleOf(s: Session) = SessionTitle.sessionTitle(titleSession(s), titleLinks, titleIssues, titleMessages)
        val roots = sessions.filter {
            it.parentId == null && it.kind == DomainContract.sessionKindThread &&
                (teamId == null || it.teamId == teamId)
        }
        val byRoot = LinkedHashMap<String, MutableList<Session>>()
        for (session in sessions) byRoot.getOrPut(session.rootId) { mutableListOf() }.add(session)
        val buckets = linkedMapOf<ThreadsGroupKey, MutableList<Pair<ThreadsListRow, List<ThreadsListRow>>>>(
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
            val title = titleOf(root)
            val row = ThreadsListRow(
                sessionId = root.id,
                teamId = root.teamId,
                title = title,
                busy = live.any { it.busy },
                needsYou = needsYou,
                activityAt = activity,
                deviceId = root.deviceId,
                depth = 0,
                parentId = null,
                rootId = root.id,
                unread = unreadAt(newestAgentInTree[root.id], root.id),
                caption = sessionCard(root, title, now).caption,
            )
            val key = when {
                needsYou -> ThreadsGroupKey.NeedsYou
                running -> ThreadsGroupKey.Running
                else -> ThreadsGroupKey.Recent
            }
            // A live tree nests its children (depth-first, creation order;
            // asides stay in the Conversation); Recent collapses to the root.
            val children = mutableListOf<ThreadsListRow>()
            if (key != ThreadsGroupKey.Recent) {
                val ordered = treeOrder(tree)
                fun walk(parentId: String, depth: Int) {
                    for (child in ordered) {
                        if (child.parentId != parentId || child.aside) continue
                        val card = sessionCard(child, titleOf(child), now)
                        val ended = child.status == DomainContract.sessionStatusEnded
                        children.add(
                            ThreadsListRow(
                                sessionId = child.id,
                                teamId = child.teamId,
                                title = card.name,
                                busy = !ended && child.busy,
                                needsYou = !ended && child.needsInput,
                                activityAt = child.updatedAt,
                                deviceId = child.deviceId,
                                depth = depth,
                                parentId = parentId,
                                rootId = root.id,
                                unread = unreadAt(newestAgent[child.id], child.id),
                                caption = card.caption,
                            ),
                        )
                        walk(child.id, depth + 1)
                    }
                }
                walk(root.id, 1)
            }
            buckets.getValue(key).add(row to children)
        }
        val order = compareByDescending<Pair<ThreadsListRow, List<ThreadsListRow>>> { ms(it.first.activityAt) ?: 0L }
            .thenBy { it.first.sessionId }
        return buckets.entries
            .map { (key, entries) ->
                ThreadsListGroup(
                    key,
                    ListCopy.GROUPS.getValue(key),
                    entries.sortedWith(order).flatMap { (row, children) -> listOf(row) + children },
                )
            }
            .filter { it.rows.isNotEmpty() }
    }

    /** EXP-1323: the Threads nav dot = any thread row is unread (`unreadCases`). */
    fun threadsUnread(groups: List<ThreadsListGroup>): Boolean =
        groups.any { group -> group.rows.any { it.depth == 0 && it.unread } }

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

    // ── EXP-1334: the thread strip never scrolls ────────────────────────────

    /**
     * Which chips get drawn, in order; the rest collapse into one "+N" (the
     * desktop's `partition_tabs`): chips pack in order until the next would
     * not fit; the overflow chip's width is reserved only once something
     * overflows; the ACTIVE chip is always visible; one chip always survives.
     */
    fun partitionTabs(
        widths: List<Float>,
        available: Float,
        gap: Float,
        overflowW: Float,
        activeIndex: Int?,
    ): List<Int> {
        val count = widths.size
        val room = maxOf(available, 0f)
        val total = widths.sum() + gap * maxOf(count - 1, 0)
        if (total <= room) return widths.indices.toList()
        val budget = maxOf(room - overflowW - gap, 0f)
        val visible = mutableListOf<Int>()
        var used = if (activeIndex == null) 0f else widths[activeIndex]
        var chips = if (activeIndex == null) 0 else 1
        for (index in 0 until count) {
            if (index == activeIndex) {
                visible.add(index)
                continue
            }
            val next = used + widths[index] + (if (chips > 0) gap else 0f)
            if (next > budget && chips > 0) break
            visible.add(index)
            used = next
            chips += 1
        }
        if (activeIndex != null && activeIndex !in visible) visible.add(activeIndex)
        if (visible.isEmpty() && count > 0) visible.add(0)
        return visible
    }

    enum class StripTier(val wire: String) { Full("full"), Short("short"), Fold("fold") }

    /** One issue chip's two measured widths: identifier + title, identifier alone. */
    data class StripIssueWidth(val full: Float, val short: Float)

    /** [issues] / [sessions] = the indexes of the chips drawn (strip order). */
    data class ThreadStripLayout(val tier: StripTier, val issues: List<Int>, val sessions: List<Int>)

    /**
     * EXP-1334: the strip between Conversation and Guide (both pinned, never
     * scrolled): issues then sessions, one [gap] between the zones. Tiers:
     * everything full → every issue short → fold: the sessions keep their OWN
     * room (what the short issues leave, at least half), each zone packs with
     * [partitionTabs] and its own "+N" ([overflowW]). Replays `stripCases`.
     */
    fun partitionThreadStrip(
        issues: List<StripIssueWidth>,
        sessions: List<Float>,
        available: Float,
        gap: Float,
        overflowW: Float,
        activeIssue: Int?,
        activeSession: Int?,
    ): ThreadStripLayout {
        val room = maxOf(available, 0f)
        fun span(widths: List<Float>) = if (widths.isEmpty()) 0f else widths.sum() + gap * (widths.size - 1)
        val sep = if (issues.isNotEmpty() && sessions.isNotEmpty()) gap else 0f
        val sessionSpan = span(sessions)
        val full = issues.map { it.full }
        val short = issues.map { it.short }
        if (span(full) + sep + sessionSpan <= room) {
            return ThreadStripLayout(StripTier.Full, issues.indices.toList(), sessions.indices.toList())
        }
        if (span(short) + sep + sessionSpan <= room) {
            return ThreadStripLayout(StripTier.Short, issues.indices.toList(), sessions.indices.toList())
        }
        fun zone(widths: List<Float>, budget: Float, active: Int?): Pair<List<Int>, Float> {
            val shown = partitionTabs(widths, budget, gap, overflowW, active)
            val used = span(shown.map { widths[it] }) + if (shown.size < widths.size) gap + overflowW else 0f
            return shown to used
        }
        val sessionBudget = if (issues.isEmpty()) {
            room
        } else {
            minOf(sessionSpan, maxOf(room / 2, room - span(short) - sep))
        }
        val (shownSessions, usedSessions) = zone(sessions, sessionBudget, activeSession)
        val issueBudget = room - if (sessions.isNotEmpty()) usedSessions + sep else 0f
        val (shownIssues, _) = zone(short, issueBudget, activeIssue)
        return ThreadStripLayout(StripTier.Fold, shownIssues, shownSessions)
    }

    /** EXP-1309/1318 (`threads-list.json` `copy.words`): a Threads row's ONE
     *  quiet word ([wire] = the fixture key). EXP-1339: [Merged] has NO text
     *  (null), it only turns the lead pr-merged violet. */
    enum class ThreadRowWord(val wire: String, val text: String?) {
        Stack("stack", "stack"),
        ReadyToMerge("readyToMerge", "ready to merge"),
        Merged("merged", null),
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
    enum class GuideMenuEntry(val wire: String) { ClosePr("close-pr") }

    /**
     * EXP-1307: the thread header's `…` on the Guide: Close PR while the
     * Guide's PR ([state] = its state, null = none) is open and the viewer is
     * a [member]; empty = no trigger (`guideMenuCases`). EXP-1338: no GitHub
     * entry, only the agent opens PRs.
     */
    fun guideMenuEntries(state: String?, member: Boolean): List<GuideMenuEntry> =
        if (state == DomainContract.prStateOpen && member) listOf(GuideMenuEntry.ClosePr) else emptyList()

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
        caption = row.caption,
        aside = row.aside,
    )

    fun sessionIssue(row: SessionIssueEntity): SessionIssue =
        SessionIssue(
            sessionId = row.sessionId,
            issueId = row.issueId,
            sortOrder = row.sortOrder,
            assignedSessionId = row.assignedSessionId,
        )

    fun issue(row: IssueEntity, parentId: String? = null): Issue =
        Issue(id = row.id, identifier = row.identifier, title = row.title, parentId = parentId)

    /** EXP-1334: child → parent off the synced `parent` relations
     *  (`issue_id` = the parent); several parents = the lowest id wins. */
    fun issueParents(relations: List<IssueRelationEntity>): Map<String, String> {
        val parents = HashMap<String, String>()
        for (relation in relations) {
            if (relation.type != DomainContract.issueRelationTypeParent || relation.issueId == relation.relatedIssueId) continue
            val current = parents[relation.relatedIssueId]
            if (current == null || relation.issueId < current) parents[relation.relatedIssueId] = relation.issueId
        }
        return parents
    }

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
        authorSessionId = row.authorSessionId,
    )

    fun sessionRead(row: SessionReadEntity): SessionRead = SessionRead(sessionId = row.sessionId, readAt = row.readAt)

    fun pullRequest(row: PullRequestEntity): PullRequest? =
        row.number?.let { PullRequest(id = row.id, number = it) }

    /** The ISO `now` the view models take, off the wall clock. */
    fun nowIso(nowMs: Long = System.currentTimeMillis()): String = java.time.Instant.ofEpochMilli(nowMs).toString()
}
